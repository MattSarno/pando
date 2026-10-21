use pando::app::web::rate_limit::{RateLimitConfig, RateLimiter};
use pando::app::{AppState, routes};
use sqlx::postgres::PgPoolOptions;
use std::{str::FromStr, sync::Arc, time::Duration};
use tracing_subscriber::EnvFilter;

const DEFAULT_LOG_FILTER: &str = "pando=info,tower_http=info,warn";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    init_tracing()?;
    let host_url = std::env::var("HOST_URL").map_err(|_| "HOST_URL must be set")?;
    let database_url = std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL must be set")?;
    let database_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let public_base_url =
        std::env::var("PUBLIC_BASE_URL").map_err(|_| "PUBLIC_BASE_URL must be set")?;

    let admin_passphrase =
        std::env::var("ADMIN_PASSPHRASE").map_err(|_| "ADMIN_PASSPHRASE must be set")?;

    let rate_limit_config = RateLimitConfig {
        window: Duration::from_secs(required_env("RATE_LIMIT_WINDOW_SECS")?),
        register_max: required_env("RATE_LIMIT_REGISTER_MAX")?,
        token_max: required_env("RATE_LIMIT_TOKEN_MAX")?,
        authorize_max: required_env("RATE_LIMIT_AUTHORIZE_MAX")?,
        passphrase_failure_max: required_env("PASSPHRASE_FAILURE_MAX")?,
        passphrase_lockout: Duration::from_secs(required_env("PASSPHRASE_LOCKOUT_SECS")?),
    };

    let app_state = AppState {
        database_pool,
        public_base_url,
        admin_passphrase,
        rate_limiter: Arc::new(RateLimiter::new(rate_limit_config)),
    };
    let app = routes::router(app_state);

    let listener = tokio::net::TcpListener::bind(host_url).await?;
    tracing::info!(address = %listener.local_addr()?, "listening");
    axum::serve(listener, app).await?;

    Ok(())
}

fn init_tracing() -> Result<(), String> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG_FILTER));
    let subscriber = tracing_subscriber::fmt().with_env_filter(filter);

    match std::env::var("LOG_FORMAT").as_deref() {
        Ok("json") => subscriber.json().with_span_list(false).init(),
        Ok("text") | Err(_) => subscriber.init(),
        Ok(_) => return Err("LOG_FORMAT must be \"json\" or \"text\"".to_string()),
    }

    Ok(())
}

fn required_env<T: FromStr>(name: &str) -> Result<T, String> {
    std::env::var(name)
        .map_err(|_| format!("{name} must be set"))?
        .parse()
        .map_err(|_| format!("{name} must be a non-negative integer"))
}
