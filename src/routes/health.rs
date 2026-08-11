use crate::AppState;
use axum::{extract::State, http::StatusCode};

pub async fn health_check(State(state): State<AppState>) -> StatusCode {
    let result = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.database_pool)
        .await;

    match result {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}
