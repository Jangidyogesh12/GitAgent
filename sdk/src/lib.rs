//! ============================================================================
//! Crate: sdk
//! ----------------------------------------------------------------------------
//! WHAT THIS CRATE IS FOR:
//!   The public programmatic API — the Rust equivalent of `src/sdk.ts` +
//!   `src/exports.ts`. `query()` runs the FULL pipeline (load agent → build
//!   tools → gates → LLM → loop) and streams normalised `SdkMessage`s over a
//!   tokio channel; `Session` keeps one agent alive across turns; `tool()`
//!   defines custom closure tools; `permissions` adds Claude-Code-style
//!   allow/deny modes.
//!
//! DESIGN PATTERNS USED:
//!   * Facade — `query()` hides loader/tools/hooks/plugins/mcp/llm/engine.
//!   * Observer — `SdkMessage` channel; the engine publishes, you subscribe.
//!   * Builder — `QueryOptions` + chained `Agent` builders underneath.
//!   * Decorator — allowed/disallowed filters + gates wrap the registry.
//!
//! MODULES PRESENT IN THIS CRATE:
//!   * `types`       — SdkMessage, QueryOptions, ToolSpec.
//!   * `permissions` — PermissionGate (Claude-Code-style modes + rules).
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
