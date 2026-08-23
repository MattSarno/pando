use crate::app::tool_errors::db_error;
use crate::app::validation::{escape_like_wildcards, validate_key};
use crate::app::{mcp::PandoTools, tool_errors::error_result};
use rmcp::serde_json::json;
use rmcp::{
    ErrorData, handler::server::wrapper::Parameters, model::CallToolResult, tool, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::types::time::OffsetDateTime;

#[derive(Debug, Serialize, Deserialize, JsonSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
enum Scope {
    Profile,
    Topic,
    Preference,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
struct MemoryEntry {
    id: i64,
    scope: Scope,
    key: String,
    subject_id: Option<String>,
    description: String,
    content: String,
    updated_at: OffsetDateTime,
    updated_by: String,
}

#[derive(Deserialize, JsonSchema)]
struct MemoryCreateParams {
    scope: Scope,
    key: String,
    content: String,
    description: String,
    subject_id: Option<String>,
    updated_by: String,
}

#[derive(Deserialize, JsonSchema)]
struct MemoryReadParams {
    scope: Scope,
    key: String,
    subject_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct MemorySearchParams {
    query: String,
    scope: Option<Scope>,
    subject_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct MemoryListParams {
    scope: Option<Scope>,
    subject_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct MemoryUpdateParams {
    scope: Scope,
    key: String,
    subject_id: Option<String>,
    content: Option<String>,
    description: Option<String>,
    updated_by: String,
}

#[derive(Deserialize, JsonSchema)]
struct MemoryDeleteParams {
    scope: Scope,
    key: String,
    subject_id: Option<String>,
    updated_by: String,
}

#[tool_router(router = memory_router, vis = "pub")]
impl PandoTools {
    #[tool(description = "Create a new memory entry")]
    async fn memory_create(
        &self,
        Parameters(body): Parameters<MemoryCreateParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Err(message) = validate_key("key", &body.key) {
            return Ok(error_result(message));
        }

        if let Some(subject_id) = body.subject_id.as_deref() {
            if let Err(message) = validate_key("subject_id", subject_id) {
                return Ok(error_result(message));
            }
        }

        if body.content.trim().is_empty() {
            return Ok(error_result("content must not be empty"));
        }

        if body.description.trim().is_empty() {
            return Ok(error_result("description must not be empty"));
        }

        let result = sqlx::query_as::<_, MemoryEntry>(
            "INSERT INTO memory_entries (scope, key, subject_id, description, content, updated_by)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *",
        )
        .bind(&body.scope)
        .bind(&body.key)
        .bind(&body.subject_id)
        .bind(&body.description)
        .bind(&body.content)
        .bind(&body.updated_by)
        .fetch_one(self.database_pool())
        .await;

        match result {
            Ok(memory) => Ok(CallToolResult::structured(json!(memory))),
            Err(sqlx::Error::Database(db_err))
                if db_err.is_unique_violation()
                    && matches!(
                        db_err.constraint(),
                        Some("memory_entries_global_key") | Some("memory_entries_subject_key")
                    ) =>
            {
                Ok(error_result(
                    "a memory entry already exists for this scope/key/subject",
                ))
            }
            Err(sqlx::Error::Database(db_err))
                if db_err.is_foreign_key_violation()
                    && db_err.constraint() == Some("memory_entries_subject_id_fkey") =>
            {
                Ok(error_result(format!(
                    "subject '{}' does not exist",
                    body.subject_id.unwrap_or_default()
                )))
            }
            Err(error) => Err(db_error("failed to create new memory", error)),
        }
    }

    #[tool(
        description = "Read a memory entry, falling back to the global default when a subject override doesn't exist"
    )]
    async fn memory_read(
        &self,
        Parameters(body): Parameters<MemoryReadParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let result = sqlx::query_as::<_, MemoryEntry>(
            "SELECT * FROM memory_entries
            WHERE scope = $1
            AND key = $2
            AND (subject_id = $3 OR subject_id IS NULL)
            ORDER BY subject_id IS NULL ASC
            LIMIT 1",
        )
        .bind(&body.scope)
        .bind(&body.key)
        .bind(&body.subject_id)
        .fetch_optional(self.database_pool())
        .await;

        match result {
            Ok(Some(memory)) => Ok(CallToolResult::structured(json!(memory))),
            Ok(None) => Ok(CallToolResult::structured(json!(null))),
            Err(error) => Err(db_error("failed to read memory entry", error)),
        }
    }

    #[tool(description = "Search memory entries by key, description, or content")]
    async fn memory_search(
        &self,
        Parameters(body): Parameters<MemorySearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let query = body.query.trim();
        if query.is_empty() {
            return Ok(error_result("query must not be empty"));
        }

        let pattern = format!("%{}%", escape_like_wildcards(&query));
        let result = sqlx::query_as::<_, MemoryEntry>(
            "SELECT * FROM memory_entries
            WHERE (key ILIKE $1 OR description ILIKE $1 OR content ILIKE $1)
            AND ($2::text IS NULL OR scope = $2)
            AND ($3::text IS NULL OR subject_id = $3)
            ORDER BY updated_at DESC",
        )
        .bind(&pattern)
        .bind(&body.scope)
        .bind(&body.subject_id)
        .fetch_all(self.database_pool())
        .await;

        match result {
            Ok(memories) => Ok(CallToolResult::structured(json!(memories))),
            Err(error) => Err(db_error("failed to read memory entries", error)),
        }
    }

    #[tool(description = "List memory entries, optionally filtered by scope and/or subject")]
    async fn memory_list(
        &self,
        Parameters(body): Parameters<MemoryListParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let result = sqlx::query_as::<_, MemoryEntry>(
            "SELECT * FROM memory_entries
            WHERE ($1::text IS NULL OR scope = $1)
            AND ($2::text IS NULL OR subject_id = $2)
            ORDER BY updated_at DESC",
        )
        .bind(&body.scope)
        .bind(&body.subject_id)
        .fetch_all(self.database_pool())
        .await;

        match result {
            Ok(memories) => Ok(CallToolResult::structured(json!(memories))),
            Err(error) => Err(db_error("failed to read memory entries", error)),
        }
    }

    #[tool(description = "Update an existing memory entry's content and/or description")]
    async fn memory_update(
        &self,
        Parameters(body): Parameters<MemoryUpdateParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if body.content.is_none() && body.description.is_none() {
            return Ok(error_result(
                "at least one of content or description must be provided",
            ));
        }

        if let Some(content) = body.content.as_deref() {
            if content.trim().is_empty() {
                return Ok(error_result("content must not be empty"));
            }
        }

        if let Some(description) = body.description.as_deref() {
            if description.trim().is_empty() {
                return Ok(error_result("description must not be empty"));
            }
        }

        let result = sqlx::query_as::<_, MemoryEntry>(
            "UPDATE memory_entries
            SET content = COALESCE($4, content),
                description = COALESCE($5, description),
                updated_at = now(),
                updated_by = $6
            WHERE scope = $1
              AND key = $2
              AND subject_id IS NOT DISTINCT FROM $3
            RETURNING *",
        )
        .bind(&body.scope)
        .bind(&body.key)
        .bind(&body.subject_id)
        .bind(&body.content)
        .bind(&body.description)
        .bind(&body.updated_by)
        .fetch_optional(self.database_pool())
        .await;

        match result {
            Ok(Some(memory)) => Ok(CallToolResult::structured(json!(memory))),
            Ok(None) => Ok(error_result(
                "no memory entry exists for that scope/key/subject",
            )),
            Err(error) => Err(db_error("failed to update memory entry", error)),
        }
    }

    #[tool(description = "Delete a memory entry")]
    async fn memory_delete(
        &self,
        Parameters(body): Parameters<MemoryDeleteParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let result = sqlx::query(
            "DELETE FROM memory_entries
            WHERE scope = $1
              AND key = $2
              AND subject_id IS NOT DISTINCT FROM $3",
        )
        .bind(&body.scope)
        .bind(&body.key)
        .bind(&body.subject_id)
        .execute(self.database_pool())
        .await;

        let deleted = match result {
            Ok(deleted) => deleted,
            Err(error) => return Err(db_error("failed to delete memory entry", error)),
        };

        if deleted.rows_affected() == 0 {
            return Ok(error_result(
                "no memory entry exists for that scope/key/subject",
            ));
        }

        Ok(CallToolResult::structured(json!({
            "deleted": {
                "scope": body.scope,
                "key": body.key,
                "subject_id": body.subject_id,
            }
        })))
    }
}
