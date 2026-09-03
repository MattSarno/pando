use crate::app::AppState;
use axum::{Json, extract::State};
use serde::Serialize;

#[derive(Serialize)]
pub struct ProtectedResourceMetadata {
    resource: String,
    authorization_servers: [String; 1],
    bearer_methods_supported: &'static [&'static str],
}

pub async fn protected_resource(State(state): State<AppState>) -> Json<ProtectedResourceMetadata> {
    Json(ProtectedResourceMetadata {
        resource: build_url(&state.public_base_url, "mcp"),
        authorization_servers: [state.public_base_url],
        bearer_methods_supported: &["header"],
    })
}

fn build_url(base: &str, segment: &str) -> String {
    let base = base.trim_end_matches('/');
    format!("{}/{}", base, segment)
}
