use rmcp::{ErrorData, model::CallToolResult, serde_json::json};

pub fn error_result(message: impl Into<String>) -> CallToolResult {
    CallToolResult::structured_error(json!({ "error": message.into() }))
}

pub fn db_error(context: &str, error: sqlx::Error) -> ErrorData {
    ErrorData::internal_error(
        context.to_string(),
        Some(json!({ "details": error.to_string() })),
    )
}
