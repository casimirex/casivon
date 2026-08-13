use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::inventory::domain::entities::Warehouse;
use crate::modules::inventory::domain::repositories::{WarehouseFilters, WarehouseRepository};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 3] = ["created_at", "name", "code"];

#[derive(Clone)]
pub struct PgWarehouseRepository {
    pool: PgPool,
}

impl PgWarehouseRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a WarehouseFilters) {
    if let Some(is_active) = filters.is_active {
        builder.push(" AND is_active = ").push_bind(is_active);
    }
    if let Some(search) = &filters.search {
        let pattern = format!("%{}%", search);
        builder
            .push(" AND (name ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR code ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
}

#[async_trait]
impl WarehouseRepository for PgWarehouseRepository {
    async fn create(&self, warehouse: &Warehouse) -> AppResult<Warehouse> {
        Ok(sqlx::query_as::<_, Warehouse>(
            r#"
            INSERT INTO warehouses
                (id, org_id, code, name, address, city, country, manager_id, is_active,
                 created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING *
            "#,
        )
        .bind(warehouse.id)
        .bind(warehouse.org_id)
        .bind(&warehouse.code)
        .bind(&warehouse.name)
        .bind(&warehouse.address)
        .bind(&warehouse.city)
        .bind(&warehouse.country)
        .bind(warehouse.manager_id)
        .bind(warehouse.is_active)
        .bind(warehouse.created_at)
        .bind(warehouse.updated_at)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Warehouse>> {
        Ok(sqlx::query_as::<_, Warehouse>("SELECT * FROM warehouses WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn find_by_code(&self, code: &str) -> AppResult<Option<Warehouse>> {
        Ok(sqlx::query_as::<_, Warehouse>("SELECT * FROM warehouses WHERE code = $1")
            .bind(code)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn update(&self, warehouse: &Warehouse) -> AppResult<Warehouse> {
        Ok(sqlx::query_as::<_, Warehouse>(
            r#"
            UPDATE warehouses SET
                name = $2, address = $3, city = $4, country = $5, manager_id = $6,
                is_active = $7, updated_at = $8
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(warehouse.id)
        .bind(&warehouse.name)
        .bind(&warehouse.address)
        .bind(&warehouse.city)
        .bind(&warehouse.country)
        .bind(warehouse.manager_id)
        .bind(warehouse.is_active)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM warehouses WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &WarehouseFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Warehouse>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM warehouses WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "created_at")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<Warehouse>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM warehouses WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }
}
