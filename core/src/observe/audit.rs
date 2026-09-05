//! ============================================================================
//! Module: engine::observe::audit
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Append-only audit log (`.gitagent/audit.jsonl`). Enabled by
//!   `compliance.recordkeeping.audit_logging`. Records session_start/end,
//!   tool_use, tool_result (content sliced to 1000 chars), and errors —
//!   one JSON object per line with an `at` timestamp, creating parent
//!   dirs on demand.
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `AuditLogger` — `disabled()` / `new()` + `record_*()` methods.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::observe::AuditLogger;
//! use std::path::PathBuf;
//! let log = AuditLogger::new(PathBuf::from("./my-agent/.gitagent/audit.jsonl"));
//! log.record_session_start("sess-1", "openai:gpt-4o-mini");
//! ```
//! ============================================================================

use std::path::PathBuf;

/// Max result chars stored per audit record (results sliced to 1000).
pub const AUDIT_RESULT_SLICE: usize = 1000;

/// Append-only audit logger (Observer of session events).
#[derive(Debug, Clone)]
pub struct AuditLogger {
    path: Option<PathBuf>,
}

impl AuditLogger {
    /// Create a real logger writing to `path`.
    ///
    /// # Example
    /// ```rust
    /// use engine::observe::AuditLogger;
    /// use std::path::PathBuf;
    /// let _ = AuditLogger::new(PathBuf::from("/tmp/a.jsonl"));
    /// ```
    pub fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    /// Create a no-op logger (auditing disabled).
    ///
    /// # Example
    /// ```rust
    /// use engine::observe::AuditLogger;
    /// let l = AuditLogger::disabled();
    /// l.record_session_start("s", "m"); // no-op, never fails
    /// ```
    pub fn disabled() -> Self {
        Self { path: None }
    }

    fn emit(&self, record: serde_json::Value) {
        if let Some(p) = &self.path {
            let mut r = record;
            r["at"] = serde_json::Value::String(chrono::Utc::now().to_rfc3339());
            helpers_slice_append(p, &r);
        }
    }

    /// Record session start (model + session id).
    pub fn record_session_start(&self, session_id: &str, model: &str) {
        self.emit(
            serde_json::json!({"event": "session_start", "session_id": session_id, "model": model}),
        );
    }

    /// Record session end (+ total USD cost).
    pub fn record_session_end(&self, session_id: &str, cost_usd: f64) {
        self.emit(serde_json::json!({"event": "session_end", "session_id": session_id, "cost_usd": cost_usd}));
    }

    /// Record a tool call (args stored verbatim — small by schema).
    pub fn record_tool_use(&self, session_id: &str, tool: &str, args: &serde_json::Value) {
        self.emit(serde_json::json!({"event": "tool_use", "session_id": session_id, "tool": tool, "args": args}));
    }

    /// Record a tool result (content sliced to 1000 chars).
    pub fn record_tool_result(&self, session_id: &str, tool: &str, content: &str, is_error: bool) {
        let slice: String = content.chars().take(AUDIT_RESULT_SLICE).collect();
        self.emit(serde_json::json!({"event": "tool_result", "session_id": session_id, "tool": tool, "content": slice, "is_error": is_error}));
    }

    /// Record an error.
    pub fn record_error(&self, session_id: &str, message: &str) {
        self.emit(
            serde_json::json!({"event": "error", "session_id": session_id, "message": message}),
        );
    }
}

fn helpers_slice_append(path: &PathBuf, v: &serde_json::Value) {
    // Cheap local append (crate::helpers::jsonl::append_jsonl would add a dep; inline it).
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).ok();
        }
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{}", serde_json::to_string(v).unwrap_or_default());
    }
}
