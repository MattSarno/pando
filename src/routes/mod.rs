mod health;

use crate::AppState;
use axum::{Router, routing::get};
use health::health_check;

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health_check))
}
