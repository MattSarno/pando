use crate::app::mcp::PandoTools;
use crate::app::mcp::tool_errors::{db_error, error_result};
use crate::app::mcp::validation::validate_key;
use rmcp::{
    ErrorData, handler::server::wrapper::Parameters, model::CallToolResult, serde_json::json, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::types::time::OffsetDateTime;

const SLUG_PKEY_CONSTRAINT: &str = "subjects_pkey";

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
enum Category {
    Project,
    Focus,
    Relationship,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
enum Status {
    Active,
    Paused,
    Finished,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
struct Subject {
    slug: String,
    title: String,
    category: Category,
    status: Option<Status>,
    summary: Option<String>,
    updated_at: OffsetDateTime,
    updated_by: String,
}

#[derive(Deserialize, JsonSchema)]
struct SubjectCreateParams {
    slug: String,
    title: String,
    category: Category,
    summary: Option<String>,
    updated_by: String,
}

#[derive(Deserialize, JsonSchema)]
struct SubjectListParams {
    slug: Option<String>,
    category: Option<Category>,
    status: Option<Status>,
}

#[allow(dead_code)]
#[derive(Deserialize, JsonSchema)]
struct SubjectDeleteParams {
    slug: String,
    updated_by: String,
}

#[derive(Deserialize, JsonSchema)]
struct SubjectUpdateParams {
    slug: String,
    title: Option<String>,
    status: Option<Status>,
    summary: Option<String>,
    updated_by: String,
}

fn default_status(category: &Category) -> Option<Status> {
    match category {
        Category::Relationship => None,
        Category::Project | Category::Focus => Some(Status::Active),
    }
}

fn validate_status_for_category(status: &Status, category: &Category) -> Result<(), String> {
    match (category, status) {
        (Category::Relationship, _) => {
            Err("relationship subjects cannot have a status".to_string())
        }
        (Category::Focus, Status::Finished) => {
            Err("focus subjects cannot have status 'finished'".to_string())
        }
        _ => Ok(()),
    }
}

#[tool_router(router = subjects_router, vis = "pub")]
impl PandoTools {
    #[tool(description = "Create a new subject")]
    async fn subject_create(
        &self,
        Parameters(body): Parameters<SubjectCreateParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Err(message) = validate_key("slug", &body.slug) {
            return Ok(error_result(message));
        }

        if body.title.trim().is_empty() {
            return Ok(error_result("title must not be empty"));
        }

        let status = default_status(&body.category);
        let summary = body.summary.filter(|summary| !summary.trim().is_empty());
        let result = sqlx::query_as::<_, Subject>(
            "INSERT INTO subjects (slug, title, category, status, summary, updated_by)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *",
        )
        .bind(&body.slug)
        .bind(&body.title)
        .bind(&body.category)
        .bind(&status)
        .bind(&summary)
        .bind(&body.updated_by)
        .fetch_one(self.database_pool())
        .await;

        match result {
            Ok(subject) => Ok(CallToolResult::structured(json!(subject))),
            Err(sqlx::Error::Database(db_err))
                if db_err.is_unique_violation()
                    && db_err.constraint() == Some(SLUG_PKEY_CONSTRAINT) =>
            {
                Ok(error_result(format!("slug '{}' already exists", body.slug)))
            }
            Err(error) => Err(db_error("failed to create subject", error)),
        }
    }

    #[tool(description = "List subjects, optionally filtered by slug, category, and/or status")]
    async fn subject_list(
        &self,
        Parameters(body): Parameters<SubjectListParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let result = sqlx::query_as::<_, Subject>(
            "SELECT * FROM subjects
             WHERE ($1::text IS NULL OR slug = $1)
               AND ($2::text IS NULL OR category = $2)
               AND ($3::text IS NULL OR status = $3)
             ORDER BY updated_at DESC",
        )
        .bind(&body.slug)
        .bind(&body.category)
        .bind(&body.status)
        .fetch_all(self.database_pool())
        .await;

        match result {
            Ok(subjects) => Ok(CallToolResult::structured(json!(subjects))),
            Err(error) => Err(db_error("failed to list subjects", error)),
        }
    }

    #[tool(description = "Update a subject's title, status, and/or summary")]
    async fn subject_update(
        &self,
        Parameters(body): Parameters<SubjectUpdateParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if body.title.is_none() && body.status.is_none() && body.summary.is_none() {
            return Ok(error_result(
                "at least one of title, status, or summary must be provided",
            ));
        }

        if let Some(title) = body.title.as_deref()
            && title.trim().is_empty()
        {
            return Ok(error_result("title must not be empty"));
        }

        if let Some(new_status) = &body.status {
            let category = match sqlx::query_scalar::<_, Category>(
                "SELECT category FROM subjects WHERE slug = $1",
            )
            .bind(&body.slug)
            .fetch_optional(self.database_pool())
            .await
            {
                Ok(category) => category,
                Err(error) => return Err(db_error("failed to look up subject", error)),
            };

            let category = match category {
                Some(category) => category,
                None => {
                    return Ok(error_result(format!("slug '{}' does not exist", body.slug)));
                }
            };

            if let Err(message) = validate_status_for_category(new_status, &category) {
                return Ok(error_result(message));
            }
        }

        let summary_provided = body.summary.is_some();
        let summary = body.summary.filter(|summary| !summary.trim().is_empty());
        let result = sqlx::query_as::<_, Subject>(
            "UPDATE subjects
             SET title = COALESCE($2, title),
                 status = COALESCE($3, status),
                 summary = CASE WHEN $4 THEN $5 ELSE summary END,
                 updated_at = now(),
                 updated_by = $6
             WHERE slug = $1
             RETURNING *",
        )
        .bind(&body.slug)
        .bind(&body.title)
        .bind(&body.status)
        .bind(summary_provided)
        .bind(&summary)
        .bind(&body.updated_by)
        .fetch_optional(self.database_pool())
        .await;

        match result {
            Ok(Some(subject)) => Ok(CallToolResult::structured(json!(subject))),
            Ok(None) => Ok(error_result(format!("slug '{}' does not exist", body.slug))),
            Err(error) => Err(db_error("failed to update subject", error)),
        }
    }

    #[tool(description = "Delete a subject and its memory")]
    async fn subject_delete(
        &self,
        Parameters(body): Parameters<SubjectDeleteParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut transaction = match self.database_pool().begin().await {
            Ok(transaction) => transaction,
            Err(error) => return Err(db_error("failed to start transaction", error)),
        };

        if let Err(error) = sqlx::query("DELETE FROM memory_entries WHERE subject_id = $1")
            .bind(&body.slug)
            .execute(&mut *transaction)
            .await
        {
            return Err(db_error("failed to delete subject's memory", error));
        }

        let result = sqlx::query("DELETE FROM subjects WHERE slug = $1")
            .bind(&body.slug)
            .execute(&mut *transaction)
            .await;

        let deleted = match result {
            Ok(deleted) => deleted,
            Err(error) => return Err(db_error("failed to delete subject", error)),
        };

        if deleted.rows_affected() == 0 {
            return Ok(error_result(format!("slug '{}' does not exist", body.slug)));
        }

        if let Err(error) = transaction.commit().await {
            return Err(db_error("failed to commit transaction", error));
        }

        Ok(CallToolResult::structured(
            json!({ "deleted": { "slug": body.slug } }),
        ))
    }
}
