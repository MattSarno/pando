use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use pando::app::web::rate_limit::{RateLimitConfig, RateLimiter};
use pando::app::{AppState, routes};
use sqlx::postgres::PgPoolOptions;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tower::ServiceExt;
use tracing::Level;
use tracing::subscriber::DefaultGuard;
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone, Default)]
struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

impl CapturedLogs {
    fn install() -> (Self, DefaultGuard) {
        let logs = Self::default();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(Level::DEBUG)
            .with_ansi(false)
            .with_writer(logs.clone())
            .finish();
        let guard = tracing::subscriber::set_default(subscriber);
        (logs, guard)
    }

    fn contents(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl io::Write for CapturedLogs {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for CapturedLogs {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn test_app(request_max: u32) -> Router {
    let database_pool = PgPoolOptions::new()
        .connect_lazy("postgres://unused@127.0.0.1:1/unused")
        .unwrap();

    routes::router(AppState {
        database_pool,
        public_base_url: "http://localhost".to_string(),
        admin_passphrase: "correct-passphrase".to_string(),
        rate_limiter: Arc::new(RateLimiter::new(RateLimitConfig {
            window: Duration::from_secs(60),
            register_max: request_max,
            token_max: request_max,
            authorize_max: request_max,
            passphrase_failure_max: 5,
            passphrase_lockout: Duration::from_secs(60),
        })),
    })
}

async fn send(app: &Router, request: Request<Body>) -> Response {
    app.clone().oneshot(request).await.unwrap()
}

fn request_id(response: &Response) -> String {
    response.headers()["x-request-id"]
        .to_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn health_checks_are_not_logged() {
    let (logs, _guard) = CapturedLogs::install();
    let app = test_app(10);

    let health = send(&app, Request::get("/health").body(Body::empty()).unwrap()).await;
    let ready = send(&app, Request::get("/ready").body(Body::empty()).unwrap()).await;

    assert!(!health.headers().contains_key("x-request-id"));
    assert!(!logs.contents().contains("/health"));
    assert!(logs.contents().contains(&request_id(&ready)));
}

#[tokio::test]
async fn request_logs_omit_credentials_and_query_strings() {
    let (logs, _guard) = CapturedLogs::install();
    let app = test_app(10);

    let request = Request::get("/ready?code=SECRET_QUERY")
        .header("authorization", "Bearer SECRET_TOKEN")
        .header("x-pando-passphrase", "SECRET_PASSPHRASE")
        .header("fly-client-ip", "203.0.113.7")
        .body(Body::empty())
        .unwrap();
    let response = send(&app, request).await;

    let logs = logs.contents();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(logs.contains(&format!("request_id={}", request_id(&response))));
    assert!(logs.contains("path=/ready"));
    assert!(!logs.contains("SECRET"));
    assert!(!logs.contains("203.0.113.7"));
}

#[tokio::test]
async fn client_supplied_request_ids_are_replaced() {
    let (logs, _guard) = CapturedLogs::install();
    let app = test_app(10);

    let request = Request::get("/ready")
        .header("x-request-id", "client-chosen-id")
        .body(Body::empty())
        .unwrap();
    let response = send(&app, request).await;

    assert_ne!(request_id(&response), "client-chosen-id");
    assert!(logs.contents().contains(&request_id(&response)));
    assert!(!logs.contents().contains("client-chosen-id"));
}

#[tokio::test]
async fn malformed_bodies_count_toward_rate_limits() {
    let (_logs, _guard) = CapturedLogs::install();
    let app = test_app(1);

    for path in ["/register", "/token", "/authorize"] {
        let first = send(&app, Request::post(path).body(Body::empty()).unwrap()).await;
        let second = send(&app, Request::post(path).body(Body::empty()).unwrap()).await;

        assert_ne!(first.status(), StatusCode::TOO_MANY_REQUESTS, "{path}");
        assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS, "{path}");
    }
}

#[tokio::test]
async fn rate_limit_denials_are_logged_once_per_window() {
    let (logs, _guard) = CapturedLogs::install();
    let app = test_app(1);

    for _ in 0..4 {
        let request = Request::post("/register")
            .header("fly-client-ip", "203.0.113.7")
            .body(Body::empty())
            .unwrap();
        send(&app, request).await;
    }

    let logs = logs.contents();
    assert_eq!(logs.matches("rate limit reached").count(), 1);
    assert!(logs.contains("ip=203.0.113.7"));
    assert_eq!(logs.matches("status=429").count(), 3);
}
