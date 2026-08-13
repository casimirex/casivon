use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::modules::inventory::domain::entities::{MovementType, ProductType, StockMovement};
use crate::modules::inventory::domain::errors::InventoryError;
use crate::modules::inventory::domain::repositories::{ProductRepository, StockRepository};
use crate::modules::purchasing::application::dto::*;
use crate::modules::purchasing::domain::entities::*;
use crate::modules::purchasing::domain::errors::PurchasingError;
use crate::modules::purchasing::domain::repositories::*;
use crate::shared::auth::CurrentUser;
use crate::shared::currency::{CurrencyResolver, DocumentCurrency};
use crate::shared::money::{calculate_line, round_money, sum_totals, to_base, LineAmounts};
use crate::shared::pagination::PaginationParams;
use crate::shared::posting::{DocumentPoster, PostablePayment, PostableReceipt, PostableReturn};

/// Tags the stock movements a receipt produces, so they can be traced back.
const RECEIPT_REFERENCE: &str = "goods_receipt";

pub struct VendorUseCases<V: VendorRepository> {
    vendors: V,    fx: Arc<dyn CurrencyResolver>,
}

impl<V: VendorRepository> VendorUseCases<V> {
    pub fn new(vendors: V, fx: Arc<dyn CurrencyResolver>) -> Self {
        Self { vendors, fx }
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

    pub async fn create(&self, req: CreateVendorRequest, user: &CurrentUser) -> AppResult<Vendor> {
        let now = Utc::now();
        let vendor = Vendor {
            id: Uuid::new_v4(),
            org_id: user.org_id,
            name: req.name,
            legal_name: req.legal_name,
            tax_id: req.tax_id,
            email: req.email,
            phone: req.phone,
            address: req.address,
            city: req.city,
            country: req.country,
            payment_terms: req.payment_terms,
            // A vendor carries no amount of its own — this is the currency its
            // purchase orders default to. Resolved against today only to refuse
            // a currency nothing could be restated from; the rate is not kept.
            currency: self.currency(req.currency.clone(), Utc::now().date_naive()).await?.code,
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
        };
        self.vendors.create(&vendor).await
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Vendor> {
        self.vendors
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Vendor {} not found", id)))
    }

    pub async fn list(
        &self,
        filters: &VendorFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Vendor>, i64)> {
        self.vendors.list(filters, params).await
    }

    pub async fn update(&self, id: Uuid, req: UpdateVendorRequest) -> AppResult<Vendor> {
        let mut vendor = self.get(id).await?;

        if let Some(v) = req.name {
            vendor.name = v;
        }
        if req.legal_name.is_some() {
            vendor.legal_name = req.legal_name;
        }
        if req.tax_id.is_some() {
            vendor.tax_id = req.tax_id;
        }
        if req.email.is_some() {
            vendor.email = req.email;
        }
        if req.phone.is_some() {
            vendor.phone = req.phone;
        }
        if req.address.is_some() {
            vendor.address = req.address;
        }
        if req.city.is_some() {
            vendor.city = req.city;
        }
        if req.country.is_some() {
            vendor.country = req.country;
        }
        if req.payment_terms.is_some() {
            vendor.payment_terms = req.payment_terms;
        }
        if let Some(v) = req.currency {
            vendor.currency = v;
        }
        if let Some(v) = req.status {
            vendor.status = v;
        }
        vendor.updated_at = Utc::now();

        self.vendors.update(&vendor).await
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        self.get(id).await?;
        self.vendors.delete(id).await
    }
}

pub struct PurchaseOrderUseCases<P: PurchaseOrderRepository, V: VendorRepository> {
    orders: P,
    vendors: V,    fx: Arc<dyn CurrencyResolver>,
}

impl<P: PurchaseOrderRepository, V: VendorRepository> PurchaseOrderUseCases<P, V> {
    pub fn new(orders: P, vendors: V, fx: Arc<dyn CurrencyResolver>) -> Self {
        Self { orders, vendors, fx }
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
        req: CreatePurchaseOrderRequest,
        user: &CurrentUser,
    ) -> AppResult<PurchaseOrderDetail> {
        let vendor = self
            .vendors
            .find_by_id(req.vendor_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Vendor {} not found", req.vendor_id)))?;

        if vendor.status != "active" {
            return Err(PurchasingError::VendorInactive(vendor.name).into());
        }

        let po_id = Uuid::new_v4();
        let (amounts, totals) = price_lines(&req.lines);
        let now = Utc::now();
        let currency = self.currency(req.currency.clone(), req.order_date).await?;

        let order = PurchaseOrder {
            id: po_id,
            org_id: user.org_id,
            po_number: self.orders.next_number().await?,
            vendor_id: req.vendor_id,
            status: PurchaseOrderStatus::DRAFT.to_string(),
            order_date: req.order_date,
            expected_date: req.expected_date,
            shipping_address: req.shipping_address,
            subtotal: Some(totals.subtotal),
            tax_amount: Some(totals.tax_amount),
            total: Some(totals.total),
            base_total: Some(currency.to_base(totals.total)),
            // Nothing has been paid yet, so the whole order is owed.
            amount_paid: Decimal::ZERO,
            amount_due: Some(totals.total),
            base_amount_paid: Decimal::ZERO,
            base_amount_due: Some(currency.to_base(totals.total)),
            fx_rate: currency.fx_rate,
            currency: currency.code,
            notes: req.notes,
            created_by: user.id,
            created_at: now,
            updated_at: now,
        };

        let lines = build_lines(po_id, &req.lines, &amounts, &[]);
        let order = self.orders.create(&order, &lines).await?;
        self.detail(order).await
    }

    pub async fn get(&self, id: Uuid) -> AppResult<PurchaseOrderDetail> {
        let order = self.require_order(id).await?;
        self.detail(order).await
    }

    pub async fn list(
        &self,
        filters: &PurchaseOrderFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<PurchaseOrder>, i64)> {
        self.orders.list(filters, params).await
    }

    pub async fn update(
        &self,
        id: Uuid,
        req: UpdatePurchaseOrderRequest,
    ) -> AppResult<PurchaseOrderDetail> {
        let mut order = self.require_order(id).await?;

        if !PurchaseOrderStatus::is_editable(&order.status) {
            return Err(PurchasingError::NotEditable(order.po_number).into());
        }

        if let Some(vendor_id) = req.vendor_id {
            order.vendor_id = vendor_id;
        }
        if req.expected_date.is_some() {
            order.expected_date = req.expected_date;
        }
        if req.shipping_address.is_some() {
            order.shipping_address = req.shipping_address;
        }
        if req.notes.is_some() {
            order.notes = req.notes;
        }
        order.updated_at = Utc::now();

        let new_lines = match &req.lines {
            Some(requested) => {
                if requested.is_empty() {
                    return Err(PurchasingError::NoLines.into());
                }
                let (amounts, totals) = price_lines(requested);
                order.subtotal = Some(totals.subtotal);
                order.tax_amount = Some(totals.tax_amount);
                order.total = Some(totals.total);

                // Preserve what has already been received against surviving lines.
                let existing = self.orders.find_lines(order.id).await?;
                Some(build_lines(order.id, requested, &amounts, &existing))
            }
            None => None,
        };

        let currency = self.currency(Some(order.currency.clone()), order.order_date).await?;
        order.fx_rate = currency.fx_rate;
        order.base_total = currency.to_base_opt(order.total);

        // Re-pricing an order changes what is owed on it. Only unreceived orders
        // reach here, and nothing can have been paid against a draft, so the
        // whole new total is outstanding.
        order.amount_due = order.total.map(|total| round_money(total - order.amount_paid));
        order.base_amount_due = order.amount_due.map(|due| to_base(due, currency.fx_rate));

        let order = self.orders.update(&order, new_lines.as_deref()).await?;
        self.detail(order).await
    }

    pub async fn set_status(&self, id: Uuid, status: &str) -> AppResult<PurchaseOrder> {
        let order = self.require_order(id).await?;

        if !PurchaseOrderStatus::can_transition(&order.status, status) {
            return Err(PurchasingError::InvalidTransition {
                from: order.status,
                to: status.to_string(),
            }
            .into());
        }

        self.orders.update_status(id, status).await
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        let order = self.require_order(id).await?;

        if order.status != PurchaseOrderStatus::DRAFT {
            return Err(PurchasingError::NotEditable(order.po_number).into());
        }

        self.orders.delete(id).await
    }

    async fn detail(&self, order: PurchaseOrder) -> AppResult<PurchaseOrderDetail> {
        let lines = self.orders.find_lines(order.id).await?;
        Ok(PurchaseOrderDetail {
            order,
            lines: lines.into_iter().map(PurchaseOrderLineView::from).collect(),
        })
    }

    async fn require_order(&self, id: Uuid) -> AppResult<PurchaseOrder> {
        self.orders
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Purchase order {} not found", id)))
    }
}

pub struct GoodsReceiptUseCases<G, P, S, R>
where
    G: GoodsReceiptRepository,
    P: PurchaseOrderRepository,
    S: StockRepository,
    R: ProductRepository,
{
    receipts: G,
    orders: P,
    stock: S,
    /// Read only to ask what kind of thing arrived. A service line is received
    /// like anything else but holds no stock, and under perpetual costing that
    /// is the difference between an asset and an expense.
    products: R,
    poster: Arc<dyn DocumentPoster>,
}

impl<G, P, S, R> GoodsReceiptUseCases<G, P, S, R>
where
    G: GoodsReceiptRepository,
    P: PurchaseOrderRepository,
    S: StockRepository,
    R: ProductRepository,
{
    pub fn new(
        receipts: G,
        orders: P,
        stock: S,
        products: R,
        poster: Arc<dyn DocumentPoster>,
    ) -> Self {
        Self { receipts, orders, stock, products, poster }
    }

    /// Books goods in against a purchase order: records the receipt, advances the
    /// PO's received quantities and status, and moves the stock into a warehouse.
    pub async fn create(
        &self,
        req: CreateGoodsReceiptRequest,
        user: &CurrentUser,
    ) -> AppResult<GoodsReceiptDetail> {
        if req.lines.is_empty() {
            return Err(PurchasingError::EmptyReceipt.into());
        }

        let order = self
            .orders
            .find_by_id(req.po_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Purchase order {} not found", req.po_id)))?;

        if !PurchaseOrderStatus::accepts_receipt(&order.status) {
            return Err(
                PurchasingError::NotReceivable(order.po_number, order.status.clone()).into()
            );
        }

        let mut po_lines = self.orders.find_lines(order.id).await?;
        let receipt_id = Uuid::new_v4();
        let mut receipt_lines = Vec::with_capacity(req.lines.len());

        for requested in &req.lines {
            if requested.quantity_received <= 0 {
                return Err(PurchasingError::NonPositiveQuantity.into());
            }

            let po_line = po_lines
                .iter_mut()
                .find(|l| l.id == requested.po_line_id)
                .ok_or_else(|| PurchasingError::LineNotOnOrder(
                    requested.po_line_id.to_string(),
                    order.po_number.clone(),
                ))?;

            let outstanding = po_line.outstanding();
            if requested.quantity_received > outstanding {
                return Err(PurchasingError::OverReceipt {
                    description: po_line.description.clone(),
                    requested: requested.quantity_received,
                    outstanding,
                }
                .into());
            }

            // Applied locally so the next iteration sees the running total, in case
            // one PO line appears twice in the same receipt.
            po_line.received_quantity += requested.quantity_received;

            receipt_lines.push(GoodsReceiptLine {
                id: Uuid::new_v4(),
                receipt_id,
                po_line_id: po_line.id,
                product_id: po_line.product_id,
                quantity_received: requested.quantity_received,
                notes: requested.notes.clone(),
            });
        }

        let all_complete = po_lines.iter().all(PurchaseOrderLine::is_fully_received);
        let new_status = PurchaseOrderStatus::after_receipt(all_complete);

        let receipt = GoodsReceipt {
            id: receipt_id,
            org_id: order.org_id,
            po_id: order.id,
            receipt_number: self.receipts.next_number().await?,
            receipt_date: req.receipt_date.unwrap_or_else(|| Utc::now().date_naive()),
            status: "received".to_string(),
            warehouse_id: Some(req.warehouse_id),
            notes: req.notes,
            created_by: user.id,
            created_at: Utc::now(),
        };

        let receipt = self.receipts.create(&receipt, &receipt_lines, new_status).await?;

        // Stock only exists for lines that name a *stocked* product; free-text
        // lines (freight) and service products are received but hold no
        // inventory. The product type mattered less when everything was a cost
        // on arrival; under perpetual costing it decides asset from expense.
        let mut stocked_lines = Vec::new();
        let mut movements = Vec::new();
        for line in &receipt_lines {
            let Some(product_id) = line.product_id else {
                continue;
            };
            let Some(product) = self.products.find_by_id(product_id).await? else {
                continue;
            };
            if !ProductType::is_stocked(&product.product_type) {
                continue;
            }
            stocked_lines.push(line.id);

            let movement = StockMovement {
                id: Uuid::new_v4(),
                org_id: order.org_id,
                product_id,
                warehouse_id: req.warehouse_id,
                to_warehouse_id: None,
                movement_type: MovementType::IN.to_string(),
                quantity: line.quantity_received,
                unit_cost: po_lines
                    .iter()
                    .find(|l| l.id == line.po_line_id)
                    .map(|l| l.unit_price),
                // The same price restated, because stock is valued and posted in
                // the base currency while `unit_cost` above is whatever the
                // order was placed in. Restated at the *order's* rate, matching
                // what the ledger posts for this delivery.
                base_unit_cost: po_lines
                    .iter()
                    .find(|l| l.id == line.po_line_id)
                    .map(|l| to_base(l.unit_price, order.fx_rate)),
                reference_type: Some(RECEIPT_REFERENCE.to_string()),
                reference_id: Some(receipt.id),
                notes: Some(format!("Goods receipt {}", receipt.receipt_number)),
                created_by: user.id,
                created_at: Utc::now(),
            };

            movements.push(movement);
        }

        // One call: a delivery lands whole or not at all. Arriving stock cannot
        // be refused for want of shelf space, so the failure this rules out is a
        // database error partway through a multi-line receipt.
        self.stock.apply_movements(&[], &movements).await?;

        // Valued from the order's line prices, because a receipt records
        // quantities and no money of its own. Restated at the order's rate: a
        // delivery is worth what the order committed to, not what the rate
        // happens to be the day the van arrives.
        // Split the same way the movements above were: what became stock, and
        // what was a cost the moment it arrived. Under periodic costing the two
        // are added back together by `receipt_entries`, so this split costs
        // nothing until somebody opts in.
        let received = receipt_lines.iter().fold(
            (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO),
            |(stocked, expensed, tax), line| {
                let Some(po_line) = po_lines.iter().find(|l| l.id == line.po_line_id) else {
                    return (stocked, expensed, tax);
                };
                let valued = calculate_line(
                    line.quantity_received,
                    po_line.unit_price,
                    Decimal::ZERO,
                    po_line.tax_rate,
                );

                if stocked_lines.contains(&line.id) {
                    (stocked + valued.net, expensed, tax + valued.tax)
                } else {
                    (stocked, expensed + valued.net, tax + valued.tax)
                }
            },
        );

        self.poster
            .goods_received(&PostableReceipt {
                id: receipt.id,
                org_id: receipt.org_id,
                number: receipt.receipt_number.clone(),
                receipt_date: receipt.receipt_date,
                fx_rate: order.fx_rate,
                stocked_net: received.0,
                expensed_net: received.1,
                tax: received.2,
                created_by: receipt.created_by,
            })
            .await?;

        let lines = self.receipts.find_lines(receipt.id).await?;
        Ok(GoodsReceiptDetail { receipt, lines, order_status: new_status.to_string() })
    }

    pub async fn get(&self, id: Uuid) -> AppResult<GoodsReceiptDetail> {
        let receipt = self
            .receipts
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Goods receipt {} not found", id)))?;

        let order_status = self
            .orders
            .find_by_id(receipt.po_id)
            .await?
            .map(|o| o.status)
            .unwrap_or_else(|| "unknown".to_string());

        let lines = self.receipts.find_lines(id).await?;
        Ok(GoodsReceiptDetail { receipt, lines, order_status })
    }

    pub async fn list(
        &self,
        filters: &GoodsReceiptFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<GoodsReceipt>, i64)> {
        self.receipts.list(filters, params).await
    }
}

/// Tags the stock movements a return produces, so they can be traced back — and
/// so `movement_entries` knows the return has already posted its own inventory
/// leg and must not post a second one.
const RETURN_REFERENCE: &str = "purchase_return";

pub struct PurchaseReturnUseCases<T, P, S, R, W>
where
    T: PurchaseReturnRepository,
    P: PurchaseOrderRepository,
    S: StockRepository,
    R: ProductRepository,
    W: VendorPaymentRepository,
{
    returns: T,
    orders: P,
    stock: S,
    products: R,
    /// Read only to re-settle the order: what is owed is the total less what has
    /// gone back *and* less what has been paid, so a return cannot work out the
    /// new figure without seeing the payment ledger too.
    payments: W,
    poster: Arc<dyn DocumentPoster>,
}

impl<T, P, S, R, W> PurchaseReturnUseCases<T, P, S, R, W>
where
    T: PurchaseReturnRepository,
    P: PurchaseOrderRepository,
    S: StockRepository,
    R: ProductRepository,
    W: VendorPaymentRepository,
{
    pub fn new(
        returns: T,
        orders: P,
        stock: S,
        products: R,
        payments: W,
        poster: Arc<dyn DocumentPoster>,
    ) -> Self {
        Self { returns, orders, stock, products, payments, poster }
    }

    /// Sends goods back to the supplier: records the return, takes the stock off
    /// the shelf, reduces what the order says is owed, and credits the ledger.
    ///
    /// The mirror of `GoodsReceiptUseCases::create` throughout, which is not
    /// tidiness for its own sake — valuing the return at the order's own line
    /// price is what makes the debit to payables and the credit to inventory the
    /// same number, so no variance account is needed.
    pub async fn create(
        &self,
        req: CreatePurchaseReturnRequest,
        user: &CurrentUser,
    ) -> AppResult<PurchaseReturnDetail> {
        let order = self
            .orders
            .find_by_id(req.po_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Purchase order {} not found", req.po_id)))?;

        if !PurchaseOrderStatus::accepts_return(&order.status) {
            return Err(PurchasingError::NothingToReturn(order.po_number).into());
        }

        let mut po_lines = self.orders.find_lines(order.id).await?;
        let return_id = Uuid::new_v4();
        let mut return_lines = Vec::with_capacity(req.lines.len());

        for requested in &req.lines {
            if requested.quantity_returned <= 0 {
                return Err(PurchasingError::NonPositiveQuantity.into());
            }

            let po_line = po_lines
                .iter_mut()
                .find(|l| l.id == requested.po_line_id)
                .ok_or_else(|| PurchasingError::LineNotOnOrder(
                    requested.po_line_id.to_string(),
                    order.po_number.clone(),
                ))?;

            // `received_quantity` is already net of anything sent back before,
            // so it is exactly how many are here to send back now.
            if requested.quantity_returned > po_line.received_quantity {
                return Err(PurchasingError::OverReturn {
                    description: po_line.description.clone(),
                    requested: requested.quantity_returned,
                    received: po_line.received_quantity,
                }
                .into());
            }

            // Applied locally so the next iteration sees the running total, in
            // case one PO line appears twice in the same return.
            po_line.received_quantity -= requested.quantity_returned;

            return_lines.push(PurchaseReturnLine {
                id: Uuid::new_v4(),
                return_id,
                po_line_id: po_line.id,
                product_id: po_line.product_id,
                quantity_returned: requested.quantity_returned,
                notes: requested.notes.clone(),
            });
        }

        // Which lines hold stock, and — before anything is written — whether the
        // stock is actually there to send back. The receipt path can skip this
        // check because nothing ever leaves through it; a return can be asked to
        // send back goods that have since been sold.
        let mut stocked_lines = Vec::new();
        for line in &return_lines {
            let Some(product_id) = line.product_id else {
                continue;
            };
            let Some(product) = self.products.find_by_id(product_id).await? else {
                continue;
            };
            if !ProductType::is_stocked(&product.product_type) {
                continue;
            }

            let available = self
                .stock
                .find_level(product_id, req.warehouse_id)
                .await?
                .map(|l| l.available())
                .unwrap_or(0);

            if available < line.quantity_returned {
                return Err(InventoryError::InsufficientStock {
                    sku: product.sku,
                    warehouse: req.warehouse_id.to_string(),
                    available,
                    requested: line.quantity_returned,
                }
                .into());
            }

            stocked_lines.push(line.id);
        }

        let all_complete = po_lines.iter().all(PurchaseOrderLine::is_fully_received);
        let new_status = PurchaseOrderStatus::after_receipt(all_complete);

        let purchase_return = PurchaseReturn {
            id: return_id,
            org_id: order.org_id,
            po_id: order.id,
            return_number: self.returns.next_number().await?,
            return_date: req.return_date.unwrap_or_else(|| Utc::now().date_naive()),
            warehouse_id: Some(req.warehouse_id),
            reason: req.reason,
            notes: req.notes,
            created_by: user.id,
            created_at: Utc::now(),
        };

        let purchase_return = self.returns.create(&purchase_return, &return_lines, new_status).await?;

        let mut movements = Vec::new();
        for line in &return_lines {
            if !stocked_lines.contains(&line.id) {
                continue;
            }
            let Some(product_id) = line.product_id else {
                continue;
            };
            let po_line = po_lines.iter().find(|l| l.id == line.po_line_id);

            let movement = StockMovement {
                id: Uuid::new_v4(),
                org_id: order.org_id,
                product_id,
                warehouse_id: req.warehouse_id,
                to_warehouse_id: None,
                movement_type: MovementType::OUT.to_string(),
                quantity: line.quantity_returned,
                unit_cost: po_line.map(|l| l.unit_price),
                // Named explicitly, and that is what tells the stock repository
                // this is goods going back at the price they arrived at rather
                // than a sale consuming at the running average. The average is
                // un-blended to match, so the valuation report and the Inventory
                // account stay the same number.
                base_unit_cost: po_line.map(|l| to_base(l.unit_price, order.fx_rate)),
                reference_type: Some(RETURN_REFERENCE.to_string()),
                reference_id: Some(purchase_return.id),
                notes: Some(format!("Purchase return {}", purchase_return.return_number)),
                created_by: user.id,
                created_at: Utc::now(),
            };

            movements.push(movement);
        }

        // One call: goods go back whole or not at all. This return moves stock
        // *out*, so a second line the shelf cannot cover is an ordinary refusal
        // rather than a database failure — and it used to send the first line's
        // goods back regardless.
        self.stock.apply_movements(&[], &movements).await?;

        // Valued from the order's line prices and split the same way the receipt
        // split them, because the return has to undo exactly what the receipt did.
        let returned = return_lines.iter().fold(
            (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO),
            |(stocked, expensed, tax), line| {
                let Some(po_line) = po_lines.iter().find(|l| l.id == line.po_line_id) else {
                    return (stocked, expensed, tax);
                };
                let valued = calculate_line(
                    line.quantity_returned,
                    po_line.unit_price,
                    Decimal::ZERO,
                    po_line.tax_rate,
                );

                if stocked_lines.contains(&line.id) {
                    (stocked + valued.net, expensed, tax + valued.tax)
                } else {
                    (stocked, expensed + valued.net, tax + valued.tax)
                }
            },
        );

        self.poster
            .goods_returned(&PostableReturn {
                id: purchase_return.id,
                org_id: purchase_return.org_id,
                number: purchase_return.return_number.clone(),
                return_date: purchase_return.return_date,
                fx_rate: order.fx_rate,
                stocked_net: returned.0,
                expensed_net: returned.1,
                tax: returned.2,
                created_by: purchase_return.created_by,
            })
            .await?;

        // The half a hand-made stock adjustment could never do: goods going back
        // reduce the debt as surely as money paid does.
        let paid = round_money(self.payments.total_paid_for_order(order.id).await?);
        let returned_total = round_money(self.returns.total_returned_for_order(order.id).await?);
        resettle_order(&self.orders, &order, paid, returned_total).await?;

        let lines = self.returns.find_lines(purchase_return.id).await?;
        Ok(PurchaseReturnDetail {
            purchase_return,
            lines,
            order_status: new_status.to_string(),
        })
    }

    pub async fn get(&self, id: Uuid) -> AppResult<PurchaseReturnDetail> {
        let purchase_return = self
            .returns
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Purchase return {} not found", id)))?;

        let order_status = self
            .orders
            .find_by_id(purchase_return.po_id)
            .await?
            .map(|o| o.status)
            .unwrap_or_else(|| "unknown".to_string());

        let lines = self.returns.find_lines(id).await?;
        Ok(PurchaseReturnDetail { purchase_return, lines, order_status })
    }

    pub async fn list(
        &self,
        filters: &PurchaseReturnFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<PurchaseReturn>, i64)> {
        self.returns.list(filters, params).await
    }
}

/// Recomputes what is paid and what is owed on an order from its own ledgers.
///
/// A free function because two documents change the answer — a payment and a
/// return — and each use case owns only one of the two ledgers. Deriving the
/// figure every time rather than accumulating it is what stops the order drifting
/// away from the documents recorded against it.
async fn resettle_order<P: PurchaseOrderRepository>(
    orders: &P,
    order: &PurchaseOrder,
    paid: Decimal,
    returned: Decimal,
) -> AppResult<PurchaseOrder> {
    let total = order.total.unwrap_or(Decimal::ZERO);

    // Goods sent back are a credit against the order, exactly as money paid is.
    // Returning something already paid for takes this negative, which is the
    // honest answer: the supplier owes you, and it nets against the next
    // purchase. Getting the money back is a refund, which does not exist yet.
    let due = round_money(total - returned - paid);

    // At the order's own rate, so paid and due reconcile against its base total.
    // What each payment actually cost is on the payment, along with the
    // difference between the two.
    orders
        .update_settlement(
            order.id,
            paid,
            due,
            to_base(paid, order.fx_rate),
            to_base(due, order.fx_rate),
        )
        .await
}

fn price_lines(
    lines: &[PurchaseOrderLineRequest],
) -> (Vec<LineAmounts>, crate::shared::money::DocumentTotals) {
    let amounts: Vec<LineAmounts> = lines
        .iter()
        .map(|l| calculate_line(l.quantity, l.unit_price, rust_decimal::Decimal::ZERO, l.tax_rate))
        .collect();
    let totals = sum_totals(amounts.iter().copied());
    (amounts, totals)
}

/// Rebuilds the line set, carrying `received_quantity` over from any existing
/// line describing the same product so an edit cannot erase receiving history.
fn build_lines(
    po_id: Uuid,
    requested: &[PurchaseOrderLineRequest],
    amounts: &[LineAmounts],
    existing: &[PurchaseOrderLine],
) -> Vec<PurchaseOrderLine> {
    requested
        .iter()
        .zip(amounts)
        .enumerate()
        .map(|(index, (req, amount))| {
            let received = existing
                .iter()
                .find(|l| l.product_id == req.product_id && l.description == req.description)
                .map(|l| l.received_quantity)
                .unwrap_or(0);

            PurchaseOrderLine {
                id: Uuid::new_v4(),
                po_id,
                product_id: req.product_id,
                description: req.description.clone(),
                quantity: req.quantity,
                unit_price: req.unit_price,
                tax_rate: req.tax_rate,
                received_quantity: received,
                line_total: amount.net,
                sort_order: index as i32,
            }
        })
        .collect()
}

// --------------------------------------------------------- vendor payments

pub struct VendorPaymentUseCases<
    W: VendorPaymentRepository,
    P: PurchaseOrderRepository,
    T: PurchaseReturnRepository,
> {
    payments: W,
    orders: P,
    /// Read only to know what has gone back. Goods returned reduce the debt just
    /// as surely as money paid does, so settlement cannot be derived from the
    /// payment ledger alone any more.
    returns: T,
    fx: Arc<dyn CurrencyResolver>,
    poster: Arc<dyn DocumentPoster>,
}

impl<W: VendorPaymentRepository, P: PurchaseOrderRepository, T: PurchaseReturnRepository>
    VendorPaymentUseCases<W, P, T>
{
    pub fn new(
        payments: W,
        orders: P,
        returns: T,
        fx: Arc<dyn CurrencyResolver>,
        poster: Arc<dyn DocumentPoster>,
    ) -> Self {
        Self { payments, orders, returns, fx, poster }
    }

    /// Records a payment and re-settles the order from its payment ledger.
    ///
    /// Deliberately the same shape as `InvoiceUseCases::record_payment`: the two
    /// are one idea pointing in opposite directions, and the rules that matter —
    /// no overpayment, the rate on the day the money moved, settlement derived
    /// rather than accumulated — are the same rules.
    pub async fn record(
        &self,
        req: RecordVendorPaymentRequest,
        user: &CurrentUser,
    ) -> AppResult<VendorPayment> {
        let order = self.require_order(req.po_id).await?;

        if !PAYMENT_METHODS.contains(&req.payment_method.as_str()) {
            return Err(PurchasingError::UnsupportedPaymentMethod(req.payment_method).into());
        }

        if req.amount <= Decimal::ZERO {
            return Err(AppError::Validation("Payment amount must be positive".to_string()));
        }

        // A draft has not been committed to, so there is nothing owed on it yet.
        if order.status == PurchaseOrderStatus::DRAFT {
            return Err(PurchasingError::NotPayable(order.po_number).into());
        }

        let already_paid = self.payments.total_paid_for_order(order.id).await?;
        let returned = self.returns.total_returned_for_order(order.id).await?;
        let total = order.total.unwrap_or(Decimal::ZERO);
        // Net of what went back: you cannot pay for goods you have sent back.
        let outstanding = round_money(total - returned - already_paid);

        if req.amount > outstanding {
            return Err(PurchasingError::PaymentExceedsBalance(
                req.amount.to_string(),
                outstanding.to_string(),
                order.po_number,
            )
            .into());
        }

        let amount = round_money(req.amount);

        // Paid in the order's currency — you settle a debt in the currency it
        // was struck in — but at the rate on the day the money left.
        let currency =
            self.fx.resolve(Some(&order.currency), req.payment_date).await?;
        let base_amount = currency.to_base(amount);

        // What the order committed this slice of the debt to costing. Settling
        // for less than that is a gain: the money went further than expected.
        let committed = to_base(amount, order.fx_rate);
        let fx_gain_loss = committed - base_amount;

        let payment = VendorPayment {
            id: Uuid::new_v4(),
            org_id: order.org_id,
            po_id: order.id,
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
        self.resettle(&order).await?;
        self.poster.vendor_payment_made(&Self::postable(&payment, &order)).await?;
        Ok(payment)
    }

    /// Reverses a payment and re-settles the order.
    pub async fn delete(&self, payment_id: Uuid) -> AppResult<()> {
        let payment = self
            .payments
            .find_by_id(payment_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Vendor payment {} not found", payment_id)))?;

        let order = self.require_order(payment.po_id).await?;

        // Reversed before the row goes, because the mirror is derived from it.
        self.poster.vendor_payment_reversed(&Self::postable(&payment, &order)).await?;

        self.payments.delete(payment_id).await?;
        self.resettle(&order).await?;
        Ok(())
    }

    pub async fn list(
        &self,
        filters: &VendorPaymentFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<VendorPayment>, i64)> {
        self.payments.list(filters, params).await
    }

    /// Recomputes what is paid and what is owed from the payment ledger, so the
    /// order can never drift away from the payments recorded against it.
    async fn resettle(&self, order: &PurchaseOrder) -> AppResult<PurchaseOrder> {
        let paid = round_money(self.payments.total_paid_for_order(order.id).await?);
        let returned = round_money(self.returns.total_returned_for_order(order.id).await?);
        resettle_order(&self.orders, order, paid, returned).await
    }

    fn postable(payment: &VendorPayment, order: &PurchaseOrder) -> PostablePayment {
        PostablePayment {
            id: payment.id,
            org_id: payment.org_id,
            document_number: order.po_number.clone(),
            payment_date: payment.payment_date,
            base_amount: payment.base_amount,
            fx_gain_loss: payment.fx_gain_loss,
            created_by: payment.created_by,
        }
    }

    async fn require_order(&self, id: Uuid) -> AppResult<PurchaseOrder> {
        self.orders
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Purchase order {} not found", id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn request(description: &str, quantity: i32, price: rust_decimal::Decimal) -> PurchaseOrderLineRequest {
        PurchaseOrderLineRequest {
            product_id: None,
            description: description.to_string(),
            quantity,
            unit_price: price,
            tax_rate: dec!(0),
        }
    }

    #[test]
    fn totals_include_tax_but_line_totals_do_not() {
        let lines = vec![PurchaseOrderLineRequest {
            tax_rate: dec!(20),
            ..request("Steel", 10, dec!(5.00))
        }];
        let (amounts, totals) = price_lines(&lines);

        assert_eq!(amounts[0].net, dec!(50.00));
        assert_eq!(totals.subtotal, dec!(50.00));
        assert_eq!(totals.tax_amount, dec!(10.00));
        assert_eq!(totals.total, dec!(60.00));
    }

    #[test]
    fn editing_lines_preserves_received_quantities() {
        let po_id = Uuid::new_v4();
        let existing = vec![PurchaseOrderLine {
            id: Uuid::new_v4(),
            po_id,
            product_id: None,
            description: "Steel".to_string(),
            quantity: 10,
            unit_price: dec!(5.00),
            tax_rate: dec!(20),
            received_quantity: 4,
            line_total: dec!(50.00),
            sort_order: 0,
        }];

        let requested = vec![request("Steel", 20, dec!(5.00)), request("Bolts", 5, dec!(1.00))];
        let (amounts, _) = price_lines(&requested);
        let rebuilt = build_lines(po_id, &requested, &amounts, &existing);

        assert_eq!(rebuilt[0].received_quantity, 4, "matching line keeps its history");
        assert_eq!(rebuilt[0].quantity, 20);
        assert_eq!(rebuilt[1].received_quantity, 0, "new line starts empty");
    }
}
