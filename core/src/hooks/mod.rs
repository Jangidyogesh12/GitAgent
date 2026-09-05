//! ============================================================================
//! Crate: hooks
//! ----------------------------------------------------------------------------
//! WHAT THIS CRATE IS FOR:
//!   Lifecycle script hooks. Loads `hooks/hooks.yaml`, spawns
//!   `sh <script>` with JSON on stdin plus a 10s timeout, parses stdout as
//!   `{action: allow|block|modify, reason?, args?}` (unparseable → allow),
//!   enforces a path-traversal guard (script must stay under its base dir),
//!   stays fail-open (hook errors log + allow, never block), and exposes
//!   `HookGate`, the `ToolGate` adapter wiring `pre_tool_use` into the engine.
//!
//! DESIGN PATTERNS USED:
//!   * Chain of Responsibility — `run_hooks()` runs definitions in order;
//!     first block/modify wins.
//!   * Adapter — `HookGate` adapts script hooks to the engine's gate trait.
//!   * Decorator — `wrap_tool_with_hooks()` decorates any tool's execution.
//!
//! MODULES PRESENT IN THIS CRATE:
//!   * `config` — HooksConfig/HookDefinition loading + merge.
//!   * `exec`   — execute_hook() + run_hooks().
//!   * `gate`   — HookGate (pre_tool_use → ToolGate).
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::hooks::{load_hooks_config, run_hooks, HookInput};
//! use std::path::Path;
//! # async fn demo() {
//! let cfg = load_hooks_config(Path::new("./my-agent"));
//! let verdict = run_hooks(&cfg.pre_tool_use, &Path::new("./my-agent/hooks").to_path_buf(), &HookInput::new("pre_tool_use", "s")).await;
//! # }
//! ```
//! ============================================================================

pub mod config;
pub mod exec;
pub mod gate;

pub use config::{load_hooks_config, merge_hooks, HookDefinition, HooksConfig};
pub use exec::{execute_hook, run_hooks, HookInput, HookVerdict};
pub use gate::HookGate;
