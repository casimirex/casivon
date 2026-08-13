use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::inventory::domain::entities::{Product, ProductCategory};
use crate::modules::inventory::domain::repositories::{
    ProductCategoryRepository, ProductFilters, ProductRepository,
};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 5] = ["created_at", "updated_at", "name", "sku", "sale_price"];

#[derive(Clone)]
pub struct PgProductRepository {
    pool: PgPool,
}

impl PgProductRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a ProductFilters) {
    if let Some(category_id) = filters.category_id {
        builder.push(" AND category_id = ").push_bind(category_id);
    }
    if let Some(product_type) = &filters.product_type {
        builder.push(" AND product_type = ").push_bind(product_type);
    }
    if let Some(is_active) = filters.is_active {
        builder.push(" AND is_active = ").push_bind(is_active);
    }
    if let Some(search) = &filters.search {
        let pattern = format!("%{}%", search);
        builder
            .push(" AND (sku ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR name ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR barcode ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
}

#[async_trait]
impl ProductRepository for PgProductRepository {
    async fn create(&self, product: &Product) -> AppResult<Product> {
        Ok(sqlx::query_as::<_, Product>(
            r#"
            INSERT INTO products
                (id, org_id, sku, name, description, product_type, category_id, unit_of_measure,
                 cost_price, average_cost, sale_price, tax_rate, is_active, barcode, weight,
                 dimensions, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
            RETURNING *
            "#,
        )
        .bind(product.id)
        .bind(product.org_id)
        .bind(&product.sku)
        .bind(&product.name)
        .bind(&product.description)
        .bind(&product.product_type)
        .bind(product.category_id)
        .bind(&product.unit_of_measure)
        .bind(product.cost_price)
        // Seeded here and never written again from this repository: from now on
        // the stock movements own it, and letting an edit of the product form
        // overwrite it would put the valuation report and the Inventory account
        // out of step with nothing to show why.
        .bind(product.average_cost)
        .bind(product.sale_price)
        .bind(product.tax_rate)
        .bind(product.is_active)
        .bind(&product.barcode)
        .bind(product.weight)
        .bind(&product.dimensions)
        .bind(product.created_at)
        .bind(product.updated_at)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Product>> {
        Ok(sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn find_by_sku(&self, sku: &str) -> AppResult<Option<Product>> {
        Ok(sqlx::query_as::<_, Product>("SELECT * FROM products WHERE sku = $1")
            .bind(sku)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn update(&self, product: &Product) -> AppResult<Product> {
        Ok(sqlx::query_as::<_, Product>(
            r#"
            UPDATE products SET
                name = $2, description = $3, product_type = $4, category_id = $5,
                unit_of_measure = $6, cost_price = $7, sale_price = $8, tax_rate = $9,
                is_active = $10, barcode = $11, weight = $12, dimensions = $13, updated_at = $14
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(product.id)
        .bind(&product.name)
        .bind(&product.description)
        .bind(&product.product_type)
        .bind(product.category_id)
        .bind(&product.unit_of_measure)
        .bind(product.cost_price)
        .bind(product.sale_price)
        .bind(product.tax_rate)
        .bind(product.is_active)
        .bind(&product.barcode)
        .bind(product.weight)
        .bind(&product.dimensions)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM products WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &ProductFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Product>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM products WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "created_at")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<Product>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM products WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }
}

#[derive(Clone)]
pub struct PgProductCategoryRepository {
    pool: PgPool,
}

impl PgProductCategoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProductCategoryRepository for PgProductCategoryRepository {
    async fn create(&self, category: &ProductCategory) -> AppResult<ProductCategory> {
        Ok(sqlx::query_as::<_, ProductCategory>(
            r#"
            INSERT INTO product_categories (id, org_id, name, parent_id, created_at)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(category.id)
        .bind(category.org_id)
        .bind(&category.name)
        .bind(category.parent_id)
        .bind(category.created_at)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<ProductCategory>> {
        Ok(
            sqlx::query_as::<_, ProductCategory>("SELECT * FROM product_categories WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    async fn update(&self, category: &ProductCategory) -> AppResult<ProductCategory> {
        Ok(sqlx::query_as::<_, ProductCategory>(
            "UPDATE product_categories SET name = $2, parent_id = $3 WHERE id = $1 RETURNING *",
        )
        .bind(category.id)
        .bind(&category.name)
        .bind(category.parent_id)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM product_categories WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_all(&self) -> AppResult<Vec<ProductCategory>> {
        Ok(sqlx::query_as::<_, ProductCategory>(
            "SELECT * FROM product_categories ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?)
    }
}
