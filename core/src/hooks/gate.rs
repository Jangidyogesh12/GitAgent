//! ============================================================================
//! Module: engine::hooks::gate
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   `HookGate` — adapts `pre_tool_use` script hooks to the engine's
//!   `ToolGate` trait, so the agent loop enforces hooks with zero knowledge
//!   of scripts (Adapter pattern). Ports `wrapToolWithHooks()` from
//!   `src/hooks.ts`: block → Deny (message becomes the tool result),
//!   modify → replacement args.
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `HookGate` — `new(agent_dir, session_id, defs)`; implements ToolGate.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::hooks::HookGate;
//! use std::path::PathBuf;
//! let gate = HookGate::new(PathBuf::from("./my-agent"), "sess-1", vec![]);
//! ```
//! ============================================================================

use crate::agent::{GateDecision, ToolGate};
use async_trait::async_trait;
use std::path::PathBuf;

use crate::hooks::config::HookDefinition;
use crate::hooks::exec::{run_hooks, HookInput, HookVerdict};

/// Script-hook adapter for the engine gate chain.
pub struct HookGate {
    /// Agent dir (`hooks/` subdir is the default script base).
    pub agent_dir: PathBuf,
    /// Session id injected into every hook payload.
    pub session_id: String,
    /// `pre_tool_use` definitions to consult.
    pub defs: Vec<HookDefinition>,
}

impl HookGate {
    /// Create the gate. Empty `defs` → always Allow (cheap fast path).
    ///
    /// # Example
    /// ```rust
    /// use engine::hooks::HookGate;
    /// use std::path::PathBuf;
    /// let g = HookGate::new(PathBuf::from("."), "s", vec![]);
    /// assert!(g.defs.is_empty());
    /// ```
    pub fn new(
        agent_dir: PathBuf,
        session_id: impl Into<String>,
        defs: Vec<HookDefinition>,
    ) -> Self {
        Self {
            agent_dir,
            session_id: session_id.into(),
            defs,
        }
    }
}

#[async_trait]
impl ToolGate for HookGate {
    async fn check(&self, tool_name: &str, args: &serde_json::Value) -> GateDecision {
        if self.defs.is_empty() {
            return GateDecision::Allow;
        }
        let base = self.agent_dir.join("hooks");
        let input =
            HookInput::new("pre_tool_use", &self.session_id).with_tool(tool_name, args.clone());
        match run_hooks(&self.defs, &base, &input).await {
            HookVerdict::Allow => GateDecision::Allow,
            HookVerdict::Block(reason) => GateDecision::Deny(reason),
            HookVerdict::Modify(new_args) => GateDecision::Modify(new_args),
        }
    }
}
