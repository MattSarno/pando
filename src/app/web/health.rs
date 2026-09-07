use crate::app::AppState;
use axum::{extract::State, http::StatusCode};

/// Process liveness. Takes no extractors so it cannot reach the database pool,
/// which keeps Fly's frequent health check from holding the Neon compute awake.
pub async fn health_check() -> StatusCode {
    StatusCode::OK
}

/// Readiness, including a database round trip. Nothing polls this on an
/// interval — each call wakes the Neon compute.
pub async fn readiness_check(State(state): State<AppState>) -> StatusCode {
    let result = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.database_pool)
        .await;

    match result {
        Ok(_) => StatusCode::OK,
        Err(error) => {
            eprintln!("readiness: database probe failed: {error}");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
