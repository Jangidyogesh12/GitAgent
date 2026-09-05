//! ============================================================================
//! Crate: tools
//! ----------------------------------------------------------------------------
//! WHAT THIS CRATE IS FOR:
//!   The agent's callable tools: `cli` (shell exec with process-group kill
//!   and tail truncation), `read` (binary sniff plus pagination), `write`
//!   (parent-creating write), `edit` (literal / replace-all / regex
//!   replace), `memory` (layered file memory with archive overflow and
//!   best-effort git commit), declarative script tools (YAML-defined tools
//!   run as child processes), plus the `builtin_tools()` Factory that
//!   assembles the registry.
//!
//! DESIGN PATTERNS USED:
//!   * Strategy — each tool implements `engine::agent::AgentTool`.
//!   * Factory — `builtin_tools()` builds the registry (local set; sandbox
//!     variants are a `SandboxExec` Strategy swap — see `crate::tools::cli`).
//!   * Command — each tool encapsulates a request (name plus args JSON) as
//!     an object the loop can invoke, log, and gate uniformly.
//!
//! MODULES PRESENT IN THIS CRATE:
//!   * `cli`         — CliTool (shell, timeout, rolling-tail cap).
//!   * `read`        — ReadTool (binary detect, paginate).
//!   * `write`       — WriteTool (create-dirs write).
//!   * `edit`        — EditTool (literal / replace_all / regex).
//!   * `memory`      — MemoryTool (layered memory plus archive overflow).
//!   * `declarative` — YAML-defined script tools (args JSON on stdin).
//!   * `factory`     — builtin_tools() registry Factory.
//!
//! HOW IT WORKS (shared contracts):
//!   * Read-only tools report Parallel and may run concurrently; mutating
//!     tools report Sequential and run one at a time.
//!   * Tool failures are returned as error result values (visible to the
//!     model), not thrown errors, so one bad call never ends the session.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::tools::builtin_tools;
//! use std::path::Path;
//! let tools = builtin_tools(Path::new("."), 120, None);
//! assert!(tools.iter().any(|t| t.name() == "cli"));
//! ```
//! ============================================================================

pub mod cli;
pub mod declarative;
pub mod edit;
pub mod factory;
pub mod memory;
pub mod read;
pub mod write;

pub use cli::{CliTool, SandboxExec};
pub use declarative::{load_declarative_tools, DeclarativeTool};
pub use edit::EditTool;
pub use factory::builtin_tools;
pub use memory::MemoryTool;
pub use read::ReadTool;
pub use write::WriteTool;
