use std::sync::Arc;

use chrono::{Duration, NaiveDate, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::modules::sales::application::dto::*;
use crate::modules::sales::domain::entities::*;
use crate::modules::sales::domain::errors::SalesError;
use crate::modules::sales::domain::repositories::*;
use crate::shared::auth::CurrentUser;
use crate::shared::currency::{CurrencyResolver, DocumentCurrency};
use crate::shared::money::{
    calculate_line, round_money, sum_totals, to_base, DocumentTotals, LineAmounts,
};
use crate::shared::posting::{
    DocumentPoster, PostableCreditNote, PostableInvoice, PostablePayment,
};
use crate::modules::inventory::domain::costing::extended_cost;
use crate::modules::inventory::domain::entities::{MovementType, StockMovement};
use crate::modules::inventory::domain::repositories::StockRepository;
use crate::shared::dispatch::{
    DispatchableInvoice, DispatchableLine, DispatchableOrder, ReservableLine, StockDispatcher,
};
use crate::shared::pagination::PaginationParams;

const DEFAULT_PAYMENT_TERMS_DAYS: i64 = 30;

/// Costs each requested line and rolls the results into document totals.
fn price_lines(lines: &[DocumentLineRequest]) -> (Vec<LineAmounts>, DocumentTotals) {
    let amounts: Vec<LineAmounts> = lines
        .iter()
        .map(|l| calculate_line(l.quantity, l.unit_price, l.discount_percent, l.tax_rate))
        .collect();
    let totals = sum_totals(amounts.iter().copied());
    (amounts, totals)
}

// ------------------------------------------------------------------- quotes

pub struct QuoteUseCases<Q: QuoteRepository, O: SalesOrderRepository> {
    quotes: Q,
    orders: O,    fx: Arc<dyn CurrencyResolver>,
}

impl<Q: QuoteRepository, O: SalesOrderRepository> QuoteUseCases<Q, O> {
    pub fn new(quotes: Q, orders: O, fx: Arc<dyn CurrencyResolver>) -> Self {
        Self { quotes, orders, fx }
    }

    /// The currency a document is raised in, together with the rate frozen onto
    /// it. Read at the point of use rather than cached, so a change under
    /// Settings applies to the next document raised.
    ///
    /// `on` is the document's own date: the rate that applied when it was
    /// raised is the rate it keeps.
    async fn currency(
        &self,
        requested: Option<String>,
        on: NaiveDate,
    ) -> AppResult<DocumentCurrency> {
        self.fx.resolve(requested.as_deref(), on).await
    }

    pub async fn create(&self, req: CreateQuoteRequest, user: &CurrentUser) -> AppResult<QuoteDetail> {
        if req.expiry_date < req.issue_date {
            return Err(SalesError::ExpiryBeforeIssue.into());
        }

        let quote_id = Uuid::new_v4();
        let (amounts, totals) = price_lines(&req.lines);
        let now = Utc::now();
        let currency = self.currency(req.currency.clone(), req.issue_date).await?;

        let quote = Quote {
            id: quote_id,
            org_id: user.org_id,
            quote_number: self.quotes.next_number().await?,
            customer_id: req.customer_id,
            contact_id: req.contact_id,
            status: QuoteStatus::DRAFT.to_string(),
            issue_date: req.issue_date,
            expiry_date: req.expiry_date,
            subtotal: Some(totals.subtotal),
            tax_amount: Some(totals.tax_amount),
            total: Some(totals.total),
            base_total: Some(currency.to_base(totals.total)),
            fx_rate: currency.fx_rate,
            currency: currency.code,
            notes: req.notes,
            terms: req.terms,
            created_by: user.id,
            created_at: now,
            updated_at: now,
        };

        let lines = build_quote_lines(quote_id, &req.lines, &amounts);
        let quote = self.quotes.create(&quote, &lines).await?;
        let lines = self.quotes.find_lines(quote.id).await?;

        Ok(QuoteDetail { quote, lines })
    }

    pub async fn get(&self, id: Uuid) -> AppResult<QuoteDetail> {
        let quote = self.require_quote(id).await?;
        let lines = self.quotes.find_lines(id).await?;
        Ok(QuoteDetail { quote, lines })
    }

    pub async fn list(
        &self,
        filters: &SalesDocumentFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Quote>, i64)> {
        self.quotes.list(filters, params).await
    }

    pub async fn update(&self, id: Uuid, req: UpdateQuoteRequest) -> AppResult<QuoteDetail> {
        let mut quote = self.require_quote(id).await?;

        if !QuoteStatus::is_editable(&quote.status) {
            return Err(SalesError::NotEditable { document: "quote", status: quote.status }.into());
        }

        if let Some(customer_id) = req.customer_id {
            quote.customer_id = customer_id;
        }
        if req.contact_id.is_some() {
            quote.contact_id = req.contact_id;
        }
        if let Some(issue_date) = req.issue_date {
            quote.issue_date = issue_date;
        }
        if let Some(expiry_date) = req.expiry_date {
            quote.expiry_date = expiry_date;
        }
        if req.notes.is_some() {
            quote.notes = req.notes;
        }
        if req.terms.is_some() {
            quote.terms = req.terms;
        }
        if quote.expiry_date < quote.issue_date {
            return Err(SalesError::ExpiryBeforeIssue.into());
        }
        quote.updated_at = Utc::now();

        let new_lines = match &req.lines {
            Some(requested) => {
                if requested.is_empty() {
                    return Err(SalesError::NoLines { document: "quote" }.into());
                }
                let (amounts, totals) = price_lines(requested);
                quote.subtotal = Some(totals.subtotal);
                quote.tax_amount = Some(totals.tax_amount);
                quote.total = Some(totals.total);
                Some(build_quote_lines(quote.id, requested, &amounts))
            }
            None => None,
        };

        // Re-resolved rather than left alone: a draft's issue date can move, and
        // the rate a document is restated at is the one in force on its own
        // date. Recomputed even when the lines did not change, because moving
        // the date alone is enough to change what the total is worth.
        let currency = self.currency(Some(quote.currency.clone()), quote.issue_date).await?;
        quote.fx_rate = currency.fx_rate;
        quote.base_total = currency.to_base_opt(quote.total);

        let quote = self.quotes.update(&quote, new_lines.as_deref()).await?;
        let lines = self.quotes.find_lines(quote.id).await?;
        Ok(QuoteDetail { quote, lines })
    }

    pub async fn set_status(&self, id: Uuid, status: &str) -> AppResult<Quote> {
        let quote = self.require_quote(id).await?;

        if !QuoteStatus::ALL.contains(&status) {
            return Err(AppError::Validation(format!(
                "Unknown quote status '{}'. Expected one of: {}",
                status,
                QuoteStatus::ALL.join(", ")
            )));
        }

        if !QuoteStatus::can_transition(&quote.status, status) {
            return Err(SalesError::InvalidTransition {
                document: "quote",
                from: quote.status,
                to: status.to_string(),
            }
            .into());
        }

        self.quotes.update_status(id, status).await
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        let quote = self.require_quote(id).await?;

        // Anything past draft is a document a customer has seen; keep the trail.
        if !QuoteStatus::is_editable(&quote.status) {
            return Err(SalesError::NotEditable { document: "quote", status: quote.status }.into());
        }

        self.quotes.delete(id).await
    }

    /// Quote -> SalesOrder. Copies the priced lines across unchanged.
    pub async fn convert_to_order(
        &self,
        id: Uuid,
        req: ConvertQuoteRequest,
        user: &CurrentUser,
    ) -> AppResult<OrderDetail> {
        let quote = self.require_quote(id).await?;

        if quote.status != QuoteStatus::ACCEPTED {
            return Err(SalesError::QuoteNotAccepted(quote.quote_number).into());
        }

        if let Some(existing) = self.quotes.find_converted_order(id).await? {
            return Err(
                SalesError::QuoteAlreadyConverted(quote.quote_number, existing.order_number).into()
            );
        }

        let quote_lines = self.quotes.find_lines(id).await?;
        if quote_lines.is_empty() {
            return Err(SalesError::NoLines { document: "quote" }.into());
        }

        let order_id = Uuid::new_v4();
        let now = Utc::now();
        let order_date = req.order_date.unwrap_or_else(|| Utc::now().date_naive());

        // The order inherits the quote's currency — the customer agreed a price
        // in it — but resolves its own rate at its own date. The amount the
        // customer owes is unchanged; what that amount is worth to the business
        // is a fact about when the order was raised, not when it was quoted.
        let currency = self.currency(Some(quote.currency.clone()), order_date).await?;

        let order = SalesOrder {
            id: order_id,
            org_id: quote.org_id,
            order_number: self.orders.next_number().await?,
            customer_id: quote.customer_id,
            contact_id: quote.contact_id,
            quote_id: Some(quote.id),
            status: OrderStatus::DRAFT.to_string(),
            order_date,
            required_date: req.required_date,
            shipping_address: req.shipping_address,
            billing_address: req.billing_address,
            subtotal: quote.subtotal,
            tax_amount: quote.tax_amount,
            total: quote.total,
            base_total: currency.to_base_opt(quote.total),
            fx_rate: currency.fx_rate,
            currency: currency.code,
            notes: quote.notes,
            created_by: user.id,
            created_at: now,
            updated_at: now,
        };

        let lines: Vec<OrderLine> = quote_lines
            .iter()
            .map(|l| OrderLine {
                id: Uuid::new_v4(),
                order_id,
                product_id: l.product_id,
                description: l.description.clone(),
                quantity: l.quantity,
                unit_price: l.unit_price,
                discount_percent: l.discount_percent,
                tax_rate: l.tax_rate,
                line_total: l.line_total,
                sort_order: l.sort_order,
            })
            .collect();

        let order = self.orders.create(&order, &lines).await?;
        let lines = order_lines_view(&self.orders, order.id).await?;
        Ok(OrderDetail { order, lines })
    }

    async fn require_quote(&self, id: Uuid) -> AppResult<Quote> {
        self.quotes
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Quote {} not found", id)))
    }
}

fn build_quote_lines(
    quote_id: Uuid,
    requested: &[DocumentLineRequest],
    amounts: &[LineAmounts],
) -> Vec<QuoteLine> {
    requested
        .iter()
        .zip(amounts)
        .enumerate()
        .map(|(index, (req, amount))| QuoteLine {
            id: Uuid::new_v4(),
            quote_id,
            product_id: req.product_id,
            description: req.description.clone(),
            quantity: req.quantity,
            unit_price: req.unit_price,
            discount_percent: req.discount_percent,
            tax_rate: req.tax_rate,
            line_total: amount.net,
            sort_order: index as i32,
        })
        .collect()
}

// ------------------------------------------------------------------- orders

pub struct OrderUseCases<O: SalesOrderRepository, I: InvoiceRepository> {
    orders: O,
    invoices: I,
    /// Where confirming an order reaches the shelves. Does nothing until a
    /// dispatch warehouse is configured.
    dispatch: Arc<dyn StockDispatcher>,
    fx: Arc<dyn CurrencyResolver>,
}

impl<O: SalesOrderRepository, I: InvoiceRepository> OrderUseCases<O, I> {
    pub fn new(
        orders: O,
        invoices: I,
        dispatch: Arc<dyn StockDispatcher>,
        fx: Arc<dyn CurrencyResolver>,
    ) -> Self {
        Self { orders, invoices, dispatch, fx }
    }

    /// The currency a document is raised in, together with the rate frozen onto
    /// it. Read at the point of use rather than cached, so a change under
    /// Settings applies to the next document raised.
    ///
    /// `on` is the document's own date: the rate that applied when it was
    /// raised is the rate it keeps.
    async fn currency(
        &self,
        requested: Option<String>,
        on: NaiveDate,
    ) -> AppResult<DocumentCurrency> {
        self.fx.resolve(requested.as_deref(), on).await
    }

    pub async fn create(&self, req: CreateOrderRequest, user: &CurrentUser) -> AppResult<OrderDetail> {
        let order_id = Uuid::new_v4();
        let (amounts, totals) = price_lines(&req.lines);
        let now = Utc::now();
        let currency = self.currency(req.currency.clone(), req.order_date).await?;

        let order = SalesOrder {
            id: order_id,
            org_id: user.org_id,
            order_number: self.orders.next_number().await?,
            customer_id: req.customer_id,
            contact_id: req.contact_id,
            quote_id: req.quote_id,
            status: OrderStatus::DRAFT.to_string(),
            order_date: req.order_date,
            required_date: req.required_date,
            shipping_address: req.shipping_address,
            billing_address: req.billing_address,
            subtotal: Some(totals.subtotal),
            tax_amount: Some(totals.tax_amount),
            total: Some(totals.total),
            base_total: Some(currency.to_base(totals.total)),
            fx_rate: currency.fx_rate,
            currency: currency.code,
            notes: req.notes,
            created_by: user.id,
            created_at: now,
            updated_at: now,
        };

        let lines = build_order_lines(order_id, &req.lines, &amounts);
        let order = self.orders.create(&order, &lines).await?;
        let lines = order_lines_view(&self.orders, order.id).await?;
        Ok(OrderDetail { order, lines })
    }

    pub async fn get(&self, id: Uuid) -> AppResult<OrderDetail> {
        let order = self.require_order(id).await?;
        let lines = order_lines_view(&self.orders, id).await?;
        Ok(OrderDetail { order, lines })
    }

    pub async fn list(
        &self,
        filters: &SalesDocumentFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<SalesOrder>, i64)> {
        self.orders.list(filters, params).await
    }

    pub async fn update(&self, id: Uuid, req: UpdateOrderRequest) -> AppResult<OrderDetail> {
        let mut order = self.require_order(id).await?;

        if !OrderStatus::is_editable(&order.status) {
            return Err(SalesError::NotEditable { document: "order", status: order.status }.into());
        }

        if let Some(customer_id) = req.customer_id {
            order.customer_id = customer_id;
        }
        if req.contact_id.is_some() {
            order.contact_id = req.contact_id;
        }
        if req.required_date.is_some() {
            order.required_date = req.required_date;
        }
        if req.shipping_address.is_some() {
            order.shipping_address = req.shipping_address;
        }
        if req.billing_address.is_some() {
            order.billing_address = req.billing_address;
        }
        if req.notes.is_some() {
            order.notes = req.notes;
        }
        order.updated_at = Utc::now();

        let new_lines = match &req.lines {
            Some(requested) => {
                if requested.is_empty() {
                    return Err(SalesError::NoLines { document: "order" }.into());
                }
                let (amounts, totals) = price_lines(requested);
                order.subtotal = Some(totals.subtotal);
                order.tax_amount = Some(totals.tax_amount);
                order.total = Some(totals.total);
                Some(build_order_lines(order.id, requested, &amounts))
            }
            None => None,
        };

        let currency = self.currency(Some(order.currency.clone()), order.order_date).await?;
        order.fx_rate = currency.fx_rate;
        order.base_total = currency.to_base_opt(order.total);

        // A **confirmed** order is editable (`OrderStatus::is_editable`), and
        // rewriting its lines *replaces* them. `stock_reservations.order_line_id`
        // cascades from those lines, so the rows would be deleted by the write
        // below — silently, and without giving the held stock back, leaving
        // `reserved_quantity` counting a reservation nothing records any more.
        //
        // So the release has to happen **before** the lines go, not after.
        let re_reserve = new_lines.is_some() && order.status == OrderStatus::CONFIRMED;
        if re_reserve {
            self.dispatch.order_released(order.id).await?;
        }

        let order = self.orders.update(&order, new_lines.as_deref()).await?;

        if re_reserve {
            self.dispatch.order_confirmed(&dispatchable_order(&self.orders, &order).await?).await?;
        }

        let lines = order_lines_view(&self.orders, order.id).await?;
        Ok(OrderDetail { order, lines })
    }

    pub async fn set_status(&self, id: Uuid, status: &str) -> AppResult<SalesOrder> {
        let order = self.require_order(id).await?;

        if !OrderStatus::ALL.contains(&status) {
            return Err(AppError::Validation(format!(
                "Unknown order status '{}'. Expected one of: {}",
                status,
                OrderStatus::ALL.join(", ")
            )));
        }

        if !OrderStatus::can_transition(&order.status, status) {
            return Err(SalesError::InvalidTransition {
                document: "order",
                from: order.status,
                to: status.to_string(),
            }
            .into());
        }

        // `shipped` and `delivered` claim the goods have gone. Where invoicing
        // is what takes them off the shelf, an order cannot make that claim
        // before it has been invoiced — otherwise it can sit in a terminal state
        // with its stock still on the shelf and still reserved, which is a count
        // that will never reconcile.
        //
        // Checked before the write, so refusing leaves the order where it was.
        if OrderStatus::asserts_goods_have_left(status) && self.dispatch.ships_automatically().await?
        {
            // Not "is there an issued invoice" but "is *all* of it invoiced".
            // The weaker question let an order be marked delivered for ten units
            // with six billed: an operator short of stock would trim the draft
            // invoice to what the shelf held, and the four that never shipped
            // became unbillable and unrecorded.
            //
            // Counted from issued invoices only: a draft has shipped nothing and
            // a cancelled one has had its goods put back.
            let issued: Vec<Uuid> = self
                .orders
                .find_invoices_for_order(id)
                .await?
                .into_iter()
                .filter(|invoice| InvoiceStatus::is_issued(&invoice.status))
                .map(|invoice| invoice.id)
                .collect();

            let shipped = self.orders.invoiced_by_invoices(&issued).await?;
            let outstanding: i32 = self
                .orders
                .find_lines(id)
                .await?
                .into_iter()
                .map(|line| {
                    let billed = shipped
                        .iter()
                        .find(|(id, _)| *id == line.id)
                        .map(|(_, qty)| *qty as i32)
                        .unwrap_or(0);
                    line.outstanding(billed)
                })
                .sum();

            if outstanding > 0 {
                return Err(SalesError::GoodsHaveNotLeft {
                    order: order.order_number,
                    status: status.to_string(),
                    outstanding,
                }
                .into());
            }
        }

        let updated = self.orders.update_status(id, status).await?;

        match status {
            // Confirming is the promise, so it is where the goods start being
            // held for this customer.
            OrderStatus::CONFIRMED => {
                self.dispatch.order_confirmed(&dispatchable_order(&self.orders, &updated).await?).await?
            }
            // Nothing is promised any more.
            OrderStatus::CANCELLED => self.dispatch.order_released(updated.id).await?,
            _ => {}
        }

        Ok(updated)
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        let order = self.require_order(id).await?;

        if order.status != OrderStatus::DRAFT {
            return Err(SalesError::NotEditable { document: "order", status: order.status }.into());
        }

        self.orders.delete(id).await
    }

    /// SalesOrder -> Invoice.
    pub async fn convert_to_invoice(
        &self,
        id: Uuid,
        req: ConvertOrderRequest,
        user: &CurrentUser,
    ) -> AppResult<InvoiceDetail> {
        let order = self.require_order(id).await?;

        if !OrderStatus::is_invoiceable(&order.status) {
            return Err(SalesError::OrderNotInvoiceable(order.order_number).into());
        }

        // An order gets one *live* invoice at a time. A cancelled one has been
        // unwound on both sides — its goods came back and its posting was
        // mirrored — so it must not keep the order from being billed again.
        // Blocking on it left orders permanently unbillable, and with nothing
        // issued they could not be shipped or delivered either.
        let order_lines = self.orders.find_lines(id).await?;
        if order_lines.is_empty() {
            return Err(SalesError::NoLines { document: "order" }.into());
        }

        // What each line has left, summed from the order's live invoices. An
        // order may be billed as many times as it takes; what it may not do is
        // bill the same units twice.
        let invoiced = self.orders.invoiced_by_order_line(id).await?;
        let billed = |line_id: Uuid| -> i32 {
            invoiced.iter().find(|(l, _)| *l == line_id).map(|(_, q)| *q as i32).unwrap_or(0)
        };

        // Omitting the lines bills everything outstanding, which is what the
        // one-click conversion has always done and what an order billed in one
        // go still does.
        let wanted: Vec<(OrderLine, i32)> = match &req.lines {
            Some(requested) => {
                if requested.is_empty() {
                    return Err(SalesError::NoLines { document: "invoice" }.into());
                }

                // Running totals, so a line named twice in one request is
                // measured against its outstanding once — the same care
                // `record_receipt` takes over a repeated PO line.
                let mut running: Vec<(Uuid, i32)> = Vec::new();
                let mut picked: Vec<(OrderLine, i32)> = Vec::new();

                for line in requested {
                    let order_line = order_lines
                        .iter()
                        .find(|l| l.id == line.order_line_id)
                        .ok_or_else(|| {
                            SalesError::LineNotOnOrder(
                                line.order_line_id.to_string(),
                                order.order_number.clone(),
                            )
                        })?;

                    let already = billed(order_line.id)
                        + running.iter().find(|(l, _)| *l == order_line.id).map_or(0, |(_, q)| *q);
                    let outstanding = order_line.outstanding(already);

                    if line.quantity > outstanding {
                        return Err(SalesError::OverInvoice {
                            description: order_line.description.clone(),
                            requested: line.quantity,
                            outstanding,
                        }
                        .into());
                    }

                    match running.iter_mut().find(|(l, _)| *l == order_line.id) {
                        Some((_, q)) => *q += line.quantity,
                        None => running.push((order_line.id, line.quantity)),
                    }
                    picked.push((order_line.clone(), line.quantity));
                }

                picked
            }
            None => order_lines
                .iter()
                .filter_map(|line| {
                    let outstanding = line.outstanding(billed(line.id));
                    (outstanding > 0).then(|| (line.clone(), outstanding))
                })
                .collect(),
        };

        if wanted.is_empty() {
            return Err(SalesError::NothingOutstanding(order.order_number).into());
        }

        let issue_date = req.issue_date.unwrap_or_else(|| Utc::now().date_naive());
        let due_date = req.due_date.unwrap_or_else(|| {
            issue_date + Duration::days(req.payment_terms_days.unwrap_or(DEFAULT_PAYMENT_TERMS_DAYS))
        });
        if due_date < issue_date {
            return Err(SalesError::DueBeforeIssue.into());
        }

        let invoice_id = Uuid::new_v4();
        let now = Utc::now();

        // Priced from what this instalment actually bills, not copied off the
        // order: an order billed in two goes must not be billed twice over.
        // Each line keeps the order's price, discount and tax rate — only the
        // quantity differs — so the instalments add back up to the order.
        let amounts: Vec<LineAmounts> = wanted
            .iter()
            .map(|(line, quantity)| {
                calculate_line(*quantity, line.unit_price, line.discount_percent, line.tax_rate)
            })
            .collect();
        let totals = sum_totals(amounts.iter().copied());
        let total = totals.total;

        // The invoice is the document that books revenue, so its rate is the one
        // realised gain and loss is later measured against. Resolved at the
        // issue date, not the order date.
        let currency = self.currency(Some(order.currency.clone()), issue_date).await?;
        let base_total = currency.to_base(total);

        let invoice = Invoice {
            id: invoice_id,
            org_id: order.org_id,
            invoice_number: self.invoices.next_number().await?,
            customer_id: order.customer_id,
            order_id: Some(order.id),
            status: InvoiceStatus::DRAFT.to_string(),
            issue_date,
            due_date,
            subtotal: Some(totals.subtotal),
            tax_amount: Some(totals.tax_amount),
            total: Some(total),
            amount_paid: Some(Decimal::ZERO),
            amount_due: Some(total),
            base_total: Some(base_total),
            base_amount_paid: Some(Decimal::ZERO),
            base_amount_due: Some(base_total),
            fx_rate: currency.fx_rate,
            currency: currency.code,
            notes: order.notes,
            created_by: user.id,
            created_at: now,
            updated_at: now,
        };

        let lines: Vec<InvoiceLine> = wanted
            .iter()
            .zip(&amounts)
            .enumerate()
            .map(|(index, ((line, quantity), amount))| InvoiceLine {
                id: Uuid::new_v4(),
                invoice_id,
                // The link the whole feature rests on: what a line has left to
                // bill is summed from these.
                order_line_id: Some(line.id),
                product_id: line.product_id,
                description: line.description.clone(),
                quantity: *quantity,
                unit_price: line.unit_price,
                discount_percent: line.discount_percent,
                tax_rate: line.tax_rate,
                line_total: amount.net,
                sort_order: index as i32,
            })
            .collect();

        let invoice = self.invoices.create(&invoice, &lines).await?;
        let lines = self.invoices.find_lines(invoice.id).await?;
        Ok(InvoiceDetail { invoice, lines, payments: Vec::new() })
    }

    async fn require_order(&self, id: Uuid) -> AppResult<SalesOrder> {
        self.orders
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Sales order {} not found", id)))
    }
}

fn build_order_lines(
    order_id: Uuid,
    requested: &[DocumentLineRequest],
    amounts: &[LineAmounts],
) -> Vec<OrderLine> {
    requested
        .iter()
        .zip(amounts)
        .enumerate()
        .map(|(index, (req, amount))| OrderLine {
            id: Uuid::new_v4(),
            order_id,
            product_id: req.product_id,
            description: req.description.clone(),
            quantity: req.quantity,
            unit_price: req.unit_price,
            discount_percent: req.discount_percent,
            tax_rate: req.tax_rate,
            line_total: amount.net,
            sort_order: index as i32,
        })
        .collect()
}

/// What the shelves need from an order: its lines, and nothing about where the
/// goods are kept.
///
/// A free function because two use cases hold an order's stock. Orders reserve
/// on confirmation and re-reserve after an edit; invoices re-reserve when a
/// cancellation puts the goods back. One mapping, so the two cannot describe the
/// same order differently.
async fn dispatchable_order<O: SalesOrderRepository>(
    orders: &O,
    order: &SalesOrder,
) -> AppResult<DispatchableOrder> {
    let lines = orders.find_lines(order.id).await?;

    Ok(DispatchableOrder {
        id: order.id,
        lines: lines
            .iter()
            .map(|line| ReservableLine {
                order_line_id: line.id,
                product_id: line.product_id,
                quantity: line.quantity,
            })
            .collect(),
    })
}

/// An order's lines, each with how much of it is still to be billed.
///
/// A free function because both use cases that hand an order back — the quote
/// conversion and the order's own CRUD — need the same view, and the invoiced
/// figure is derived per request rather than stored on the row.
async fn order_lines_view<O: SalesOrderRepository>(
    orders: &O,
    order_id: Uuid,
) -> AppResult<Vec<OrderLineView>> {
    let lines = orders.find_lines(order_id).await?;
    let invoiced = orders.invoiced_by_order_line(order_id).await?;

    Ok(lines
        .into_iter()
        .map(|line| {
            let billed = invoiced
                .iter()
                .find(|(id, _)| *id == line.id)
                .map(|(_, qty)| *qty as i32)
                .unwrap_or(0);
            OrderLineView::new(line, billed)
        })
        .collect())
}

// ----------------------------------------------------------------- invoices

pub struct InvoiceUseCases<
    I: InvoiceRepository,
    P: PaymentRepository,
    C: CreditNoteRepository,
    O: SalesOrderRepository,
> {
    invoices: I,
    payments: P,
    /// Read only to settle the invoice: what is owed is the total less what has
    /// been credited as well as less what has been paid.
    notes: C,
    /// Read only when an invoice is cancelled: whether its order still stands,
    /// and what it needs held. `OrderUseCases` holds an `InvoiceRepository` for
    /// the same reason — the two documents share a lifecycle.
    orders: O,
    /// Where issuing an invoice reaches the shelves. Does nothing until a
    /// dispatch warehouse is configured.
    dispatch: Arc<dyn StockDispatcher>,
    fx: Arc<dyn CurrencyResolver>,
    /// Where issuing and settling an invoice reach the books. Behind a trait so
    /// sales does not need to know how they are kept; a no-op until the
    /// organisation maps its accounts.
    poster: Arc<dyn DocumentPoster>,
}

impl<
        I: InvoiceRepository,
        P: PaymentRepository,
        C: CreditNoteRepository,
        O: SalesOrderRepository,
    > InvoiceUseCases<I, P, C, O>
{
    pub fn new(
        invoices: I,
        payments: P,
        notes: C,
        orders: O,
        dispatch: Arc<dyn StockDispatcher>,
        fx: Arc<dyn CurrencyResolver>,
        poster: Arc<dyn DocumentPoster>,
    ) -> Self {
        Self { invoices, payments, notes, orders, dispatch, fx, poster }
    }

    /// What the ledger needs from an invoice, from the invoice we already have.
    /// What the shelves need from this invoice: its lines, and nothing about
    /// where the goods are kept.
    async fn dispatchable(&self, invoice: &Invoice) -> AppResult<DispatchableInvoice> {
        let lines = self.invoices.find_lines(invoice.id).await?;

        Ok(DispatchableInvoice {
            id: invoice.id,
            org_id: invoice.org_id,
            order_id: invoice.order_id,
            number: invoice.invoice_number.clone(),
            lines: lines
                .iter()
                .map(|line| DispatchableLine {
                    product_id: line.product_id,
                    quantity: line.quantity,
                    order_line_id: line.order_line_id,
                })
                .collect(),
        })
    }

    /// The order this invoice was raised from, if it came from one at all.
    ///
    /// An invoice can be raised directly against a customer
    /// (`CreateInvoiceRequest::order_id` is optional), and then nothing about
    /// orders applies to it.
    async fn order_for(&self, invoice: &Invoice) -> AppResult<Option<SalesOrder>> {
        match invoice.order_id {
            Some(order_id) => self.orders.find_by_id(order_id).await,
            None => Ok(None),
        }
    }

    /// Moves the order this invoice came from to `partially_shipped` when it
    /// still owes goods.
    ///
    /// Only ever *into* that state. Coming back out of it is the operator's
    /// call — marking the order shipped — and the lifecycle guard now refuses
    /// that while anything is outstanding. A cancelled instalment therefore
    /// leaves the order reading `partially_shipped` with everything owed again,
    /// which is honest: work has been done on it and then undone.
    async fn advance_order(&self, invoice: &Invoice) -> AppResult<()> {
        let Some(order) = self.order_for(invoice).await? else {
            return Ok(());
        };

        let lines = self.orders.find_lines(order.id).await?;
        let invoiced = self.orders.invoiced_by_order_line(order.id).await?;
        let all_invoiced = lines.iter().all(|line| {
            let billed = invoiced
                .iter()
                .find(|(id, _)| *id == line.id)
                .map(|(_, qty)| *qty as i32)
                .unwrap_or(0);
            line.is_fully_invoiced(billed)
        });

        if let Some(next) = OrderStatus::after_invoice(&order.status, all_invoiced) {
            self.orders.update_status(order.id, next).await?;
        }

        Ok(())
    }

    /// Takes the order's hold back after a cancellation returned its goods.
    ///
    /// Issuing released the reservation so the invoice would not be blocked by
    /// its own order. Cancelling undoes issuing, so the hold has to come back
    /// with the goods — otherwise a live order silently stops protecting stock
    /// it is still promised, and anyone else can take it.
    ///
    /// Runs *after* the goods are on the shelf: there is nothing to hold until
    /// they are back. What it holds is what is available, by the same rule as
    /// confirming — if somebody took the stock in the meantime, the order gets
    /// what is left.
    async fn rehold(&self, invoice: &Invoice) -> AppResult<()> {
        let Some(order) = self.order_for(invoice).await? else {
            return Ok(());
        };

        // A cancelled order wants nothing; a shipped or delivered one cannot
        // reach here, because cancelling its invoice is refused.
        if !OrderStatus::still_expects_goods(&order.status) {
            return Ok(());
        }

        // Released before re-holding, the way editing a confirmed order does.
        // A part-shipped order still holds what its remaining lines need, and
        // `stock_reservations` allows one row per line — so re-holding on top of
        // that hold would collide with it. Giving the whole thing back first
        // also happens to be right: the cancelled instalment's units are
        // outstanding again, so what the order should hold is the whole of it,
        // as far as the shelf reaches.
        self.dispatch.order_released(order.id).await?;
        self.dispatch.order_confirmed(&dispatchable_order(&self.orders, &order).await?).await
    }

    fn postable(invoice: &Invoice) -> PostableInvoice {
        PostableInvoice {
            id: invoice.id,
            org_id: invoice.org_id,
            number: invoice.invoice_number.clone(),
            issue_date: invoice.issue_date,
            currency: invoice.currency.clone(),
            fx_rate: invoice.fx_rate,
            base_total: invoice.base_total.unwrap_or(Decimal::ZERO),
            tax_amount: invoice.tax_amount.unwrap_or(Decimal::ZERO),
            created_by: invoice.created_by,
        }
    }

    fn postable_payment(payment: &Payment, invoice: &Invoice) -> PostablePayment {
        PostablePayment {
            id: payment.id,
            org_id: payment.org_id,
            document_number: invoice.invoice_number.clone(),
            payment_date: payment.payment_date,
            base_amount: payment.base_amount,
            fx_gain_loss: payment.fx_gain_loss,
            created_by: payment.created_by,
        }
    }

    /// The currency a document is raised in, together with the rate frozen onto
    /// it. Read at the point of use rather than cached, so a change under
    /// Settings applies to the next document raised.
    ///
    /// `on` is the document's own date: the rate that applied when it was
    /// raised is the rate it keeps.
    async fn currency(
        &self,
        requested: Option<String>,
        on: NaiveDate,
    ) -> AppResult<DocumentCurrency> {
        self.fx.resolve(requested.as_deref(), on).await
    }

    pub async fn create(
        &self,
        req: CreateInvoiceRequest,
        user: &CurrentUser,
    ) -> AppResult<InvoiceDetail> {
        if req.due_date < req.issue_date {
            return Err(SalesError::DueBeforeIssue.into());
        }

        let invoice_id = Uuid::new_v4();
        let (amounts, totals) = price_lines(&req.lines);
        let now = Utc::now();
        let currency = self.currency(req.currency.clone(), req.issue_date).await?;
        let base_total = currency.to_base(totals.total);

        let invoice = Invoice {
            id: invoice_id,
            org_id: user.org_id,
            invoice_number: self.invoices.next_number().await?,
            customer_id: req.customer_id,
            order_id: req.order_id,
            status: InvoiceStatus::DRAFT.to_string(),
            issue_date: req.issue_date,
            due_date: req.due_date,
            subtotal: Some(totals.subtotal),
            tax_amount: Some(totals.tax_amount),
            total: Some(totals.total),
            amount_paid: Some(Decimal::ZERO),
            amount_due: Some(totals.total),
            base_total: Some(base_total),
            base_amount_paid: Some(Decimal::ZERO),
            base_amount_due: Some(base_total),
            fx_rate: currency.fx_rate,
            currency: currency.code,
            notes: req.notes,
            created_by: user.id,
            created_at: now,
            updated_at: now,
        };

        let lines = build_invoice_lines(invoice_id, &req.lines, &amounts);
        let invoice = self.invoices.create(&invoice, &lines).await?;
        let lines = self.invoices.find_lines(invoice.id).await?;
        Ok(InvoiceDetail { invoice, lines, payments: Vec::new() })
    }

    pub async fn get(&self, id: Uuid) -> AppResult<InvoiceDetail> {
        let invoice = self.require_invoice(id).await?;
        let lines = self.invoices.find_lines(id).await?;
        let (payments, _) = self
            .payments
            .list(
                &PaymentFilters { invoice_id: Some(id), ..Default::default() },
                &PaginationParams { page: 1, per_page: 200, sort: None },
            )
            .await?;
        Ok(InvoiceDetail { invoice, lines, payments })
    }

    pub async fn list(
        &self,
        filters: &SalesDocumentFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Invoice>, i64)> {
        // Refresh overdue flags before answering, so the list a user sees is honest.
        self.invoices.mark_overdue(Utc::now().date_naive()).await?;
        self.invoices.list(filters, params).await
    }

    pub async fn update(&self, id: Uuid, req: UpdateInvoiceRequest) -> AppResult<InvoiceDetail> {
        let mut invoice = self.require_invoice(id).await?;

        if !InvoiceStatus::is_editable(&invoice.status) {
            return Err(
                SalesError::NotEditable { document: "invoice", status: invoice.status }.into()
            );
        }

        if let Some(customer_id) = req.customer_id {
            invoice.customer_id = customer_id;
        }
        if let Some(issue_date) = req.issue_date {
            invoice.issue_date = issue_date;
        }
        if let Some(due_date) = req.due_date {
            invoice.due_date = due_date;
        }
        if req.notes.is_some() {
            invoice.notes = req.notes;
        }
        if invoice.due_date < invoice.issue_date {
            return Err(SalesError::DueBeforeIssue.into());
        }
        invoice.updated_at = Utc::now();

        let new_lines = match &req.lines {
            Some(requested) => {
                if requested.is_empty() {
                    return Err(SalesError::NoLines { document: "invoice" }.into());
                }
                let (amounts, totals) = price_lines(requested);
                invoice.subtotal = Some(totals.subtotal);
                invoice.tax_amount = Some(totals.tax_amount);
                invoice.total = Some(totals.total);
                // A draft invoice has no payments, so everything is still due.
                invoice.amount_due = Some(totals.total);
                Some(build_invoice_lines(invoice.id, requested, &amounts))
            }
            None => None,
        };

        // Only reachable while the invoice is still a draft, so there are no
        // payments to disturb and everything outstanding restates together.
        let currency = self.currency(Some(invoice.currency.clone()), invoice.issue_date).await?;
        invoice.fx_rate = currency.fx_rate;
        invoice.base_total = currency.to_base_opt(invoice.total);
        invoice.base_amount_due = currency.to_base_opt(invoice.amount_due);

        let invoice = self.invoices.update(&invoice, new_lines.as_deref()).await?;
        let lines = self.invoices.find_lines(invoice.id).await?;
        Ok(InvoiceDetail { invoice, lines, payments: Vec::new() })
    }

    pub async fn set_status(
        &self,
        id: Uuid,
        status: &str,
        user: &CurrentUser,
    ) -> AppResult<Invoice> {
        let invoice = self.require_invoice(id).await?;

        // Captured before the transition check consumes it. What the invoice
        // *was* decides what cancelling has to unwind: only an issued invoice
        // has shipped anything or posted anything.
        let was_issued = InvoiceStatus::is_issued(&invoice.status);

        if !InvoiceStatus::ALL.contains(&status) {
            return Err(AppError::Validation(format!(
                "Unknown invoice status '{}'. Expected one of: {}",
                status,
                InvoiceStatus::ALL.join(", ")
            )));
        }

        if !InvoiceStatus::can_transition(&invoice.status, status) {
            return Err(SalesError::InvalidTransition {
                document: "invoice",
                from: invoice.status,
                to: status.to_string(),
            }
            .into());
        }

        // `paid` is a consequence of payments, never a manual claim.
        if status == InvoiceStatus::PAID {
            let paid = self.payments.total_paid_for_invoice(id).await?;
            let total = invoice.total.unwrap_or(Decimal::ZERO);
            if paid < total {
                return Err(AppError::Conflict(format!(
                    "Invoice {} still has {} outstanding; record the payment instead",
                    invoice.invoice_number,
                    round_money(total - paid)
                )));
            }
        }

        // Cancelling puts the goods back on the shelf, so an order already
        // claiming they reached the customer would be contradicted by its own
        // invoice — the mirror of the state the lifecycle guard exists to
        // prevent. Crediting is the document for goods that have gone.
        //
        // Refused before the write, and only where invoicing is what ships:
        // with no dispatch warehouse nothing moves, so there is no shelf to
        // contradict and cancelling stays as unrestricted as it has always been.
        if status == InvoiceStatus::CANCELLED && self.dispatch.ships_automatically().await? {
            if let Some(order) = self.order_for(&invoice).await? {
                if OrderStatus::asserts_goods_have_left(&order.status) {
                    return Err(SalesError::GoodsAlreadyGone {
                        invoice: invoice.invoice_number,
                        order: order.order_number,
                        status: order.status,
                    }
                    .into());
                }
            }
        }

        // Shipping happens *before* the status write, unlike posting, and the
        // difference is real: having the stock is a **precondition** of issuing,
        // while posting is a consequence of it. Once a dispatch warehouse is
        // configured, issuing an invoice the shelf cannot cover is refused — and
        // it has to be refused with the invoice still a draft, rather than sent
        // with an error and no goods behind it.
        if status == InvoiceStatus::SENT {
            self.dispatch.invoice_issued(&self.dispatchable(&invoice).await?, user).await?;
        }

        let updated = self.invoices.update_status(id, status).await?;

        // The books move after the document does, so a posting failure can never
        // leave an invoice claiming a state it never reached. The reverse gap —
        // sent but not yet posted — is the one we accept, and it is visible in
        // the unposted report and fixable from it.
        match status {
            // Issuing is what recognises the revenue and creates the
            // receivable. A draft has done neither.
            InvoiceStatus::SENT => {
                self.poster.invoice_issued(&Self::postable(&updated)).await?;
                // An instalment moves its order to `partially_shipped`, the way
                // a goods receipt moves a purchase order to
                // `partially_received`. Derived from the lines rather than
                // requested, so it cannot disagree with what has been billed.
                self.advance_order(&updated).await?;
            }

            // Cancelling undoes issuing — which means there is nothing to undo
            // unless the invoice was issued. Without this gate, cancelling a
            // **draft** posted a reversal of a posting that never happened and
            // brought stock in that never went out, inventing goods and blending
            // them into the moving average.
            InvoiceStatus::CANCELLED if was_issued => {
                self.poster.invoice_cancelled(&Self::postable(&updated)).await?;
                // After the write, because bringing goods back cannot fail the
                // way sending them out can.
                self.dispatch.invoice_cancelled(&self.dispatchable(&updated).await?, user).await?;
                self.rehold(&updated).await?;
            }
            _ => {}
        }

        Ok(updated)
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        let invoice = self.require_invoice(id).await?;

        if !InvoiceStatus::is_editable(&invoice.status) {
            return Err(
                SalesError::NotEditable { document: "invoice", status: invoice.status }.into()
            );
        }

        self.invoices.delete(id).await
    }

    /// Records a payment and re-settles the invoice from the payment ledger.
    pub async fn record_payment(
        &self,
        req: RecordPaymentRequest,
        user: &CurrentUser,
    ) -> AppResult<Payment> {
        let invoice = self.require_invoice(req.invoice_id).await?;

        if !PaymentMethod::is_valid(&req.payment_method) {
            return Err(SalesError::UnsupportedPaymentMethod(req.payment_method).into());
        }

        if req.amount <= Decimal::ZERO {
            return Err(AppError::Validation("Payment amount must be positive".to_string()));
        }

        // A draft has raised no receivable, so there is nothing for money to
        // settle — the same rule a draft purchase order and a draft invoice
        // being credited already follow. Checked before the general refusal so
        // the answer says what to do about it rather than "cannot take further
        // payments", which a document that has taken none reads oddly.
        if invoice.status == InvoiceStatus::DRAFT {
            return Err(SalesError::NotPayable(invoice.invoice_number).into());
        }

        if !InvoiceStatus::accepts_payment(&invoice.status) {
            return Err(SalesError::InvoiceClosedToPayment(
                invoice.invoice_number,
                invoice.status,
            )
            .into());
        }

        let already_paid = self.payments.total_paid_for_invoice(invoice.id).await?;
        let credited = self.notes.total_credited_for_invoice(invoice.id).await?;
        let total = invoice.total.unwrap_or(Decimal::ZERO);
        // Net of what has been credited: a customer cannot be asked to pay for
        // goods they have already been credited for.
        let outstanding = round_money(total - credited - already_paid);

        if req.amount > outstanding {
            return Err(SalesError::PaymentExceedsBalance(
                req.amount.to_string(),
                outstanding.to_string(),
                invoice.invoice_number,
            )
            .into());
        }

        let amount = round_money(req.amount);

        // A payment is always in the invoice's currency — you settle a debt in
        // the currency it was raised in — but at the rate on the day the money
        // arrived, which is what makes it worth something different.
        let currency = self.currency(Some(invoice.currency.clone()), req.payment_date).await?;
        let base_amount = currency.to_base(amount);

        // What this slice of the debt was booked as worth when the invoice
        // recognised the revenue. The difference is realised, not a paper
        // movement: the money is in, and it bought more or less base currency
        // than the sale was recorded at.
        let booked = to_base(amount, invoice.fx_rate);
        let fx_gain_loss = base_amount - booked;

        let payment = Payment {
            id: Uuid::new_v4(),
            org_id: invoice.org_id,
            invoice_id: invoice.id,
            amount,
            fx_rate: currency.fx_rate,
            base_amount,
            fx_gain_loss,
            currency: currency.code,
            payment_method: req.payment_method,
            payment_date: req.payment_date,
            reference: req.reference,
            notes: req.notes,
            created_by: user.id,
            created_at: Utc::now(),
        };

        let payment = self.payments.create(&payment).await?;
        self.resettle(&invoice, Utc::now().date_naive()).await?;
        self.poster.payment_received(&Self::postable_payment(&payment, &invoice)).await?;
        Ok(payment)
    }

    /// Reverses a payment and re-settles the invoice.
    pub async fn delete_payment(&self, payment_id: Uuid) -> AppResult<()> {
        let payment = self
            .payments
            .find_by_id(payment_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Payment {} not found", payment_id)))?;

        let invoice = self.require_invoice(payment.invoice_id).await?;

        // Reversed before the payment row goes, because the mirror is derived
        // from it. The entries stay either way — a reversal is posted, not
        // erased, so the ledger still shows that money arrived and went back.
        self.poster.payment_reversed(&Self::postable_payment(&payment, &invoice)).await?;

        self.payments.delete(payment_id).await?;
        self.resettle(&invoice, Utc::now().date_naive()).await?;
        Ok(())
    }

    pub async fn list_payments(
        &self,
        filters: &PaymentFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Payment>, i64)> {
        self.payments.list(filters, params).await
    }

    /// Recomputes `amount_paid` / `amount_due` / status from the ledgers, so the
    /// invoice can never drift away from the documents recorded against it.
    async fn resettle(&self, invoice: &Invoice, today: NaiveDate) -> AppResult<Invoice> {
        let paid = round_money(self.payments.total_paid_for_invoice(invoice.id).await?);
        let credited = round_money(self.notes.total_credited_for_invoice(invoice.id).await?);
        settle_invoice(&self.invoices, invoice, paid, credited, today).await
    }

    async fn require_invoice(&self, id: Uuid) -> AppResult<Invoice> {
        self.invoices
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Invoice {} not found", id)))
    }
}

/// Tags the stock movement a credit note produces, so it can be traced back —
/// and so `movement_entries` knows the note has already posted its own inventory
/// leg and must not post an adjustment on top.
const CREDIT_NOTE_REFERENCE: &str = "sales_credit_note";

pub struct CreditNoteUseCases<C, I, S, P>
where
    C: CreditNoteRepository,
    I: InvoiceRepository,
    S: StockRepository,
    P: PaymentRepository,
{
    notes: C,
    invoices: I,
    /// Only touched when a warehouse is named — goods coming back onto a shelf.
    stock: S,
    /// Read to re-settle the invoice: what is owed is the total less what has
    /// been credited *and* less what has been paid.
    payments: P,
    poster: Arc<dyn DocumentPoster>,
}

impl<C, I, S, P> CreditNoteUseCases<C, I, S, P>
where
    C: CreditNoteRepository,
    I: InvoiceRepository,
    S: StockRepository,
    P: PaymentRepository,
{
    pub fn new(notes: C, invoices: I, stock: S, payments: P, poster: Arc<dyn DocumentPoster>) -> Self {
        Self { notes, invoices, stock, payments, poster }
    }

    /// Credits a customer against an invoice.
    ///
    /// This is the document that unblocks the case with no answer before it: a
    /// paid invoice has no outgoing status transition, so it could not be
    /// cancelled or adjusted. A credit note does not touch the status machine at
    /// all — it recomputes settlement, and what is now owed *to* the customer
    /// shows as a negative `amount_due`.
    pub async fn create(
        &self,
        req: CreateCreditNoteRequest,
        user: &CurrentUser,
    ) -> AppResult<CreditNoteDetail> {
        let invoice = self
            .invoices
            .find_by_id(req.invoice_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Invoice {} not found", req.invoice_id)))?;

        // A draft has not been issued, so there is no receivable to relieve.
        if invoice.status == InvoiceStatus::DRAFT {
            return Err(SalesError::NotCreditable(invoice.invoice_number).into());
        }

        let invoice_lines = self.invoices.find_lines(invoice.id).await?;
        let already: Vec<(Uuid, i64)> = self.notes.credited_by_invoice_line(invoice.id).await?;

        let note_id = Uuid::new_v4();
        let mut lines = Vec::with_capacity(req.lines.len());
        let mut amounts = Vec::with_capacity(req.lines.len());
        // Tracks this request's own quantities too, so one invoice line appearing
        // twice in the same note cannot slip past the cap.
        let mut running: Vec<(Uuid, i64)> = already.clone();

        for (index, requested) in req.lines.iter().enumerate() {
            let invoice_line = invoice_lines
                .iter()
                .find(|l| l.id == requested.invoice_line_id)
                .ok_or_else(|| SalesError::LineNotOnInvoice(
                    requested.invoice_line_id.to_string(),
                    invoice.invoice_number.clone(),
                ))?;

            let credited = running
                .iter()
                .find(|(id, _)| *id == invoice_line.id)
                .map(|(_, qty)| *qty)
                .unwrap_or(0) as i32;

            if requested.quantity + credited > invoice_line.quantity {
                return Err(SalesError::OverCredit {
                    description: invoice_line.description.clone(),
                    requested: requested.quantity,
                    invoiced: invoice_line.quantity,
                    already: credited,
                }
                .into());
            }

            match running.iter_mut().find(|(id, _)| *id == invoice_line.id) {
                Some((_, qty)) => *qty += requested.quantity as i64,
                None => running.push((invoice_line.id, requested.quantity as i64)),
            }

            // Priced from the invoice line, discount and tax rate included: a
            // credit is worth what was charged, not what the item is worth today.
            let valued = calculate_line(
                requested.quantity,
                invoice_line.unit_price,
                invoice_line.discount_percent,
                invoice_line.tax_rate,
            );

            lines.push(CreditNoteLine {
                id: Uuid::new_v4(),
                credit_note_id: note_id,
                invoice_line_id: invoice_line.id,
                product_id: invoice_line.product_id,
                description: invoice_line.description.clone(),
                quantity: requested.quantity,
                unit_price: invoice_line.unit_price,
                discount_percent: invoice_line.discount_percent,
                tax_rate: invoice_line.tax_rate,
                line_total: valued.net,
                sort_order: index as i32,
            });
            amounts.push(valued);
        }

        let totals = sum_totals(amounts);
        // The invoice's rate, not today's: the receivable was raised at it.
        let currency = DocumentCurrency {
            code: invoice.currency.clone(),
            fx_rate: invoice.fx_rate,
        };

        let note = CreditNote {
            id: note_id,
            org_id: invoice.org_id,
            credit_note_number: self.notes.next_number().await?,
            invoice_id: invoice.id,
            customer_id: invoice.customer_id,
            issue_date: req.issue_date.unwrap_or_else(|| Utc::now().date_naive()),
            reason: req.reason,
            warehouse_id: req.warehouse_id,
            subtotal: totals.subtotal,
            tax_amount: totals.tax_amount,
            total: totals.total,
            currency: currency.code.clone(),
            fx_rate: currency.fx_rate,
            base_total: currency.to_base(totals.total),
            notes: req.notes,
            created_by: user.id,
            created_at: Utc::now(),
        };

        let note = self.notes.create(&note, &lines).await?;

        // Goods coming back, if a warehouse was named. They return at the current
        // moving average — physically indistinguishable from what is already on
        // the shelf, and weighted average has no layer to identify them with.
        let mut returned_cost = Decimal::ZERO;
        if let Some(warehouse_id) = req.warehouse_id {
            let mut movements = Vec::new();
            for line in &lines {
                let Some(product_id) = line.product_id else {
                    continue;
                };

                let movement = StockMovement {
                    id: Uuid::new_v4(),
                    org_id: invoice.org_id,
                    product_id,
                    warehouse_id,
                    to_warehouse_id: None,
                    movement_type: MovementType::IN.to_string(),
                    quantity: line.quantity,
                    unit_cost: None,
                    // Left for the repository to fill from the average. Naming a
                    // cost here would mean un-blending it, which is right for a
                    // purchase return — where the price is a documented fact —
                    // and wrong here.
                    base_unit_cost: None,
                    reference_type: Some(CREDIT_NOTE_REFERENCE.to_string()),
                    reference_id: Some(note.id),
                    notes: Some(format!("Credit note {}", note.credit_note_number)),
                    created_by: user.id,
                    created_at: Utc::now(),
                };

                movements.push(movement);
            }

            // One call, so a credit note puts all of its goods back or none of
            // them. Each is costed at the average as the transaction finds it,
            // which is why the stored movements are read back rather than the
            // ones just built.
            for (stored, _) in self.stock.apply_movements(&[], &movements).await? {
                returned_cost += extended_cost(stored.quantity, stored.base_unit_cost);
            }
        }

        // Settlement before posting, so a reader of the invoice never sees the
        // ledger credited while the balance still says otherwise.
        self.resettle(&invoice).await?;

        self.poster
            .credit_note_issued(&PostableCreditNote {
                id: note.id,
                org_id: note.org_id,
                number: note.credit_note_number.clone(),
                issue_date: note.issue_date,
                fx_rate: note.fx_rate,
                base_total: note.base_total,
                tax_amount: note.tax_amount,
                returned_cost: round_money(returned_cost),
                created_by: note.created_by,
            })
            .await?;

        let lines = self.notes.find_lines(note.id).await?;
        Ok(CreditNoteDetail { credit_note: note, lines })
    }

    pub async fn get(&self, id: Uuid) -> AppResult<CreditNoteDetail> {
        let credit_note = self
            .notes
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Credit note {} not found", id)))?;

        let lines = self.notes.find_lines(id).await?;
        Ok(CreditNoteDetail { credit_note, lines })
    }

    pub async fn list(
        &self,
        filters: &CreditNoteFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<CreditNote>, i64)> {
        self.notes.list(filters, params).await
    }

    /// Recomputes what the invoice is owed, counting credits as well as payments.
    async fn resettle(&self, invoice: &Invoice) -> AppResult<Invoice> {
        let paid = round_money(self.payments.total_paid_for_invoice(invoice.id).await?);
        let credited = round_money(self.notes.total_credited_for_invoice(invoice.id).await?);
        settle_invoice(&self.invoices, invoice, paid, credited, Utc::now().date_naive()).await
    }
}

/// Writes what an invoice is paid and owed, given both of its ledgers.
///
/// A free function because two documents change the answer — a payment and a
/// credit note — and each use case owns only one of the two. Deriving it every
/// time rather than accumulating is what keeps the invoice honest about the
/// documents recorded against it.
async fn settle_invoice<I: InvoiceRepository>(
    invoices: &I,
    invoice: &Invoice,
    paid: Decimal,
    credited: Decimal,
    today: NaiveDate,
) -> AppResult<Invoice> {
    let total = invoice.total.unwrap_or(Decimal::ZERO);

    // A credit relieves the receivable exactly as a payment does. Crediting more
    // than is left outstanding takes this negative — which is the honest answer:
    // the money is owed back to the customer, and it nets against their next
    // invoice. Refunding it is a separate document that does not exist yet.
    let due = round_money(total - credited - paid);

    // Restated at the invoice's own rate, so that paid + due always reconciles
    // against the invoice's base total. The payments themselves were worth
    // something slightly different — that difference is each payment's realised
    // gain or loss, and belongs there rather than smeared into the receivable.
    let base_paid = to_base(paid, invoice.fx_rate);
    let base_due = to_base(due, invoice.fx_rate);

    let status = if invoice.status == InvoiceStatus::CANCELLED {
        // A cancelled invoice is closed; money moving against it is a
        // bookkeeping correction and must not revive the document.
        InvoiceStatus::CANCELLED
    } else if invoice.status == InvoiceStatus::DRAFT {
        // ...and must not *issue* one either. Writing `sent` here would issue an
        // invoice without shipping its goods or recording its revenue, because
        // both of those hang off `set_status` and nothing else. That is what a
        // single unit paid against a draft used to do: the document read `sent`,
        // the shelf was untouched, the books were empty, and the order it came
        // from could then be marked delivered.
        //
        // Unreachable while payments and credit notes both refuse drafts, which
        // is the point — it stays unreachable if a third document ever settles
        // an invoice.
        InvoiceStatus::DRAFT
    } else if due <= Decimal::ZERO {
        // Includes the fully credited case, where nothing is owed either way.
        InvoiceStatus::PAID
    } else if invoice.due_date < today {
        InvoiceStatus::OVERDUE
    } else {
        // Anything still owing and not yet past its due date is a live
        // receivable. Reached by an invoice that was marked paid and had a
        // payment reversed, or a credit undone — it would otherwise stay marked
        // paid while owing the full amount again.
        InvoiceStatus::SENT
    };

    invoices.update_settlement(invoice.id, paid, due, base_paid, base_due, status).await
}

fn build_invoice_lines(
    invoice_id: Uuid,
    requested: &[DocumentLineRequest],
    amounts: &[LineAmounts],
) -> Vec<InvoiceLine> {
    requested
        .iter()
        .zip(amounts)
        .enumerate()
        .map(|(index, (req, amount))| InvoiceLine {
            id: Uuid::new_v4(),
            invoice_id,
            // Raised straight against a customer, so there is no order line
            // behind it. The conversion path builds its own lines and fills this.
            order_line_id: None,
            product_id: req.product_id,
            description: req.description.clone(),
            quantity: req.quantity,
            unit_price: req.unit_price,
            discount_percent: req.discount_percent,
            tax_rate: req.tax_rate,
            line_total: amount.net,
            sort_order: index as i32,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn line(quantity: i32, unit_price: Decimal, discount: Decimal, tax: Decimal) -> DocumentLineRequest {
        DocumentLineRequest {
            product_id: None,
            description: "Widget".to_string(),
            quantity,
            unit_price,
            discount_percent: discount,
            tax_rate: tax,
        }
    }

    #[test]
    fn document_totals_come_from_the_lines() {
        let (amounts, totals) =
            price_lines(&[line(2, dec!(100.00), dec!(0), dec!(20)), line(1, dec!(50.00), dec!(10), dec!(20))]);

        assert_eq!(amounts.len(), 2);
        assert_eq!(totals.subtotal, dec!(245.00));
        assert_eq!(totals.tax_amount, dec!(49.00));
        assert_eq!(totals.total, dec!(294.00));
    }

    #[test]
    fn line_sort_order_follows_request_order() {
        let requested = vec![line(1, dec!(10), dec!(0), dec!(0)), line(1, dec!(20), dec!(0), dec!(0))];
        let (amounts, _) = price_lines(&requested);
        let built = build_quote_lines(Uuid::new_v4(), &requested, &amounts);

        assert_eq!(built[0].sort_order, 0);
        assert_eq!(built[1].sort_order, 1);
        assert_eq!(built[1].unit_price, dec!(20));
    }

    #[test]
    fn line_total_excludes_tax() {
        let requested = vec![line(1, dec!(100.00), dec!(0), dec!(20))];
        let (amounts, totals) = price_lines(&requested);
        let built = build_invoice_lines(Uuid::new_v4(), &requested, &amounts);

        assert_eq!(built[0].line_total, dec!(100.00));
        assert_eq!(totals.total, dec!(120.00));
    }
}
