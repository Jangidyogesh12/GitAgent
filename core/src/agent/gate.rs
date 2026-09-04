//! ============================================================================
//! Module: engine::agent::gate
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The `beforeToolCall` seam from pi-agent-core: interceptors consulted
//!   before EVERY tool execution. Ports the TS `pre_tool_use` hooks
//!   (`src/hooks.ts` + `src/sdk-hooks.ts`) and the Claude-Code-style
//!   permission gate (`rust/gitagent-rs/.../permissions.rs`).
//!
//! DESIGN PATTERNS USED:
//!   * Chain of Responsibility — gates run in order; the first
//!     `Deny`/`Modify` wins and short-circuits the rest.
//!   * Strategy — each gate is an interchangeable policy object.
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `GateDecision` — Allow | Modify(new_args) | Deny(message).
//!   * `ToolGate`     — async `check(tool_name, args)` policy trait.
//!   * `AllowAllGate` — no-op gate (everything passes), useful default/tests.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::agent::{GateDecision, ToolGate, AllowAllGate};
//! // let gates: Vec<std::sync::Arc<dyn ToolGate>> = vec![std::sync::Arc::new(AllowAllGate)];
//! ```
//! ============================================================================

use async_trait::async_trait;

/// Verdict of one gate for one tool call.
///
/// # Description
/// `Modify` replaces the args the tool will receive (hook "modify" action);
/// `Deny` becomes the tool result text so the MODEL sees why it was blocked
/// (mirrors `wrapToolWithHooks` throwing → caught → result string).
#[derive(Debug, Clone)]
pub enum GateDecision {
    /// Proceed with the original args.
    Allow,
    /// Proceed with replacement args.
    Modify(serde_json::Value),
    /// Block; the message becomes the (error) tool result.
    Deny(String),
}

/// A pre-tool-use policy (Chain of Responsibility link).
#[async_trait]
pub trait ToolGate: Send + Sync {
    /// Inspect a pending tool call and return a verdict.
    ///
    /// # Example
    /// ```rust,no_run
    /// use engine::agent::{GateDecision, ToolGate};
    /// use async_trait::async_trait;
    /// struct NoRm;
    /// #[async_trait]
    /// impl ToolGate for NoRm {
    ///     async fn check(&self, tool: &str, args: &serde_json::Value) -> GateDecision {
    ///         if tool == "cli" && args.to_string().contains("rm -rf") {
    ///             GateDecision::Deny("rm -rf is blocked".into())
    ///         } else { GateDecision::Allow }
    ///     }
    /// }
    /// ```
    async fn check(&self, tool_name: &str, args: &serde_json::Value) -> GateDecision;
}

/// No-op gate: allows everything. Default when no policy is configured.
///
/// # Example
/// ```rust,no_run
/// use engine::agent::{AllowAllGate, ToolGate};
/// // ToolGate::check(&AllowAllGate, "cli", &serde_json::json!({})).await;
/// ```
pub struct AllowAllGate;

#[async_trait]
impl ToolGate for AllowAllGate {
    async fn check(&self, _tool: &str, _args: &serde_json::Value) -> GateDecision {
        GateDecision::Allow
    }
}
