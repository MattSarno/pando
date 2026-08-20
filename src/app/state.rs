use sqlx::{Pool, Postgres};

#[derive(Debug, Clone)]
pub struct AppState {
    pub database_pool: Pool<Postgres>,
}
