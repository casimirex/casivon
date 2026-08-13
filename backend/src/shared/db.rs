use sqlx::PgPool;

#[derive(Clone)]
pub struct DbPool(pub PgPool);

impl std::ops::Deref for DbPool {
    type Target = PgPool;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
