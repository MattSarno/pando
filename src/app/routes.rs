use crate::app::web::health::health_check;
use crate::app::web::oauth::{authorization_server::auth_server, protected_resource::protected_resource};
use crate::app::{AppState, mcp::PandoTools};
use axum::{Router, routing::get};
use rmcp::transport::streamable_http_server::{
    StreamableHttpService, session::local::LocalSessionManager,
};

pub fn router(state: AppState) -> Router {
    let mcp_state = state.clone();
    let mcp_service = StreamableHttpService::new(
        move || Ok(PandoTools::new(mcp_state.clone())),
        LocalSessionManager::default().into(),
        Default::default(),
    );

    Router::new()
        .route("/health", get(health_check))
        .route("/.well-known/oauth-authorization-server", get(auth_server))
        .route("/.well-known/oauth-protected-resource", get(protected_resource))
        .with_state(state)
        .nest_service("/mcp", mcp_service)
}
