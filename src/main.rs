use pando::app::{AppState, routes};
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
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

    let app_state = AppState {
        database_pool,
        public_base_url,
        admin_passphrase,
    };
    let app = routes::router(app_state);

    let listener = tokio::net::TcpListener::bind(host_url).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
