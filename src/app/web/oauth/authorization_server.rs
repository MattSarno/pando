use crate::app::AppState;
use axum::{Json, extract::State};
use serde::Serialize;

#[derive(Serialize)]
pub struct AuthorizationServerMetadata {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    registration_endpoint: String,
    response_types_supported: &'static [&'static str],
    grant_types_supported: &'static [&'static str],
    code_challenge_methods_supported: &'static [&'static str],
    token_endpoint_auth_methods_supported: &'static [&'static str],
}

pub async fn auth_server(State(state): State<AppState>) -> Json<AuthorizationServerMetadata> {
    Json(AuthorizationServerMetadata {
        issuer: state.public_base_url.clone(),
        authorization_endpoint: build_url(&state.public_base_url, "authorize"),
        token_endpoint: build_url(&state.public_base_url, "token"),
        registration_endpoint: build_url(&state.public_base_url, "register"),
        response_types_supported: &["code"],
        grant_types_supported: &["authorization_code", "refresh_token"],
        code_challenge_methods_supported: &["S256"],
        token_endpoint_auth_methods_supported: &["none"],
    })
}

fn build_url(base: &str, segment: &str) -> String {
    let base = base.trim_end_matches('/');
    format!("{}/{}", base, segment)
}
