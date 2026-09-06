use axum::{
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};
use crate::app::AppState;

pub async fn require_bearer_token(
    State(app_state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));

    let Some(token) = token else {
        return unauthorized(&app_state.public_base_url);
    };

    let token_hash = hex::encode(Sha256::digest(token.as_bytes()));
    let valid = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM oauth_tokens
        WHERE access_token_hash = $1 AND revoked_at IS NULL AND access_expires_at > now()",
    )
    .bind(&token_hash)
    .fetch_optional(&app_state.database_pool)
    .await;

    match valid {
        Ok(Some(_)) => next.run(request).await,
        Ok(None) => unauthorized(&app_state.public_base_url),
        Err(error) => {
            eprintln!("require_bearer_token: failed to look up token: {error}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn unauthorized(public_base_url: &str) -> Response {
    let mut response = StatusCode::UNAUTHORIZED.into_response();
    let value = format!(
        r#"Bearer resource_metadata="{public_base_url}/.well-known/oauth-protected-resource""#
    );
    
    if let Ok(header_value) = HeaderValue::from_str(&value) {
        response
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, header_value);
    }

    response
}
