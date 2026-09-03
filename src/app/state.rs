use sqlx::{Pool, Postgres};

#[derive(Debug, Clone)]
pub struct AppState {
    pub database_pool: Pool<Postgres>,
    pub public_base_url: String,
}
