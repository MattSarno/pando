use crate::app::AppState;
use crate::app::web::rate_limit::client_ip;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
};
use subtle::ConstantTimeEq;
use tracing::error;

const PASSPHRASE_HEADER: &str = "X-Pando-Passphrase";

pub async fn health_check() -> StatusCode {
    StatusCode::OK
}

pub async fn readiness_check(State(state): State<AppState>, headers: HeaderMap) -> StatusCode {
    let ip = client_ip(&headers);
    if state.rate_limiter.is_locked_out(ip) {
        return StatusCode::TOO_MANY_REQUESTS;
    }

    let Some(passphrase) = headers.get(PASSPHRASE_HEADER) else {
        return StatusCode::UNAUTHORIZED;
    };

    let authorized = bool::from(
        passphrase
            .as_bytes()
            .ct_eq(state.admin_passphrase.as_bytes()),
    );
    if !authorized {
        state.rate_limiter.record_passphrase_failure(ip);
        return StatusCode::UNAUTHORIZED;
    }
    state.rate_limiter.clear_passphrase_failures(ip);

    let result = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.database_pool)
        .await;

    match result {
        Ok(_) => StatusCode::OK,
        Err(error) => {
            error!(%error, "database probe failed");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
