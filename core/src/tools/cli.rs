//! ============================================================================
//! Module: engine::tools::cli
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The `cli` tool — run a shell command, capture output, kill on timeout.
//!   Flow: `sh -c` spawn with piped stdout/stderr in tool cwd, await with
//!   timeout, on timeout terminate the whole process group with SIGTERM,
//!   grace 3s, then reap; on exit fold stdout plus stderr-on-failure into
//!   a rolling-tail truncated result. Non-zero exit becomes an error result
//!   value (not a Rust Err); empty output becomes `(no output)`; over-long
//!   output keeps the trailing ~100KB with a truncation marker.
//!
//! DESIGN PATTERNS USED:
//!   * Strategy — `SandboxExec` swaps local shell execution for a remote
//!     argv runner without duplicating timeout/truncation logic.
//!     `None` means run locally.
//!   * Command — the shell string plus timeout travels as one invocable unit.
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `SandboxExec` — Strategy trait for remote execution backends.
//!   * `CliTool`     — the tool (Sequential: mutates the world).
//!
//! HOW IT WORKS (lifecycle + limits):
//!   * Local spawn uses `sh -c` so compound commands, pipes, and redirects
//!     work; `kill_on_drop(true)` guarantees reaping on timeout paths.
//!   * Group kill matters because bare child kill leaves backgrounded
//!     grandchildren alive holding the pipe open, which would hang the
//!     output read; signalling the negative PID reaches the whole group.
//!   * Output handling: lossy UTF-8 decode, append `[stderr]` section on
//!     failure, keep the tail (most recent lines are usually the
//!     diagnostic), append `Exit code: N` on failure for model clarity.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::tools::CliTool;
//! use engine::agent::AgentTool;
//! use std::path::PathBuf;
//! # async fn demo() {
//! let t = CliTool::new(PathBuf::from("."), 120, None);
//! let out = t.execute("c1", serde_json::json!({"command": "echo hi"})).await.unwrap();
//! assert!(!out.is_error);
//! # }
//! ```
//! ============================================================================

use crate::agent::{AgentTool, ExecutionMode, ToolOutput};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Remote-execution backend (Strategy). Alternate runner for sandboxes.
///
/// # Description
/// `run(command, cwd, timeout)` executes WITHOUT a local shell and returns
/// stdout (plus stderr appended on failure, like the local path). Implement
/// for isolated/remote backends; `None` in `CliTool` means run locally.
#[async_trait]
pub trait SandboxExec: Send + Sync {
    /// Run `command` remotely; see trait docs for the output contract.
    async fn run(
        &self,
        command: &str,
        cwd: &std::path::Path,
        timeout: Duration,
    ) -> anyhow::Result<String>;
}

/// Run-a-shell-command tool (Sequential — mutates the machine).
pub struct CliTool {
    /// Working directory for commands.
    pub cwd: PathBuf,
    /// Default timeout seconds (manifest `runtime.timeout` or 120).
    pub default_timeout: u64,
    /// Remote backend; None = local `sh -c`.
    pub sandbox: Option<Arc<dyn SandboxExec>>,
}

impl CliTool {
    /// Create the tool for `cwd` with a default timeout and optional backend.
    ///
    /// # Example
    /// ```rust
    /// use engine::tools::CliTool;
    /// use std::path::PathBuf;
    /// let t = CliTool::new(PathBuf::from("."), 120, None);
    /// assert_eq!(t.default_timeout, 120);
    /// ```
    pub fn new(cwd: PathBuf, default_timeout: u64, sandbox: Option<Arc<dyn SandboxExec>>) -> Self {
        Self {
            cwd,
            default_timeout,
            sandbox,
        }
    }
}

#[async_trait]
impl AgentTool for CliTool {
    fn name(&self) -> &str {
        "cli"
    }
    fn description(&self) -> &str {
        "Run a shell command on the machine and capture its output"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "Shell command to execute"},
                "timeout": {"type": "number", "description": "Timeout in seconds (default: 120)"}
            },
            "required": ["command"]
        })
    }
    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Sequential
    }

    async fn execute(&self, _id: &str, args: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if command.trim().is_empty() {
            return Ok(ToolOutput::err("Error: `command` is required"));
        }
        let timeout_s = args
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(self.default_timeout);
        let timeout = Duration::from_secs(timeout_s.max(1));

        // Remote backend short-circuits local spawn.
        if let Some(sb) = &self.sandbox {
            return match tokio::time::timeout(timeout, sb.run(&command, &self.cwd, timeout)).await {
                Err(_) => Ok(ToolOutput::err(format!(
                    "Command timed out after {timeout_s}s"
                ))),
                Ok(Err(e)) => Ok(ToolOutput::err(format!("Error: {e:#}"))),
                Ok(Ok(o)) => Ok(ToolOutput::ok(o)),
            };
        }

        // Local: `sh -c` with piped output. On timeout SIGTERM the child
        // and, best-effort, its process GROUP (covers backgrounded
        // grandchildren that would otherwise hold the pipe open).
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(&command).current_dir(&self.cwd);
        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);
        let child = cmd.spawn()?;
        let child_id = child.id();

        let out = tokio::time::timeout(timeout, child.wait_with_output()).await;
        let output = match out {
            Err(_) => {
                // Timeout: best-effort group kill, 3s grace, then the
                // `kill_on_drop(true)` reap finishes the job.
                #[cfg(unix)]
                if let Some(pid) = child_id {
                    let _ = tokio::process::Command::new("kill")
                        .args(["-TERM", &format!("-{pid}")])
                        .output()
                        .await;
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
                return Ok(ToolOutput::err(format!(
                    "Command timed out after {timeout_s}s"
                )));
            }
            Ok(Err(e)) => {
                return Ok(ToolOutput::err(format!(
                    "Error: failed to run command: {e}"
                )))
            }
            Ok(Ok(o)) => o,
        };
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            if !stderr.trim().is_empty() {
                text.push_str(&format!("\n[stderr]\n{stderr}"));
            }
            let code = output
                .status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".into());
            let shown =
                crate::helpers::truncate_tail(text.trim(), crate::helpers::MAX_OUTPUT_CHARS);
            let shown = if shown.trim().is_empty() {
                "(no output)".to_string()
            } else {
                shown
            };
            return Ok(ToolOutput::err(format!("{shown}\nExit code: {code}")));
        }
        let shown = crate::helpers::truncate_tail(text.trim(), crate::helpers::MAX_OUTPUT_CHARS);
        Ok(ToolOutput::ok(if shown.trim().is_empty() {
            "(no output)".to_string()
        } else {
            shown
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runs_echo() {
        let t = CliTool::new(std::env::temp_dir(), 10, None);
        let out = t
            .execute("c1", serde_json::json!({"command": "echo hi"}))
            .await
            .unwrap();
        assert!(!out.is_error && out.content.contains("hi"));
    }

    #[tokio::test]
    async fn nonzero_exit_is_error_result() {
        let t = CliTool::new(std::env::temp_dir(), 10, None);
        let out = t
            .execute("c1", serde_json::json!({"command": "exit 3"}))
            .await
            .unwrap();
        assert!(out.is_error && out.content.contains("Exit code: 3"));
    }

    #[tokio::test]
    async fn timeout_kills_sleep() {
        let t = CliTool::new(std::env::temp_dir(), 1, None);
        let out = t
            .execute("c1", serde_json::json!({"command": "sleep 30"}))
            .await
            .unwrap();
        assert!(out.is_error && out.content.contains("timed out"));
    }
}
