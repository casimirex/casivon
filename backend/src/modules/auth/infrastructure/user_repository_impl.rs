use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::modules::auth::application::dto::UserFilters;
use crate::modules::auth::domain::entities::User;
use crate::modules::auth::domain::repositories::UserRepository;
use crate::shared::pagination::PaginationParams;

/// Sortable columns. `password_hash` is not among them, and neither is anything
/// else a caller might hope to order by to infer its contents.
const SORTABLE: [&str; 5] = ["created_at", "updated_at", "first_name", "last_name", "email"];

#[derive(Clone)]
pub struct PgUserRepository {
    pool: PgPool,
}

impl PgUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a UserFilters) {
    if let Some(role) = &filters.role {
        builder.push(" AND role = ").push_bind(role);
    }
    if let Some(is_active) = filters.is_active {
        builder.push(" AND is_active = ").push_bind(is_active);
    }
    if let Some(search) = &filters.search {
        let pattern = format!("%{}%", search);
        builder
            .push(" AND (first_name ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR last_name ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR email ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
}

#[async_trait]
impl UserRepository for PgUserRepository {
    async fn create(&self, user: &User) -> AppResult<User> {
        let row = sqlx::query_as::<_, User>(
            r#"
            INSERT INTO users (id, email, password_hash, first_name, last_name, role, org_id, is_active, email_verified, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING *
            "#
        )
        .bind(user.id)
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(&user.first_name)
        .bind(&user.last_name)
        .bind(&user.role)
        .bind(user.org_id)
        .bind(user.is_active)
        .bind(user.email_verified)
        .bind(user.created_at)
        .bind(user.updated_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(db_err) if db_err.constraint().is_some() => {
                AppError::Conflict("User with this email already exists".to_string())
            }
            _ => AppError::Database(e),
        })?;

        Ok(row)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<User>> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(user)
    }

    async fn find_by_email(&self, email: &str) -> AppResult<Option<User>> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;
        Ok(user)
    }

    async fn update(&self, user: &User) -> AppResult<User> {
        let row = sqlx::query_as::<_, User>(
            r#"
            UPDATE users
            SET email = $2, first_name = $3, last_name = $4, role = $5,
                org_id = $6, is_active = $7, email_verified = $8, updated_at = $9
            WHERE id = $1
            RETURNING *
            "#
        )
        .bind(user.id)
        .bind(&user.email)
        .bind(&user.first_name)
        .bind(&user.last_name)
        .bind(&user.role)
        .bind(user.org_id)
        .bind(user.is_active)
        .bind(user.email_verified)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &UserFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<User>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM users WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "created_at")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let users = query.build_query_as::<User>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM users WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((users, total))
    }

    async fn count(&self) -> AppResult<i64> {
        Ok(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await?)
    }

    async fn replace_password(&self, user_id: Uuid, password_hash: &str) -> AppResult<()> {
        // Incremented in SQL rather than read-modify-written, so two concurrent
        // resets cannot both stamp the same epoch.
        sqlx::query(
            "UPDATE users SET password_hash = $2, session_epoch = session_epoch + 1 WHERE id = $1",
        )
        .bind(user_id)
        .bind(password_hash)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn mark_email_verified(&self, user_id: Uuid) -> AppResult<()> {
        sqlx::query("UPDATE users SET email_verified = true, updated_at = NOW() WHERE id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}
