//! ============================================================================
//! Crate: plugins
//! ----------------------------------------------------------------------------
//! WHAT THIS CRATE IS FOR:
//!   The plugin system. Handles `plugin.yaml` manifests (id kebab-case,
//!   provides {tools, hooks, skills, prompt}, config schema with
//!   user > env > default resolution), 3-scope discovery (local `plugins/`
//!   → global `~/.gitagent/plugins/` → installed `.gitagent/plugins/`),
//!   auto-install from `source` git URLs, tool-name collision detection
//!   (whole plugin skipped on clash), and programmatic-entry stubs
//!   (register() API is represented as data here; script hooks +
//!   declarative tools carry the behaviour).
//!
//! DESIGN PATTERNS USED:
//!   * Plugin (of course) — discovery + validation + merged contributions.
//!   * Adapter — `plugin_prompt_additions()` / `plugin_hook_configs()`
//!     adapt plugin dirs to loader/hook inputs.
//!
//! MODULES PRESENT IN THIS CRATE:
//!   * `types`    — PluginManifest, PluginConfigValue, LoadedPlugin.
//!   * `discover` — discover/install/load + contributions.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::plugins::discover_plugins;
//! use std::path::Path;
//! let plugins = discover_plugins(Path::new("./my-agent"), &serde_json::Value::Null);
//! ```
//! ============================================================================

pub mod discover;
pub mod types;

pub use discover::{discover_plugins, plugin_hook_configs, plugin_prompt_additions};
pub use types::{LoadedPlugin, PluginManifest};
