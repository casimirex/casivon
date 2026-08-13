use axum::{
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

const MAX_PER_PAGE: i64 = 200;

/// Shared query string for every list endpoint.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PaginationParams {
    /// 1-based. Values below 1 are clamped.
    #[serde(default = "default_page")]
    #[param(default = 1, minimum = 1, example = 1)]
    pub page: i64,
    /// Clamped to 200: one request cannot ask for the whole table.
    #[serde(default = "default_per_page")]
    #[param(default = 20, minimum = 1, maximum = 200, example = 20)]
    pub per_page: i64,
    /// `?sort=-created_at` — a leading minus means descending. Each endpoint
    /// has its own allow-list of sortable columns; anything else falls back to
    /// that endpoint's default.
    #[param(example = "-created_at")]
    pub sort: Option<String>,
}

fn default_page() -> i64 {
    1
}
fn default_per_page() -> i64 {
    20
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self { page: 1, per_page: 20, sort: None }
    }
}

impl PaginationParams {
    /// Callers must use these instead of the raw fields: a negative `page` would
    /// otherwise produce a negative OFFSET and a 500 from Postgres.
    pub fn page(&self) -> i64 {
        self.page.max(1)
    }

    pub fn per_page(&self) -> i64 {
        self.per_page.clamp(1, MAX_PER_PAGE)
    }

    pub fn offset(&self) -> i64 {
        (self.page() - 1) * self.per_page()
    }

    /// Builds an `ORDER BY` clause from `?sort=`, rejecting anything not on the
    /// caller's allow-list. Column names cannot be bound as parameters, so the
    /// allow-list is what keeps this free of injection.
    pub fn order_by(&self, allowed: &[&str], default: &str) -> String {
        let Some(raw) = self.sort.as_deref() else {
            return format!("ORDER BY {} DESC", default);
        };

        let (column, direction) = match raw.strip_prefix('-') {
            Some(rest) => (rest, "DESC"),
            None => (raw.trim_start_matches('+'), "ASC"),
        };

        if allowed.contains(&column) {
            format!("ORDER BY {} {}", column, direction)
        } else {
            format!("ORDER BY {} DESC", default)
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaginatedResponse<T> {
    pub success: bool,
    pub data: Vec<T>,
    pub pagination: PaginationMeta,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaginationMeta {
    pub page: i64,
    pub per_page: i64,
    pub total: i64,
    pub total_pages: i64,
}

impl<T> PaginatedResponse<T> {
    pub fn new(data: Vec<T>, total: i64, params: &PaginationParams) -> Self {
        let per_page = params.per_page();
        Self {
            success: true,
            data,
            pagination: PaginationMeta {
                page: params.page(),
                per_page,
                total,
                total_pages: (total as f64 / per_page as f64).ceil() as i64,
            },
        }
    }
}

impl<T: Serialize> IntoResponse for PaginatedResponse<T> {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(page: i64, per_page: i64, sort: Option<&str>) -> PaginationParams {
        PaginationParams { page, per_page, sort: sort.map(str::to_string) }
    }

    #[test]
    fn clamps_out_of_range_input() {
        let p = params(-3, 5_000, None);
        assert_eq!(p.page(), 1);
        assert_eq!(p.per_page(), MAX_PER_PAGE);
        assert_eq!(p.offset(), 0);
    }

    #[test]
    fn offset_follows_page_size() {
        assert_eq!(params(3, 20, None).offset(), 40);
    }

    #[test]
    fn sort_allow_list_is_enforced() {
        let allowed = ["created_at", "name"];
        assert_eq!(params(1, 20, Some("-name")).order_by(&allowed, "created_at"), "ORDER BY name DESC");
        assert_eq!(params(1, 20, Some("name")).order_by(&allowed, "created_at"), "ORDER BY name ASC");
        assert_eq!(
            params(1, 20, Some("name; DROP TABLE users")).order_by(&allowed, "created_at"),
            "ORDER BY created_at DESC"
        );
    }

    #[test]
    fn total_pages_rounds_up() {
        let page = PaginatedResponse::new(vec![1, 2, 3], 101, &params(1, 20, None));
        assert_eq!(page.pagination.total_pages, 6);
    }
}
