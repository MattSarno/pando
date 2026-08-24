use crate::app::{mcp::PandoTools, tool_errors::{db_error, error_result}, validation::validate_key};
use rmcp::{
    ErrorData, handler::server::wrapper::Parameters, model::CallToolResult, serde_json::json, tool, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use sqlx::types::time::{Date, OffsetDateTime};
use time::format_description::well_known::Iso8601;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
enum Status {
    Open,
    InProgress,
    Done,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
enum Priority {
    Low,
    Medium,
    High,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
struct Task {
    id: i64,
    text: String,
    status: Status,
    priority: Priority,
    linked_subject: Option<String>,
    due_date: Option<Date>,
    source: String,
    created_at: OffsetDateTime,
    completed_at: Option<OffsetDateTime>,
    updated_at: OffsetDateTime,
    updated_by: String,
}

#[derive(Deserialize, JsonSchema)]
struct TaskCreateParams {
    text: String,
    source: String,
    priority: Option<Priority>,
    due_date: Option<String>,
    linked_subject: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct TaskListParams {
    id: Option<i64>,
    status: Option<Status>,
    stale_days: Option<i32>,
    linked_subject: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
enum UpdatableStatus {
    Open,
    InProgress,
}

#[derive(Deserialize, JsonSchema)]
struct TaskUpdateParams {
    id: i64,
    updated_by: String,
    status: Option<UpdatableStatus>,
    priority: Option<Priority>,
    due_date: Option<String>,
    text: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct TaskCompleteParams {
    id: i64,
    updated_by: String,
}

#[derive(Deserialize, JsonSchema)]
struct TaskDeleteParams {
    id: i64,
    updated_by: String,
}

async fn validate_status(pool: &PgPool, id: i64) -> Result<Result<(), CallToolResult>, ErrorData> {
    let status = sqlx::query_scalar::<_, Status>("SELECT status FROM tasks WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await;

    match status {
        Ok(None) => Ok(Err(error_result(format!("task {id} does not exist")))),
        Ok(Some(Status::Done)) => Ok(Err(error_result(format!("task {id} is already done")))),
        Ok(Some(_)) => Ok(Ok(())),
        Err(error) => Err(db_error("failed to look up task", error)),
    }
}

#[tool_router(router = tasks_router, vis = "pub")]
impl PandoTools {
    #[tool(description = "Create a new task")]
    async fn task_create(
        &self,
        Parameters(body): Parameters<TaskCreateParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if body.text.trim().is_empty() {
            return Ok(error_result("text must not be empty"));
        }

        if body.source.trim().is_empty() {
            return Ok(error_result("source must not be empty"));
        }

        let priority = body.priority.unwrap_or(Priority::Medium);

        if let Some(subject_id) = body.linked_subject.as_deref() {
            if let Err(message) = validate_key("linked_subject", subject_id) {
                return Ok(error_result(message));
            }
        }

        let due_date = match body.due_date.as_deref() {
            Some(due_date) => match Date::parse(due_date, &Iso8601::DATE) {
                Ok(due_date) => Some(due_date),
                Err(_) => {
                    return Ok(error_result("due_date must be a valid date (YYYY-MM-DD)"));
                }
            },
            None => None,
        };

        let default_status = Status::Open;
        let result = sqlx::query_as::<_, Task>(
            "INSERT INTO tasks (text, status, priority, linked_subject, due_date, source, updated_by)
            VALUES ($1, $2, $3, $4, $5, $6, $6)
            RETURNING *",
        )
        .bind(&body.text)
        .bind(&default_status)
        .bind(&priority)
        .bind(&body.linked_subject)
        .bind(&due_date)
        .bind(&body.source)
        .fetch_one(self.database_pool())
        .await;

        match result {
            Ok(task) => Ok(CallToolResult::structured(json!(task))),
            Err(sqlx::Error::Database(db_err))
                if db_err.is_foreign_key_violation()
                    && db_err.constraint() == Some("tasks_linked_subject_fkey") =>
            {
                Ok(error_result(format!(
                    "subject '{}' does not exist",
                    body.linked_subject.unwrap_or_default()
                )))
            }
            Err(error) => Err(db_error("failed to create task", error)),
        }
    }

    #[tool(description = "List tasks, optionally filtered by id, status, staleness, and/or linked subject")]
    async fn task_list(
        &self,
        Parameters(body): Parameters<TaskListParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let result = sqlx::query_as::<_, Task>(
            "SELECT * FROM tasks
             WHERE ($1::bigint IS NULL OR id = $1)
               AND ($2::text IS NULL OR status = $2)
               AND ($3::text IS NULL OR linked_subject = $3)
               AND ($4::int IS NULL OR updated_at < now() - ($4::int * interval '1 day'))
             ORDER BY updated_at DESC",
        )
        .bind(&body.id)
        .bind(&body.status)
        .bind(&body.linked_subject)
        .bind(&body.stale_days)
        .fetch_all(self.database_pool())
        .await;

        match result {
            Ok(tasks) => Ok(CallToolResult::structured(json!(tasks))),
            Err(error) => Err(db_error("failed to list tasks", error)),
        }
    }

    #[tool(description = "Update an existing task's status, priority, due date, and/or text")]
    async fn task_update(
        &self,
        Parameters(body): Parameters<TaskUpdateParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if body.status.is_none()
            && body.priority.is_none()
            && body.due_date.is_none()
            && body.text.is_none()
        {
            return Ok(error_result(
                "at least one of status, priority, due_date, or text must be provided",
            ));
        }

        if let Some(text) = body.text.as_deref() {
            if text.trim().is_empty() {
                return Ok(error_result("text must not be empty"));
            }
        }

        if body.status.is_some() {
            if let Err(response) = validate_status(self.database_pool(), body.id).await? {
                return Ok(response);
            }
        }

        let due_date_provided = body.due_date.is_some();
        let due_date = match body.due_date.as_deref() {
            Some("") => None,
            Some(due_date) => match Date::parse(due_date, &Iso8601::DATE) {
                Ok(due_date) => Some(due_date),
                Err(_) => {
                    return Ok(error_result("due_date must be a valid date (YYYY-MM-DD)"));
                }
            },
            None => None,
        };

        let result = sqlx::query_as::<_, Task>(
            "UPDATE tasks
            SET status = COALESCE($2, status),
                priority = COALESCE($3, priority),
                due_date = CASE WHEN $4 THEN $5 ELSE due_date END,
                text = COALESCE($6, text),
                updated_at = now(),
                updated_by = $7
            WHERE id = $1
            RETURNING *",
        )
        .bind(&body.id)
        .bind(&body.status)
        .bind(&body.priority)
        .bind(due_date_provided)
        .bind(&due_date)
        .bind(&body.text)
        .bind(&body.updated_by)
        .fetch_optional(self.database_pool())
        .await;

        match result {
            Ok(Some(task)) => Ok(CallToolResult::structured(json!(task))),
            Ok(None) => Ok(error_result(format!("task {} does not exist", body.id))),
            Err(error) => Err(db_error("failed to update task", error)),
        }
    }

    #[tool(description = "Mark a task done")]
    async fn task_complete(
        &self,
        Parameters(body): Parameters<TaskCompleteParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Err(response) = validate_status(self.database_pool(), body.id).await? {
            return Ok(response);
        }

        let result = sqlx::query_as::<_, Task>(
            "UPDATE tasks
            SET status = 'done',
                completed_at = now(),
                updated_at = now(),
                updated_by = $2
            WHERE id = $1
            RETURNING *",
        )
        .bind(&body.id)
        .bind(&body.updated_by)
        .fetch_optional(self.database_pool())
        .await;

        match result {
            Ok(Some(task)) => Ok(CallToolResult::structured(json!(task))),
            Ok(None) => Ok(error_result(format!("task {} does not exist", body.id))),
            Err(error) => Err(db_error("failed to complete task", error)),
        }
    }

    #[tool(description = "Delete a task")]
    async fn task_delete(
        &self,
        Parameters(body): Parameters<TaskDeleteParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let result = sqlx::query("DELETE FROM tasks WHERE id = $1")
            .bind(&body.id)
            .execute(self.database_pool())
            .await;

        let deleted = match result {
            Ok(deleted) => deleted,
            Err(error) => return Err(db_error("failed to delete task", error)),
        };

        if deleted.rows_affected() == 0 {
            return Ok(error_result(format!("task {} does not exist", body.id)));
        }

        Ok(CallToolResult::structured(
            json!({ "deleted": { "id": body.id } }),
        ))
    }
}
