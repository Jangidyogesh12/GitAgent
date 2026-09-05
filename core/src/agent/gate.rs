//! ============================================================================
//! Module: engine::agent::gate
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Pre-tool-call policy seam: interceptors consulted before EVERY tool
//!   execution. Gates can allow, rewrite args, or deny with a model-visible
//!   message. Denials become error result text so the model sees why the
//!   call was blocked instead of the session failing.
//!
//! DESIGN PATTERNS USED:
//!   * Chain of Responsibility — gates run in order; the first
//!     `Deny`/`Modify` wins and short-circuits the rest.
//!   * Strategy — each gate is an interchangeable policy object.
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `GateDecision` — Allow | Modify(new_args) | Deny(message).
//!   * `ToolGate`     — async `check(tool_name, args)` policy trait.
//!   * `AllowAllGate` — no-op gate (everything passes), default for tests.
//!
//! HOW IT WORKS:
//!   * The runner walks the gate list per call; Allow continues, Modify
//!     replaces args for downstream gates and the tool, Deny stops the
//!     chain and produces the error result immediately.
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
/// `Modify` replaces the args the tool will receive; `Deny` becomes the
/// tool result text so the model sees why it was blocked.
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
