use super::AppState;
use rmcp::{
    ServerHandler, 
    handler::server::tool::ToolRouter, 
    tool_handler, 
    tool_router,
};

pub struct PandoTools {
    state: AppState,
    tool_router: ToolRouter<PandoTools>,
}

#[tool_router]
impl PandoTools {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_handler]
impl ServerHandler for PandoTools {}
