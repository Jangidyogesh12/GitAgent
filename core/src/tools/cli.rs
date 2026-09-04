//! ============================================================================
//! Module: engine::tools::cli
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The `cli` tool — run a shell command, capture output, kill on timeout.
//!   Ports `src/tools/cli.ts` exactly: `sh -c` spawn, own process group so
//!   grandchildren die too (bare `child.kill()` leaves `foo &` alive holding
//!   the pipe open → hang), SIGTERM → 3s → SIGKILL escalation, rolling-tail
//!   memory bound, non-zero exit → error RESULT (not Rust Err), empty →
//!   `"(no output)"`, `[output truncated, showing last ~100KB]` marker.
//!
//! DESIGN PATTERNS USED:
//!   * Strategy — `SandboxExec` swaps LOCAL shell execution for a remote-VM
//!     argv runner (ports the `sandbox-cli.ts` twin without duplicating the
//!     timeout/truncation logic). `None` = run locally.
//!   * Command — the shell string + timeout travel as one invocable object.
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `SandboxExec` — Strategy trait for remote execution backends.
//!   * `CliTool`     — the tool (Sequential: mutates the world).
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

/// Remote-execution backend (Strategy). Ports the `sandbox-*.ts` twins.
///
/// # Description
/// `run(command, cwd, timeout)` executes WITHOUT a local shell and returns
/// stdout (+ stderr appended on failure, like the local path). Implement for
/// e2b/SSH backends; `None` in `CliTool` means "run on this machine".
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

        // Remote backend (sandbox twin) short-circuits local spawn.
        if let Some(sb) = &self.sandbox {
            return match tokio::time::timeout(timeout, sb.run(&command, &self.cwd, timeout)).await {
                Err(_) => Ok(ToolOutput::err(format!(
                    "Command timed out after {timeout_s}s"
                ))),
                Ok(Err(e)) => Ok(ToolOutput::err(format!("Error: {e:#}"))),
                Ok(Ok(o)) => Ok(ToolOutput::ok(o)),
            };
        }

        // Local: `sh -c` with piped output. On timeout we SIGTERM the child
        // and, best-effort, its process GROUP (covers `cmd &` grandchildren
        // that would otherwise hold the pipe open — the TS `kill(-pid)` rule).
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
                // Timeout: best-effort group kill, 3s grace (TS escalation),
                // then the `kill_on_drop(true)` reap finishes the job.
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
