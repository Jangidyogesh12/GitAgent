//! ============================================================================
//! Crate: mcp
//! ----------------------------------------------------------------------------
//! WHAT THIS CRATE IS FOR:
//!   Model Context Protocol client. Handles stdio / http / sse server
//!   configs with `${VAR}` interpolation, parallel fail-soft connect,
//!   paginated `tools/list` (follows `nextCursor`), `<server>__<tool>`
//!   names sanitised to `[a-zA-Z0-9_-]` + truncated to 64 chars, collision
//!   checks, result flattening (text joined, image/audio → `[image: ...]`,
//!   resource → `[resource: ...]`, empty + structuredContent → JSON,
//!   isError prefix), and idempotent `cleanup()`.
//!
//!   Transport note: the **stdio** transport is fully implemented
//!   (JSON-RPC 2.0 over the child process pipes: initialize →
//!   notifications/initialized → tools/list → tools/call). HTTP/SSE servers
//!   are parsed from config and reported as unsupported — the manager
//!   stays fail-soft and the session continues.
//!
//! DESIGN PATTERNS USED:
//!   * Adapter — `McpTool` adapts a remote MCP tool to `AgentTool`.
//!   * Facade — `McpManager::setup()` hides connect + list + register.
//!   * RAII — `cleanup()` is idempotent; children die with the manager.
//!
//! MODULES PRESENT IN THIS CRATE:
//!   * `types`   — McpServerConfig (+ sanitise/truncate helpers).
//!   * `manager` — McpManager setup/call/cleanup + flatten_result().
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::mcp::McpManager;
//! # async fn demo() {
//! let mut mgr = McpManager::setup(&serde_json::json!({})).await;
//! println!("{} tools", mgr.tools.len());
//! mgr.cleanup().await;
//! # }
//! ```
//! ============================================================================

pub mod manager;
pub mod types;

pub use manager::{flatten_result, McpManager, McpTool};
pub use types::{sanitise_tool_name, McpServerConfig};
