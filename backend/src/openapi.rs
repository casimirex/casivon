use utoipa::{
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    Modify, OpenApi,
};

use crate::modules;
use crate::shared::pagination::{PaginatedResponse, PaginationMeta};
use crate::shared::response::{ApiResponse, DeletedResponse, ErrorDetail, ErrorResponse};

/// Adds the bearer scheme once, rather than repeating its definition on every
/// operation that references it.
struct BearerAuth;

impl Modify for BearerAuth {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .description(Some(
                            "Access token from `/auth/login`. Lasts 15 minutes; \
                             exchange the refresh token at `/auth/refresh` for a new one.",
                        ))
                        .build(),
                ),
            );
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "ERP API",
        version = "0.1.0",
        description = "\
Every response carries the same envelope: `{ \"success\": true, \"data\": … }` on \
success, `{ \"success\": false, \"error\": { \"code\", \"message\" } }` on failure. \
List endpoints add a `pagination` object alongside `data`.

**Money** is serialised as a *string* (`\"1080.00\"`), never a JSON number — \
numbers are IEEE-754 doubles and would quietly lose cents. Send it back the same \
way. Totals are always computed server-side.

**Sorting** uses `?sort=column` or `?sort=-column` for descending. Each endpoint \
has its own allow-list; anything outside it falls back to that endpoint's \
default rather than erroring, since a column name cannot be a bound parameter.",
    ),
    servers((url = "/", description = "This server")),
    modifiers(&BearerAuth),
    components(schemas(
        ApiResponse<serde_json::Value>,
        PaginatedResponse<serde_json::Value>,
        PaginationMeta,
        DeletedResponse,
        ErrorResponse,
        ErrorDetail,
    )),
    paths(
        // ---- Auth
        modules::auth::handlers::register,
        modules::auth::handlers::login,
        modules::auth::handlers::refresh_token,
        modules::auth::handlers::logout,
        modules::auth::handlers::forgot_password,
        modules::auth::handlers::reset_password,
        modules::auth::handlers::verify_email,
        modules::auth::handlers::resend_verification,
        // ---- Users
        modules::auth::handlers::get_me,
        modules::auth::handlers::update_me,
        modules::auth::handlers::change_my_password,
        modules::auth::handlers::list_users,
        modules::auth::handlers::update_user_role,
        modules::auth::handlers::update_user_status,
        // ---- CRM
        modules::crm::handlers::list_contacts,
        modules::crm::handlers::create_contact,
        modules::crm::handlers::get_contact,
        modules::crm::handlers::update_contact,
        modules::crm::handlers::delete_contact,
        modules::crm::handlers::list_companies,
        modules::crm::handlers::create_company,
        modules::crm::handlers::get_company,
        modules::crm::handlers::update_company,
        modules::crm::handlers::delete_company,
        modules::crm::handlers::list_opportunities,
        modules::crm::handlers::create_opportunity,
        modules::crm::handlers::opportunity_pipeline,
        modules::crm::handlers::get_opportunity,
        modules::crm::handlers::update_opportunity,
        modules::crm::handlers::delete_opportunity,
        modules::crm::handlers::list_activities,
        modules::crm::handlers::create_activity,
        modules::crm::handlers::get_activity,
        modules::crm::handlers::update_activity,
        modules::crm::handlers::delete_activity,
        // ---- Sales
        modules::sales::handlers::create_quote,
        modules::sales::handlers::list_quotes,
        modules::sales::handlers::get_quote,
        modules::sales::handlers::update_quote,
        modules::sales::handlers::delete_quote,
        modules::sales::handlers::update_quote_status,
        modules::sales::handlers::convert_quote_to_order,
        modules::sales::handlers::create_order,
        modules::sales::handlers::list_orders,
        modules::sales::handlers::get_order,
        modules::sales::handlers::update_order,
        modules::sales::handlers::delete_order,
        modules::sales::handlers::update_order_status,
        modules::sales::handlers::convert_order_to_invoice,
        modules::sales::handlers::create_invoice,
        modules::sales::handlers::list_invoices,
        modules::sales::handlers::get_invoice,
        modules::sales::handlers::update_invoice,
        modules::sales::handlers::delete_invoice,
        modules::sales::handlers::update_invoice_status,
        modules::sales::handlers::record_payment,
        modules::sales::handlers::list_payments,
        modules::sales::handlers::delete_payment,
        modules::sales::handlers::create_credit_note,
        modules::sales::handlers::list_credit_notes,
        modules::sales::handlers::get_credit_note,
        // ---- Inventory
        modules::inventory::handlers::list_products,
        modules::inventory::handlers::create_product,
        modules::inventory::handlers::get_product,
        modules::inventory::handlers::update_product,
        modules::inventory::handlers::delete_product,
        modules::inventory::handlers::list_categories,
        modules::inventory::handlers::create_category,
        modules::inventory::handlers::update_category,
        modules::inventory::handlers::delete_category,
        modules::inventory::handlers::list_warehouses,
        modules::inventory::handlers::create_warehouse,
        modules::inventory::handlers::get_warehouse,
        modules::inventory::handlers::update_warehouse,
        modules::inventory::handlers::delete_warehouse,
        modules::inventory::handlers::warehouse_stock,
        modules::inventory::handlers::list_movements,
        modules::inventory::handlers::record_movement,
        modules::inventory::handlers::low_stock,
        modules::inventory::handlers::set_reorder_policy,
        modules::inventory::handlers::stock_valuation,
        modules::inventory::handlers::list_boms,
        modules::inventory::handlers::create_bom,
        modules::inventory::handlers::get_bom,
        modules::inventory::handlers::update_bom,
        modules::inventory::handlers::delete_bom,
        // ---- Purchasing
        modules::purchasing::handlers::list_vendors,
        modules::purchasing::handlers::create_vendor,
        modules::purchasing::handlers::get_vendor,
        modules::purchasing::handlers::update_vendor,
        modules::purchasing::handlers::delete_vendor,
        modules::purchasing::handlers::list_pos,
        modules::purchasing::handlers::create_po,
        modules::purchasing::handlers::get_po,
        modules::purchasing::handlers::update_po,
        modules::purchasing::handlers::delete_po,
        modules::purchasing::handlers::update_po_status,
        modules::purchasing::handlers::list_receipts,
        modules::purchasing::handlers::create_receipt,
        modules::purchasing::handlers::get_receipt,
        // ---- Accounting
        modules::accounting::handlers::list_accounts,
        modules::accounting::handlers::create_account,
        modules::accounting::handlers::account_tree,
        modules::accounting::handlers::recalculate_balances,
        modules::accounting::handlers::get_account,
        modules::accounting::handlers::update_account,
        modules::accounting::handlers::delete_account,
        modules::accounting::handlers::list_ledger_entries,
        modules::accounting::handlers::create_ledger_entry,
        modules::accounting::handlers::get_ledger_entry,
        modules::accounting::handlers::delete_ledger_entry,
        modules::accounting::handlers::list_bank_accounts,
        modules::accounting::handlers::create_bank_account,
        modules::accounting::handlers::get_bank_account,
        modules::accounting::handlers::update_bank_account,
        modules::accounting::handlers::delete_bank_account,
        modules::accounting::handlers::list_tax_rates,
        modules::accounting::handlers::create_tax_rate,
        modules::accounting::handlers::update_tax_rate,
        modules::accounting::handlers::delete_tax_rate,
        modules::accounting::handlers::trial_balance,
        modules::accounting::handlers::profit_and_loss,
        modules::accounting::handlers::balance_sheet,
        // ---- HR
        modules::hr::handlers::list_employees,
        modules::hr::handlers::create_employee,
        modules::hr::handlers::get_employee,
        modules::hr::handlers::update_employee,
        modules::hr::handlers::delete_employee,
        modules::hr::handlers::get_leave_balance,
        modules::hr::handlers::list_leave_requests,
        modules::hr::handlers::create_leave_request,
        modules::hr::handlers::get_leave_request,
        modules::hr::handlers::delete_leave_request,
        modules::hr::handlers::decide_leave_request,
        modules::hr::handlers::list_expense_reports,
        modules::hr::handlers::create_expense_report,
        modules::hr::handlers::get_expense_report,
        modules::hr::handlers::update_expense_report,
        modules::hr::handlers::delete_expense_report,
        modules::hr::handlers::update_expense_status,
        // ---- Projects
        modules::projects::handlers::list_projects,
        modules::projects::handlers::create_project,
        modules::projects::handlers::list_tasks,
        modules::projects::handlers::create_task,
        modules::projects::handlers::get_task,
        modules::projects::handlers::update_task,
        modules::projects::handlers::delete_task,
        modules::projects::handlers::update_task_status,
        modules::projects::handlers::list_time_entries,
        modules::projects::handlers::create_time_entry,
        modules::projects::handlers::get_time_entry,
        modules::projects::handlers::update_time_entry,
        modules::projects::handlers::delete_time_entry,
        modules::projects::handlers::get_project,
        modules::projects::handlers::update_project,
        modules::projects::handlers::delete_project,
        modules::projects::handlers::update_project_status,
        modules::projects::handlers::list_project_tasks,
        modules::projects::handlers::list_project_time_entries,
        modules::purchasing::handlers::create_return,
        modules::purchasing::handlers::list_returns,
        modules::purchasing::handlers::get_return,
        modules::purchasing::handlers::record_vendor_payment,
        modules::purchasing::handlers::list_vendor_payments,
        modules::purchasing::handlers::delete_vendor_payment,
        // ---- Accounting: automatic posting
        modules::accounting::handlers::get_posting_accounts,
        modules::accounting::handlers::update_posting_accounts,
        modules::accounting::handlers::unposted_documents,
        modules::accounting::handlers::post_unposted_documents,
        modules::accounting::handlers::inventory_opening,
        modules::accounting::handlers::post_inventory_opening,
        // ---- Search
        modules::search::handlers::search,
        // ---- Files
        modules::files::handlers::upload,
        modules::files::handlers::download,
        // ---- Settings
        modules::settings::handlers::get_organization,
        modules::settings::handlers::update_organization,
        modules::settings::handlers::available_currencies,
        modules::settings::handlers::list_fx_rates,
        modules::settings::handlers::upsert_fx_rate,
        modules::settings::handlers::delete_fx_rate,
    ),
    tags(
        (name = "Auth", description = "Registration, sign-in and password reset. The only endpoints that need no token."),
        (name = "Users", description = "Your own account, and user administration for admins."),
        (name = "CRM", description = "Contacts, companies, opportunities and the activity log."),
        (name = "Sales", description = "Quotes, orders, invoices and payments. Documents move through a state machine and totals are computed server-side."),
        (name = "Inventory", description = "Products, warehouses, stock levels and bills of materials. Levels only ever change through a movement."),
        (name = "Purchasing", description = "Vendors, purchase orders and goods receipts. Receiving posts stock and advances the order in one transaction."),
        (name = "Accounting", description = "Chart of accounts, double-entry ledger, tax rates and reports. Restricted to accountants and managers."),
        (name = "HR", description = "Employees, leave and expense reports. Approvals are restricted to HR and managers."),
        (name = "Projects", description = "Projects, tasks and time entries. Progress and hours are derived, never set directly."),
        (name = "Search", description = "One query across every module the caller is allowed to see. A salesperson and an accountant get different answers to the same term."),
        (name = "Files", description = "Uploaded receipts and documents. Reads return a short-lived presigned link rather than the bytes, so a browser fetches the file straight from object storage. A file is readable by exactly whoever may read the record it is attached to."),
        (name = "Settings", description = "Company profile shown on outgoing documents, and the exchange rates every foreign-currency amount is restated with."),
    ),
)]
pub struct ApiDoc;
