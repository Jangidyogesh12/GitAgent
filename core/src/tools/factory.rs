//! ============================================================================
//! Module: engine::tools::factory
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The tool registry Factory — `builtin_tools()` assembles the local tool
//!   set and marks stateful tools Sequential. Ports
//!   `createBuiltinTools()` from `src/tools/index.ts` (local vs sandbox pick
//!   + sequential marking of cli/write/edit/memory/task_tracker/...).
//!
//! DESIGN PATTERNS USED:
//!   * Factory — one function builds the whole `Vec<Arc<dyn AgentTool>>`
//!     registry from (agent_dir, cli_timeout, sandbox backend).
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `builtin_tools()` — build the local (+sandbox-backed cli) registry.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::tools::builtin_tools;
//! use std::path::Path;
//! let tools = builtin_tools(Path::new("."), 120, None);
//! assert!(tools.iter().any(|t| t.name() == "memory"));
//! ```
//! ============================================================================

use crate::agent::AgentTool;
use std::path::Path;
use std::sync::Arc;

use crate::tools::cli::{CliTool, SandboxExec};
use crate::tools::edit::EditTool;
use crate::tools::memory::MemoryTool;
use crate::tools::read::ReadTool;
use crate::tools::write::WriteTool;

/// Build the builtin tool registry (Factory pattern).
///
/// # Description
/// Ports `createBuiltinTools()`: always returns cli/read/write/edit/memory.
/// `read` stays Parallel; the rest report Sequential themselves. When
/// `sandbox` is Some, the `cli` tool runs through that backend (the
/// `sandbox-cli.ts` twin) instead of the local shell. `task_tracker` and
/// `skill_learner` come from the `learning` crate and are appended
/// by the SDK layer (kept separate so tools stay dependency-light).
///
/// # Example
/// ```rust,no_run
/// use engine::tools::builtin_tools;
/// use std::path::Path;
/// let tools = builtin_tools(Path::new("./my-agent"), 120, None);
/// assert_eq!(tools.len(), 5);
/// ```
pub fn builtin_tools(
    agent_dir: &Path,
    cli_timeout: u64,
    sandbox: Option<Arc<dyn SandboxExec>>,
) -> Vec<Arc<dyn AgentTool>> {
    vec![
        Arc::new(CliTool::new(agent_dir.to_path_buf(), cli_timeout, sandbox)) as Arc<dyn AgentTool>,
        Arc::new(ReadTool::new(agent_dir.to_path_buf())),
        Arc::new(WriteTool::new(agent_dir.to_path_buf())),
        Arc::new(EditTool::new(agent_dir.to_path_buf())),
        Arc::new(MemoryTool::new(agent_dir.to_path_buf())),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_five_builtins() {
        let tools = builtin_tools(Path::new("/tmp"), 120, None);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert_eq!(names, vec!["cli", "read", "write", "edit", "memory"]);
    }
}
