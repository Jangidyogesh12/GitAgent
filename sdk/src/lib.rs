//! ============================================================================
//! Crate: sdk
//! ----------------------------------------------------------------------------
//! WHAT THIS CRATE IS FOR:
//!   The public programmatic API for embedding the agent in Rust code.
//!   `query()` runs the full single-shot pipeline (load agent → build tools
//!   → gates → model client → agent loop) and streams normalised
//!   `SdkMessage`s over a tokio channel; `Session` keeps one agent alive
//!   across turns; `tool()` defines custom closure tools; `permissions`
//!   adds allow/deny modes and rules.
//!
//! HOW IT WORKS:
//!   * `query(opts)`: spawns a background task running `run_query()` and
//!     returns the receiver. Setup failures arrive as `SdkMessage::Error`;
//!     success ends with `System("session_end …")` and channel close.
//!   * `Session::open(opts)` performs the same load + registry + gates +
//!     client build once, then each `send(prompt)` runs one turn through
//!     the shared `Agent` (transcript accumulates across turns).
//!   * Tools: builtin + learning + declarative + plugin (collision-skipped)
//!     + MCP + `extra_tools`, narrowed by the allowlist-then-denylist
//!     filter in `build_registry()`.
//!   * Gates run in order: `PermissionGate` (mode + ordered allow/deny
//!     rules) first, then the script `HookGate` when hooks are configured.
//!
//! DESIGN PATTERNS USED:
//!   * Facade — `query()` hides loader/tools/hooks/plugins/mcp/llm/engine.
//!   * Observer — `SdkMessage` channel; the engine publishes, you subscribe.
//!   * Builder — `QueryOptions` + chained `Agent` builders underneath.
//!   * Decorator — allowed/disallowed filters + gates wrap the registry.
//!
//! MODULES PRESENT IN THIS CRATE:
//!   * `types`       — SdkMessage, QueryOptions, ToolSpec.
//!   * `permissions` — PermissionGate (modes + rules).
//!   * `fns`         — FnTool: closure-defined custom tools.
//!   * `query`       — query() Facade + build_registry().
//!   * `session`     — multi-turn Session handle.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use sdk::{query, QueryOptions};
//! use std::path::PathBuf;
//! # async fn demo() {
//! let opts = QueryOptions::new(PathBuf::from("./my-agent"), "Summarise this repo");
//! let mut rx = query(opts);
//! while let Some(msg) = rx.recv().await { let _ = msg; }
//! # }
//! ```
//! ============================================================================

pub mod fns;
pub mod permissions;
pub mod query;
pub mod session;
pub mod types;

pub use crate::session::Session;
pub use fns::{tool, FnTool};
pub use permissions::{PermissionGate, PermissionMode};
pub use query::{build_registry, query, QueryError};
pub use types::{QueryOptions, SdkMessage};
