use crate::app::AppState;
use crate::app::web::rate_limit::{Bucket, client_ip};
use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use rand::distr::{Alphanumeric, SampleString};
use serde::{Deserialize, Serialize};
use tracing::error;

#[derive(Deserialize)]
pub struct RegisterRequest {
    redirect_uris: Vec<String>,
    client_name: Option<String>,
}

#[derive(Serialize)]
pub struct RegisterSuccess {
    client_id: String,
    redirect_uris: Vec<String>,
    client_name: Option<String>,
}

#[derive(Serialize)]
pub struct RegisterError {
    error: String,
    error_description: String,
}

pub async fn register_client(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<RegisterRequest>, JsonRejection>,
) -> Response {
    if !state
        .rate_limiter
        .check(Bucket::Register, client_ip(&headers))
    {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(RegisterError {
                error: "temporarily_unavailable".to_string(),
                error_description: "too many registration attempts, try again later".to_string(),
            }),
        )
            .into_response();
    }

    let Json(body) = match body {
        Ok(body) => body,
        Err(rejection) => return rejection.into_response(),
    };

    if body.redirect_uris.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(RegisterError {
                error: "invalid_client_metadata".to_string(),
                error_description: "redirect URIs cannot be empty".to_string(),
            }),
        )
            .into_response();
    }

    let client_id = Alphanumeric.sample_string(&mut rand::rng(), 32);
    let db_response = sqlx::query(
        "INSERT INTO oauth_clients (client_id, redirect_uris, client_name) VALUES ($1, $2, $3)",
    )
    .bind(&client_id)
    .bind(&body.redirect_uris)
    .bind(&body.client_name)
    .execute(&state.database_pool)
    .await;

    match db_response {
        Ok(_) => (
            StatusCode::CREATED,
            Json(RegisterSuccess {
                client_id,
                redirect_uris: body.redirect_uris,
                client_name: body.client_name,
            }),
        )
            .into_response(),
        Err(error) => {
            error!(%error, "failed to insert oauth client");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
