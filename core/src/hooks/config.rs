//! ============================================================================
//! Module: engine::hooks::config
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Hook configuration types + loading. Ports the `HooksConfig` shape from
//!   `src/hooks.ts` (`hooks/hooks.yaml` → per-event definition lists) and
//!   `mergeHooksConfigs()` from `src/plugins.ts` (agent hooks first, then
//!   each plugin's, concatenated per event).
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `HookDefinition`    — {script, description?, base_dir?}.
//!   * `HooksConfig`       — per-event lists + `for_event()` accessor.
//!   * `load_hooks_config()` — read agent `hooks/hooks.yaml` (missing → empty).
//!   * `merge_hooks()`     — concatenate base + plugin configs per event.
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::hooks::{HooksConfig, merge_hooks};
//! let merged = merge_hooks(&HooksConfig::default(), &[HooksConfig::default()]);
//! assert!(merged.pre_tool_use.is_empty());
//! ```
//! ============================================================================

use serde::{Deserialize, Serialize};
use std::path::Path;

/// One hook: a script run with JSON on stdin.
///
/// # Description
/// `base_dir` is the plugin dir for plugin hooks (agent hooks resolve under
/// `<agent>/hooks/`); it anchors the path-traversal guard in `exec.rs`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookDefinition {
    /// Script path (relative to the hooks dir / plugin base dir).
    #[serde(default)]
    pub script: String,
    /// Human description (logs only).
    #[serde(default)]
    pub description: String,
    /// Base dir scripts resolve under (plugin hooks set this).
    #[serde(default, skip_serializing)]
    pub base_dir: String,
}

/// Per-lifecycle-event hook lists (mirrors the TS HooksConfig events).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HooksConfig {
    /// Run at session start; `block` aborts the session (exit 1 in CLI).
    #[serde(default)]
    pub on_session_start: Vec<HookDefinition>,
    /// Run before every tool call; block/modify enforced.
    #[serde(default)]
    pub pre_tool_use: Vec<HookDefinition>,
    /// Run after tool failures (observability).
    #[serde(default)]
    pub post_tool_failure: Vec<HookDefinition>,
    /// Run after each assistant turn (observability).
    #[serde(default)]
    pub post_response: Vec<HookDefinition>,
    /// Run before each user query; block skips the turn.
    #[serde(default)]
    pub pre_query: Vec<HookDefinition>,
    /// Run after `write`/`edit` tool calls (path in payload).
    #[serde(default)]
    pub file_changed: Vec<HookDefinition>,
    /// Run on session errors (observability).
    #[serde(default)]
    pub on_error: Vec<HookDefinition>,
}

impl HooksConfig {
    /// Access the list for one event name (for generic dispatchers).
    ///
    /// # Example
    /// ```rust
    /// use engine::hooks::HooksConfig;
    /// assert!(HooksConfig::default().for_event("pre_tool_use").is_empty());
    /// ```
    pub fn for_event(&self, event: &str) -> &[HookDefinition] {
        match event {
            "on_session_start" => &self.on_session_start,
            "pre_tool_use" => &self.pre_tool_use,
            "post_tool_failure" => &self.post_tool_failure,
            "post_response" => &self.post_response,
            "pre_query" => &self.pre_query,
            "file_changed" => &self.file_changed,
            _ => &self.on_error,
        }
    }
}

/// Load `<agent>/hooks/hooks.yaml` (missing/invalid → empty config).
///
/// # Description
/// Fail-soft like the TS loader: hooks are auxiliary machinery; a missing
/// file simply means "no hooks".
///
/// # Example
/// ```rust,no_run
/// use engine::hooks::load_hooks_config;
/// use std::path::Path;
/// let cfg = load_hooks_config(Path::new("./my-agent"));
/// ```
pub fn load_hooks_config(agent_dir: &Path) -> HooksConfig {
    #[derive(Deserialize, Default)]
    struct File {
        #[serde(default)]
        hooks: HooksConfig,
    }
    std::fs::read_to_string(agent_dir.join("hooks/hooks.yaml"))
        .ok()
        .and_then(|t| serde_yaml::from_str::<File>(&t).ok())
        .map(|f| f.hooks)
        .unwrap_or_default()
}

/// Concatenate base + plugin hook lists per event (agent first).
///
/// # Description
/// Ports `mergeHooksConfigs()`: per-event arrays concatenated, agent hooks
/// first so they run before plugin hooks.
///
/// # Example
/// ```rust
/// use engine::hooks::{HooksConfig, merge_hooks};
/// let m = merge_hooks(&HooksConfig::default(), &[]);
/// assert!(m.pre_tool_use.is_empty());
/// ```
pub fn merge_hooks(base: &HooksConfig, plugins: &[HooksConfig]) -> HooksConfig {
    let mut out = base.clone();
    for p in plugins {
        out.on_session_start
            .extend(p.on_session_start.iter().cloned());
        out.pre_tool_use.extend(p.pre_tool_use.iter().cloned());
        out.post_tool_failure
            .extend(p.post_tool_failure.iter().cloned());
        out.post_response.extend(p.post_response.iter().cloned());
        out.pre_query.extend(p.pre_query.iter().cloned());
        out.file_changed.extend(p.file_changed.iter().cloned());
        out.on_error.extend(p.on_error.iter().cloned());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_empty_config() {
        let cfg = load_hooks_config(Path::new("/definitely/missing"));
        assert!(cfg.pre_tool_use.is_empty());
    }
}
