use async_trait::async_trait;
use utoipa::IntoParams;
use chrono::NaiveDate;
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::inventory::domain::entities::*;
use crate::shared::pagination::PaginationParams;

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ProductFilters {
    pub category_id: Option<Uuid>,
    pub product_type: Option<String>,
    pub is_active: Option<bool>,
    /// Matches SKU, name or barcode.
    pub search: Option<String>,
}

#[async_trait]
pub trait ProductRepository: Send + Sync {
    async fn create(&self, product: &Product) -> AppResult<Product>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Product>>;
    async fn find_by_sku(&self, sku: &str) -> AppResult<Option<Product>>;
    async fn update(&self, product: &Product) -> AppResult<Product>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &ProductFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Product>, i64)>;
}

#[async_trait]
pub trait ProductCategoryRepository: Send + Sync {
    async fn create(&self, category: &ProductCategory) -> AppResult<ProductCategory>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<ProductCategory>>;
    async fn update(&self, category: &ProductCategory) -> AppResult<ProductCategory>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list_all(&self) -> AppResult<Vec<ProductCategory>>;
}

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct WarehouseFilters {
    pub is_active: Option<bool>,
    pub search: Option<String>,
}

#[async_trait]
pub trait WarehouseRepository: Send + Sync {
    async fn create(&self, warehouse: &Warehouse) -> AppResult<Warehouse>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Warehouse>>;
    async fn find_by_code(&self, code: &str) -> AppResult<Option<Warehouse>>;
    async fn update(&self, warehouse: &Warehouse) -> AppResult<Warehouse>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &WarehouseFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Warehouse>, i64)>;
}

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MovementFilters {
    pub product_id: Option<Uuid>,
    pub warehouse_id: Option<Uuid>,
    pub movement_type: Option<String>,
    pub reference_type: Option<String>,
    pub reference_id: Option<Uuid>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
}

/// One line's worth of a hold to give back.
///
/// Per line and per quantity, not per order: an invoice may ship only part of
/// what an order promised, and releasing the whole order would hand back stock
/// still owed on the lines this instalment does not touch.
#[derive(Debug, Clone, Copy)]
pub struct ReservationRelease {
    pub order_line_id: Uuid,
    pub quantity: i32,
}

/// One line's worth of stock to hold.
#[derive(Debug, Clone, Copy)]
pub struct ReservationRequest {
    pub order_line_id: Uuid,
    pub product_id: Uuid,
    pub quantity: i32,
}


/// Stock levels are derived state: they may only change through a movement, so
/// the movement and the level update share one transaction inside the repository.
/// Reservations follow the same rule — the rows and `reserved_quantity` move
/// together.
#[async_trait]
pub trait StockRepository: Send + Sync {
    /// Applies a whole document's movements as one fact: all of them, or none,
    /// returning each movement as stored alongside the resulting source level.
    ///
    /// Plural because a document that moves several lines used to move them one
    /// transaction at a time. A line refused halfway through left the earlier
    /// lines already gone from the shelf — costed, and belonging to a document
    /// that was never issued — and retrying moved them a second time.
    ///
    /// `release` gives back the holds this document consumes inside the same
    /// transaction, because the release and the movements are one fact too: the
    /// goods stop being held *and* they leave. Splitting them is what dropped
    /// the hold whenever the leaving was refused.
    ///
    /// Availability is re-checked here with the level locked. A caller may check
    /// first and refuse sooner; this is the check two simultaneous shipments of
    /// the same unit cannot both pass.
    async fn apply_movements(
        &self,
        release: &[ReservationRelease],
        movements: &[StockMovement],
    ) -> AppResult<Vec<(StockMovement, StockLevel)>>;
    async fn find_level(
        &self,
        product_id: Uuid,
        warehouse_id: Uuid,
    ) -> AppResult<Option<StockLevel>>;
    async fn levels_for_product(&self, product_id: Uuid) -> AppResult<Vec<StockLevel>>;
    async fn levels_for_warehouse(
        &self,
        warehouse_id: Uuid,
        params: &PaginationParams,
    ) -> AppResult<(Vec<StockLevel>, i64)>;
    /// Holds stock for a confirmed order, and returns what was actually held.
    ///
    /// `wanted` is what the order asked for; each shelf keeps what it can and
    /// the rest of the promise stays unreserved. Writing the reservation rows and
    /// moving `reserved_quantity` happen together, so the running total on the
    /// level can never disagree with the rows that make it up.
    async fn reserve_for_order(
        &self,
        order_id: Uuid,
        warehouse_id: Uuid,
        wanted: &[ReservationRequest],
    ) -> AppResult<Vec<StockReservation>>;

    /// Gives back everything an order was holding. Safe to call on an order that
    /// holds nothing, which is the ordinary case for one confirmed before
    /// reservations existed.
    async fn release_order(&self, order_id: Uuid) -> AppResult<u64>;

    /// Everything at or below its reorder level.
    async fn low_stock(&self, params: &PaginationParams) -> AppResult<(Vec<StockLevel>, i64)>;
    async fn set_reorder_policy(
        &self,
        product_id: Uuid,
        warehouse_id: Uuid,
        reorder_level: i32,
        reorder_quantity: i32,
    ) -> AppResult<StockLevel>;
    async fn list_movements(
        &self,
        filters: &MovementFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<StockMovement>, i64)>;
    /// Total stock value across all warehouses, at cost.
    async fn valuation(&self) -> AppResult<rust_decimal::Decimal>;
}

#[async_trait]
pub trait BomRepository: Send + Sync {
    async fn create(&self, bom: &BillOfMaterials, lines: &[BomLine]) -> AppResult<BillOfMaterials>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<BillOfMaterials>>;
    async fn find_lines(&self, bom_id: Uuid) -> AppResult<Vec<BomLine>>;
    async fn find_by_product_version(
        &self,
        product_id: Uuid,
        version: &str,
    ) -> AppResult<Option<BillOfMaterials>>;
    async fn update(
        &self,
        bom: &BillOfMaterials,
        lines: Option<&[BomLine]>,
    ) -> AppResult<BillOfMaterials>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        product_id: Option<Uuid>,
        params: &PaginationParams,
    ) -> AppResult<(Vec<BillOfMaterials>, i64)>;
}
