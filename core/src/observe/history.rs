//! ============================================================================
//! Module: engine::observe::history
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Per-branch chat history (`.gitagent/chat-history/<branch>.jsonl`).
//!   Appends `{role, content, at}` records and reads them back (malformed
//!   lines skipped); branch slashes are mapped to `_` for filenames.
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `ChatHistory` — `new()` + `append()` + `read()` + `count()`.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::observe::ChatHistory;
//! use std::path::PathBuf;
//! let h = ChatHistory::new(PathBuf::from("./my-agent/.gitagent"), "main");
//! h.append("user", "hello");
//! ```
//! ============================================================================

use std::path::PathBuf;

/// Per-branch JSONL chat history store.
#[derive(Debug, Clone)]
pub struct ChatHistory {
    path: PathBuf,
}

impl ChatHistory {
    /// Open history for `branch` under `<gitagent_dir>/chat-history/`.
    ///
    /// # Example
    /// ```rust
    /// use engine::observe::ChatHistory;
    /// use std::path::PathBuf;
    /// let h = ChatHistory::new(PathBuf::from("/tmp/g"), "main");
    /// assert_eq!(h.count(), 0);
    /// ```
    pub fn new(gitagent_dir: PathBuf, branch: &str) -> Self {
        let safe: String = branch
            .chars()
            .map(|c| if c == '/' { '_' } else { c })
            .collect();
        Self {
            path: gitagent_dir.join(format!("chat-history/{safe}.jsonl")),
        }
    }

    /// Append one `{role, content}` message.
    ///
    /// # Example
    /// ```rust,no_run
    /// use engine::observe::ChatHistory;
    /// use std::path::PathBuf;
    /// ChatHistory::new(PathBuf::from("/tmp/g"), "main").append("user", "hi");
    /// ```
    pub fn append(&self, role: &str, content: &str) {
        let v = serde_json::json!({"role": role, "content": content, "at": chrono::Utc::now().to_rfc3339()});
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(f, "{v}");
        }
    }

    /// Read all messages back (malformed lines skipped).
    ///
    /// # Example
    /// ```rust,no_run
    /// use engine::observe::ChatHistory;
    /// use std::path::PathBuf;
    /// let msgs = ChatHistory::new(PathBuf::from("/tmp/g"), "main").read();
    /// ```
    pub fn read(&self) -> Vec<serde_json::Value> {
        std::fs::read_to_string(&self.path)
            .map(|t| {
                t.lines()
                    .filter(|l| !l.trim().is_empty())
                    .filter_map(|l| serde_json::from_str(l).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Number of stored messages.
    ///
    /// # Example
    /// ```rust
    /// use engine::observe::ChatHistory;
    /// use std::path::PathBuf;
    /// assert_eq!(ChatHistory::new(PathBuf::from("/nope"), "b").count(), 0);
    /// ```
    pub fn count(&self) -> usize {
        self.read().len()
    }
}
