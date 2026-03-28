use rmcp::{
    ServerHandler,
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{
        CallToolResult, Content, ServerCapabilities, ServerInfo,
    },
    tool, tool_handler, tool_router,
    ErrorData as McpError,
    schemars,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::mcp::value_json::{json_to_keln_value, keln_value_to_json};
use crate::verify::result::VerificationResult;

// =============================================================================
// Input parameter types
// =============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompileInput {
    /// Keln source code to parse and type-check
    pub source: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct VerifyInput {
    /// Keln source code to compile and verify
    pub source: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunInput {
    /// Keln source code to compile and run
    pub source: String,
    /// Name of the function to execute
    pub fn_name: String,
    /// JSON-encoded argument to pass to the function (optional, defaults to null)
    #[serde(default)]
    pub arg: Option<serde_json::Value>,
}

// =============================================================================
// KelnServer — the MCP tool handler
// =============================================================================

pub struct KelnServer {
    tool_router: ToolRouter<KelnServer>,
}

#[tool_router]
impl KelnServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    /// Parse and type-check Keln source code. Returns structured type errors
    /// immediately without running verify blocks.
    #[tool(description = "Parse and type-check Keln source code. Returns structured type errors immediately without running verify blocks.")]
    fn compile(
        &self,
        Parameters(CompileInput { source }): Parameters<CompileInput>,
    ) -> Result<CallToolResult, McpError> {
        match crate::types::check_source(&source) {
            Err(parse_err) => {
                let result = serde_json::json!({
                    "ok": false,
                    "error_count": 1,
                    "errors": [{ "message": parse_err }]
                });
                Ok(CallToolResult::success(vec![Content::text(
                    result.to_string(),
                )]))
            }
            Ok(errors) => {
                let ok = errors.is_empty();
                let error_count = errors.len();
                let error_list: Vec<serde_json::Value> = errors
                    .iter()
                    .map(|e| serde_json::json!({ "message": e.message }))
                    .collect();
                let result = serde_json::json!({
                    "ok": ok,
                    "error_count": error_count,
                    "errors": error_list
                });
                Ok(CallToolResult::success(vec![Content::text(
                    result.to_string(),
                )]))
            }
        }
    }

    /// Compile and run all verify blocks in a Keln program. Returns the full
    /// VerificationResult including given case pass/fail, forall property test
    /// results, and confidence scores.
    #[tool(description = "Compile and run all verify blocks in a Keln program. Returns the full VerificationResult including given case pass/fail, forall property test results, and confidence scores.")]
    fn verify(
        &self,
        Parameters(VerifyInput { source }): Parameters<VerifyInput>,
    ) -> Result<CallToolResult, McpError> {
        // Run in a thread with a large stack to support deeply recursive Keln programs
        // evaluated by the tree-walking evaluator (given/forall cases).
        let json_str = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024) // 64 MB
            .spawn(move || {
                let mut ex = match crate::verify::VerifyExecutor::from_source(&source) {
                    Ok(ex) => ex,
                    Err(e) => {
                        return serde_json::json!({
                            "compile_errors": [e],
                            "test_failures": [],
                            "coverage_gaps": [],
                            "proof_violations": [],
                            "fuzz_status": [],
                            "is_clean": false
                        }).to_string();
                    }
                };
                let fn_results = ex.verify_all();
                let mut vr = VerificationResult::from_fn_results(&fn_results);
                vr.fuzz_status = ex.fuzz_trusted_modules();
                vr.to_json()
            })
            .expect("failed to spawn verify thread")
            .join()
            .unwrap_or_else(|_| serde_json::json!({
                "compile_errors": ["verify thread panicked"],
                "test_failures": [],
                "coverage_gaps": [],
                "proof_violations": [],
                "fuzz_status": [],
                "is_clean": false
            }).to_string());

        Ok(CallToolResult::success(vec![Content::text(json_str)]))
    }

    /// Compile a Keln program, lower it to bytecode, and execute a named
    /// function. The arg is a JSON value that gets converted to a Keln Value.
    #[tool(description = "Compile a Keln program, lower it to bytecode, and execute a named function. The arg is a JSON value that gets converted to a Keln Value.")]
    fn run(
        &self,
        Parameters(RunInput { source, fn_name, arg }): Parameters<RunInput>,
    ) -> Result<CallToolResult, McpError> {
        // Parse the source
        let program = match crate::parser::parse(&source) {
            Ok(p) => p,
            Err(e) => {
                let result = serde_json::json!({ "error": format!("parse error: {}", e) });
                return Ok(CallToolResult::success(vec![Content::text(
                    result.to_string(),
                )]));
            }
        };

        // Lower to bytecode
        let module = match crate::vm::lower::lower_program(&program) {
            Ok(m) => m,
            Err(e) => {
                let result = serde_json::json!({ "error": format!("lower error: {}", e) });
                return Ok(CallToolResult::success(vec![Content::text(
                    result.to_string(),
                )]));
            }
        };

        // Convert JSON arg to Keln Value
        let keln_arg = match arg {
            None => crate::eval::Value::Unit,
            Some(j) => json_to_keln_value(j),
        };

        // Execute
        match crate::vm::exec::execute_fn(&module, &fn_name, keln_arg) {
            Ok(v) => {
                let result = serde_json::json!({ "result": keln_value_to_json(&v) });
                Ok(CallToolResult::success(vec![Content::text(
                    result.to_string(),
                )]))
            }
            Err(e) => {
                let result = serde_json::json!({ "error": format!("{}", e) });
                Ok(CallToolResult::success(vec![Content::text(
                    result.to_string(),
                )]))
            }
        }
    }
}

#[tool_handler]
impl ServerHandler for KelnServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "Keln language toolchain MCP server. \
                 Tools: compile (type-check source), verify (run verify blocks), \
                 run (execute a function via bytecode VM)."
                    .to_string(),
            )
    }
}
