use pando::app::{AppState, routes};
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let host_url = std::env::var("HOST_URL")?;
    let database_url = std::env::var("DATABASE_URL")?;
    let database_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let app_state = AppState { database_pool };
    let app = routes::router().with_state(app_state);

    let listener = tokio::net::TcpListener::bind(host_url).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
