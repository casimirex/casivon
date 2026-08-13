use axum::{
    routing::{get, post},
    Router,
};

use crate::infrastructure::state::AppState;
use crate::modules::accounting::handlers;

pub fn accounting_routes() -> Router<AppState> {
    Router::new()
        .route("/accounts", get(handlers::list_accounts).post(handlers::create_account))
        .route("/accounts/tree", get(handlers::account_tree))
        .route("/accounts/recalculate", post(handlers::recalculate_balances))
        .route(
            "/accounts/:id",
            get(handlers::get_account)
                .put(handlers::update_account)
                .delete(handlers::delete_account),
        )
        .route(
            "/ledger-entries",
            get(handlers::list_ledger_entries).post(handlers::create_ledger_entry),
        )
        .route(
            "/ledger-entries/:id",
            get(handlers::get_ledger_entry).delete(handlers::delete_ledger_entry),
        )
        .route(
            "/bank-accounts",
            get(handlers::list_bank_accounts).post(handlers::create_bank_account),
        )
        .route(
            "/bank-accounts/:id",
            get(handlers::get_bank_account)
                .put(handlers::update_bank_account)
                .delete(handlers::delete_bank_account),
        )
        .route("/tax-rates", get(handlers::list_tax_rates).post(handlers::create_tax_rate))
        .route(
            "/tax-rates/:id",
            axum::routing::put(handlers::update_tax_rate).delete(handlers::delete_tax_rate),
        )
        .route(
            "/posting-accounts",
            get(handlers::get_posting_accounts).put(handlers::update_posting_accounts),
        )
        .route("/unposted", get(handlers::unposted_documents))
        .route("/post-unposted", post(handlers::post_unposted_documents))
        .route(
            "/inventory-opening",
            get(handlers::inventory_opening).post(handlers::post_inventory_opening),
        )
        .route("/reports/trial-balance", get(handlers::trial_balance))
        .route("/reports/profit-and-loss", get(handlers::profit_and_loss))
        .route("/reports/balance-sheet", get(handlers::balance_sheet))
}
