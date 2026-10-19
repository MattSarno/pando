use crate::app::web::rate_limit::RateLimiter;
use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct AppState {
    pub database_pool: Pool<Postgres>,
    pub public_base_url: String,
    pub admin_passphrase: String,
    pub rate_limiter: Arc<RateLimiter>,
}
