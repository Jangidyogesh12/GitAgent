//! ============================================================================
//! Module: engine::tools::memory
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The `memory` tool — git-backed long-term memory. Ports
//!   `src/tools/memory.ts`: layered config (`memory/memory.yaml`, working
//!   layer = `"working"` else first, default `memory/MEMORY.md`), `load`
//!   (trimmed content / `"No memories yet."`), `save` (overflow archive to
//!   `memory/archive/YYYY-MM.md` when over `max_lines`, then
//!   `git add + git commit -m`, quotes escaped, commit failure NON-fatal).
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `MemoryLayer` — {name, path, max_lines} config record.
//!   * `MemoryTool`  — `new(agent_dir)`; `load_layer_config()` helper.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::tools::MemoryTool;
//! use engine::agent::AgentTool;
//! use std::path::PathBuf;
//! # async fn demo() {
//! let t = MemoryTool::new(PathBuf::from("./my-agent"));
//! let out = t.execute("m1", serde_json::json!({"action": "load"})).await.unwrap();
//! # }
//! ```
//! ============================================================================

use crate::agent::{AgentTool, ExecutionMode, ToolOutput};
use async_trait::async_trait;
use std::path::PathBuf;

/// Default memory file when no layer config exists.
pub const DEFAULT_MEMORY_PATH: &str = "memory/MEMORY.md";

/// One `{name, path, max_lines}` layer from `memory/memory.yaml`.
#[derive(Debug, Clone)]
pub struct MemoryLayer {
    /// Layer name (`"working"` is preferred as the active layer).
    pub name: String,
    /// Repo-relative file path.
    pub path: String,
    /// Max lines before overflow archiving (None = unbounded).
    pub max_lines: Option<usize>,
}

/// Git-backed memory tool (Sequential — commits to git).
pub struct MemoryTool {
    /// Agent directory (memory paths resolve under it).
    pub agent_dir: PathBuf,
}

impl MemoryTool {
    /// Create the tool for an agent directory.
    ///
    /// # Example
    /// ```rust
    /// use engine::tools::MemoryTool;
    /// use std::path::PathBuf;
    /// assert_eq!(MemoryTool::new(PathBuf::from(".")).name(), "memory");
    /// ```
    pub fn new(agent_dir: PathBuf) -> Self {
        Self { agent_dir }
    }

    /// Registry name (kept for tests).
    pub fn name(&self) -> &str {
        "memory"
    }

    /// Read `memory/memory.yaml` layers; empty vec when unconfigured.
    ///
    /// # Example
    /// ```rust,no_run
    /// use engine::tools::MemoryTool;
    /// use std::path::PathBuf;
    /// let layers = MemoryTool::new(PathBuf::from(".")).load_layer_config();
    /// ```
    pub fn load_layer_config(&self) -> Vec<MemoryLayer> {
        let path = self.agent_dir.join("memory/memory.yaml");
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => return vec![],
        };
        #[derive(serde::Deserialize)]
        struct Cfg {
            #[serde(default)]
            layers: Vec<LayerCfg>,
        }
        #[derive(serde::Deserialize)]
        struct LayerCfg {
            #[serde(default)]
            name: String,
            #[serde(default)]
            path: String,
            #[serde(default)]
            max_lines: Option<usize>,
        }
        serde_yaml::from_str::<Cfg>(&text)
            .map(|c| {
                c.layers
                    .into_iter()
                    .map(|l| MemoryLayer {
                        name: l.name,
                        path: l.path,
                        max_lines: l.max_lines,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn working_layer(&self) -> MemoryLayer {
        let layers = self.load_layer_config();
        if let Some(w) = layers.iter().find(|l| l.name == "working") {
            return w.clone();
        }
        layers.into_iter().next().unwrap_or(MemoryLayer {
            name: "working".into(),
            path: DEFAULT_MEMORY_PATH.into(),
            max_lines: None,
        })
    }
}

#[async_trait]
impl AgentTool for MemoryTool {
    fn name(&self) -> &str {
        "memory"
    }
    fn description(&self) -> &str {
        "Load or save the agent's git-committed long-term memory"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {"type": "string", "description": "Whether to load or save memory"},
                "content": {"type": "string", "description": "Memory content to save (required for save)"},
                "message": {"type": "string", "description": "Commit message describing why memory changed (required for save)"}
            },
            "required": ["action"]
        })
    }
    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Sequential
    }

    async fn execute(&self, _id: &str, args: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let layer = self.working_layer();
        let mem_path = self.agent_dir.join(&layer.path);
        match action {
            "load" => {
                let content = std::fs::read_to_string(&mem_path).unwrap_or_default();
                let trimmed = content.trim();
                if trimmed.is_empty() || trimmed == "# Memory" {
                    Ok(ToolOutput::ok("No memories yet."))
                } else {
                    Ok(ToolOutput::ok(trimmed.to_string()))
                }
            }
            "save" => {
                let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
                let message = args
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("update memory");
                // Overflow archive: keep last N lines, append the rest to
                // memory/archive/YYYY-MM.md (TS `archiveOverflow` rule).
                let mut to_write = content.to_string();
                if let Some(max) = layer.max_lines {
                    let lines: Vec<&str> = content.lines().collect();
                    if lines.len() > max {
                        let overflow = lines[..lines.len() - max].join("\n");
                        let keep = lines[lines.len() - max..].join("\n");
                        let month = chrono::Utc::now().format("%Y-%m").to_string();
                        let arch = self.agent_dir.join(format!("memory/archive/{month}.md"));
                        if let Some(p) = arch.parent() {
                            std::fs::create_dir_all(p).ok();
                        }
                        let sep = format!("\n\n_Archived: {}_\n", chrono::Utc::now().to_rfc3339());
                        let mut prev = std::fs::read_to_string(&arch).unwrap_or_default();
                        prev.push_str(&overflow);
                        prev.push_str(&sep);
                        std::fs::write(&arch, prev).ok();
                        to_write = keep;
                    }
                }
                if let Some(p) = mem_path.parent() {
                    std::fs::create_dir_all(p).ok();
                }
                if let Err(e) = std::fs::write(&mem_path, &to_write) {
                    return Ok(ToolOutput::err(format!("Error: cannot save memory: {e}")));
                }
                // Git commit: best-effort (TS: commit failure is a warning).
                let msg_esc = message.replace('"', "'");
                let add = std::process::Command::new("git")
                    .args(["add", &layer.path])
                    .current_dir(&self.agent_dir)
                    .output();
                if add.map(|o| o.status.success()).unwrap_or(false) {
                    let commit = std::process::Command::new("git")
                        .args(["commit", "-m", &msg_esc])
                        .current_dir(&self.agent_dir)
                        .output();
                    if let Ok(o) = commit {
                        if !o.status.success() {
                            let w = String::from_utf8_lossy(&o.stderr);
                            return Ok(ToolOutput::ok(format!(
                                "Memory saved (commit warning: {})",
                                w.trim()
                            )));
                        }
                    }
                }
                Ok(ToolOutput::ok(format!("Memory saved to {}", layer.path)))
            }
            _ => Ok(ToolOutput::err(
                "Error: `action` must be \"load\" or \"save\"",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn load_empty_reports_none() {
        let d = std::env::temp_dir().join(format!("ga-mem-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let t = MemoryTool::new(d.clone());
        let out = t
            .execute("m", serde_json::json!({"action": "load"}))
            .await
            .unwrap();
        assert_eq!(out.content, "No memories yet.");
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn save_then_load_round_trips() {
        let d = std::env::temp_dir().join(format!("ga-mem2-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let t = MemoryTool::new(d.clone());
        t.execute(
            "m",
            serde_json::json!({"action": "save", "content": "# hi", "message": "t"}),
        )
        .await
        .unwrap();
        let out = t
            .execute("m", serde_json::json!({"action": "load"}))
            .await
            .unwrap();
        assert!(out.content.contains("hi"));
        std::fs::remove_dir_all(&d).ok();
    }
}
