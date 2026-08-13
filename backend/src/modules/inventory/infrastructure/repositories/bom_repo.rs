use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::inventory::domain::entities::{BillOfMaterials, BomLine};
use crate::modules::inventory::domain::repositories::BomRepository;
use crate::shared::pagination::PaginationParams;

#[derive(Clone)]
pub struct PgBomRepository {
    pool: PgPool,
}

impl PgBomRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

async fn insert_lines(tx: &mut Transaction<'_, Postgres>, lines: &[BomLine]) -> AppResult<()> {
    for line in lines {
        sqlx::query(
            r#"
            INSERT INTO bom_lines
                (id, bom_id, component_id, quantity_required, unit_of_measure, sort_order)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(line.id)
        .bind(line.bom_id)
        .bind(line.component_id)
        .bind(line.quantity_required)
        .bind(&line.unit_of_measure)
        .bind(line.sort_order)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

#[async_trait]
impl BomRepository for PgBomRepository {
    async fn create(&self, bom: &BillOfMaterials, lines: &[BomLine]) -> AppResult<BillOfMaterials> {
        let mut tx = self.pool.begin().await?;

        let created = sqlx::query_as::<_, BillOfMaterials>(
            r#"
            INSERT INTO bills_of_materials
                (id, org_id, product_id, version, quantity_to_produce, is_active,
                 created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            "#,
        )
        .bind(bom.id)
        .bind(bom.org_id)
        .bind(bom.product_id)
        .bind(&bom.version)
        .bind(bom.quantity_to_produce)
        .bind(bom.is_active)
        .bind(bom.created_at)
        .bind(bom.updated_at)
        .fetch_one(&mut *tx)
        .await?;

        insert_lines(&mut tx, lines).await?;
        tx.commit().await?;
        Ok(created)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<BillOfMaterials>> {
        Ok(
            sqlx::query_as::<_, BillOfMaterials>("SELECT * FROM bills_of_materials WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    async fn find_lines(&self, bom_id: Uuid) -> AppResult<Vec<BomLine>> {
        Ok(
            sqlx::query_as::<_, BomLine>("SELECT * FROM bom_lines WHERE bom_id = $1 ORDER BY sort_order")
                .bind(bom_id)
                .fetch_all(&self.pool)
                .await?,
        )
    }

    async fn find_by_product_version(
        &self,
        product_id: Uuid,
        version: &str,
    ) -> AppResult<Option<BillOfMaterials>> {
        Ok(sqlx::query_as::<_, BillOfMaterials>(
            "SELECT * FROM bills_of_materials WHERE product_id = $1 AND version = $2",
        )
        .bind(product_id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?)
    }

    async fn update(
        &self,
        bom: &BillOfMaterials,
        lines: Option<&[BomLine]>,
    ) -> AppResult<BillOfMaterials> {
        let mut tx = self.pool.begin().await?;

        let updated = sqlx::query_as::<_, BillOfMaterials>(
            r#"
            UPDATE bills_of_materials
            SET quantity_to_produce = $2, is_active = $3, updated_at = $4
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(bom.id)
        .bind(bom.quantity_to_produce)
        .bind(bom.is_active)
        .bind(Utc::now())
        .fetch_one(&mut *tx)
        .await?;

        if let Some(lines) = lines {
            sqlx::query("DELETE FROM bom_lines WHERE bom_id = $1")
                .bind(bom.id)
                .execute(&mut *tx)
                .await?;
            insert_lines(&mut tx, lines).await?;
        }

        tx.commit().await?;
        Ok(updated)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM bills_of_materials WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        product_id: Option<Uuid>,
        params: &PaginationParams,
    ) -> AppResult<(Vec<BillOfMaterials>, i64)> {
        let rows = sqlx::query_as::<_, BillOfMaterials>(
            r#"
            SELECT * FROM bills_of_materials
            WHERE ($1::uuid IS NULL OR product_id = $1)
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(product_id)
        .bind(params.per_page())
        .bind(params.offset())
        .fetch_all(&self.pool)
        .await?;

        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM bills_of_materials WHERE ($1::uuid IS NULL OR product_id = $1)",
        )
        .bind(product_id)
        .fetch_one(&self.pool)
        .await?;

        Ok((rows, total))
    }
}
