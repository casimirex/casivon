use thiserror::Error;

use crate::error::AppError;

#[derive(Error, Debug)]
pub enum InventoryError {
    #[error("'{0}' is not a valid movement type")]
    UnknownMovementType(String),

    #[error("A transfer needs a destination warehouse")]
    TransferNeedsDestination,

    #[error("A transfer cannot use the same warehouse as source and destination")]
    TransferToSameWarehouse,

    #[error("Movement quantity must not be zero")]
    ZeroQuantity,

    #[error("Only 'adjustment' movements may carry a negative quantity")]
    NegativeQuantityNotAllowed,

    #[error("Only {available} unit(s) of '{sku}' are available in '{warehouse}', {requested} requested")]
    InsufficientStock { sku: String, warehouse: String, available: i32, requested: i32 },

    #[error("Product '{0}' is a service and does not carry stock")]
    NotStocked(String),

    #[error("SKU '{0}' is already in use")]
    DuplicateSku(String),

    #[error("Warehouse code '{0}' is already in use")]
    DuplicateWarehouseCode(String),

    #[error("A bill of materials cannot list its own product as a component")]
    SelfReferencingBom,

    #[error("Product '{0}' already has an active bill of materials at version '{1}'")]
    DuplicateBomVersion(String, String),

    #[error("A bill of materials needs at least one component")]
    EmptyBom,
}

impl From<InventoryError> for AppError {
    fn from(err: InventoryError) -> Self {
        match err {
            InventoryError::InsufficientStock { .. } | InventoryError::NotStocked(_) => {
                AppError::Conflict(err.to_string())
            }
            InventoryError::DuplicateSku(_)
            | InventoryError::DuplicateWarehouseCode(_)
            | InventoryError::DuplicateBomVersion(..) => AppError::Conflict(err.to_string()),
            _ => AppError::Validation(err.to_string()),
        }
    }
}
