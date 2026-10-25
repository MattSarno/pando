use crate::app::mcp::tool_errors::db_error;
use crate::app::web::oauth::middleware::AuthenticatedClient;
use axum::http::request::Parts;
use rmcp::ErrorData;
use sqlx::{PgPool, Postgres, Transaction};

pub async fn begin_as(
    pool: &PgPool,
    parts: &Parts,
    actor: &str,
) -> Result<Transaction<'static, Postgres>, ErrorData> {
    let Some(AuthenticatedClient(client_id)) = parts.extensions.get::<AuthenticatedClient>() else {
        return Err(ErrorData::internal_error(
            "mutation reached a tool without an authenticated client",
            None,
        ));
    };

    let mut transaction = match pool.begin().await {
        Ok(transaction) => transaction,
        Err(error) => return Err(db_error("failed to start transaction", error)),
    };

    if let Err(error) = sqlx::query(
        "SELECT set_config('pando.actor', $1, true), set_config('pando.client_id', $2, true)",
    )
    .bind(actor)
    .bind(client_id)
    .execute(&mut *transaction)
    .await
    {
        return Err(db_error("failed to set audit actor", error));
    }

    Ok(transaction)
}

pub async fn commit(transaction: Transaction<'static, Postgres>) -> Result<(), ErrorData> {
    match transaction.commit().await {
        Ok(()) => Ok(()),
        Err(error) => Err(db_error("failed to commit transaction", error)),
    }
}
