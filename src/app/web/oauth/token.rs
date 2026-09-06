use axum::{
    Form, Json,
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::distr::{Alphanumeric, SampleString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};

use crate::app::AppState;

const ACCESS_TOKEN_TTL_SECONDS: i64 = 3600;
const REFRESH_TOKEN_TTL_DAYS: i64 = 30;

#[derive(Deserialize)]
pub struct TokenRequest {
    grant_type: String,
    client_id: String,
    redirect_uri: Option<String>,
    code: Option<String>,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Serialize)]
pub struct TokenResponse {
    access_token: String,
    token_type: &'static str,
    expires_in: i64,
    refresh_token: String,
}

#[derive(Serialize)]
pub struct TokenError {
    error: String,
    error_description: String,
}

struct AuthorizationCodeGrant {
    client_id: String,
    redirect_uri: String,
    code: String,
    code_verifier: String,
}

struct RefreshTokenGrant {
    client_id: String,
    refresh_token: String,
}

pub async fn request_token(
    State(app_state): State<AppState>,
    Form(params): Form<TokenRequest>,
) -> Response {
    match params.grant_type.as_str() {
        "authorization_code" => {
            let Some(code) = params.code else {
                return invalid_request("code is required");
            };
            let Some(redirect_uri) = params.redirect_uri else {
                return invalid_request("redirect_uri is required");
            };
            let Some(code_verifier) = params.code_verifier else {
                return invalid_request("code_verifier is required");
            };

            let grant = AuthorizationCodeGrant {
                client_id: params.client_id,
                redirect_uri,
                code,
                code_verifier,
            };
            authorization_code(grant, &app_state).await
        }
        "refresh_token" => {
            let Some(refresh_token) = params.refresh_token else {
                return invalid_request("refresh_token is required");
            };

            let grant = RefreshTokenGrant {
                client_id: params.client_id,
                refresh_token,
            };
            refresh(grant, &app_state).await
        }
        _ => (
            StatusCode::BAD_REQUEST,
            Json(TokenError {
                error: "unsupported_grant_type".to_string(),
                error_description: format!("grant_type: {} not supported", params.grant_type),
            }),
        )
            .into_response(),
    }
}

fn invalid_request(msg: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(TokenError {
            error: "invalid_request".into(),
            error_description: msg.into(),
        }),
    )
        .into_response()
}

fn token_error(error: &str, description: impl Into<String>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(TokenError {
            error: error.to_string(),
            error_description: description.into(),
        }),
    )
        .into_response()
}

fn token_response(access_token: String, refresh_token: String) -> Response {
    let mut response = (
        StatusCode::OK,
        Json(TokenResponse {
            access_token,
            token_type: "Bearer",
            expires_in: ACCESS_TOKEN_TTL_SECONDS,
            refresh_token,
        }),
    )
        .into_response();

    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));

    response
}

async fn authorization_code(grant: AuthorizationCodeGrant, app_state: &AppState) -> Response {
    let code_challenge: Option<String> = match sqlx::query_scalar(
        "UPDATE oauth_auth_codes
        SET used_at = now()
        WHERE code = $1 AND client_id = $2 AND redirect_uri = $3
        AND used_at IS NULL AND expires_at > now()
        RETURNING code_challenge",
    )
    .bind(&grant.code)
    .bind(&grant.client_id)
    .bind(&grant.redirect_uri)
    .fetch_optional(&app_state.database_pool)
    .await
    {
        Ok(code_challenge) => code_challenge,
        Err(error) => {
            eprintln!("authorization_code: failed to update code_challenge: {error}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let Some(code_challenge) = code_challenge else {
        return token_error("invalid_grant", "invalid or expired authorization code");
    };

    let computed_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(grant.code_verifier.as_bytes()));
    if computed_challenge != code_challenge {
        return token_error(
            "invalid_grant",
            "code_verifier does not match code_challenge",
        );
    }

    let access_token = Alphanumeric.sample_string(&mut rand::rng(), 43);
    let refresh_token = Alphanumeric.sample_string(&mut rand::rng(), 43);
    let access_expires_at = OffsetDateTime::now_utc() + Duration::seconds(ACCESS_TOKEN_TTL_SECONDS);
    let refresh_expires_at = OffsetDateTime::now_utc() + Duration::days(REFRESH_TOKEN_TTL_DAYS);

    let insert_result = sqlx::query(
        "INSERT INTO oauth_tokens (client_id, access_token_hash, refresh_token_hash, access_expires_at, refresh_expires_at)
        VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&grant.client_id)
    .bind(hex::encode(Sha256::digest(access_token.as_bytes())))
    .bind(hex::encode(Sha256::digest(refresh_token.as_bytes())))
    .bind(access_expires_at)
    .bind(refresh_expires_at)
    .execute(&app_state.database_pool)
    .await;

    if let Err(error) = insert_result {
        eprintln!("authorization_code: failed to insert token: {error}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    token_response(access_token, refresh_token)
}

async fn refresh(grant: RefreshTokenGrant, app_state: &AppState) -> Response {
    let refresh_token_hash = hex::encode(Sha256::digest(grant.refresh_token.as_bytes()));

    let mut transaction = match app_state.database_pool.begin().await {
        Ok(tx) => tx,
        Err(error) => {
            eprintln!("refresh: failed to start transaction: {error}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let revoke_result = sqlx::query(
        "UPDATE oauth_tokens
        SET revoked_at = now()
        WHERE refresh_token_hash = $1 AND client_id = $2
        AND revoked_at IS NULL AND refresh_expires_at > now()",
    )
    .bind(&refresh_token_hash)
    .bind(&grant.client_id)
    .execute(&mut *transaction)
    .await;

    let rows_affected = match revoke_result {
        Ok(result) => result.rows_affected(),
        Err(error) => {
            eprintln!("refresh: failed to revoke old token: {error}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if rows_affected == 0 {
        return token_error(
            "invalid_grant",
            "invalid, expired, or revoked refresh token",
        );
    }

    let access_token = Alphanumeric.sample_string(&mut rand::rng(), 43);
    let refresh_token = Alphanumeric.sample_string(&mut rand::rng(), 43);
    let access_expires_at = OffsetDateTime::now_utc() + Duration::seconds(ACCESS_TOKEN_TTL_SECONDS);
    let refresh_expires_at = OffsetDateTime::now_utc() + Duration::days(REFRESH_TOKEN_TTL_DAYS);

    let insert_result = sqlx::query(
        "INSERT INTO oauth_tokens (client_id, access_token_hash, refresh_token_hash, access_expires_at, refresh_expires_at)
        VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&grant.client_id)
    .bind(hex::encode(Sha256::digest(access_token.as_bytes())))
    .bind(hex::encode(Sha256::digest(refresh_token.as_bytes())))
    .bind(access_expires_at)
    .bind(refresh_expires_at)
    .execute(&mut *transaction)
    .await;

    if let Err(error) = insert_result {
        eprintln!("refresh: failed to insert new token: {error}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    if let Err(error) = transaction.commit().await {
        eprintln!("refresh: failed to commit transaction: {error}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    token_response(access_token, refresh_token)
}
