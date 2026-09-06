use crate::app::web::health::health_check;
use crate::app::web::oauth::authorize::{authorize_form, authorize_submit};
use crate::app::web::oauth::middleware::require_bearer_token;
use crate::app::web::oauth::register::register_client;
use crate::app::web::oauth::token::request_token;
use crate::app::web::oauth::{
    authorization_server::auth_server, protected_resource::protected_resource,
};
use crate::app::{AppState, mcp::PandoTools};
use axum::http::Uri;
use axum::routing::post;
use axum::{Router, middleware, routing::get};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tower::ServiceBuilder;

pub fn router(state: AppState) -> Router {
    let mcp_state = state.clone();
    let mcp_config = StreamableHttpServerConfig::default()
        .with_allowed_hosts(allowed_mcp_hosts(&state.public_base_url));
    let mcp_service = StreamableHttpService::new(
        move || Ok(PandoTools::new(mcp_state.clone())),
        LocalSessionManager::default().into(),
        mcp_config,
    );

    let protected_mcp_service = ServiceBuilder::new()
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_bearer_token,
        ))
        .service(mcp_service);

    Router::new()
        .route("/health", get(health_check))
        .route("/.well-known/oauth-authorization-server", get(auth_server))
        .route(
            "/.well-known/oauth-protected-resource",
            get(protected_resource),
        )
        .route("/register", post(register_client))
        .route("/authorize", get(authorize_form))
        .route("/authorize", post(authorize_submit))
        .route("/token", post(request_token))
        .with_state(state)
        .nest_service("/mcp", protected_mcp_service)
}

fn allowed_mcp_hosts(public_base_url: &str) -> Vec<String> {
    let mut hosts = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];

    if let Ok(uri) = public_base_url.parse::<Uri>() {
        if let Some(authority) = uri.authority() {
            hosts.push(authority.as_str().to_string());
        }
    }

    hosts
}
