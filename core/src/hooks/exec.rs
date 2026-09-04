//! ============================================================================
//! Module: engine::hooks::exec
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Hook execution. Ports `executeHook` + `runHooks` from `src/hooks.ts`:
//!   spawn `sh <script>` with the JSON payload on stdin, 10s timeout,
//!   stdout parsed as `{action, reason?, args?}` (empty/unparseable → allow),
//!   path-traversal guard, EPIPE swallowed, non-zero exit → Err (fail-OPEN
//!   upstream), sequential run with first block/modify winning.
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `HookVerdict`   — Allow | Block(reason) | Modify(new_args).
//!   * `HookInput`     — {event, session_id, tool?, args?, extra?} payload.
//!   * `execute_hook()`— run ONE script (fail-open → Allow on error).
//!   * `run_hooks()`   — run a list in order (Chain of Responsibility).
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::hooks::{run_hooks, HookInput};
//! use std::path::PathBuf;
//! # async fn demo() {
//! let input = HookInput::new("pre_tool_use", "sess-1");
//! let v = run_hooks(&[], &PathBuf::from("./hooks"), &input).await; // no hooks → Allow
//! # }
//! ```
//! ============================================================================

use std::path::PathBuf;
use std::time::Duration;

use crate::hooks::config::HookDefinition;

/// Timeout for hook scripts (TS hard-codes 10s).
pub const HOOK_TIMEOUT_SECS: u64 = 10;

/// Verdict of one hook / a hook chain.
#[derive(Debug, Clone, PartialEq)]
pub enum HookVerdict {
    /// Proceed unchanged.
    Allow,
    /// Stop the action (`reason` is shown to the user / model).
    Block(String),
    /// Proceed with replacement tool args.
    Modify(serde_json::Value),
}

/// JSON payload delivered on a hook script's stdin.
#[derive(Debug, Clone)]
pub struct HookInput {
    /// Lifecycle event name (e.g. `pre_tool_use`).
    pub event: String,
    /// Session id for correlation.
    pub session_id: String,
    /// Tool name (tool events only).
    pub tool: Option<String>,
    /// Tool args (tool events only).
    pub args: Option<serde_json::Value>,
    /// Extra free-form fields.
    pub extra: serde_json::Value,
}

impl HookInput {
    /// Build a bare event payload (no tool context).
    ///
    /// # Example
    /// ```rust
    /// use engine::hooks::HookInput;
    /// let i = HookInput::new("on_session_start", "s1");
    /// assert_eq!(i.event, "on_session_start");
    /// ```
    pub fn new(event: &str, session_id: &str) -> Self {
        Self {
            event: event.into(),
            session_id: session_id.into(),
            tool: None,
            args: None,
            extra: serde_json::Value::Null,
        }
    }

    /// Attach tool context (for `pre_tool_use` / `file_changed`).
    ///
    /// # Example
    /// ```rust
    /// use engine::hooks::HookInput;
    /// let i = HookInput::new("pre_tool_use", "s").with_tool("cli", serde_json::json!({}));
    /// assert_eq!(i.tool.unwrap(), "cli");
    /// ```
    pub fn with_tool(mut self, tool: &str, args: serde_json::Value) -> Self {
        self.tool = Some(tool.into());
        self.args = Some(args);
        self
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "event": self.event,
            "session_id": self.session_id,
            "tool": self.tool,
            "args": self.args,
            "extra": self.extra,
        })
    }
}

/// Run ONE hook script; never fails the session (fail-OPEN).
///
/// # Description
/// Resolves `script` under `base_dir` (agent `hooks/` dir when the
/// definition's own `base_dir` is empty), rejects `../` escapes + absolute
/// escapes, spawns `sh`, writes payload JSON to stdin (EPIPE swallowed),
/// waits ≤10s, parses stdout JSON `{action, reason, args}`. ANY error
/// (spawn, timeout, non-zero exit, bad JSON) → `Allow` + stderr note — the
/// TS rule "hook errors never block".
///
/// # Example
/// ```rust,no_run
/// use engine::hooks::{execute_hook, HookDefinition, HookInput};
/// use std::path::PathBuf;
/// # async fn demo() {
/// let def = HookDefinition { script: "allow.sh".into(), description: String::new(), base_dir: String::new() };
/// let v = execute_hook(&def, &PathBuf::from("./hooks"), &HookInput::new("pre_tool_use", "s")).await;
/// # }
/// ```
pub async fn execute_hook(
    def: &HookDefinition,
    default_base: &std::path::Path,
    input: &HookInput,
) -> HookVerdict {
    let base = if def.base_dir.is_empty() {
        default_base.to_path_buf()
    } else {
        PathBuf::from(&def.base_dir)
    };
    // Resolve + normalise lexically, then require containment (traversal guard).
    let resolved = base.join(&def.script);
    let normalised = normalise(&resolved);
    let base_norm = normalise(&base);
    if normalised != base_norm && !normalised.starts_with(&format!("{base_norm}/")) {
        eprintln!("hook warning: script escapes base dir: {}", def.script);
        return HookVerdict::Allow;
    }
    if !resolved.is_file() {
        eprintln!("hook warning: script not found: {}", resolved.display());
        return HookVerdict::Allow;
    }
    // The child runs with cwd=base, so the script MUST be absolute: a
    // relative argv would be re-resolved inside the new cwd (→ exit 127).
    let abs = std::fs::canonicalize(&resolved).unwrap_or_else(|_| {
        std::env::current_dir()
            .unwrap_or(PathBuf::from("."))
            .join(&resolved)
    });
    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg(&abs).current_dir(&base);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    cmd.kill_on_drop(true);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("hook warning: cannot spawn sh: {e}");
            return HookVerdict::Allow;
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        let payload = serde_json::to_string(&input.to_json()).unwrap_or_else(|_| "{}".into());
        // EPIPE (hook ignores stdin) is swallowed — TS rule.
        let _ = stdin.write_all(payload.as_bytes()).await;
    }
    let out = tokio::time::timeout(
        Duration::from_secs(HOOK_TIMEOUT_SECS),
        child.wait_with_output(),
    )
    .await;
    let output = match out {
        Err(_) => {
            eprintln!(
                "hook warning: {} timed out after {HOOK_TIMEOUT_SECS}s",
                def.script
            );
            return HookVerdict::Allow;
        }
        Ok(Err(e)) => {
            eprintln!("hook warning: {}: {e}", def.script);
            return HookVerdict::Allow;
        }
        Ok(Ok(o)) => o,
    };
    if !output.status.success() {
        eprintln!("hook warning: {} exited non-zero", def.script);
        return HookVerdict::Allow;
    }
    parse_verdict(&String::from_utf8_lossy(&output.stdout))
}

/// Parse hook stdout into a verdict (empty/garbage → Allow).
///
/// # Example
/// ```rust
/// use engine::hooks::exec::{parse_verdict, HookVerdict};
/// assert_eq!(parse_verdict(r#"{"action":"block","reason":"no"}"#), HookVerdict::Block("no".into()));
/// assert_eq!(parse_verdict("not json"), HookVerdict::Allow);
/// ```
pub fn parse_verdict(stdout: &str) -> HookVerdict {
    let t = stdout.trim();
    if t.is_empty() {
        return HookVerdict::Allow;
    }
    let v: serde_json::Value = match serde_json::from_str(t) {
        Ok(v) => v,
        Err(_) => return HookVerdict::Allow,
    };
    match v.get("action").and_then(|a| a.as_str()).unwrap_or("allow") {
        "block" => HookVerdict::Block(
            v.get("reason")
                .and_then(|r| r.as_str())
                .unwrap_or("blocked by hook")
                .to_string(),
        ),
        "modify" => HookVerdict::Modify(v.get("args").cloned().unwrap_or(serde_json::json!({}))),
        _ => HookVerdict::Allow,
    }
}

/// Run hook definitions in order; first Block/Modify wins (else Allow).
///
/// # Description
/// Chain of Responsibility: sequential, short-circuit on first decisive
/// verdict. Individual hook errors already collapsed to Allow inside
/// [`execute_hook`], so this never fails.
///
/// # Example
/// ```rust,no_run
/// use engine::hooks::{run_hooks, HookInput};
/// use std::path::PathBuf;
/// # async fn demo() {
/// let v = run_hooks(&[], &PathBuf::from("./hooks"), &HookInput::new("pre_query", "s")).await;
/// assert_eq!(v, engine::hooks::HookVerdict::Allow);
/// # }
/// ```
pub async fn run_hooks(
    defs: &[HookDefinition],
    default_base: &std::path::Path,
    input: &HookInput,
) -> HookVerdict {
    for def in defs {
        match execute_hook(def, default_base, input).await {
            HookVerdict::Allow => {}
            decisive => return decisive,
        }
    }
    HookVerdict::Allow
}

fn normalise(p: &std::path::Path) -> String {
    let mut parts: Vec<String> = vec![];
    for c in p.components() {
        use std::path::Component::*;
        match c {
            CurDir => {}
            ParentDir => {
                parts.pop();
            }
            Normal(s) => parts.push(s.to_string_lossy().into_owned()),
            RootDir => parts.push(String::new()),
            Prefix(p) => parts.push(p.as_os_str().to_string_lossy().into_owned()),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_parsing() {
        assert_eq!(parse_verdict(""), HookVerdict::Allow);
        assert_eq!(parse_verdict("garbage"), HookVerdict::Allow);
        assert_eq!(
            parse_verdict(r#"{"action":"modify","args":{"a":1}}"#),
            HookVerdict::Modify(serde_json::json!({"a": 1}))
        );
    }

    #[tokio::test]
    async fn missing_script_allows() {
        let def = HookDefinition {
            script: "nope.sh".into(),
            description: String::new(),
            base_dir: String::new(),
        };
        let v = execute_hook(
            &def,
            &std::env::temp_dir(),
            &HookInput::new("pre_tool_use", "s"),
        )
        .await;
        assert_eq!(v, HookVerdict::Allow);
    }

    #[tokio::test]
    async fn relative_base_dir_still_finds_script() {
        // Regression: the child runs with cwd=base, so the script argv must
        // be absolutised — a relative argv used to die with exit 127.
        let base = std::path::PathBuf::from("target/hook-rel-test");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(
            base.join("deny.sh"),
            "#!/bin/sh\ncat > /dev/null\necho '{\"action\":\"block\",\"reason\":\"no\"}'\n",
        )
        .unwrap();
        let def = HookDefinition {
            script: "deny.sh".into(),
            description: String::new(),
            base_dir: String::new(),
        };
        let v = execute_hook(&def, &base, &HookInput::new("pre_tool_use", "s")).await;
        assert_eq!(v, HookVerdict::Block("no".into()));
        std::fs::remove_dir_all(&base).ok();
    }
}
