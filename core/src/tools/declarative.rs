//! ============================================================================
//! Module: engine::tools::declarative
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Declarative script tools: each YAML file defines a name, description,
//!   JSON input schema, and a script plus runtime interpreter. At runtime
//!   the tool spawns `runtime <abs-script-path>` with the call args as JSON
//!   on stdin, waits up to 120s, then decodes stdout as (1) an image-marker
//!   line, (2) a JSON `text`/`result` field, else (3) raw trimmed text.
//!   Non-zero exit becomes an error result; empty output becomes
//!   `"(no output)"`.
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `DeclarativeTool`         — one script-backed tool (Sequential).
//!   * `load_declarative_tools()`— scan `tools/*.yaml` (fail-soft each).
//!
//! HOW IT WORKS (safety + data flow):
//!   * Path guard: the script must resolve under `<agent_dir>/tools/`;
//!     traversal attempts are rejected before spawning.
//!   * Absolute script path is used because the child runs with
//!     cwd=`agent_dir`, where a relative argv would no longer resolve.
//!   * Sequential mode is deliberate: scripts can touch anything, and
//!     serial execution is always safe.
//!
//! HOW TO USE (example):
//! ```yaml
//! # tools/shout.yaml
//! name: shout
//! description: Uppercase stdin text
//! input_schema: {type: object, properties: {text: {type: string}}, required: [text]}
//! implementation: {script: shout.sh, runtime: sh}
//! ```
//! ============================================================================

use crate::agent::{AgentTool, ExecutionMode, ToolOutput};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Timeout for declarative script tools (fixed 120s budget).
pub const DECLARATIVE_TIMEOUT_SECS: u64 = 120;

/// One script-backed tool loaded from `tools/*.yaml`.
pub struct DeclarativeTool {
    /// Agent dir (scripts resolve under `<dir>/tools/`).
    pub agent_dir: PathBuf,
    /// Tool name from the yaml.
    pub tool_name: String,
    /// Description shown to the model.
    pub tool_desc: String,
    /// JSON-schema of args (passed through to the model verbatim).
    pub schema: serde_json::Value,
    /// Interpreter, e.g. `sh` (default), `python3`, `node`.
    pub runtime: String,
    /// Script path relative to `<dir>/tools/`.
    pub script: String,
}

impl DeclarativeTool {
    /// Registry name.
    pub fn name(&self) -> &str {
        &self.tool_name
    }
}

#[async_trait]
impl AgentTool for DeclarativeTool {
    fn name(&self) -> &str {
        &self.tool_name
    }
    fn description(&self) -> &str {
        &self.tool_desc
    }
    fn parameters(&self) -> serde_json::Value {
        self.schema.clone()
    }
    fn execution_mode(&self) -> ExecutionMode {
        // Sequential is the fail-safe default: scripts can touch anything,
        // and serialisation is always safe.
        ExecutionMode::Sequential
    }

    async fn execute(&self, _id: &str, args: serde_json::Value) -> anyhow::Result<ToolOutput> {
        // Path-traversal guard: script must stay under the tools directory.
        let script_path = self.agent_dir.join("tools").join(&self.script);
        let tools_dir = self.agent_dir.join("tools");
        let norm_ok = script_path.starts_with(&tools_dir)
            || script_path.starts_with(tools_dir.canonicalize().unwrap_or(tools_dir.clone()));
        if !norm_ok || !script_path.is_file() {
            return Ok(ToolOutput::err(format!(
                "Error: script {} not found under tools/",
                self.script
            )));
        }
        // Absolute script path: the child runs with cwd=agent_dir, and a
        // relative argv would be re-resolved inside the new cwd (→ exit 127).
        let abs_script = std::fs::canonicalize(&script_path).unwrap_or(script_path);
        let mut cmd = tokio::process::Command::new(&self.runtime);
        cmd.arg(&abs_script).current_dir(&self.agent_dir);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return Ok(ToolOutput::err(format!(
                    "Error: cannot spawn {}: {e}",
                    self.runtime
                )))
            }
        };
        // Args JSON goes on stdin per the script-tool contract.
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let payload = serde_json::to_string(&args).unwrap_or_else(|_| "{}".into());
            let _ = stdin.write_all(payload.as_bytes()).await;
        }
        let out = tokio::time::timeout(
            Duration::from_secs(DECLARATIVE_TIMEOUT_SECS),
            child.wait_with_output(),
        )
        .await;
        let output = match out {
            Err(_) => {
                return Ok(ToolOutput::err(format!(
                    "Tool timed out after {DECLARATIVE_TIMEOUT_SECS}s"
                )))
            }
            Ok(Err(e)) => return Ok(ToolOutput::err(format!("Error: {e}"))),
            Ok(Ok(o)) => o,
        };
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Ok(ToolOutput::err(format!("Error: {}", stderr.trim())));
        }
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        Ok(ToolOutput::ok(decode_stdout(&stdout)))
    }
}

/// Decode script stdout into model-visible text.
///
/// # Description
/// (1) Any line starting `data:image/` with `;base64,` is summarised as an
/// image marker (first 120 chars kept, payload omitted); (2) valid JSON
/// with a `text`/`result` string field yields that field; (3) otherwise raw
/// trimmed text; empty input yields `"(no output)"`.
///
/// # Example
/// ```rust
/// use engine::tools::declarative::decode_stdout;
/// assert_eq!(decode_stdout(r#"{"text": "hi"}"#), "hi");
/// assert_eq!(decode_stdout("   "), "(no output)");
/// ```
pub fn decode_stdout(stdout: &str) -> String {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return "(no output)".to_string();
    }
    for line in trimmed.lines() {
        if line.starts_with("data:image/") && line.contains(";base64,") {
            return format!(
                "[image output, data omitted: {}...]",
                &line[..line.len().min(120)]
            );
        }
    }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        for key in ["text", "result"] {
            if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
                return s.to_string();
            }
        }
    }
    trimmed.to_string()
}

/// Scan `tools/*.yaml` and build script tools (fail-soft per file).
///
/// # Description
/// Invalid files are skipped with a stderr warning — one broken tool file
/// must not end the session.
///
/// # Example
/// ```rust,no_run
/// use engine::tools::load_declarative_tools;
/// use std::path::Path;
/// let tools = load_declarative_tools(Path::new("./my-agent"));
/// ```
pub fn load_declarative_tools(agent_dir: &Path) -> Vec<DeclarativeTool> {
    let mut out = vec![];
    let dir = agent_dir.join("tools");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    let mut files: Vec<std::path::PathBuf> = entries
        .filter_map(|e| e.ok().map(|x| x.path()))
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("yaml") | Some("yml")
            )
        })
        .collect();
    files.sort();
    for f in files {
        let text = match std::fs::read_to_string(&f) {
            Ok(t) => t,
            Err(_) => continue,
        };
        #[derive(serde::Deserialize)]
        struct Def {
            #[serde(default)]
            name: String,
            #[serde(default)]
            description: String,
            #[serde(default)]
            input_schema: serde_json::Value,
            #[serde(default)]
            implementation: Impl,
        }
        #[derive(serde::Deserialize, Default)]
        struct Impl {
            #[serde(default)]
            script: String,
            #[serde(default)]
            runtime: String,
        }
        let def: Def = match serde_yaml::from_str(&text) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("warning: skipping tool file {}: {e}", f.display());
                continue;
            }
        };
        if def.name.is_empty() || def.implementation.script.is_empty() {
            eprintln!(
                "warning: skipping tool file {}: missing name/implementation.script",
                f.display()
            );
            continue;
        }
        out.push(DeclarativeTool {
            agent_dir: agent_dir.to_path_buf(),
            tool_name: def.name,
            tool_desc: def.description,
            schema: if def.input_schema.is_null() {
                serde_json::json!({"type": "object"})
            } else {
                def.input_schema
            },
            runtime: if def.implementation.runtime.is_empty() {
                "sh".into()
            } else {
                def.implementation.runtime
            },
            script: def.implementation.script,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_json_text_field() {
        assert_eq!(decode_stdout(r#"{"text": "hi"}"#), "hi");
        assert_eq!(decode_stdout("   "), "(no output)");
        assert_eq!(decode_stdout("plain"), "plain");
    }

    #[tokio::test]
    async fn relative_agent_dir_still_runs_script() {
        // Regression: the child runs with cwd=agent_dir, so the script argv
        // must be absolutised — a relative argv used to die with exit 127.
        // (`AgentTool` is already in scope via `super::*`.)
        let dir = std::path::PathBuf::from("target/decl-rel-test");
        std::fs::create_dir_all(dir.join("tools")).unwrap();
        std::fs::write(dir.join("tools/echo.sh"), "#!/bin/sh\ncat\n").unwrap();
        let t = DeclarativeTool {
            agent_dir: dir.clone(),
            tool_name: "echo".into(),
            tool_desc: String::new(),
            schema: serde_json::json!({"type": "object"}),
            runtime: "sh".into(),
            script: "echo.sh".into(),
        };
        let out = t.execute("c1", serde_json::json!({"a": 1})).await.unwrap();
        assert!(!out.is_error, "unexpected error: {}", out.content);
        assert!(out.content.contains("\"a\""));
        std::fs::remove_dir_all(&dir).ok();
    }
}
