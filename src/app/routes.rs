use crate::app::AppState;
use crate::app::health::health_check;
use axum::{Router, routing::get};

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health_check))
}
