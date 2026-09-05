//! ============================================================================
//! Module: engine::tools::read
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The `read` tool — read a file with binary detection + line pagination.
//!   Flow: resolve `~/` via HOME and relative paths via tool cwd, load raw
//!   bytes, sniff for binary, decode lossy UTF-8, apply a char-wise 100KB
//!   cap, then return a 1-indexed offset/limit page plus a continuation
//!   footer (`[Showing lines X-Y of Z ...]`) when output was truncated.
//!
//! DESIGN PATTERNS USED:
//!   * Strategy — implements `AgentTool`; stays `Parallel` because reads are
//!     side-effect free, so concurrent batches are safe.
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `ReadTool` — `new(cwd)`; `execute` validates `path`, paginates text.
//!
//! HOW IT WORKS (data flow + limits):
//!   * Path resolution: leading `~/` expands to HOME; absolute paths pass
//!     through; relative paths join onto `cwd` (the agent directory).
//!   * Binary sniff: scan first 8192 bytes for a null byte; on hit return
//!     `[Binary file: <path> (<n> bytes)]` instead of content. 8KB is enough
//!     to catch executables/images while keeping the check O(1).
//!   * Text path: lossy UTF-8 decode, take at most 100_000 chars, then
//!     paginate with defaults 1 / 2000 lines. Footer carries the next
//!     offset so the model can page through large files.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::tools::ReadTool;
//! use engine::agent::AgentTool;
//! use std::path::PathBuf;
//! # async fn demo() {
//! let t = ReadTool::new(PathBuf::from("."));
//! let out = t.execute("r1", serde_json::json!({"path": "agent.yaml"})).await.unwrap();
//! # }
//! ```
//! ============================================================================

use crate::agent::{AgentTool, ExecutionMode, ToolOutput};
use async_trait::async_trait;
use std::path::PathBuf;

/// File-reading tool (Parallel — the only concurrent-safe builtin).
pub struct ReadTool {
    /// Base dir for relative paths.
    pub cwd: PathBuf,
}

impl ReadTool {
    /// Create the tool rooted at `cwd`.
    ///
    /// # Example
    /// ```rust
    /// use engine::tools::ReadTool;
    /// use std::path::PathBuf;
    /// let t = ReadTool::new(PathBuf::from("."));
    /// assert_eq!(t.name(), "read");
    /// ```
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }

    /// Human-readable name (used by `AgentTool::name`, kept for tests).
    pub fn name(&self) -> &str {
        "read"
    }
}

#[async_trait]
impl AgentTool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }
    fn description(&self) -> &str {
        "Read a file with line pagination (binary files are reported, not dumped)"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to the file to read (relative or absolute)"},
                "offset": {"type": "number", "description": "Line number to start from (1-indexed)"},
                "limit": {"type": "number", "description": "Maximum number of lines to read"}
            },
            "required": ["path"]
        })
    }
    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Parallel
    }

    async fn execute(&self, _id: &str, args: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let raw = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        if raw.trim().is_empty() {
            return Ok(ToolOutput::err("Error: `path` is required"));
        }
        let path = crate::helpers::resolve_path(&self.cwd, raw);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                return Ok(ToolOutput::err(format!(
                    "Error: cannot read {}: {e}",
                    path.display()
                )))
            }
        };
        // Binary sniff: a null byte in the first 8192 bytes means binary —
        // report size instead of dumping non-text into model context.
        if bytes.iter().take(8192).any(|&b| b == 0) {
            return Ok(ToolOutput::ok(format!(
                "[Binary file: {} ({} bytes)]",
                path.display(),
                bytes.len()
            )));
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(crate::helpers::MAX_READ_LINES as u64) as usize;
        // Byte cap: char-wise 100KB slice before pagination so one page
        // always fits inside the model context window.
        let capped: String = text.chars().take(crate::helpers::MAX_READ_BYTES).collect();
        match crate::helpers::paginate_lines(&capped, offset.max(1), limit.max(1)) {
            Ok((page, footer)) => {
                let mut out = page;
                if !footer.is_empty() {
                    out.push('\n');
                    out.push_str(&footer);
                }
                Ok(ToolOutput::ok(out))
            }
            Err(e) => Ok(ToolOutput::err(format!("Error: {e:#}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reads_and_paginates() {
        let d = std::env::temp_dir().join(format!("ga-read-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("a.txt"), "l1\nl2\nl3").unwrap();
        let t = ReadTool::new(d.clone());
        let out = t
            .execute("r", serde_json::json!({"path": "a.txt", "limit": 2}))
            .await
            .unwrap();
        assert!(
            !out.is_error && out.content.contains("l2") && out.content.contains("Showing lines")
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn detects_binary() {
        let d = std::env::temp_dir().join(format!("ga-bin-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("b.bin"), [0u8, 1, 2, 3]).unwrap();
        let t = ReadTool::new(d.clone());
        let out = t
            .execute("r", serde_json::json!({"path": "b.bin"}))
            .await
            .unwrap();
        assert!(out.content.contains("Binary file"));
        std::fs::remove_dir_all(&d).ok();
    }
}
