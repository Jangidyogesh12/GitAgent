//! ============================================================================
//! Module: engine::agent::tool
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The Strategy-pattern seam for tools: the engine talks to `AgentTool`
//!   trait objects, so builtin tools, declarative YAML tools, MCP tools and
//!   SDK-defined tools are interchangeable. Also defines `ExecutionMode`
//!   (pi's parallel-vs-sequential rule) and `ToolOutput`.
//!
//! DESIGN PATTERNS USED:
//!   * Strategy — `AgentTool` trait; the loop calls `execute()` polymorphically.
//!   * Factory — describes (not implements) how `builtin_tools()` builds the
//!     registry; see the `tools` crate for the factory itself.
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `ExecutionMode` — Parallel (pure/read-only) vs Sequential (mutating).
//!   * `ToolOutput`    — content + is_error + terminate_loop flag.
//!   * `AgentTool`     — name/description/parameters/execution_mode/execute.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::agent::{AgentTool, ExecutionMode, ToolOutput};
//! use async_trait::async_trait;
//! struct Echo;
//! #[async_trait]
//! impl AgentTool for Echo {
//!     fn name(&self) -> &str { "echo" }
//!     fn description(&self) -> &str { "Echo text back" }
//!     fn parameters(&self) -> serde_json::Value { serde_json::json!({"type":"object"}) }
//!     async fn execute(&self, _id: &str, args: serde_json::Value) -> anyhow::Result<ToolOutput> {
//!         Ok(ToolOutput::ok(args.to_string()))
//!     }
//! }
//! ```
//! ============================================================================

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Concurrency contract of a tool (pi's exact rule is implemented in
/// `agent.rs`: a batch runs concurrently UNLESS any tool in it — or the
/// agent itself — is `Sequential`, in which case the whole batch serialises).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionMode {
    /// Safe to run alongside other tools (e.g. `read`).
    Parallel,
    /// Must run alone (mutates files/git/processes: cli, write, edit...).
    Sequential,
}

/// The result of one tool execution.
///
/// # Description
/// `is_error` marks failures as model-visible data ("Error: ..."), never as
/// Rust `Err` (tool bugs must not kill the session). `terminate_loop` ends
/// the session after this batch — reserved for future control tools.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    /// Result text (or JSON-stringified structured data).
    pub content: String,
    /// True when the tool failed / was denied by a gate.
    pub is_error: bool,
    /// When true, the loop stops after this batch.
    pub terminate_loop: bool,
}

impl ToolOutput {
    /// Successful result.
    ///
    /// # Example
    /// ```rust
    /// use engine::agent::tool::ToolOutput;
    /// let o = ToolOutput::ok("hi");
    /// assert!(!o.is_error);
    /// ```
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
            terminate_loop: false,
        }
    }

    /// Failed result (still model-visible data, not a Rust error).
    ///
    /// # Example
    /// ```rust
    /// use engine::agent::tool::ToolOutput;
    /// assert!(ToolOutput::err("nope").is_error);
    /// ```
    pub fn err(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
            terminate_loop: false,
        }
    }
}

/// A callable capability of the agent (Strategy pattern).
///
/// # Description
/// Implementors are the agent's "hands". The default `execution_mode()` is
/// `Parallel`; mutating tools MUST override it to `Sequential` (mirrors
/// `src/tools/index.ts` marking cli/write/edit/memory/... sequential).
#[async_trait]
pub trait AgentTool: Send + Sync {
    /// Registry name (e.g. `"cli"`; MCP tools use `"server__tool"`).
    fn name(&self) -> &str;
    /// One-line description shown to the model.
    fn description(&self) -> &str;
    /// JSON-Schema of the arguments object (sent to the model).
    fn parameters(&self) -> serde_json::Value;
    /// Concurrency contract; defaults to `Parallel`.
    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Parallel
    }
    /// Run the tool. Return `ToolOutput::err(...)` on failure, never `Err`
    /// for domain errors (only for truly unexpected crashes).
    async fn execute(&self, id: &str, args: serde_json::Value) -> anyhow::Result<ToolOutput>;
}
