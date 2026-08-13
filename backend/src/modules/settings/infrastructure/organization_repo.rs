use async_trait::async_trait;
use sqlx::PgPool;

use crate::error::AppResult;
use crate::modules::settings::application::dto::UpdateOrganizationRequest;
use crate::modules::settings::domain::entities::OrganizationSettings;
use crate::modules::settings::domain::repositories::OrganizationRepository;

#[derive(Clone)]
pub struct PgOrganizationRepository {
    pool: PgPool,
}

impl PgOrganizationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OrganizationRepository for PgOrganizationRepository {
    async fn get(&self) -> AppResult<OrganizationSettings> {
        let row = sqlx::query_as::<_, OrganizationSettings>(
            "SELECT * FROM organization_settings WHERE singleton",
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }

    async fn has_financial_documents(&self) -> AppResult<bool> {
        // The documents that carry a currency *and* an amount. Reference data
        // (vendors, employees, accounts) is not included: those carry a currency
        // but nothing that would be misread as a different sum of money.
        let exists = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (SELECT 1 FROM quotes)
                OR EXISTS (SELECT 1 FROM sales_orders)
                OR EXISTS (SELECT 1 FROM invoices)
                OR EXISTS (SELECT 1 FROM payments)
                OR EXISTS (SELECT 1 FROM purchase_orders)
                OR EXISTS (SELECT 1 FROM general_ledger_entries)
                OR EXISTS (SELECT 1 FROM expense_reports)
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(exists)
    }

    async fn update(&self, req: &UpdateOrganizationRequest) -> AppResult<OrganizationSettings> {
        // Three cases per nullable field, and they are all different:
        //   omitted   -> parameter is NULL -> keep what is stored
        //   ""        -> clear the column
        //   a value   -> store it
        //
        // `COALESCE($n, column)` alone cannot express this, because it cannot
        // tell an omitted field from one the form is trying to clear. The two
        // NOT NULL columns have no clearing case, so they do use COALESCE.
        let row = sqlx::query_as::<_, OrganizationSettings>(
            r#"
            UPDATE organization_settings SET
                name             = COALESCE($1, name),
                legal_name       = CASE WHEN $2::text  IS NULL THEN legal_name    ELSE NULLIF($2, '')  END,
                email            = CASE WHEN $3::text  IS NULL THEN email         ELSE NULLIF($3, '')  END,
                phone            = CASE WHEN $4::text  IS NULL THEN phone         ELSE NULLIF($4, '')  END,
                website          = CASE WHEN $5::text  IS NULL THEN website       ELSE NULLIF($5, '')  END,
                tax_number       = CASE WHEN $6::text  IS NULL THEN tax_number    ELSE NULLIF($6, '')  END,
                address_line1    = CASE WHEN $7::text  IS NULL THEN address_line1 ELSE NULLIF($7, '')  END,
                address_line2    = CASE WHEN $8::text  IS NULL THEN address_line2 ELSE NULLIF($8, '')  END,
                city             = CASE WHEN $9::text  IS NULL THEN city          ELSE NULLIF($9, '')  END,
                postal_code      = CASE WHEN $10::text IS NULL THEN postal_code   ELSE NULLIF($10, '') END,
                country          = CASE WHEN $11::text IS NULL THEN country       ELSE NULLIF($11, '') END,
                default_currency = COALESCE($12, default_currency),
                default_dispatch_warehouse_id =
                    CASE WHEN $13::text IS NULL THEN default_dispatch_warehouse_id
                         ELSE NULLIF($13, '')::uuid END
            WHERE singleton
            RETURNING *
            "#,
        )
        .bind(req.name.as_deref())
        .bind(req.legal_name.as_deref())
        .bind(req.email.as_deref())
        .bind(req.phone.as_deref())
        .bind(req.website.as_deref())
        .bind(req.tax_number.as_deref())
        .bind(req.address_line1.as_deref())
        .bind(req.address_line2.as_deref())
        .bind(req.city.as_deref())
        .bind(req.postal_code.as_deref())
        .bind(req.country.as_deref())
        .bind(req.default_currency.as_deref().map(str::to_uppercase))
        .bind(req.default_dispatch_warehouse_id.as_deref())
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }
}
