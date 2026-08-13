use async_trait::async_trait;
use utoipa::IntoParams;
use chrono::NaiveDate;
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::purchasing::domain::entities::*;
use crate::shared::pagination::PaginationParams;

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct VendorFilters {
    pub status: Option<String>,
    pub country: Option<String>,
    pub search: Option<String>,
}

#[async_trait]
pub trait VendorRepository: Send + Sync {
    async fn create(&self, vendor: &Vendor) -> AppResult<Vendor>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Vendor>>;
    async fn update(&self, vendor: &Vendor) -> AppResult<Vendor>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &VendorFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Vendor>, i64)>;
}

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PurchaseOrderFilters {
    pub status: Option<String>,
    pub vendor_id: Option<Uuid>,
    pub search: Option<String>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
}

#[async_trait]
pub trait PurchaseOrderRepository: Send + Sync {
    async fn create(
        &self,
        order: &PurchaseOrder,
        lines: &[PurchaseOrderLine],
    ) -> AppResult<PurchaseOrder>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<PurchaseOrder>>;
    async fn find_lines(&self, po_id: Uuid) -> AppResult<Vec<PurchaseOrderLine>>;
    async fn update(
        &self,
        order: &PurchaseOrder,
        lines: Option<&[PurchaseOrderLine]>,
    ) -> AppResult<PurchaseOrder>;
    async fn update_status(&self, id: Uuid, status: &str) -> AppResult<PurchaseOrder>;
    /// Writes the settlement columns after a vendor payment is recorded or
    /// removed. `base_amount_paid` and `base_amount_due` are restated at the
    /// *order's* rate, so the two always reconcile against its base total.
    async fn update_settlement(
        &self,
        id: Uuid,
        amount_paid: rust_decimal::Decimal,
        amount_due: rust_decimal::Decimal,
        base_amount_paid: rust_decimal::Decimal,
        base_amount_due: rust_decimal::Decimal,
    ) -> AppResult<PurchaseOrder>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &PurchaseOrderFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<PurchaseOrder>, i64)>;
    async fn next_number(&self) -> AppResult<String>;
}

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct VendorPaymentFilters {
    pub po_id: Option<Uuid>,
    pub payment_method: Option<String>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
}

#[async_trait]
pub trait VendorPaymentRepository: Send + Sync {
    async fn create(&self, payment: &VendorPayment) -> AppResult<VendorPayment>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<VendorPayment>>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &VendorPaymentFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<VendorPayment>, i64)>;
    /// In the order's own currency — every payment against an order is in it,
    /// and this total is compared against the order's `total` to decide what is
    /// still outstanding.
    async fn total_paid_for_order(&self, po_id: Uuid) -> AppResult<rust_decimal::Decimal>;
}

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GoodsReceiptFilters {
    pub po_id: Option<Uuid>,
    pub warehouse_id: Option<Uuid>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
}

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PurchaseReturnFilters {
    pub po_id: Option<Uuid>,
    pub warehouse_id: Option<Uuid>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
}

#[async_trait]
pub trait GoodsReceiptRepository: Send + Sync {
    /// Writes the receipt, its lines, the `received_quantity` increments on the
    /// PO lines and the new PO status in a single transaction. Receiving stock
    /// must not be able to half-happen.
    async fn create(
        &self,
        receipt: &GoodsReceipt,
        lines: &[GoodsReceiptLine],
        new_order_status: &str,
    ) -> AppResult<GoodsReceipt>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<GoodsReceipt>>;
    async fn find_lines(&self, receipt_id: Uuid) -> AppResult<Vec<GoodsReceiptLine>>;
    async fn list(
        &self,
        filters: &GoodsReceiptFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<GoodsReceipt>, i64)>;
    async fn next_number(&self) -> AppResult<String>;
}

#[async_trait]
pub trait PurchaseReturnRepository: Send + Sync {
    /// Writes the return, its lines, the `received_quantity` *decrements* on the
    /// PO lines and the order's new status in one transaction — the mirror of
    /// `GoodsReceiptRepository::create`, and atomic for the same reason.
    async fn create(
        &self,
        ret: &PurchaseReturn,
        lines: &[PurchaseReturnLine],
        new_order_status: &str,
    ) -> AppResult<PurchaseReturn>;

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<PurchaseReturn>>;
    async fn find_lines(&self, return_id: Uuid) -> AppResult<Vec<PurchaseReturnLine>>;
    async fn list(
        &self,
        filters: &PurchaseReturnFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<PurchaseReturn>, i64)>;
    async fn next_number(&self) -> AppResult<String>;

    /// What has been sent back against an order, valued at its own line prices.
    ///
    /// Mirrors `VendorPaymentRepository::total_paid_for_order`, and is used the
    /// same way: settlement is *derived* from these ledgers rather than
    /// accumulated on the order, so the order cannot drift away from the
    /// documents recorded against it.
    ///
    /// There is deliberately no companion query for *how many units* have gone
    /// back: a return decrements `purchase_order_lines.received_quantity`, so
    /// that column already means "how many are here", which is exactly how many
    /// can still be sent back.
    async fn total_returned_for_order(&self, po_id: Uuid) -> AppResult<rust_decimal::Decimal>;
}
