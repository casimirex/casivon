use axum::extract::{Path, Query, State};
use axum::Extension;
use uuid::Uuid;

use crate::error::AppResult;
use crate::infrastructure::state::AppState;
use crate::modules::sales::application::dto::*;
use crate::modules::inventory::infrastructure::repositories::PgStockRepository;
use crate::modules::sales::application::use_cases::{
    CreditNoteUseCases, InvoiceUseCases, OrderUseCases, QuoteUseCases,
};
use crate::modules::sales::domain::entities::{CreditNote, Invoice, Payment, Quote, SalesOrder};
use crate::modules::sales::domain::repositories::{
    CreditNoteFilters, PaymentFilters, SalesDocumentFilters,
};
use crate::modules::sales::infrastructure::repositories::*;
use crate::shared::auth::CurrentUser;
use crate::shared::pagination::{PaginatedResponse, PaginationParams};
use crate::shared::response::{ApiResponse, Created, DeletedResponse, ErrorResponse};
use crate::shared::validation::ValidatedJson;

// Repository wiring lives here so the use cases stay free of SQLx types.
fn quote_use_cases(
    state: &AppState,
) -> QuoteUseCases<PgQuoteRepository, PgSalesOrderRepository> {
    QuoteUseCases::new(
        PgQuoteRepository::new(state.db.clone()),
        PgSalesOrderRepository::new(state.db.clone()), state.fx.clone())
}

fn order_use_cases(state: &AppState) -> OrderUseCases<PgSalesOrderRepository, PgInvoiceRepository> {
    OrderUseCases::new(
        PgSalesOrderRepository::new(state.db.clone()),
        PgInvoiceRepository::new(state.db.clone()),
        state.dispatch.clone(),
        state.fx.clone(),
    )
}

fn invoice_use_cases(
    state: &AppState,
) -> InvoiceUseCases<
    PgInvoiceRepository,
    PgPaymentRepository,
    PgCreditNoteRepository,
    PgSalesOrderRepository,
> {
    InvoiceUseCases::new(
        PgInvoiceRepository::new(state.db.clone()),
        PgPaymentRepository::new(state.db.clone()),
        PgCreditNoteRepository::new(state.db.clone()),
        // Cancelling an invoice asks its order whether it still stands, and
        // gives back the hold that issuing released.
        PgSalesOrderRepository::new(state.db.clone()),
        state.dispatch.clone(),
        state.fx.clone(),
        state.poster.clone(),
    )
}

/// Crediting touches inventory when goods come back, so this use case is wired
/// with the stock repository from that module.
fn credit_notes(
    state: &AppState,
) -> CreditNoteUseCases<
    PgCreditNoteRepository,
    PgInvoiceRepository,
    PgStockRepository,
    PgPaymentRepository,
> {
    CreditNoteUseCases::new(
        PgCreditNoteRepository::new(state.db.clone()),
        PgInvoiceRepository::new(state.db.clone()),
        PgStockRepository::new(state.db.clone()),
        PgPaymentRepository::new(state.db.clone()),
        state.poster.clone(),
    )
}

// ------------------------------------------------------------------- quotes

#[utoipa::path(
    post, path = "/api/v1/sales/quotes", tag = "Sales",
    request_body = CreateQuoteRequest,
    responses((status = 201, body = ApiResponse<QuoteDetail>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_quote(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateQuoteRequest>,
) -> AppResult<Created<QuoteDetail>> {
    Ok(Created(quote_use_cases(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/sales/quotes", tag = "Sales",
    params(PaginationParams, SalesDocumentFilters),
    responses((status = 200, body = PaginatedResponse<Quote>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_quotes(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<SalesDocumentFilters>,
) -> AppResult<PaginatedResponse<Quote>> {
    let (data, total) = quote_use_cases(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/sales/quotes/{id}", tag = "Sales",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<QuoteDetail>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_quote(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<QuoteDetail>> {
    Ok(ApiResponse::new(quote_use_cases(&state).get(id).await?))
}

#[utoipa::path(
    put, path = "/api/v1/sales/quotes/{id}", tag = "Sales",
    params(("id" = Uuid, Path)),
    request_body = UpdateQuoteRequest,
    responses((status = 200, body = ApiResponse<QuoteDetail>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_quote(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateQuoteRequest>,
) -> AppResult<ApiResponse<QuoteDetail>> {
    Ok(ApiResponse::new(quote_use_cases(&state).update(id, req).await?))
}

#[utoipa::path(
    put, path = "/api/v1/sales/quotes/{id}/status", tag = "Sales",
    params(("id" = Uuid, Path)),
    request_body = UpdateStatusRequest,
    responses((status = 200, body = ApiResponse<Quote>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_quote_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateStatusRequest>,
) -> AppResult<ApiResponse<Quote>> {
    Ok(ApiResponse::new(quote_use_cases(&state).set_status(id, &req.status).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/sales/quotes/{id}", tag = "Sales",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_quote(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    quote_use_cases(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}

#[utoipa::path(
    post, path = "/api/v1/sales/quotes/{id}/convert-to-order", tag = "Sales",
    params(("id" = Uuid, Path)),
    request_body = ConvertQuoteRequest,
    responses((status = 201, body = ApiResponse<OrderDetail>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn convert_quote_to_order(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<ConvertQuoteRequest>,
) -> AppResult<Created<OrderDetail>> {
    Ok(Created(quote_use_cases(&state).convert_to_order(id, req, &user).await?))
}

// ------------------------------------------------------------------- orders

#[utoipa::path(
    post, path = "/api/v1/sales/orders", tag = "Sales",
    request_body = CreateOrderRequest,
    responses((status = 201, body = ApiResponse<OrderDetail>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_order(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateOrderRequest>,
) -> AppResult<Created<OrderDetail>> {
    Ok(Created(order_use_cases(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/sales/orders", tag = "Sales",
    params(PaginationParams, SalesDocumentFilters),
    responses((status = 200, body = PaginatedResponse<SalesOrder>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_orders(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<SalesDocumentFilters>,
) -> AppResult<PaginatedResponse<SalesOrder>> {
    let (data, total) = order_use_cases(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/sales/orders/{id}", tag = "Sales",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<OrderDetail>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_order(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<OrderDetail>> {
    Ok(ApiResponse::new(order_use_cases(&state).get(id).await?))
}

#[utoipa::path(
    put, path = "/api/v1/sales/orders/{id}", tag = "Sales",
    params(("id" = Uuid, Path)),
    request_body = UpdateOrderRequest,
    responses((status = 200, body = ApiResponse<OrderDetail>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_order(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateOrderRequest>,
) -> AppResult<ApiResponse<OrderDetail>> {
    Ok(ApiResponse::new(order_use_cases(&state).update(id, req).await?))
}

#[utoipa::path(
    put, path = "/api/v1/sales/orders/{id}/status", tag = "Sales",
    params(("id" = Uuid, Path)),
    request_body = UpdateStatusRequest,
    responses((status = 200, body = ApiResponse<SalesOrder>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_order_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateStatusRequest>,
) -> AppResult<ApiResponse<SalesOrder>> {
    Ok(ApiResponse::new(order_use_cases(&state).set_status(id, &req.status).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/sales/orders/{id}", tag = "Sales",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_order(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    order_use_cases(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}

#[utoipa::path(
    post, path = "/api/v1/sales/orders/{id}/convert-to-invoice", tag = "Sales",
    params(("id" = Uuid, Path)),
    request_body = ConvertOrderRequest,
    responses((status = 201, body = ApiResponse<InvoiceDetail>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn convert_order_to_invoice(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<ConvertOrderRequest>,
) -> AppResult<Created<InvoiceDetail>> {
    Ok(Created(order_use_cases(&state).convert_to_invoice(id, req, &user).await?))
}

// ----------------------------------------------------------------- invoices

#[utoipa::path(
    post, path = "/api/v1/sales/invoices", tag = "Sales",
    request_body = CreateInvoiceRequest,
    responses((status = 201, body = ApiResponse<InvoiceDetail>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_invoice(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateInvoiceRequest>,
) -> AppResult<Created<InvoiceDetail>> {
    Ok(Created(invoice_use_cases(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/sales/invoices", tag = "Sales",
    params(PaginationParams, SalesDocumentFilters),
    responses((status = 200, body = PaginatedResponse<Invoice>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_invoices(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<SalesDocumentFilters>,
) -> AppResult<PaginatedResponse<Invoice>> {
    let (data, total) = invoice_use_cases(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/sales/invoices/{id}", tag = "Sales",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<InvoiceDetail>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_invoice(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<InvoiceDetail>> {
    Ok(ApiResponse::new(invoice_use_cases(&state).get(id).await?))
}

#[utoipa::path(
    put, path = "/api/v1/sales/invoices/{id}", tag = "Sales",
    params(("id" = Uuid, Path)),
    request_body = UpdateInvoiceRequest,
    responses((status = 200, body = ApiResponse<InvoiceDetail>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_invoice(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateInvoiceRequest>,
) -> AppResult<ApiResponse<InvoiceDetail>> {
    Ok(ApiResponse::new(invoice_use_cases(&state).update(id, req).await?))
}

#[utoipa::path(
    put, path = "/api/v1/sales/invoices/{id}/status", tag = "Sales",
    params(("id" = Uuid, Path)),
    request_body = UpdateStatusRequest,
    responses((status = 200, body = ApiResponse<Invoice>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_invoice_status(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateStatusRequest>,
) -> AppResult<ApiResponse<Invoice>> {
    Ok(ApiResponse::new(
        invoice_use_cases(&state).set_status(id, &req.status, &user).await?,
    ))
}

#[utoipa::path(
    delete, path = "/api/v1/sales/invoices/{id}", tag = "Sales",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_invoice(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    invoice_use_cases(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}

// ----------------------------------------------------------------- payments

#[utoipa::path(
    post, path = "/api/v1/sales/payments", tag = "Sales",
    request_body = RecordPaymentRequest,
    responses((status = 201, body = ApiResponse<Payment>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn record_payment(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<RecordPaymentRequest>,
) -> AppResult<Created<Payment>> {
    Ok(Created(invoice_use_cases(&state).record_payment(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/sales/payments", tag = "Sales",
    params(PaginationParams, PaymentFilters),
    responses((status = 200, body = PaginatedResponse<Payment>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_payments(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<PaymentFilters>,
) -> AppResult<PaginatedResponse<Payment>> {
    let (data, total) = invoice_use_cases(&state).list_payments(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    delete, path = "/api/v1/sales/payments/{id}", tag = "Sales",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_payment(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    invoice_use_cases(&state).delete_payment(id).await?;
    Ok(DeletedResponse::ok())
}

// ------------------------------------------------------------- credit notes

/// Credits a customer against an invoice.
///
/// Works on a **paid** invoice, which is the case that had no answer before:
/// `paid` has no outgoing status transition, so the invoice could be neither
/// cancelled nor adjusted. This does not touch the status machine — it
/// recomputes settlement, and what is now owed back shows as a negative
/// `amount_due`.
#[utoipa::path(
    post, path = "/api/v1/sales/credit-notes", tag = "Sales",
    request_body = CreateCreditNoteRequest,
    responses((status = 201, body = ApiResponse<CreditNoteDetail>), (status = 422, description = "Validation failed, or more credited than was invoiced", body = ErrorResponse), (status = 404, description = "Invoice not found", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_credit_note(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateCreditNoteRequest>,
) -> AppResult<Created<CreditNoteDetail>> {
    Ok(Created(credit_notes(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/sales/credit-notes", tag = "Sales",
    params(PaginationParams, CreditNoteFilters),
    responses((status = 200, body = PaginatedResponse<CreditNote>)),
    security(("bearer" = [])),
)]
pub async fn list_credit_notes(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<CreditNoteFilters>,
) -> AppResult<PaginatedResponse<CreditNote>> {
    let (rows, total) = credit_notes(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(rows, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/sales/credit-notes/{id}", tag = "Sales",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<CreditNoteDetail>), (status = 404, description = "Not found", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_credit_note(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<CreditNoteDetail>> {
    Ok(ApiResponse::new(credit_notes(&state).get(id).await?))
}
