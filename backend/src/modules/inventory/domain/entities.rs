use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct ProductCategory {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Product {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub sku: String,
    pub name: String,
    pub description: Option<String>,
    pub product_type: String, // product, service, raw_material
    pub category_id: Option<Uuid>,
    pub unit_of_measure: String,
    /// A standing figure somebody typed in — what the product is *expected* to
    /// cost. Not what stock is valued at; see `average_cost`.
    pub cost_price: Option<Decimal>,
    /// The moving weighted average, maintained by the stock movements
    /// themselves. Read-only from outside: setting it by hand would put the
    /// valuation report and the Inventory account out of step with nothing to
    /// show why.
    pub average_cost: Option<Decimal>,
    pub sale_price: Option<Decimal>,
    pub tax_rate: Option<Decimal>,
    pub is_active: bool,
    pub barcode: Option<String>,
    pub weight: Option<Decimal>,
    pub dimensions: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Warehouse {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub code: String,
    pub name: String,
    pub address: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub manager_id: Option<Uuid>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// On-hand quantity for one product in one warehouse.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct StockLevel {
    pub id: Uuid,
    pub product_id: Uuid,
    pub warehouse_id: Uuid,
    pub quantity: i32,
    pub reserved_quantity: i32,
    pub reorder_level: Option<i32>,
    pub reorder_quantity: Option<i32>,
    pub updated_at: DateTime<Utc>,
}

impl StockLevel {
    /// What can actually be shipped: on hand minus what is already committed.
    pub fn available(&self) -> i32 {
        self.quantity - self.reserved_quantity
    }

    /// How much of `wanted` this shelf can hold for a confirmed order.
    ///
    /// Confirming an order short of stock still confirms — selling before buying
    /// is ordinary, and refusing would block it outright — so this reserves what
    /// is there and leaves the rest of the promise unreserved. The shortfall is
    /// caught later, when issuing the invoice refuses if the goods never arrived.
    ///
    /// Never negative: a shelf already oversold has nothing left to promise, and
    /// a negative reservation would *increase* what looks available.
    pub fn reservable(&self, wanted: i32) -> i32 {
        wanted.min(self.available()).max(0)
    }

    pub fn needs_reorder(&self) -> bool {
        matches!(self.reorder_level, Some(level) if self.available() <= level)
    }
}

/// Stock held for a confirmed order.
///
/// `quantity` is what was actually held, which is **not** the ordered quantity:
/// confirming an order short of stock reserves what is on the shelf and leaves
/// the rest of the promise unreserved. Releasing has to give back exactly what
/// was taken, which is why this is a stored row rather than a figure derived
/// from the order line.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct StockReservation {
    pub id: Uuid,
    pub order_id: Uuid,
    pub order_line_id: Uuid,
    pub product_id: Uuid,
    pub warehouse_id: Uuid,
    pub quantity: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct StockMovement {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub product_id: Uuid,
    pub warehouse_id: Uuid,
    /// Destination warehouse; only set for transfers.
    pub to_warehouse_id: Option<Uuid>,
    pub movement_type: String, // in, out, transfer, adjustment
    pub quantity: i32,
    /// What the goods cost **in the document's currency** — a purchase order
    /// line's price, copied through by the goods receipt.
    pub unit_cost: Option<Decimal>,
    /// What they cost in the base currency, which is what stock is valued and
    /// posted at. Filled from the product's average when the caller has no
    /// figure of its own, which is the case for everything going out.
    pub base_unit_cost: Option<Decimal>,
    pub reference_type: Option<String>,
    pub reference_id: Option<Uuid>,
    pub notes: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct BillOfMaterials {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub product_id: Uuid,
    pub version: String,
    pub quantity_to_produce: i32,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct BomLine {
    pub id: Uuid,
    pub bom_id: Uuid,
    pub component_id: Uuid,
    pub quantity_required: i32,
    pub unit_of_measure: String,
    pub sort_order: i32,
}

pub struct MovementType;

impl MovementType {
    pub const IN: &'static str = "in";
    pub const OUT: &'static str = "out";
    pub const TRANSFER: &'static str = "transfer";
    pub const ADJUSTMENT: &'static str = "adjustment";

    pub const ALL: [&'static str; 4] = [Self::IN, Self::OUT, Self::TRANSFER, Self::ADJUSTMENT];

    pub fn is_valid(value: &str) -> bool {
        Self::ALL.contains(&value)
    }

    /// Whether the movement takes stock away from `warehouse_id`.
    pub fn removes_from_source(value: &str) -> bool {
        matches!(value, Self::OUT | Self::TRANSFER)
    }

    pub fn requires_destination(value: &str) -> bool {
        value == Self::TRANSFER
    }

    /// Signed effect on the source warehouse for a positive `quantity`.
    /// Adjustments carry their own sign, so the caller passes the raw quantity.
    pub fn source_delta(value: &str, quantity: i32) -> i32 {
        match value {
            Self::IN => quantity,
            Self::OUT | Self::TRANSFER => -quantity,
            _ => quantity, // adjustment: quantity may be negative
        }
    }
}

pub struct ProductType;

impl ProductType {
    pub const ALL: [&'static str; 3] = ["product", "service", "raw_material"];

    pub fn is_valid(value: &str) -> bool {
        Self::ALL.contains(&value)
    }

    /// Services have no stock to track.
    pub fn is_stocked(value: &str) -> bool {
        value != "service"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(quantity: i32, reserved: i32, reorder: Option<i32>) -> StockLevel {
        StockLevel {
            id: Uuid::new_v4(),
            product_id: Uuid::new_v4(),
            warehouse_id: Uuid::new_v4(),
            quantity,
            reserved_quantity: reserved,
            reorder_level: reorder,
            reorder_quantity: Some(50),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn available_excludes_reserved_stock() {
        assert_eq!(level(100, 30, None).available(), 70);
    }

    #[test]
    fn reorder_triggers_on_available_not_on_hand() {
        // 100 on hand looks healthy, but 95 is committed elsewhere.
        assert!(level(100, 95, Some(10)).needs_reorder());
        assert!(!level(100, 0, Some(10)).needs_reorder());
    }

    #[test]
    fn no_reorder_level_means_no_alert() {
        assert!(!level(0, 0, None).needs_reorder());
    }

    #[test]
    fn a_shelf_that_can_cover_an_order_reserves_all_of_it() {
        assert_eq!(level(100, 0, None).reservable(10), 10);
        // Already committed elsewhere: only what is left over can be promised.
        assert_eq!(level(100, 95, None).reservable(10), 5);
    }

    #[test]
    fn a_short_shelf_reserves_what_it_has() {
        assert_eq!(level(6, 0, None).reservable(10), 6);
        assert_eq!(level(0, 0, None).reservable(10), 0);
    }

    /// A shelf already oversold has nothing left to promise, and a negative
    /// reservation would make *more* look available than there is.
    #[test]
    fn an_oversold_shelf_reserves_nothing_rather_than_a_negative() {
        assert_eq!(level(5, 20, None).reservable(10), 0);
    }

    #[test]
    fn reserving_nothing_is_nothing() {
        assert_eq!(level(100, 0, None).reservable(0), 0);
        assert_eq!(level(100, 0, None).reservable(-5), 0);
    }

    #[test]
    fn movement_deltas_follow_their_type() {
        assert_eq!(MovementType::source_delta(MovementType::IN, 10), 10);
        assert_eq!(MovementType::source_delta(MovementType::OUT, 10), -10);
        assert_eq!(MovementType::source_delta(MovementType::TRANSFER, 10), -10);
        // Adjustments are signed by the caller.
        assert_eq!(MovementType::source_delta(MovementType::ADJUSTMENT, -4), -4);
    }

    #[test]
    fn only_transfers_need_a_destination() {
        assert!(MovementType::requires_destination(MovementType::TRANSFER));
        assert!(!MovementType::requires_destination(MovementType::OUT));
    }

    #[test]
    fn services_are_not_stocked() {
        assert!(!ProductType::is_stocked("service"));
        assert!(ProductType::is_stocked("product"));
    }
}
