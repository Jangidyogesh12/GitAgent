//! ============================================================================
//! Module: engine::loader
//! ----------------------------------------------------------------------------
//! WHAT THIS MODULE IS FOR:
//!   The assembly line — turns an agent DIRECTORY into a `LoadedAgent`
//!   (manifest + one big system prompt + session id). Inputs: agent dir,
//!   optional model flag, optional session id. Steps: read `agent.yaml`,
//!   shallow-clone `extends`/dependencies, read identity files, discover
//!   skills/knowledge/workflows/sub-agents/examples/plugins, then join the
//!   sections with "\n\n" in a fixed deterministic order. Outputs:
//!   `LoadedAgent` ready for the session loop. Invariant: missing or
//!   invalid optional pieces are skipped, never fatal.
//!
//! DESIGN PATTERNS USED:
//!   * Builder — `PromptBuilder` accumulates optional sections, then
//!     `build()` joins them (skipping empties).
//!   * Facade — `load_agent()` is the single front door hiding discovery.
//!
//! MODULES PRESENT IN THIS MODULE:
//!   * `discover` — skills/knowledge/workflows/agents/examples readers.
//!   * `prompt`   — PromptBuilder (ordered section assembly).
//!   * `load`     — load_agent() + git helpers + session state.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::loader::load_agent;
//! use std::path::Path;
//! let agent = load_agent(Path::new("./my-agent"), None, None).unwrap();
//! assert!(!agent.system_prompt.is_empty());
//! ```
//! ============================================================================

pub mod discover;
pub mod load;
pub mod prompt;

pub use discover::{
    discover_agents, discover_examples, discover_skills, discover_workflows, load_knowledge,
    AgentInfo, SkillInfo, WorkflowInfo,
};
pub use load::{clone_git_repo, load_agent, LoadedAgent};
pub use prompt::PromptBuilder;
