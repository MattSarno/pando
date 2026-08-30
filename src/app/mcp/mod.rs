use super::AppState;
use rmcp::{ServerHandler, handler::server::tool::ToolRouter, tool_handler};
use sqlx::PgPool;

mod memory;
mod subject;
mod task;
mod tool_errors;
mod validation;

#[derive(Debug)]
pub struct PandoTools {
    state: AppState,
    tool_router: ToolRouter<PandoTools>,
}

impl PandoTools {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            tool_router: Self::subjects_router() + Self::memory_router() + Self::tasks_router(),
        }
    }

    pub fn database_pool(&self) -> &PgPool {
        &self.state.database_pool
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for PandoTools {}
