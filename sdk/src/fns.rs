//! ============================================================================
//! Module: sdk::fns
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Custom closure-defined tools. Ports the TS `tool(name, description,
//!   inputSchema, handler)` helper from `src/sdk.ts`: wrap a plain Rust
//!   closure into a real `AgentTool` (Strategy) so SDK users can give the
//!   agent new hands in a few lines.
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `FnTool` — closure-backed tool (Sequential: closures may do anything).
//!   * `tool()` — constructor helper mirroring the TS `tool()` signature.
//!
//! HOW TO USE (example):
//! ```rust
//! use sdk::tool;
//! let t = tool("shout", "Uppercase text", serde_json::json!({"type": "object"}),
//!     |args| Ok(args.get("text").and_then(|v| v.as_str()).unwrap_or("").to_uppercase()));
//! assert_eq!(t.name(), "shout");
//! ```
//! ============================================================================

use engine::agent::{AgentTool, ExecutionMode, ToolOutput};
use std::sync::Arc;

/// A tool defined by a plain synchronous closure.
pub struct FnTool {
    tool_name: String,
    tool_desc: String,
    schema: serde_json::Value,
    handler: Arc<dyn Fn(serde_json::Value) -> anyhow::Result<String> + Send + Sync>,
}

impl FnTool {
    /// Registry name.
    pub fn name(&self) -> &str {
        &self.tool_name
    }
}

/// Manual `Future` return (no `async_trait`): this crate is downstream of
/// the `core` package, where that macro's `engine::…` paths would resolve to
/// our crate instead of std. `std::` paths used explicitly throughout.
impl AgentTool for FnTool {
    fn name(&self) -> &str {
        &self.tool_name
    }
    fn description(&self) -> &str {
        &self.tool_desc
    }
    fn parameters(&self) -> serde_json::Value {
        self.schema.clone()
    }
    fn execution_mode(&self) -> ExecutionMode {
        // Fail-safe default: user closures may touch anything.
        ExecutionMode::Sequential
    }
    fn execute<'life0, 'life1, 'async_trait>(
        &'life0 self,
        _id: &'life1 str,
        args: serde_json::Value,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = anyhow::Result<ToolOutput>> + Send + 'async_trait>,
    >
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        // Run blocking closures off the async runtime (never stall the loop).
        let handler = self.handler.clone();
        Box::pin(async move {
            match tokio::task::spawn_blocking(move || handler(args)).await {
                Ok(Ok(s)) => Ok(ToolOutput::ok(s)),
                Ok(Err(e)) => Ok(ToolOutput::err(format!("Error: {e:#}"))),
                Err(e) => Ok(ToolOutput::err(format!("Error: tool panicked: {e}"))),
            }
        })
    }
}

/// Define a custom tool from a closure (mirrors TS `tool()`).
///
/// # Description
/// `handler` receives the args JSON and returns the result text; `Err`
/// becomes an error RESULT (model-visible), never a session crash.
///
/// # Example
/// ```rust
/// use sdk::tool;
/// let t = tool("add", "Add a+b", serde_json::json!({"type": "object"}), |a| {
///     let n = a.get("a").and_then(|v| v.as_i64()).unwrap_or(0);
///     Ok(n.to_string())
/// });
/// ```
pub fn tool(
    name: &str,
    description: &str,
    schema: serde_json::Value,
    handler: impl Fn(serde_json::Value) -> anyhow::Result<String> + Send + Sync + 'static,
) -> FnTool {
    FnTool {
        tool_name: name.to_string(),
        tool_desc: description.to_string(),
        schema,
        handler: Arc::new(handler),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn closure_tool_runs() {
        let t = tool(
            "echo2",
            "echo",
            serde_json::json!({"type": "object"}),
            |a| Ok(a.to_string()),
        );
        let out = AgentTool::execute(&t, "1", serde_json::json!({"x": 1}))
            .await
            .unwrap();
        assert!(!out.is_error);
    }
}
