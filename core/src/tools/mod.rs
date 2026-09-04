//! ============================================================================
//! Crate: tools
//! ----------------------------------------------------------------------------
//! WHAT THIS CRATE IS FOR:
//!   The agent's "hands" — every callable tool. Ports `src/tools/*.ts`:
//!   `cli` (shell exec with process-group kill), `read` (binary sniff +
//!   pagination), `write` (mkdir -p + write), `edit` (exact/regex replace),
//!   `memory` (git-backed layered memory), declarative YAML tools
//!   (`tools/*.yaml` → script subprocess), plus the `builtin_tools()`
//!   Factory that picks the registry.
//!
//! DESIGN PATTERNS USED:
//!   * Strategy — each tool implements `engine::agent::AgentTool`.
//!   * Factory — `builtin_tools()` builds the registry (local set; sandbox
//!     variants are a `SandboxExec` Strategy swap — see `cli.rs`).
//!   * Command — each tool encapsulates a request (name + args JSON) as an
//!     object the loop can invoke, log, and gate uniformly.
//!
//! MODULES PRESENT IN THIS CRATE:
//!   * `cli`         — CliTool (shell, timeout, rolling-tail cap).
//!   * `read`        — ReadTool (binary detect, paginate).
//!   * `write`       — WriteTool (create-dirs write).
//!   * `edit`        — EditTool (exact / replace_all / regex).
//!   * `memory`      — MemoryTool (layered git memory + archive overflow).
//!   * `declarative` — YAML-defined script tools (args JSON on stdin).
//!   * `factory`     — builtin_tools() registry Factory.
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
