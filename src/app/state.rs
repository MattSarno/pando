use sqlx::{Pool, Postgres};

#[derive(Clone)]
pub struct AppState {
    pub database_pool: Pool<Postgres>,
}
