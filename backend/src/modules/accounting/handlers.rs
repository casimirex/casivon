use axum::extract::{Path, Query, State};
use axum::Extension;
use uuid::Uuid;

use crate::error::AppResult;
use crate::infrastructure::state::AppState;
use crate::modules::accounting::application::dto::*;
use crate::modules::accounting::application::use_cases::*;
use crate::modules::accounting::domain::entities::{
    Account, BankAccount, GeneralLedgerEntry, TaxRate,
};
use crate::modules::accounting::domain::repositories::*;
use crate::modules::accounting::infrastructure::repositories::*;
use crate::shared::auth::CurrentUser;
use crate::shared::pagination::{PaginatedResponse, PaginationParams};
use crate::shared::response::{ApiResponse, Created, DeletedResponse, ErrorResponse};
use crate::shared::validation::ValidatedJson;

/// Accounting is finance-only: everything here is gated behind the accountant
/// role (admins pass automatically).
const ACCOUNTING_ROLES: [&str; 2] = ["accountant", "manager"];

fn accounts(state: &AppState) -> AccountUseCases<PgAccountRepository> {
    AccountUseCases::new(PgAccountRepository::new(state.db.clone()), state.fx.clone())
}

fn ledger(state: &AppState) -> LedgerUseCases<PgLedgerRepository, PgAccountRepository> {
    LedgerUseCases::new(
        PgLedgerRepository::new(state.db.clone()),
        PgAccountRepository::new(state.db.clone()), state.fx.clone())
}

fn bank_accounts(
    state: &AppState,
) -> BankAccountUseCases<PgBankAccountRepository, PgAccountRepository> {
    BankAccountUseCases::new(
        PgBankAccountRepository::new(state.db.clone()),
        PgAccountRepository::new(state.db.clone()),
    )
}

fn tax_rates(state: &AppState) -> TaxRateUseCases<PgTaxRateRepository> {
    TaxRateUseCases::new(PgTaxRateRepository::new(state.db.clone()))
}

fn posting(state: &AppState) -> PostingUseCases<PgPostingRepository, PgAccountRepository> {
    PostingUseCases::new(
        PgPostingRepository::new(state.db.clone()),
        PgAccountRepository::new(state.db.clone()),
        state.fx.clone(),
        state.poster.clone(),
    )
}

/// Which account each automatic posting uses, and whether posting is on.
///
/// Readable by anyone with accounting access: a journal line that appeared on
/// its own should be explainable by whoever finds it in the ledger.
#[utoipa::path(
    get, path = "/api/v1/accounting/posting-accounts", tag = "Accounting",
    responses((status = 200, body = ApiResponse<PostingConfiguration>), (status = 403, description = "Accounting access required", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_posting_accounts(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<ApiResponse<PostingConfiguration>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;
    Ok(ApiResponse::new(posting(&state).get().await?))
}

/// Admin-only. This decides where every future invoice books its revenue.
#[utoipa::path(
    put, path = "/api/v1/accounting/posting-accounts", tag = "Accounting",
    request_body = UpdatePostingAccountsRequest,
    responses((status = 200, body = ApiResponse<PostingConfiguration>), (status = 422, description = "An account is the wrong type, inactive, or not in the base currency", body = ErrorResponse), (status = 403, description = "Admin only", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_posting_accounts(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<UpdatePostingAccountsRequest>,
) -> AppResult<ApiResponse<PostingConfiguration>> {
    if !user.is_admin() {
        return Err(crate::error::AppError::Forbidden("Only an administrator can change how documents post to the ledger".into()));
    }

    Ok(ApiResponse::new(posting(&state).update(req).await?))
}

/// What the ledger is owed.
///
/// Empty is the healthy state. Documents appear here either because they predate
/// automatic posting, or because a posting did not complete — the gap this
/// design accepts in exchange for not threading a transaction through every
/// repository in the codebase.
#[utoipa::path(
    get, path = "/api/v1/accounting/unposted", tag = "Accounting",
    responses((status = 200, body = ApiResponse<UnpostedReport>), (status = 403, description = "Accounting access required", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn unposted_documents(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<ApiResponse<UnpostedReport>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;
    Ok(ApiResponse::new(posting(&state).unposted().await?))
}

/// Posts everything outstanding. Admin-only, and safe to run twice.
#[utoipa::path(
    post, path = "/api/v1/accounting/post-unposted", tag = "Accounting",
    responses((status = 200, body = ApiResponse<PostingRunReport>), (status = 422, description = "Posting is not configured", body = ErrorResponse), (status = 403, description = "Admin only", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn post_unposted_documents(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<ApiResponse<PostingRunReport>> {
    if !user.is_admin() {
        return Err(crate::error::AppError::Forbidden("Only an administrator can change how documents post to the ledger".into()));
    }

    Ok(ApiResponse::new(posting(&state).post_unposted().await?))
}

/// What switching to perpetual costing would put on the balance sheet.
///
/// Stock already on the shelves was expensed when it arrived, so selling it
/// under perpetual costing would credit an Inventory account that was never
/// debited. This is the preview of the entry that squares that.
#[utoipa::path(
    get, path = "/api/v1/accounting/inventory-opening", tag = "Accounting",
    responses((status = 200, body = ApiResponse<InventoryOpeningReport>), (status = 403, description = "Accounting access required", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn inventory_opening(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<ApiResponse<InventoryOpeningReport>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;
    Ok(ApiResponse::new(posting(&state).inventory_opening().await?))
}

/// Posts the opening entry. Admin-only, and writes nothing the second time.
#[utoipa::path(
    post, path = "/api/v1/accounting/inventory-opening", tag = "Accounting",
    responses((status = 200, body = ApiResponse<InventoryOpeningReport>), (status = 422, description = "Perpetual inventory is not configured", body = ErrorResponse), (status = 403, description = "Admin only", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn post_inventory_opening(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<ApiResponse<InventoryOpeningReport>> {
    if !user.is_admin() {
        return Err(crate::error::AppError::Forbidden(
            "Only an administrator can change how documents post to the ledger".into(),
        ));
    }

    Ok(ApiResponse::new(posting(&state).post_inventory_opening(&user).await?))
}

// ----------------------------------------------------------------- accounts

#[utoipa::path(
    post, path = "/api/v1/accounting/accounts", tag = "Accounting",
    request_body = CreateAccountRequest,
    responses((status = 201, body = ApiResponse<Account>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_account(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateAccountRequest>,
) -> AppResult<Created<Account>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;
    Ok(Created(accounts(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/accounting/accounts", tag = "Accounting",
    params(PaginationParams, AccountFilters),
    responses((status = 200, body = PaginatedResponse<Account>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_accounts(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<AccountFilters>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<PaginatedResponse<Account>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;

    let (data, total) = accounts(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/accounting/accounts/tree", tag = "Accounting",
    responses((status = 200, body = ApiResponse<Vec<AccountNode>>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn account_tree(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<ApiResponse<Vec<AccountNode>>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;

    Ok(ApiResponse::new(accounts(&state).tree().await?))
}

#[utoipa::path(
    get, path = "/api/v1/accounting/accounts/{id}", tag = "Accounting",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<Account>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_account(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<ApiResponse<Account>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;

    Ok(ApiResponse::new(accounts(&state).get(id).await?))
}

#[utoipa::path(
    put, path = "/api/v1/accounting/accounts/{id}", tag = "Accounting",
    params(("id" = Uuid, Path)),
    request_body = UpdateAccountRequest,
    responses((status = 200, body = ApiResponse<Account>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_account(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateAccountRequest>,
) -> AppResult<ApiResponse<Account>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;
    Ok(ApiResponse::new(accounts(&state).update(id, req).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/accounting/accounts/{id}", tag = "Accounting",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_account(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;
    accounts(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}

#[utoipa::path(
    post, path = "/api/v1/accounting/accounts/recalculate", tag = "Accounting",
    responses((status = 200, body = ApiResponse<serde_json::Value>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn recalculate_balances(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<ApiResponse<serde_json::Value>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;
    let updated = accounts(&state).recalculate_balances().await?;
    Ok(ApiResponse::new(serde_json::json!({ "accounts_updated": updated })))
}

// ------------------------------------------------------------------- ledger

#[utoipa::path(
    post, path = "/api/v1/accounting/ledger-entries", tag = "Accounting",
    request_body = CreateLedgerEntryRequest,
    responses((status = 201, body = ApiResponse<GeneralLedgerEntry>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_ledger_entry(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateLedgerEntryRequest>,
) -> AppResult<Created<GeneralLedgerEntry>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;
    Ok(Created(ledger(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/accounting/ledger-entries", tag = "Accounting",
    params(PaginationParams, LedgerFilters),
    responses((status = 200, body = PaginatedResponse<GeneralLedgerEntry>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_ledger_entries(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<LedgerFilters>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<PaginatedResponse<GeneralLedgerEntry>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;

    let (data, total) = ledger(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/accounting/ledger-entries/{id}", tag = "Accounting",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<GeneralLedgerEntry>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_ledger_entry(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<ApiResponse<GeneralLedgerEntry>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;

    Ok(ApiResponse::new(ledger(&state).get(id).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/accounting/ledger-entries/{id}", tag = "Accounting",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_ledger_entry(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;
    ledger(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}

// ------------------------------------------------------------ bank accounts

#[utoipa::path(
    post, path = "/api/v1/accounting/bank-accounts", tag = "Accounting",
    request_body = CreateBankAccountRequest,
    responses((status = 201, body = ApiResponse<BankAccount>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_bank_account(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateBankAccountRequest>,
) -> AppResult<Created<BankAccount>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;
    Ok(Created(bank_accounts(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/accounting/bank-accounts", tag = "Accounting",
    params(PaginationParams),
    responses((status = 200, body = PaginatedResponse<BankAccount>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_bank_accounts(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<PaginatedResponse<BankAccount>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;

    let (data, total) = bank_accounts(&state).list(&params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/accounting/bank-accounts/{id}", tag = "Accounting",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<BankAccount>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_bank_account(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<ApiResponse<BankAccount>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;

    Ok(ApiResponse::new(bank_accounts(&state).get(id).await?))
}

#[utoipa::path(
    put, path = "/api/v1/accounting/bank-accounts/{id}", tag = "Accounting",
    params(("id" = Uuid, Path)),
    request_body = UpdateBankAccountRequest,
    responses((status = 200, body = ApiResponse<BankAccount>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_bank_account(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateBankAccountRequest>,
) -> AppResult<ApiResponse<BankAccount>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;
    Ok(ApiResponse::new(bank_accounts(&state).update(id, req).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/accounting/bank-accounts/{id}", tag = "Accounting",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_bank_account(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;
    bank_accounts(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}

// --------------------------------------------------------------- tax rates

#[utoipa::path(
    post, path = "/api/v1/accounting/tax-rates", tag = "Accounting",
    request_body = CreateTaxRateRequest,
    responses((status = 201, body = ApiResponse<TaxRate>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_tax_rate(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateTaxRateRequest>,
) -> AppResult<Created<TaxRate>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;
    Ok(Created(tax_rates(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/accounting/tax-rates", tag = "Accounting",
    params(PaginationParams),
    responses((status = 200, body = PaginatedResponse<TaxRate>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_tax_rates(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<PaginatedResponse<TaxRate>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;

    let (data, total) = tax_rates(&state).list(&params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    put, path = "/api/v1/accounting/tax-rates/{id}", tag = "Accounting",
    params(("id" = Uuid, Path)),
    request_body = UpdateTaxRateRequest,
    responses((status = 200, body = ApiResponse<TaxRate>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_tax_rate(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateTaxRateRequest>,
) -> AppResult<ApiResponse<TaxRate>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;
    Ok(ApiResponse::new(tax_rates(&state).update(id, req).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/accounting/tax-rates/{id}", tag = "Accounting",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_tax_rate(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;
    tax_rates(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}

// ----------------------------------------------------------------- reports

#[utoipa::path(
    get, path = "/api/v1/accounting/reports/trial-balance", tag = "Accounting",
    params(ReportPeriodQuery),
    responses((status = 200, body = ApiResponse<TrialBalanceReport>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn trial_balance(
    State(state): State<AppState>,
    Query(query): Query<ReportPeriodQuery>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<ApiResponse<TrialBalanceReport>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;

    Ok(ApiResponse::new(ledger(&state).trial_balance(&query).await?))
}

#[utoipa::path(
    get, path = "/api/v1/accounting/reports/profit-and-loss", tag = "Accounting",
    params(ReportPeriodQuery),
    responses((status = 200, body = ApiResponse<ProfitAndLossReport>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn profit_and_loss(
    State(state): State<AppState>,
    Query(query): Query<ReportPeriodQuery>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<ApiResponse<ProfitAndLossReport>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;

    Ok(ApiResponse::new(ledger(&state).profit_and_loss(&query).await?))
}

#[utoipa::path(
    get, path = "/api/v1/accounting/reports/balance-sheet", tag = "Accounting",
    params(ReportPeriodQuery),
    responses((status = 200, body = ApiResponse<BalanceSheetReport>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn balance_sheet(
    State(state): State<AppState>,
    Query(query): Query<ReportPeriodQuery>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<ApiResponse<BalanceSheetReport>> {
    user.require_any_role(&ACCOUNTING_ROLES)?;

    Ok(ApiResponse::new(ledger(&state).balance_sheet(&query).await?))
}
