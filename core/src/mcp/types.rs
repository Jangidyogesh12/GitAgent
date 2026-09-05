//! ============================================================================
//! Module: engine::mcp::types
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   MCP server config types. Defines stdio (`command/args/env/cwd/
//!   timeoutMs`) vs http/sse (`url/headers/timeoutMs`) configs, plus the
//!   tool-naming rules (sanitise + 64-char cap).
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `McpServerConfig`     — untagged enum deserialised from agent.yaml.
//!   * `sanitise_tool_name()`— `[^a-zA-Z0-9_-]` → `_`, truncate to 64.
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::mcp::sanitise_tool_name;
//! assert_eq!(sanitise_tool_name("srv", "my tool!"), "srv__my_tool_");
//! ```
//! ============================================================================

use serde::{Deserialize, Serialize};

/// Default per-server connect + listTools timeout (30_000 ms).
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;
/// Provider-imposed tool-name length cap (names truncate to 64).
pub const MAX_TOOL_NAME_LEN: usize = 64;

/// One MCP server declaration (untagged: presence of `command` vs `url`
/// decides the variant).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpServerConfig {
    /// Spawn a local process speaking JSON-RPC over stdio.
    Stdio {
        /// Command to spawn (e.g. `npx`, `python3`).
        command: String,
        /// Argv (each `${VAR}`-interpolated at setup).
        #[serde(default)]
        args: Vec<String>,
        /// Extra env merged over the process env.
        #[serde(default)]
        env: std::collections::HashMap<String, String>,
        /// Working directory for the child.
        #[serde(default)]
        cwd: Option<String>,
        /// Connect/list timeout ms.
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    /// Remote HTTP/SSE endpoint (parsed; transport pending — see `crate::mcp::manager`).
    Http {
        /// http | sse (defaults to http when absent).
        #[serde(default, rename = "type")]
        kind: Option<String>,
        /// Endpoint URL.
        url: String,
        /// Extra headers (`${VAR}`-interpolated).
        #[serde(default)]
        headers: std::collections::HashMap<String, String>,
        /// Timeout ms.
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
}

/// Build `<server>__<tool>`, sanitised + capped.
///
/// # Description
/// `[^a-zA-Z0-9_-]` → `_`, then truncate to 64 chars. Callers must
/// collision-check against existing names (manager does).
///
/// # Example
/// ```rust
/// use engine::mcp::sanitise_tool_name;
/// assert_eq!(sanitise_tool_name("srv", "a b!c"), "srv__a_b_c");
/// ```
pub fn sanitise_tool_name(server: &str, tool: &str) -> String {
    let raw = format!("{server}__{tool}");
    let clean: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    clean.chars().take(MAX_TOOL_NAME_LEN).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitises_and_caps() {
        assert_eq!(sanitise_tool_name("s", "a b!"), "s__a_b_");
        assert_eq!(
            sanitise_tool_name("s", &"x".repeat(100)).len(),
            MAX_TOOL_NAME_LEN
        );
    }
}
