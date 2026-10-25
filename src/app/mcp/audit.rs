use crate::app::mcp::{
    PandoTools,
    tool_errors::{db_error, error_result},
};
use rmcp::{
    ErrorData,
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    serde_json::{self, Value, json},
    tool, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::types::time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
enum Entity {
    Subject,
    Memory,
    Task,
}

#[derive(sqlx::FromRow)]
struct AuditRow {
    id: i64,
    at: OffsetDateTime,
    actor: String,
    client_id: Option<String>,
    client_name: Option<String>,
    entity: Entity,
    entity_key: String,
    action: String,
    before: Option<String>,
}

#[derive(Serialize)]
struct AuditEntry {
    id: i64,
    at: OffsetDateTime,
    actor: String,
    client_id: Option<String>,
    client_name: Option<String>,
    entity: Entity,
    entity_key: String,
    action: String,
    before: Option<Value>,
}

#[derive(Deserialize, JsonSchema)]
struct AuditListParams {
    since: Option<String>,
    after_id: Option<i64>,
    entity: Option<Entity>,
    entity_key: Option<String>,
    actor: Option<String>,
    exclude_actor: Option<String>,
    limit: Option<i64>,
    include_before: Option<bool>,
}

impl TryFrom<AuditRow> for AuditEntry {
    type Error = serde_json::Error;

    fn try_from(row: AuditRow) -> Result<Self, Self::Error> {
        let before = match row.before {
            Some(before) => Some(serde_json::from_str(&before)?),
            None => None,
        };

        Ok(Self {
            id: row.id,
            at: row.at,
            actor: row.actor,
            client_id: row.client_id,
            client_name: row.client_name,
            entity: row.entity,
            entity_key: row.entity_key,
            action: row.action,
            before,
        })
    }
}

#[tool_router(router = audit_router, vis = "pub")]
impl PandoTools {
    #[tool(
        description = "List recorded changes to subjects, memory, and tasks, newest first, or oldest first after a given audit id"
    )]
    async fn audit_list(
        &self,
        Parameters(body): Parameters<AuditListParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let since = match body.since.as_deref() {
            Some(since) => match OffsetDateTime::parse(since, &Rfc3339) {
                Ok(since) => Some(since),
                Err(_) => {
                    return Ok(error_result(
                        "since must be an RFC 3339 timestamp (e.g. 2026-10-25T18:00:00Z)",
                    ));
                }
            },
            None => None,
        };

        let limit = body.limit.unwrap_or(DEFAULT_LIMIT);
        if limit < 1 {
            return Ok(error_result("limit must be at least 1"));
        }

        let result = sqlx::query_as::<_, AuditRow>(
            "SELECT audit.id, audit.at, audit.actor, audit.client_id,
                    left(client.client_name, 64) AS client_name,
                    audit.entity, audit.entity_key, audit.action,
                    CASE WHEN $8 THEN audit.before::text END AS before
             FROM audit_log audit
             LEFT JOIN oauth_clients client ON client.client_id = audit.client_id
             WHERE ($1::timestamptz IS NULL OR audit.at >= $1)
               AND ($2::bigint IS NULL OR audit.id > $2)
               AND ($3::text IS NULL OR audit.entity = $3)
               AND ($4::text IS NULL OR audit.entity_key = $4)
               AND ($5::text IS NULL OR audit.actor = $5)
               AND ($6::text IS NULL OR audit.actor <> $6)
             ORDER BY CASE WHEN $2 IS NULL THEN audit.at END DESC,
                      CASE WHEN $2 IS NULL THEN audit.id END DESC,
                      CASE WHEN $2 IS NOT NULL THEN audit.id END ASC
             LIMIT $7",
        )
        .bind(since)
        .bind(body.after_id)
        .bind(&body.entity)
        .bind(&body.entity_key)
        .bind(&body.actor)
        .bind(&body.exclude_actor)
        .bind(limit.min(MAX_LIMIT))
        .bind(body.include_before.unwrap_or(false))
        .fetch_all(self.database_pool())
        .await;

        let rows = match result {
            Ok(rows) => rows,
            Err(error) => return Err(db_error("failed to list audit log", error)),
        };

        let entries = match rows
            .into_iter()
            .map(AuditEntry::try_from)
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(entries) => entries,
            Err(error) => {
                return Err(ErrorData::internal_error(
                    "failed to decode audit log row",
                    Some(json!({ "details": error.to_string() })),
                ));
            }
        };

        Ok(CallToolResult::structured(json!(entries)))
    }
}
