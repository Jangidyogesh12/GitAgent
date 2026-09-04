//! ============================================================================
//! Module: sdk::types
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Public SDK data types. Ports `src/sdk-types.ts`: the `GCMessage` union
//!   (here `SdkMessage`), `QueryOptions`, and the custom-tool spec.
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `SdkMessage`   — Delta | Assistant | ToolUse | ToolResult | System.
//!   * `QueryOptions` — dir/prompt/model/turns/filters/suffix... (+ `new()`).
//!
//! HOW TO USE (example):
//! ```rust
//! use sdk::QueryOptions;
//! use std::path::PathBuf;
//! let opts = QueryOptions::new(PathBuf::from("."), "hi").with_max_turns(5);
//! assert_eq!(opts.max_turns, Some(5));
//! ```
//! ============================================================================

use std::path::PathBuf;

/// One normalised stream item from `query()` (ports `GCMessage`).
#[derive(Debug, Clone)]
pub enum SdkMessage {
    /// A text fragment (streaming delta).
    Delta(String),
    /// A finished assistant turn (full text).
    Assistant(String),
    /// A tool call started (id, name, args).
    ToolUse(String, String, serde_json::Value),
    /// A tool call finished (id, name, content, is_error).
    ToolResult(String, String, String, bool),
    /// Lifecycle note (`session_start`, `session_end`, `hook_blocked`...).
    System(String),
    /// Terminal failure for the whole query.
    Error(String),
}

/// Options for one `query()` call (ports `QueryOptions`).
///
/// `Clone` (not `Debug`) — the tool registry holds trait objects.
#[derive(Clone)]
pub struct QueryOptions {
    /// Agent directory (must contain `agent.yaml`).
    pub dir: PathBuf,
    /// The user prompt (single-shot; use `Session` for multi-turn).
    pub prompt: String,
    /// `provider:model` override (wins over the manifest).
    pub model: Option<String>,
    /// Max turns override (default: manifest `max_turns`).
    pub max_turns: Option<u32>,
    /// Allowlist applied BEFORE the denylist (None = all tools).
    pub allowed_tools: Option<Vec<String>>,
    /// Denylist applied AFTER the allowlist.
    pub disallowed_tools: Vec<String>,
    /// Appended to the assembled system prompt (`"\n\n" + suffix`).
    pub system_prompt_suffix: Option<String>,
    /// Extra custom tools appended to the registry.
    pub extra_tools: Vec<std::sync::Arc<dyn engine::agent::AgentTool>>,
    /// Permission mode (Claude-Code-style gate); None = Default.
    pub permission_mode: Option<crate::permissions::PermissionMode>,
    /// Permission rules, e.g. `deny:cli(rm -rf)`, `allow:read`.
    pub permission_rules: Vec<String>,
    /// Session id override (fresh UUID when None).
    pub session_id: Option<String>,
}

impl QueryOptions {
    /// Minimal options: agent dir + prompt.
    ///
    /// # Example
    /// ```rust
    /// use sdk::QueryOptions;
    /// use std::path::PathBuf;
    /// let o = QueryOptions::new(PathBuf::from("."), "hello");
    /// assert!(o.model.is_none());
    /// ```
    pub fn new(dir: PathBuf, prompt: impl Into<String>) -> Self {
        Self {
            dir,
            prompt: prompt.into(),
            model: None,
            max_turns: None,
            allowed_tools: None,
            disallowed_tools: vec![],
            system_prompt_suffix: None,
            extra_tools: vec![],
            permission_mode: None,
            permission_rules: vec![],
            session_id: None,
        }
    }

    /// Override max turns (Builder).
    ///
    /// # Example
    /// ```rust
    /// use sdk::QueryOptions;
    /// use std::path::PathBuf;
    /// assert_eq!(QueryOptions::new(PathBuf::from("."), "x").with_max_turns(3).max_turns, Some(3));
    /// ```
    pub fn with_max_turns(mut self, n: u32) -> Self {
        self.max_turns = Some(n);
        self
    }

    /// Override the model (Builder).
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }
}
