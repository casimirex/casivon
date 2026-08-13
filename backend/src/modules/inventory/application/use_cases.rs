use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::modules::inventory::application::dto::*;
use crate::modules::inventory::domain::costing::extended_cost;
use crate::modules::inventory::domain::entities::*;
use crate::modules::inventory::domain::errors::InventoryError;
use crate::modules::inventory::domain::repositories::*;
use crate::shared::auth::CurrentUser;
use crate::shared::pagination::PaginationParams;
use crate::shared::posting::{DocumentPoster, PostableMovement};

const DEFAULT_UOM: &str = "piece";

pub struct ProductUseCases<P: ProductRepository, S: StockRepository> {
    products: P,
    stock: S,
}

impl<P: ProductRepository, S: StockRepository> ProductUseCases<P, S> {
    pub fn new(products: P, stock: S) -> Self {
        Self { products, stock }
    }

    pub async fn create(&self, req: CreateProductRequest, user: &CurrentUser) -> AppResult<Product> {
        if self.products.find_by_sku(&req.sku).await?.is_some() {
            return Err(InventoryError::DuplicateSku(req.sku).into());
        }

        let now = Utc::now();
        let product = Product {
            id: Uuid::new_v4(),
            org_id: user.org_id,
            sku: req.sku,
            name: req.name,
            description: req.description,
            product_type: req.product_type.unwrap_or_else(|| "product".to_string()),
            category_id: req.category_id,
            unit_of_measure: req.unit_of_measure.unwrap_or_else(|| DEFAULT_UOM.to_string()),
            cost_price: req.cost_price,
            // Seeded from the standing cost so the first sale of a product that
            // was never received through a purchase order still carries a cost.
            // The first receipt begins moving it toward what was really paid.
            average_cost: req.cost_price,
            sale_price: req.sale_price,
            tax_rate: req.tax_rate,
            is_active: true,
            barcode: req.barcode,
            weight: req.weight,
            dimensions: req.dimensions,
            created_at: now,
            updated_at: now,
        };

        self.products.create(&product).await
    }

    pub async fn get(&self, id: Uuid) -> AppResult<ProductDetail> {
        let product = self.require_product(id).await?;
        let levels = self.stock.levels_for_product(id).await?;

        let total_on_hand = levels.iter().map(|l| l.quantity).sum();
        let total_available = levels.iter().map(StockLevel::available).sum();

        Ok(ProductDetail {
            product,
            stock_levels: levels.into_iter().map(StockLevelView::from).collect(),
            total_on_hand,
            total_available,
        })
    }

    pub async fn list(
        &self,
        filters: &ProductFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Product>, i64)> {
        self.products.list(filters, params).await
    }

    pub async fn update(&self, id: Uuid, req: UpdateProductRequest) -> AppResult<Product> {
        let mut product = self.require_product(id).await?;

        if let Some(v) = req.name {
            product.name = v;
        }
        if req.description.is_some() {
            product.description = req.description;
        }
        if let Some(v) = req.product_type {
            product.product_type = v;
        }
        if req.category_id.is_some() {
            product.category_id = req.category_id;
        }
        if let Some(v) = req.unit_of_measure {
            product.unit_of_measure = v;
        }
        if req.cost_price.is_some() {
            product.cost_price = req.cost_price;
        }
        if req.sale_price.is_some() {
            product.sale_price = req.sale_price;
        }
        if req.tax_rate.is_some() {
            product.tax_rate = req.tax_rate;
        }
        if req.barcode.is_some() {
            product.barcode = req.barcode;
        }
        if req.weight.is_some() {
            product.weight = req.weight;
        }
        if req.dimensions.is_some() {
            product.dimensions = req.dimensions;
        }
        if let Some(v) = req.is_active {
            product.is_active = v;
        }
        product.updated_at = Utc::now();

        self.products.update(&product).await
    }

    /// Products are deactivated rather than deleted once stock has moved through
    /// them — movement history must keep pointing at a real product row.
    pub async fn delete(&self, id: Uuid) -> AppResult<Option<Product>> {
        let product = self.require_product(id).await?;
        let levels = self.stock.levels_for_product(id).await?;

        if levels.iter().any(|l| l.quantity != 0) {
            let mut deactivated = product;
            deactivated.is_active = false;
            deactivated.updated_at = Utc::now();
            return Ok(Some(self.products.update(&deactivated).await?));
        }

        self.products.delete(id).await?;
        Ok(None)
    }

    async fn require_product(&self, id: Uuid) -> AppResult<Product> {
        self.products
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Product {} not found", id)))
    }
}

pub struct CategoryUseCases<C: ProductCategoryRepository> {
    categories: C,
}

impl<C: ProductCategoryRepository> CategoryUseCases<C> {
    pub fn new(categories: C) -> Self {
        Self { categories }
    }

    pub async fn create(
        &self,
        req: CreateCategoryRequest,
        user: &CurrentUser,
    ) -> AppResult<ProductCategory> {
        let category = ProductCategory {
            id: Uuid::new_v4(),
            org_id: user.org_id,
            name: req.name,
            parent_id: req.parent_id,
            created_at: Utc::now(),
        };
        self.categories.create(&category).await
    }

    pub async fn list(&self) -> AppResult<Vec<ProductCategory>> {
        self.categories.list_all().await
    }

    pub async fn get(&self, id: Uuid) -> AppResult<ProductCategory> {
        self.categories
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Category {} not found", id)))
    }

    pub async fn update(
        &self,
        id: Uuid,
        req: UpdateCategoryRequest,
    ) -> AppResult<ProductCategory> {
        let mut category = self.get(id).await?;

        if let Some(v) = req.name {
            category.name = v;
        }
        if let Some(parent_id) = req.parent_id {
            if parent_id == id {
                return Err(AppError::Validation(
                    "A category cannot be its own parent".to_string(),
                ));
            }
            category.parent_id = Some(parent_id);
        }

        self.categories.update(&category).await
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        self.get(id).await?;
        self.categories.delete(id).await
    }
}

pub struct WarehouseUseCases<W: WarehouseRepository> {
    warehouses: W,
}

impl<W: WarehouseRepository> WarehouseUseCases<W> {
    pub fn new(warehouses: W) -> Self {
        Self { warehouses }
    }

    pub async fn create(
        &self,
        req: CreateWarehouseRequest,
        user: &CurrentUser,
    ) -> AppResult<Warehouse> {
        if self.warehouses.find_by_code(&req.code).await?.is_some() {
            return Err(InventoryError::DuplicateWarehouseCode(req.code).into());
        }

        let now = Utc::now();
        let warehouse = Warehouse {
            id: Uuid::new_v4(),
            org_id: user.org_id,
            code: req.code,
            name: req.name,
            address: req.address,
            city: req.city,
            country: req.country,
            manager_id: req.manager_id,
            is_active: true,
            created_at: now,
            updated_at: now,
        };
        self.warehouses.create(&warehouse).await
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Warehouse> {
        self.warehouses
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Warehouse {} not found", id)))
    }

    pub async fn list(
        &self,
        filters: &WarehouseFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Warehouse>, i64)> {
        self.warehouses.list(filters, params).await
    }

    pub async fn update(&self, id: Uuid, req: UpdateWarehouseRequest) -> AppResult<Warehouse> {
        let mut warehouse = self.get(id).await?;

        if let Some(v) = req.name {
            warehouse.name = v;
        }
        if req.address.is_some() {
            warehouse.address = req.address;
        }
        if req.city.is_some() {
            warehouse.city = req.city;
        }
        if req.country.is_some() {
            warehouse.country = req.country;
        }
        if req.manager_id.is_some() {
            warehouse.manager_id = req.manager_id;
        }
        if let Some(v) = req.is_active {
            warehouse.is_active = v;
        }
        warehouse.updated_at = Utc::now();

        self.warehouses.update(&warehouse).await
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        self.get(id).await?;
        self.warehouses.delete(id).await
    }
}

pub struct StockUseCases<S: StockRepository, P: ProductRepository, W: WarehouseRepository> {
    stock: S,
    products: P,
    warehouses: W,
    /// Where stock leaving becomes a cost. Does nothing until the inventory
    /// accounts are mapped, which is what keeps an installation on periodic
    /// costing until it chooses otherwise.
    poster: Arc<dyn DocumentPoster>,
}

impl<S: StockRepository, P: ProductRepository, W: WarehouseRepository> StockUseCases<S, P, W> {
    pub fn new(stock: S, products: P, warehouses: W, poster: Arc<dyn DocumentPoster>) -> Self {
        Self { stock, products, warehouses, poster }
    }

    /// The one door through which stock levels change.
    pub async fn record_movement(
        &self,
        req: RecordMovementRequest,
        user: &CurrentUser,
    ) -> AppResult<MovementResult> {
        let mut applied = self.record_movements(vec![req], &[], user).await?;
        // `record_movements` returns one result per request, so a single request
        // always yields exactly one.
        Ok(applied.remove(0))
    }

    /// Everything one document moves, applied together or not at all.
    ///
    /// A document used to move its lines one at a time, so a line the shelf
    /// could not cover left the earlier lines already gone — costed, and against
    /// a document that was never issued — and retrying moved them again.
    ///
    /// `release` hands back the holds this document consumes, in the same
    /// transaction as the movements. Shipping checks what is *available*, so an
    /// order's own reservation would otherwise block its own shipment; and doing
    /// the release separately is what left the hold gone whenever the shipment
    /// was refused. Per line, because an invoice may ship only part of an order.
    ///
    /// Every request is checked before any stock moves, so the ordinary refusal
    /// still names the line at fault. The repository checks again under a lock,
    /// which is what makes two simultaneous shipments of the last unit safe —
    /// and what catches two lines of the same product that only overdraw the
    /// shelf when added together.
    pub async fn record_movements(
        &self,
        requests: Vec<RecordMovementRequest>,
        release: &[ReservationRelease],
        user: &CurrentUser,
    ) -> AppResult<Vec<MovementResult>> {
        let mut planned = Vec::with_capacity(requests.len());
        for req in requests {
            planned.push(self.plan_movement(req, user).await?);
        }

        let movements: Vec<StockMovement> =
            planned.iter().map(|(movement, _)| movement.clone()).collect();
        let applied = self.stock.apply_movements(release, &movements).await?;

        let mut results = Vec::with_capacity(applied.len());
        for ((movement, level), (_, product)) in applied.into_iter().zip(planned) {
            // Posted after the stock has actually moved, and with the cost the
            // movement was valued at rather than the product's average now:
            // another receipt landing between the two would otherwise cost this
            // sale at a price it was never sold at.
            self.poster
                .stock_moved(&PostableMovement {
                    id: movement.id,
                    org_id: movement.org_id,
                    movement_type: movement.movement_type.clone(),
                    quantity_delta: MovementType::source_delta(
                        &movement.movement_type,
                        movement.quantity,
                    ),
                    value: extended_cost(movement.quantity, movement.base_unit_cost),
                    entry_date: movement.created_at.date_naive(),
                    reference_type: movement.reference_type.clone(),
                    description: format!("{} — {}", product.sku, product.name),
                    created_by: movement.created_by,
                })
                .await?;

            results.push(MovementResult { movement, stock_level: level.into() });
        }

        Ok(results)
    }

    /// Checks one request against every rule and builds the movement it asks
    /// for, without moving anything.
    ///
    /// Returns the product too: posting needs its name, and looking it up again
    /// after the stock moved would read an average the movement did not use.
    async fn plan_movement(
        &self,
        req: RecordMovementRequest,
        user: &CurrentUser,
    ) -> AppResult<(StockMovement, Product)> {
        if !MovementType::is_valid(&req.movement_type) {
            return Err(InventoryError::UnknownMovementType(req.movement_type).into());
        }
        if req.quantity == 0 {
            return Err(InventoryError::ZeroQuantity.into());
        }
        if req.quantity < 0 && req.movement_type != MovementType::ADJUSTMENT {
            return Err(InventoryError::NegativeQuantityNotAllowed.into());
        }

        let product = self
            .products
            .find_by_id(req.product_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Product {} not found", req.product_id)))?;

        if !ProductType::is_stocked(&product.product_type) {
            return Err(InventoryError::NotStocked(product.sku).into());
        }

        // Existence only, like the destination check below: the warehouse's name
        // is needed to refuse a movement the shelf cannot cover, and that
        // refusal is raised where the level is locked.
        if self.warehouses.find_by_id(req.warehouse_id).await?.is_none() {
            return Err(AppError::NotFound(format!("Warehouse {} not found", req.warehouse_id)));
        }

        if MovementType::requires_destination(&req.movement_type) {
            let destination_id =
                req.to_warehouse_id.ok_or(InventoryError::TransferNeedsDestination)?;
            if destination_id == req.warehouse_id {
                return Err(InventoryError::TransferToSameWarehouse.into());
            }
            if self.warehouses.find_by_id(destination_id).await?.is_none() {
                return Err(AppError::NotFound(format!(
                    "Destination warehouse {} not found",
                    destination_id
                )));
            }
        }

        // Anything leaving the source warehouse has to actually be there — and
        // that is checked in the repository, with the level locked, not here.
        //
        // Here it would be both weaker and, now, wrong. Weaker because two
        // shipments of the last unit could each read the level before either
        // moved it. Wrong because an order's reservation is released *inside*
        // the same transaction as its shipment, and a check that runs before
        // that release sees the order's own goods as unavailable — so an order
        // blocks its own shipment, which is exactly what releasing first exists
        // to prevent.

        let movement = StockMovement {
            id: Uuid::new_v4(),
            org_id: user.org_id,
            product_id: req.product_id,
            warehouse_id: req.warehouse_id,
            to_warehouse_id: if MovementType::requires_destination(&req.movement_type) {
                req.to_warehouse_id
            } else {
                None
            },
            movement_type: req.movement_type,
            quantity: req.quantity,
            unit_cost: req.unit_cost.or(product.cost_price),
            // Left for the repository to fill from the product's running
            // average, inside the same transaction that moves the level. Only a
            // caller with a purchase price of its own — the goods receipt —
            // supplies this, and it does so in the base currency.
            base_unit_cost: None,
            reference_type: req.reference_type,
            reference_id: req.reference_id,
            notes: req.notes,
            created_by: user.id,
            created_at: Utc::now(),
        };

        Ok((movement, product))
    }

    pub async fn list_movements(
        &self,
        filters: &MovementFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<StockMovement>, i64)> {
        self.stock.list_movements(filters, params).await
    }

    pub async fn levels_for_warehouse(
        &self,
        warehouse_id: Uuid,
        params: &PaginationParams,
    ) -> AppResult<(Vec<StockLevelView>, i64)> {
        let (levels, total) = self.stock.levels_for_warehouse(warehouse_id, params).await?;
        Ok((levels.into_iter().map(StockLevelView::from).collect(), total))
    }

    pub async fn low_stock(
        &self,
        params: &PaginationParams,
    ) -> AppResult<(Vec<StockLevelView>, i64)> {
        let (levels, total) = self.stock.low_stock(params).await?;
        Ok((levels.into_iter().map(StockLevelView::from).collect(), total))
    }

    pub async fn set_reorder_policy(
        &self,
        req: SetReorderPolicyRequest,
    ) -> AppResult<StockLevelView> {
        let level = self
            .stock
            .set_reorder_policy(
                req.product_id,
                req.warehouse_id,
                req.reorder_level,
                req.reorder_quantity,
            )
            .await?;
        Ok(level.into())
    }

    pub async fn valuation(&self) -> AppResult<ValuationResponse> {
        Ok(ValuationResponse { total_value: self.stock.valuation().await? })
    }
}

pub struct BomUseCases<B: BomRepository, P: ProductRepository> {
    boms: B,
    products: P,
}

impl<B: BomRepository, P: ProductRepository> BomUseCases<B, P> {
    pub fn new(boms: B, products: P) -> Self {
        Self { boms, products }
    }

    pub async fn create(&self, req: CreateBomRequest, user: &CurrentUser) -> AppResult<BomDetail> {
        if req.lines.is_empty() {
            return Err(InventoryError::EmptyBom.into());
        }

        // A product built from itself would recurse forever during explosion.
        if req.lines.iter().any(|l| l.component_id == req.product_id) {
            return Err(InventoryError::SelfReferencingBom.into());
        }

        let product = self
            .products
            .find_by_id(req.product_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Product {} not found", req.product_id)))?;

        let version = req.version.unwrap_or_else(|| "1.0".to_string());
        if self.boms.find_by_product_version(req.product_id, &version).await?.is_some() {
            return Err(InventoryError::DuplicateBomVersion(product.sku, version).into());
        }

        for line in &req.lines {
            if self.products.find_by_id(line.component_id).await?.is_none() {
                return Err(AppError::NotFound(format!(
                    "Component product {} not found",
                    line.component_id
                )));
            }
        }

        let bom_id = Uuid::new_v4();
        let now = Utc::now();
        let bom = BillOfMaterials {
            id: bom_id,
            org_id: user.org_id,
            product_id: req.product_id,
            version,
            quantity_to_produce: req.quantity_to_produce.unwrap_or(1),
            is_active: true,
            created_at: now,
            updated_at: now,
        };

        let lines = build_bom_lines(bom_id, &req.lines);
        let bom = self.boms.create(&bom, &lines).await?;
        let lines = self.boms.find_lines(bom.id).await?;
        Ok(BomDetail { bom, lines })
    }

    pub async fn get(&self, id: Uuid) -> AppResult<BomDetail> {
        let bom = self.require_bom(id).await?;
        let lines = self.boms.find_lines(id).await?;
        Ok(BomDetail { bom, lines })
    }

    pub async fn list(
        &self,
        product_id: Option<Uuid>,
        params: &PaginationParams,
    ) -> AppResult<(Vec<BillOfMaterials>, i64)> {
        self.boms.list(product_id, params).await
    }

    pub async fn update(&self, id: Uuid, req: UpdateBomRequest) -> AppResult<BomDetail> {
        let mut bom = self.require_bom(id).await?;

        if let Some(v) = req.quantity_to_produce {
            bom.quantity_to_produce = v;
        }
        if let Some(v) = req.is_active {
            bom.is_active = v;
        }
        bom.updated_at = Utc::now();

        let new_lines = match &req.lines {
            Some(requested) => {
                if requested.is_empty() {
                    return Err(InventoryError::EmptyBom.into());
                }
                if requested.iter().any(|l| l.component_id == bom.product_id) {
                    return Err(InventoryError::SelfReferencingBom.into());
                }
                Some(build_bom_lines(bom.id, requested))
            }
            None => None,
        };

        let bom = self.boms.update(&bom, new_lines.as_deref()).await?;
        let lines = self.boms.find_lines(bom.id).await?;
        Ok(BomDetail { bom, lines })
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        self.require_bom(id).await?;
        self.boms.delete(id).await
    }

    async fn require_bom(&self, id: Uuid) -> AppResult<BillOfMaterials> {
        self.boms
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Bill of materials {} not found", id)))
    }
}

fn build_bom_lines(bom_id: Uuid, requested: &[BomLineRequest]) -> Vec<BomLine> {
    requested
        .iter()
        .enumerate()
        .map(|(index, line)| BomLine {
            id: Uuid::new_v4(),
            bom_id,
            component_id: line.component_id,
            quantity_required: line.quantity_required,
            unit_of_measure: line
                .unit_of_measure
                .clone()
                .unwrap_or_else(|| DEFAULT_UOM.to_string()),
            sort_order: index as i32,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bom_lines_default_their_unit_and_keep_order() {
        let bom_id = Uuid::new_v4();
        let lines = build_bom_lines(
            bom_id,
            &[
                BomLineRequest {
                    component_id: Uuid::new_v4(),
                    quantity_required: 2,
                    unit_of_measure: None,
                },
                BomLineRequest {
                    component_id: Uuid::new_v4(),
                    quantity_required: 5,
                    unit_of_measure: Some("kg".to_string()),
                },
            ],
        );

        assert_eq!(lines[0].unit_of_measure, DEFAULT_UOM);
        assert_eq!(lines[0].sort_order, 0);
        assert_eq!(lines[1].unit_of_measure, "kg");
        assert_eq!(lines[1].sort_order, 1);
        assert!(lines.iter().all(|l| l.bom_id == bom_id));
    }
}
