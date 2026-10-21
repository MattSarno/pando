use crate::app::web::health::{health_check, readiness_check};
use crate::app::web::oauth::authorize::{authorize_form, authorize_submit};
use crate::app::web::oauth::middleware::require_bearer_token;
use crate::app::web::oauth::register::register_client;
use crate::app::web::oauth::token::request_token;
use crate::app::web::oauth::{
    authorization_server::auth_server, protected_resource::protected_resource,
};
use crate::app::{AppState, mcp::PandoTools};
use axum::extract::Request;
use axum::http::Uri;
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::post;
use axum::{Router, middleware, routing::get};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tower::ServiceBuilder;
use tower_http::request_id::{
    MakeRequestUuid, PropagateRequestIdLayer, RequestId, SetRequestIdLayer,
};
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::{Level, Span, info_span};

const REQUEST_ID_HEADER: &str = "x-request-id";

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

    let request_logging = ServiceBuilder::new()
        .layer(middleware::from_fn(strip_client_request_id))
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(request_span)
                .on_response(DefaultOnResponse::new().level(Level::INFO))
                .on_failure(()),
        )
        .layer(PropagateRequestIdLayer::x_request_id());

    Router::new()
        .route("/ready", get(readiness_check))
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
        .layer(request_logging)
        // Registered after the layer so Fly's 30s health checks stay out of the logs.
        .route("/health", get(health_check))
}

async fn strip_client_request_id(mut request: Request, next: Next) -> Response {
    request.headers_mut().remove(REQUEST_ID_HEADER);
    next.run(request).await
}

fn request_span(request: &Request) -> Span {
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .and_then(|id| id.header_value().to_str().ok())
        .unwrap_or("unknown");

    info_span!(
        "request",
        request_id = %request_id,
        method = %request.method(),
        path = %request.uri().path(),
    )
}

fn allowed_mcp_hosts(public_base_url: &str) -> Vec<String> {
    let mut hosts = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];

    if let Ok(uri) = public_base_url.parse::<Uri>()
        && let Some(authority) = uri.authority()
    {
        hosts.push(authority.as_str().to_string());
    }

    hosts
}
