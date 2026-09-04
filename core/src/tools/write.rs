//! ============================================================================
//! Module: engine::tools::write
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The `write` tool — create/overwrite a file, parents auto-created.
//!   Ports `src/tools/write.ts`: mkdir -p unless `createDirs === false`,
//!   returns `Wrote N bytes to <path>`.
//!
//! DESIGN PATTERNS USED:
//!   * Strategy + Command — `AgentTool` impl; Sequential (mutates files).
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `WriteTool` — `new(cwd)`.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::tools::WriteTool;
//! use engine::agent::AgentTool;
//! use std::path::PathBuf;
//! # async fn demo() {
//! let t = WriteTool::new(PathBuf::from("/tmp"));
//! let out = t.execute("w1", serde_json::json!({"path": "a.txt", "content": "hi"})).await.unwrap();
//! # }
//! ```
//! ============================================================================

use crate::agent::{AgentTool, ExecutionMode, ToolOutput};
use async_trait::async_trait;
use std::path::PathBuf;

/// File-writing tool (Sequential — mutates the filesystem).
pub struct WriteTool {
    /// Base dir for relative paths.
    pub cwd: PathBuf,
}

impl WriteTool {
    /// Create the tool rooted at `cwd`.
    ///
    /// # Example
    /// ```rust
    /// use engine::tools::WriteTool;
    /// use std::path::PathBuf;
    /// let t = WriteTool::new(PathBuf::from("."));
    /// assert_eq!(t.name(), "write");
    /// ```
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }

    /// Registry name (kept for tests).
    pub fn name(&self) -> &str {
        "write"
    }
}

#[async_trait]
impl AgentTool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }
    fn description(&self) -> &str {
        "Create or overwrite a file (parent directories are created automatically)"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to the file to write (relative or absolute)"},
                "content": {"type": "string", "description": "Content to write to the file"},
                "createDirs": {"type": "boolean", "description": "Create parent directories if needed (default: true)"}
            },
            "required": ["path", "content"]
        })
    }
    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Sequential
    }

    async fn execute(&self, _id: &str, args: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let raw = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
        if raw.trim().is_empty() {
            return Ok(ToolOutput::err("Error: `path` is required"));
        }
        let path = crate::helpers::resolve_path(&self.cwd, raw);
        let create_dirs = args
            .get("createDirs")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if create_dirs {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        return Ok(ToolOutput::err(format!("Error: cannot create dirs: {e}")));
                    }
                }
            }
        }
        match std::fs::write(&path, content) {
            Ok(()) => Ok(ToolOutput::ok(format!(
                "Wrote {} bytes to {}",
                content.len(),
                path.display()
            ))),
            Err(e) => Ok(ToolOutput::err(format!(
                "Error: cannot write {}: {e}",
                path.display()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn writes_with_parents() {
        let d = std::env::temp_dir().join(format!("ga-write-{}", std::process::id()));
        let t = WriteTool::new(d.clone());
        let out = t
            .execute(
                "w",
                serde_json::json!({"path": "sub/a.txt", "content": "hello"}),
            )
            .await
            .unwrap();
        assert!(!out.is_error && out.content.contains("Wrote 5 bytes"));
        std::fs::remove_dir_all(&d).ok();
    }
}
