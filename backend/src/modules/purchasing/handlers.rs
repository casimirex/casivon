use axum::extract::{Path, Query, State};
use axum::Extension;
use uuid::Uuid;

use crate::error::AppResult;
use crate::infrastructure::state::AppState;
use crate::modules::inventory::infrastructure::repositories::{PgProductRepository, PgStockRepository};
use crate::modules::purchasing::application::dto::*;
use crate::modules::purchasing::application::use_cases::*;
use crate::modules::purchasing::domain::entities::{
    GoodsReceipt, PurchaseOrder, PurchaseReturn, Vendor, VendorPayment,
};
use crate::modules::purchasing::domain::repositories::*;
use crate::modules::purchasing::infrastructure::repositories::*;
use crate::shared::auth::CurrentUser;
use crate::shared::pagination::{PaginatedResponse, PaginationParams};
use crate::shared::response::{ApiResponse, Created, DeletedResponse, ErrorResponse};
use crate::shared::validation::ValidatedJson;

fn vendors(state: &AppState) -> VendorUseCases<PgVendorRepository> {
    VendorUseCases::new(PgVendorRepository::new(state.db.clone()), state.fx.clone())
}

fn orders(
    state: &AppState,
) -> PurchaseOrderUseCases<PgPurchaseOrderRepository, PgVendorRepository> {
    PurchaseOrderUseCases::new(
        PgPurchaseOrderRepository::new(state.db.clone()),
        PgVendorRepository::new(state.db.clone()), state.fx.clone())
}

/// Receiving goods touches inventory, so this use case is wired with the stock
/// repository from that module.
fn receipts(
    state: &AppState,
) -> GoodsReceiptUseCases<
    PgGoodsReceiptRepository,
    PgPurchaseOrderRepository,
    PgStockRepository,
    PgProductRepository,
> {
    GoodsReceiptUseCases::new(
        PgGoodsReceiptRepository::new(state.db.clone()),
        PgPurchaseOrderRepository::new(state.db.clone()),
        PgStockRepository::new(state.db.clone()),
        PgProductRepository::new(state.db.clone()),
        state.poster.clone(),
    )
}

fn returns(
    state: &AppState,
) -> PurchaseReturnUseCases<
    PgPurchaseReturnRepository,
    PgPurchaseOrderRepository,
    PgStockRepository,
    PgProductRepository,
    PgVendorPaymentRepository,
> {
    PurchaseReturnUseCases::new(
        PgPurchaseReturnRepository::new(state.db.clone()),
        PgPurchaseOrderRepository::new(state.db.clone()),
        PgStockRepository::new(state.db.clone()),
        PgProductRepository::new(state.db.clone()),
        PgVendorPaymentRepository::new(state.db.clone()),
        state.poster.clone(),
    )
}

fn vendor_payments(
    state: &AppState,
) -> VendorPaymentUseCases<
    PgVendorPaymentRepository,
    PgPurchaseOrderRepository,
    PgPurchaseReturnRepository,
> {
    VendorPaymentUseCases::new(
        PgVendorPaymentRepository::new(state.db.clone()),
        PgPurchaseOrderRepository::new(state.db.clone()),
        PgPurchaseReturnRepository::new(state.db.clone()),
        state.fx.clone(),
        state.poster.clone(),
    )
}

// --------------------------------------------------------- vendor payments

/// Records money going out against a purchase order.
#[utoipa::path(
    post, path = "/api/v1/purchasing/vendor-payments", tag = "Purchasing",
    request_body = RecordVendorPaymentRequest,
    responses((status = 201, body = ApiResponse<VendorPayment>), (status = 422, description = "Validation failed, or the payment exceeds what is outstanding", body = ErrorResponse), (status = 409, description = "The order is still a draft", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn record_vendor_payment(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<RecordVendorPaymentRequest>,
) -> AppResult<Created<VendorPayment>> {
    Ok(Created(vendor_payments(&state).record(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/purchasing/vendor-payments", tag = "Purchasing",
    params(PaginationParams, VendorPaymentFilters),
    responses((status = 200, body = PaginatedResponse<VendorPayment>)),
    security(("bearer" = [])),
)]
pub async fn list_vendor_payments(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<VendorPaymentFilters>,
) -> AppResult<PaginatedResponse<VendorPayment>> {
    let (rows, total) = vendor_payments(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(rows, total, &params))
}

/// Reverses a payment. The ledger keeps both the payment and its reversal.
#[utoipa::path(
    delete, path = "/api/v1/purchasing/vendor-payments/{id}", tag = "Purchasing",
    params(("id" = Uuid, Path, description = "Vendor payment id")),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_vendor_payment(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    vendor_payments(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}

// ------------------------------------------------------------------ vendors

#[utoipa::path(
    post, path = "/api/v1/purchasing/vendors", tag = "Purchasing",
    request_body = CreateVendorRequest,
    responses((status = 201, body = ApiResponse<Vendor>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_vendor(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateVendorRequest>,
) -> AppResult<Created<Vendor>> {
    Ok(Created(vendors(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/purchasing/vendors", tag = "Purchasing",
    params(PaginationParams, VendorFilters),
    responses((status = 200, body = PaginatedResponse<Vendor>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_vendors(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<VendorFilters>,
) -> AppResult<PaginatedResponse<Vendor>> {
    let (data, total) = vendors(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/purchasing/vendors/{id}", tag = "Purchasing",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<Vendor>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_vendor(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<Vendor>> {
    Ok(ApiResponse::new(vendors(&state).get(id).await?))
}

#[utoipa::path(
    put, path = "/api/v1/purchasing/vendors/{id}", tag = "Purchasing",
    params(("id" = Uuid, Path)),
    request_body = UpdateVendorRequest,
    responses((status = 200, body = ApiResponse<Vendor>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_vendor(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateVendorRequest>,
) -> AppResult<ApiResponse<Vendor>> {
    Ok(ApiResponse::new(vendors(&state).update(id, req).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/purchasing/vendors/{id}", tag = "Purchasing",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_vendor(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    vendors(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}

// ---------------------------------------------------------- purchase orders

#[utoipa::path(
    post, path = "/api/v1/purchasing/purchase-orders", tag = "Purchasing",
    request_body = CreatePurchaseOrderRequest,
    responses((status = 201, body = ApiResponse<PurchaseOrderDetail>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_po(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreatePurchaseOrderRequest>,
) -> AppResult<Created<PurchaseOrderDetail>> {
    Ok(Created(orders(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/purchasing/purchase-orders", tag = "Purchasing",
    params(PaginationParams, PurchaseOrderFilters),
    responses((status = 200, body = PaginatedResponse<PurchaseOrder>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_pos(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<PurchaseOrderFilters>,
) -> AppResult<PaginatedResponse<PurchaseOrder>> {
    let (data, total) = orders(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/purchasing/purchase-orders/{id}", tag = "Purchasing",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<PurchaseOrderDetail>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_po(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<PurchaseOrderDetail>> {
    Ok(ApiResponse::new(orders(&state).get(id).await?))
}

#[utoipa::path(
    put, path = "/api/v1/purchasing/purchase-orders/{id}", tag = "Purchasing",
    params(("id" = Uuid, Path)),
    request_body = UpdatePurchaseOrderRequest,
    responses((status = 200, body = ApiResponse<PurchaseOrderDetail>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_po(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdatePurchaseOrderRequest>,
) -> AppResult<ApiResponse<PurchaseOrderDetail>> {
    Ok(ApiResponse::new(orders(&state).update(id, req).await?))
}

#[utoipa::path(
    put, path = "/api/v1/purchasing/purchase-orders/{id}/status", tag = "Purchasing",
    params(("id" = Uuid, Path)),
    request_body = UpdateStatusRequest,
    responses((status = 200, body = ApiResponse<PurchaseOrder>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_po_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateStatusRequest>,
) -> AppResult<ApiResponse<PurchaseOrder>> {
    Ok(ApiResponse::new(orders(&state).set_status(id, &req.status).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/purchasing/purchase-orders/{id}", tag = "Purchasing",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_po(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    orders(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}

// ----------------------------------------------------------- goods receipts

#[utoipa::path(
    post, path = "/api/v1/purchasing/goods-receipts", tag = "Purchasing",
    request_body = CreateGoodsReceiptRequest,
    responses((status = 201, body = ApiResponse<GoodsReceiptDetail>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_receipt(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateGoodsReceiptRequest>,
) -> AppResult<Created<GoodsReceiptDetail>> {
    Ok(Created(receipts(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/purchasing/goods-receipts", tag = "Purchasing",
    params(PaginationParams, GoodsReceiptFilters),
    responses((status = 200, body = PaginatedResponse<GoodsReceipt>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_receipts(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<GoodsReceiptFilters>,
) -> AppResult<PaginatedResponse<GoodsReceipt>> {
    let (data, total) = receipts(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

// ---------------------------------------------------------- purchase returns

#[utoipa::path(
    post, path = "/api/v1/purchasing/purchase-returns", tag = "Purchasing",
    request_body = CreatePurchaseReturnRequest,
    responses((status = 201, body = ApiResponse<PurchaseReturnDetail>), (status = 422, description = "Validation failed, or more requested than is on the shelf", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_return(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreatePurchaseReturnRequest>,
) -> AppResult<Created<PurchaseReturnDetail>> {
    Ok(Created(returns(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/purchasing/purchase-returns", tag = "Purchasing",
    params(PaginationParams, PurchaseReturnFilters),
    responses((status = 200, body = PaginatedResponse<PurchaseReturn>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_returns(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<PurchaseReturnFilters>,
) -> AppResult<PaginatedResponse<PurchaseReturn>> {
    let (data, total) = returns(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/purchasing/purchase-returns/{id}", tag = "Purchasing",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<PurchaseReturnDetail>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_return(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<PurchaseReturnDetail>> {
    Ok(ApiResponse::new(returns(&state).get(id).await?))
}

#[utoipa::path(
    get, path = "/api/v1/purchasing/goods-receipts/{id}", tag = "Purchasing",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<GoodsReceiptDetail>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_receipt(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<GoodsReceiptDetail>> {
    Ok(ApiResponse::new(receipts(&state).get(id).await?))
}
