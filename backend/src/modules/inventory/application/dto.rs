use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use crate::modules::inventory::domain::entities::*;

// ------------------------------------------------------------------ products

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateProductRequest {
    #[validate(length(min = 1, max = 100, message = "SKU is required"))]
    pub sku: String,
    #[validate(length(min = 1, max = 255, message = "Product name is required"))]
    pub name: String,
    pub description: Option<String>,
    #[validate(custom = "validate_product_type")]
    pub product_type: Option<String>,
    pub category_id: Option<Uuid>,
    #[validate(length(min = 1, max = 50))]
    pub unit_of_measure: Option<String>,
    pub cost_price: Option<Decimal>,
    pub sale_price: Option<Decimal>,
    /// A whole percentage: 20 means 20%, the same convention as everywhere else.
    #[validate(custom = "crate::shared::validation::validate_percentage")]
    pub tax_rate: Option<Decimal>,
    #[validate(length(max = 100))]
    pub barcode: Option<String>,
    pub weight: Option<Decimal>,
    #[validate(length(max = 100))]
    pub dimensions: Option<String>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateProductRequest {
    #[validate(length(min = 1, max = 255))]
    pub name: Option<String>,
    pub description: Option<String>,
    #[validate(custom = "validate_product_type")]
    pub product_type: Option<String>,
    pub category_id: Option<Uuid>,
    #[validate(length(min = 1, max = 50))]
    pub unit_of_measure: Option<String>,
    pub cost_price: Option<Decimal>,
    pub sale_price: Option<Decimal>,
    /// A whole percentage: 20 means 20%, the same convention as everywhere else.
    #[validate(custom = "crate::shared::validation::validate_percentage")]
    pub tax_rate: Option<Decimal>,
    pub barcode: Option<String>,
    pub weight: Option<Decimal>,
    pub dimensions: Option<String>,
    pub is_active: Option<bool>,
}

fn validate_product_type(value: &str) -> Result<(), validator::ValidationError> {
    one_of(value, &ProductType::ALL, "product_type")
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateCategoryRequest {
    #[validate(length(min = 1, max = 100, message = "Category name is required"))]
    pub name: String,
    pub parent_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateCategoryRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,
    pub parent_id: Option<Uuid>,
}

// --------------------------------------------------------------- warehouses

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateWarehouseRequest {
    #[validate(length(min = 1, max = 50, message = "Warehouse code is required"))]
    pub code: String,
    #[validate(length(min = 1, max = 100, message = "Warehouse name is required"))]
    pub name: String,
    pub address: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub manager_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateWarehouseRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,
    pub address: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub manager_id: Option<Uuid>,
    pub is_active: Option<bool>,
}

// ------------------------------------------------------------------- stock

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RecordMovementRequest {
    pub product_id: Uuid,
    pub warehouse_id: Uuid,
    /// Required for transfers, ignored otherwise.
    pub to_warehouse_id: Option<Uuid>,
    #[validate(custom = "validate_movement_type")]
    pub movement_type: String,
    /// Positive for in/out/transfer; may be negative for an adjustment.
    pub quantity: i32,
    pub unit_cost: Option<Decimal>,
    pub reference_type: Option<String>,
    pub reference_id: Option<Uuid>,
    pub notes: Option<String>,
}

fn validate_movement_type(value: &str) -> Result<(), validator::ValidationError> {
    one_of(value, &MovementType::ALL, "movement_type")
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct SetReorderPolicyRequest {
    pub product_id: Uuid,
    pub warehouse_id: Uuid,
    #[validate(range(min = 0, message = "Reorder level cannot be negative"))]
    pub reorder_level: i32,
    #[validate(range(min = 0, message = "Reorder quantity cannot be negative"))]
    pub reorder_quantity: i32,
}

// --------------------------------------------------------------------- boms

#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema)]
pub struct BomLineRequest {
    pub component_id: Uuid,
    #[validate(range(min = 1, message = "Component quantity must be at least 1"))]
    pub quantity_required: i32,
    #[validate(length(min = 1, max = 50))]
    pub unit_of_measure: Option<String>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateBomRequest {
    pub product_id: Uuid,
    #[validate(length(min = 1, max = 20))]
    pub version: Option<String>,
    #[validate(range(min = 1, message = "Quantity to produce must be at least 1"))]
    pub quantity_to_produce: Option<i32>,
    #[validate(length(min = 1, message = "A bill of materials needs at least one component"))]
    #[validate]
    pub lines: Vec<BomLineRequest>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateBomRequest {
    #[validate(range(min = 1))]
    pub quantity_to_produce: Option<i32>,
    pub is_active: Option<bool>,
    #[validate]
    pub lines: Option<Vec<BomLineRequest>>,
}

// ---------------------------------------------------------------- responses

#[derive(Debug, Serialize, ToSchema)]
pub struct ProductDetail {
    #[serde(flatten)]
    pub product: Product,
    /// Per-warehouse levels, so the detail page can show where the stock sits.
    pub stock_levels: Vec<StockLevelView>,
    pub total_on_hand: i32,
    pub total_available: i32,
}

/// A stock level with the two derived numbers the UI keeps recomputing.
#[derive(Debug, Serialize, ToSchema)]
pub struct StockLevelView {
    #[serde(flatten)]
    pub level: StockLevel,
    pub available: i32,
    pub needs_reorder: bool,
}

impl From<StockLevel> for StockLevelView {
    fn from(level: StockLevel) -> Self {
        Self { available: level.available(), needs_reorder: level.needs_reorder(), level }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BomDetail {
    #[serde(flatten)]
    pub bom: BillOfMaterials,
    pub lines: Vec<BomLine>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MovementResult {
    pub movement: StockMovement,
    pub stock_level: StockLevelView,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ValuationResponse {
    pub total_value: Decimal,
}

fn one_of(
    value: &str,
    allowed: &[&str],
    code: &'static str,
) -> Result<(), validator::ValidationError> {
    if allowed.contains(&value) {
        return Ok(());
    }
    let mut err = validator::ValidationError::new(code);
    err.message = Some(format!("Must be one of: {}", allowed.join(", ")).into());
    Err(err)
}
