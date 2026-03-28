pub mod tools;
pub mod value_json;

use rmcp::{RoleServer, ServiceExt, service::RunningService, transport::stdio};

/// Run the Keln MCP server over stdio.
/// This function blocks until the client disconnects or an error occurs.
pub async fn run_mcp_server() -> Result<(), Box<dyn std::error::Error>> {
    let server = tools::KelnServer::new();
    let service: RunningService<RoleServer, _> = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
