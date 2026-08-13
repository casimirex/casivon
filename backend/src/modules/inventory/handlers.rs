use axum::extract::{Path, Query, State};
use axum::Extension;
use uuid::Uuid;

use crate::error::AppResult;
use crate::infrastructure::state::AppState;
use crate::modules::inventory::application::dto::*;
use crate::modules::inventory::application::use_cases::*;
use crate::modules::inventory::domain::entities::*;
use crate::modules::inventory::domain::repositories::*;
use crate::modules::inventory::infrastructure::repositories::*;
use crate::shared::auth::CurrentUser;
use crate::shared::pagination::{PaginatedResponse, PaginationParams};
use crate::shared::response::{ApiResponse, Created, DeletedResponse, ErrorResponse};
use crate::shared::validation::ValidatedJson;

fn products(state: &AppState) -> ProductUseCases<PgProductRepository, PgStockRepository> {
    ProductUseCases::new(
        PgProductRepository::new(state.db.clone()),
        PgStockRepository::new(state.db.clone()),
    )
}

fn categories(state: &AppState) -> CategoryUseCases<PgProductCategoryRepository> {
    CategoryUseCases::new(PgProductCategoryRepository::new(state.db.clone()))
}

fn warehouses(state: &AppState) -> WarehouseUseCases<PgWarehouseRepository> {
    WarehouseUseCases::new(PgWarehouseRepository::new(state.db.clone()))
}

fn stock(
    state: &AppState,
) -> StockUseCases<PgStockRepository, PgProductRepository, PgWarehouseRepository> {
    StockUseCases::new(
        PgStockRepository::new(state.db.clone()),
        PgProductRepository::new(state.db.clone()),
        PgWarehouseRepository::new(state.db.clone()),
        state.poster.clone(),
    )
}

fn boms(state: &AppState) -> BomUseCases<PgBomRepository, PgProductRepository> {
    BomUseCases::new(
        PgBomRepository::new(state.db.clone()),
        PgProductRepository::new(state.db.clone()),
    )
}

// ----------------------------------------------------------------- products

#[utoipa::path(
    post, path = "/api/v1/inventory/products", tag = "Inventory",
    request_body = CreateProductRequest,
    responses((status = 201, body = ApiResponse<Product>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_product(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateProductRequest>,
) -> AppResult<Created<Product>> {
    Ok(Created(products(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/inventory/products", tag = "Inventory",
    params(PaginationParams, ProductFilters),
    responses((status = 200, body = PaginatedResponse<Product>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_products(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<ProductFilters>,
) -> AppResult<PaginatedResponse<Product>> {
    let (data, total) = products(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/inventory/products/{id}", tag = "Inventory",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<ProductDetail>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_product(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<ProductDetail>> {
    Ok(ApiResponse::new(products(&state).get(id).await?))
}

#[utoipa::path(
    put, path = "/api/v1/inventory/products/{id}", tag = "Inventory",
    params(("id" = Uuid, Path)),
    request_body = UpdateProductRequest,
    responses((status = 200, body = ApiResponse<Product>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_product(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateProductRequest>,
) -> AppResult<ApiResponse<Product>> {
    Ok(ApiResponse::new(products(&state).update(id, req).await?))
}

/// Deletes the product, or deactivates it when stock history would be orphaned.
#[utoipa::path(
    delete, path = "/api/v1/inventory/products/{id}", tag = "Inventory",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<serde_json::Value>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_product(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<serde_json::Value>> {
    match products(&state).delete(id).await? {
        Some(deactivated) => Ok(ApiResponse::new(serde_json::json!({
            "deleted": false,
            "deactivated": true,
            "product": deactivated,
        }))),
        None => Ok(ApiResponse::new(serde_json::json!({
            "deleted": true,
            "deactivated": false,
        }))),
    }
}

// --------------------------------------------------------------- categories

#[utoipa::path(
    post, path = "/api/v1/inventory/categories", tag = "Inventory",
    request_body = CreateCategoryRequest,
    responses((status = 201, body = ApiResponse<ProductCategory>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_category(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateCategoryRequest>,
) -> AppResult<Created<ProductCategory>> {
    Ok(Created(categories(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/inventory/categories", tag = "Inventory",
    responses((status = 200, body = ApiResponse<Vec<ProductCategory>>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_categories(
    State(state): State<AppState>,
) -> AppResult<ApiResponse<Vec<ProductCategory>>> {
    Ok(ApiResponse::new(categories(&state).list().await?))
}

#[utoipa::path(
    put, path = "/api/v1/inventory/categories/{id}", tag = "Inventory",
    params(("id" = Uuid, Path)),
    request_body = UpdateCategoryRequest,
    responses((status = 200, body = ApiResponse<ProductCategory>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_category(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateCategoryRequest>,
) -> AppResult<ApiResponse<ProductCategory>> {
    Ok(ApiResponse::new(categories(&state).update(id, req).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/inventory/categories/{id}", tag = "Inventory",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_category(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    categories(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}

// --------------------------------------------------------------- warehouses

#[utoipa::path(
    post, path = "/api/v1/inventory/warehouses", tag = "Inventory",
    request_body = CreateWarehouseRequest,
    responses((status = 201, body = ApiResponse<Warehouse>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_warehouse(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateWarehouseRequest>,
) -> AppResult<Created<Warehouse>> {
    Ok(Created(warehouses(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/inventory/warehouses", tag = "Inventory",
    params(PaginationParams, WarehouseFilters),
    responses((status = 200, body = PaginatedResponse<Warehouse>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_warehouses(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<WarehouseFilters>,
) -> AppResult<PaginatedResponse<Warehouse>> {
    let (data, total) = warehouses(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/inventory/warehouses/{id}", tag = "Inventory",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<Warehouse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_warehouse(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<Warehouse>> {
    Ok(ApiResponse::new(warehouses(&state).get(id).await?))
}

#[utoipa::path(
    put, path = "/api/v1/inventory/warehouses/{id}", tag = "Inventory",
    params(("id" = Uuid, Path)),
    request_body = UpdateWarehouseRequest,
    responses((status = 200, body = ApiResponse<Warehouse>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_warehouse(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateWarehouseRequest>,
) -> AppResult<ApiResponse<Warehouse>> {
    Ok(ApiResponse::new(warehouses(&state).update(id, req).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/inventory/warehouses/{id}", tag = "Inventory",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_warehouse(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    warehouses(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}

#[utoipa::path(
    get, path = "/api/v1/inventory/warehouses/{id}/stock", tag = "Inventory",
    params(("id" = Uuid, Path), PaginationParams),
    responses((status = 200, body = PaginatedResponse<StockLevelView>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn warehouse_stock(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<PaginationParams>,
) -> AppResult<PaginatedResponse<StockLevelView>> {
    let (data, total) = stock(&state).levels_for_warehouse(id, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

// -------------------------------------------------------------------- stock

#[utoipa::path(
    post, path = "/api/v1/inventory/movements", tag = "Inventory",
    request_body = RecordMovementRequest,
    responses((status = 201, body = ApiResponse<MovementResult>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn record_movement(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<RecordMovementRequest>,
) -> AppResult<Created<MovementResult>> {
    Ok(Created(stock(&state).record_movement(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/inventory/movements", tag = "Inventory",
    params(PaginationParams, MovementFilters),
    responses((status = 200, body = PaginatedResponse<StockMovement>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_movements(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<MovementFilters>,
) -> AppResult<PaginatedResponse<StockMovement>> {
    let (data, total) = stock(&state).list_movements(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/inventory/stock/low", tag = "Inventory",
    params(PaginationParams),
    responses((status = 200, body = PaginatedResponse<StockLevelView>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn low_stock(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> AppResult<PaginatedResponse<StockLevelView>> {
    let (data, total) = stock(&state).low_stock(&params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    post, path = "/api/v1/inventory/stock/reorder-policy", tag = "Inventory",
    request_body = SetReorderPolicyRequest,
    responses((status = 200, body = ApiResponse<StockLevelView>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn set_reorder_policy(
    State(state): State<AppState>,
    ValidatedJson(req): ValidatedJson<SetReorderPolicyRequest>,
) -> AppResult<ApiResponse<StockLevelView>> {
    Ok(ApiResponse::new(stock(&state).set_reorder_policy(req).await?))
}

#[utoipa::path(
    get, path = "/api/v1/inventory/stock/valuation", tag = "Inventory",
    responses((status = 200, body = ApiResponse<ValuationResponse>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn stock_valuation(
    State(state): State<AppState>,
) -> AppResult<ApiResponse<ValuationResponse>> {
    Ok(ApiResponse::new(stock(&state).valuation().await?))
}

// --------------------------------------------------------------------- boms

#[utoipa::path(
    post, path = "/api/v1/inventory/boms", tag = "Inventory",
    request_body = CreateBomRequest,
    responses((status = 201, body = ApiResponse<BomDetail>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_bom(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateBomRequest>,
) -> AppResult<Created<BomDetail>> {
    Ok(Created(boms(&state).create(req, &user).await?))
}

#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BomListFilters {
    pub product_id: Option<Uuid>,
}

#[utoipa::path(
    get, path = "/api/v1/inventory/boms", tag = "Inventory",
    params(PaginationParams, BomListFilters),
    responses((status = 200, body = PaginatedResponse<BillOfMaterials>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_boms(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<BomListFilters>,
) -> AppResult<PaginatedResponse<BillOfMaterials>> {
    let (data, total) = boms(&state).list(filters.product_id, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/inventory/boms/{id}", tag = "Inventory",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<BomDetail>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_bom(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<BomDetail>> {
    Ok(ApiResponse::new(boms(&state).get(id).await?))
}

#[utoipa::path(
    put, path = "/api/v1/inventory/boms/{id}", tag = "Inventory",
    params(("id" = Uuid, Path)),
    request_body = UpdateBomRequest,
    responses((status = 200, body = ApiResponse<BomDetail>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_bom(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateBomRequest>,
) -> AppResult<ApiResponse<BomDetail>> {
    Ok(ApiResponse::new(boms(&state).update(id, req).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/inventory/boms/{id}", tag = "Inventory",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_bom(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    boms(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}
