//! ============================================================================
//! Module: engine::mcp::manager
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The MCP client manager (Facade + Adapter + RAII). Connects all
//!   configured servers in parallel and fail-soft, speaks stdio JSON-RPC
//!   (initialize → notifications/initialized → tools/list with cursor
//!   pagination), registers each remote tool under a sanitised
//!   `<server>__<tool>` name, invokes tools via `tools/call` with
//!   `flatten_result()`, and shuts children down via idempotent `cleanup()`.
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `McpTool`         — Adapter: remote tool as `AgentTool` (Sequential).
//!   * `McpManager`      — `setup()` / `cleanup()` + `tools` registry.
//!   * `flatten_result()`— MCP content blocks → model-readable string.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::mcp::McpManager;
//! # async fn demo() {
//! let mut mgr = McpManager::setup(&serde_json::json!({"docs": {"command": "docs-mcp"}})).await;
//! mgr.cleanup().await;
//! # }
//! ```
//! ============================================================================

use crate::agent::{AgentTool, ExecutionMode, ToolOutput};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::mcp::types::{sanitise_tool_name, McpServerConfig, DEFAULT_TIMEOUT_MS};

/// Flatten MCP result content blocks into model-readable text.
///
/// # Description
/// Text blocks are joined with `\n`; image/audio become
/// `[image: <mime>, data omitted]`; resource blocks yield their embedded
/// text or `[resource: <uri> (<mime>)]`; resource_link becomes
/// `[resource_link: <uri>]`; empty content with structuredContent falls
/// back to its JSON; `isError` prefixes the output with `"Error: "`.
///
/// # Example
/// ```rust
/// use engine::mcp::flatten_result;
/// let s = flatten_result(&serde_json::json!({"content": [{"type": "text", "text": "hi"}]}), false);
/// assert_eq!(s, "hi");
/// ```
pub fn flatten_result(result: &serde_json::Value, is_error: bool) -> String {
    let mut parts: Vec<String> = vec![];
    if let Some(items) = result.get("content").and_then(|c| c.as_array()) {
        for item in items {
            let t = item.get("type").and_then(|x| x.as_str()).unwrap_or("");
            match t {
                "text" => {
                    if let Some(s) = item.get("text").and_then(|x| x.as_str()) {
                        parts.push(s.to_string());
                    }
                }
                "image" | "audio" => {
                    parts.push(format!(
                        "[image: {}, data omitted]",
                        item.get("mimeType").and_then(|x| x.as_str()).unwrap_or("?")
                    ));
                }
                "resource" => {
                    if let Some(res) = item.get("resource") {
                        if let Some(s) = res.get("text").and_then(|x| x.as_str()) {
                            parts.push(s.to_string());
                        } else {
                            parts.push(format!(
                                "[resource: {} ({})]",
                                res.get("uri").and_then(|x| x.as_str()).unwrap_or("?"),
                                res.get("mimeType").and_then(|x| x.as_str()).unwrap_or("?")
                            ));
                        }
                    }
                }
                "resource_link" => {
                    parts.push(format!(
                        "[resource_link: {}]",
                        item.get("uri").and_then(|x| x.as_str()).unwrap_or("?")
                    ));
                }
                _ => {
                    parts.push(format!("[unsupported content: {t}]"));
                }
            }
        }
    }
    if parts.is_empty() {
        if let Some(sc) = result.get("structuredContent") {
            parts.push(serde_json::to_string(sc).unwrap_or_default());
        }
    }
    let text = parts.join("\n");
    if is_error {
        format!("Error: {text}")
    } else {
        text
    }
}

struct LiveServer {
    child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    stdout: tokio::io::BufReader<tokio::process::ChildStdout>,
    next_id: u64,
    timeout: Duration,
}

/// One remote MCP tool adapted to `AgentTool` (Adapter pattern).
pub struct McpTool {
    /// Registered (sanitised) name.
    pub registered_name: String,
    /// Original server-side name (used in `tools/call`).
    pub original_name: String,
    /// Description from tools/list.
    pub desc: String,
    /// Input schema from tools/list.
    pub schema: serde_json::Value,
    /// Owning server handle (private: one in-flight request per server).
    server: Arc<Mutex<LiveServer>>,
}

#[async_trait]
impl AgentTool for McpTool {
    fn name(&self) -> &str {
        &self.registered_name
    }
    fn description(&self) -> &str {
        &self.desc
    }
    fn parameters(&self) -> serde_json::Value {
        self.schema.clone()
    }
    fn execution_mode(&self) -> ExecutionMode {
        // Fail-closed: remote tools run sequentially (not concurrency-safe).
        ExecutionMode::Sequential
    }
    async fn execute(&self, _id: &str, args: serde_json::Value) -> anyhow::Result<ToolOutput> {
        // NOTE: the tokio Mutex guard is held across IO awaits. This is the
        // documented stdio-server pattern: one in-flight request per server
        // keeps JSON-RPC line-framing unambiguous (no interleaved replies).
        let payload = {
            let mut srv = self.server.lock().await;
            let id = srv.next_id;
            srv.next_id += 1;
            let req = serde_json::json!({
                "jsonrpc": "2.0", "id": id, "method": "tools/call",
                "params": {"name": self.original_name, "arguments": args}
            });
            let line = serde_json::to_string(&req).unwrap_or_default();
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
            if srv
                .stdin
                .write_all(format!("{line}\n").as_bytes())
                .await
                .is_err()
            {
                return Ok(ToolOutput::err("Error: MCP server stdin closed"));
            }
            let timeout = srv.timeout;
            let mut resp_line = String::new();
            match tokio::time::timeout(timeout, srv.stdout.read_line(&mut resp_line)).await {
                Err(_) => return Ok(ToolOutput::err("Error: MCP tools/call timed out")),
                Ok(Err(e)) => return Ok(ToolOutput::err(format!("Error: MCP read failed: {e}"))),
                Ok(Ok(0)) => return Ok(ToolOutput::err("Error: MCP server closed connection")),
                Ok(Ok(_)) => resp_line,
            }
        };
        let v: serde_json::Value = serde_json::from_str(&payload).unwrap_or(serde_json::json!({}));
        if let Some(err) = v.get("error") {
            return Ok(ToolOutput::err(format!("Error: {err}")));
        }
        let result = v.get("result").cloned().unwrap_or(serde_json::json!({}));
        let is_error = result
            .get("isError")
            .and_then(|b| b.as_bool())
            .unwrap_or(false);
        Ok(if is_error {
            ToolOutput::err(flatten_result(&result, true))
        } else {
            ToolOutput::ok(flatten_result(&result, false))
        })
    }
}

/// Manager facade: connect + register + cleanup (RAII-ish, idempotent).
pub struct McpManager {
    /// Adapted tools ready for the registry (collision-checked).
    pub tools: Vec<Arc<dyn AgentTool>>,
    servers: Vec<Arc<Mutex<LiveServer>>>,
    closed: bool,
}

impl McpManager {
    /// Connect to every server in `config` in parallel, fail-soft.
    ///
    /// # Description
    /// `config` is the raw `mcp_servers` table (`{name: McpServerConfig}`).
    /// Each server connects independently; failures warn + skip so a bad
    /// server never kills the session. Tool names are sanitised + capped
    /// + collision-checked against each other.
    ///
    /// # Example
    /// ```rust,no_run
    /// use engine::mcp::McpManager;
    /// # async fn demo() {
    /// let mgr = McpManager::setup(&serde_json::Value::Null).await;
    /// assert!(mgr.tools.is_empty());
    /// # }
    /// ```
    pub async fn setup(config: &serde_json::Value) -> Self {
        let mut servers_cfg: Vec<(String, McpServerConfig)> = vec![];
        if let Some(obj) = config.as_object() {
            for (name, raw) in obj {
                // `${VAR}` interpolation across the raw JSON first.
                let lookup = |k: &str| std::env::var(k).ok();
                let interp = crate::helpers::interpolate_value(raw, &lookup);
                match serde_json::from_value::<McpServerConfig>(interp) {
                    Ok(c) => servers_cfg.push((name.clone(), c)),
                    Err(e) => eprintln!("warning: MCP server {name}: bad config: {e}"),
                }
            }
        }
        let futs = servers_cfg.into_iter().map(|(name, cfg)| async move {
            match connect_one(&name, cfg).await {
                Ok((srv, tools)) => Some((srv, tools)),
                Err(e) => {
                    eprintln!("warning: MCP server {name}: {e:#}");
                    None
                }
            }
        });
        let mut tools: Vec<Arc<dyn AgentTool>> = vec![];
        let mut servers = vec![];
        for opt in futures::future::join_all(futs).await.into_iter().flatten() {
            let (srv, mut ts) = opt;
            let server = Arc::new(Mutex::new(srv));
            for t in ts.drain(..) {
                // Collision check: skip clashing names.
                if tools
                    .iter()
                    .any(|e: &Arc<dyn AgentTool>| e.name() == t.registered_name)
                {
                    eprintln!("warning: MCP tool name clash: {}", t.registered_name);
                    continue;
                }
                let with_server = McpTool {
                    registered_name: t.registered_name.clone(),
                    original_name: t.original_name,
                    desc: t.desc,
                    schema: t.schema,
                    server: server.clone(),
                };
                tools.push(Arc::new(with_server));
            }
            servers.push(server);
        }
        Self {
            tools,
            servers,
            closed: false,
        }
    }

    /// Close all server connections (idempotent; `Promise.allSettled` style).
    ///
    /// # Example
    /// ```rust,no_run
    /// use engine::mcp::McpManager;
    /// # async fn demo() {
    /// let mut mgr = McpManager::setup(&serde_json::Value::Null).await;
    /// mgr.cleanup().await;
    /// mgr.cleanup().await; // safe twice
    /// # }
    /// ```
    pub async fn cleanup(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        for s in &self.servers {
            let mut srv = s.lock().await;
            srv.child.kill().await.ok();
        }
    }
}

struct PendingTool {
    registered_name: String,
    original_name: String,
    desc: String,
    schema: serde_json::Value,
}

async fn connect_one(
    name: &str,
    cfg: McpServerConfig,
) -> anyhow::Result<(LiveServer, Vec<PendingTool>)> {
    let (command, args, env, cwd, timeout_ms) = match cfg {
        McpServerConfig::Stdio {
            command,
            args,
            env,
            cwd,
            timeout_ms,
        } => (command, args, env, cwd, timeout_ms),
        McpServerConfig::Http { kind, url, .. } => {
            anyhow::bail!(
                "transport {} ({url}) not implemented here — configure a stdio server instead",
                kind.unwrap_or_else(|| "http".into())
            );
        }
    };
    let timeout = Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));
    let mut cmd = tokio::process::Command::new(&command);
    cmd.args(&args);
    for (k, v) in &env {
        cmd.env(k, v);
    }
    if let Some(c) = cwd {
        cmd.current_dir(c);
    }
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    cmd.kill_on_drop(true);
    let mut child = cmd.spawn()?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("no stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("no stdout"))?;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let reader = tokio::io::BufReader::new(stdout);
    let mut srv = LiveServer {
        child,
        stdin,
        stdout: reader,
        next_id: 1,
        timeout,
    };

    // Handshake: initialize → notifications/initialized.
    let init = serde_json::json!({
        "jsonrpc": "2.0", "id": 0, "method": "initialize",
        "params": {"protocolVersion": "2024-11-05",
                   "capabilities": {},
                   "clientInfo": {"name": "gitagent", "version": env!("CARGO_PKG_VERSION")}}
    });
    srv.stdin
        .write_all(format!("{}\n", serde_json::to_string(&init)?).as_bytes())
        .await?;
    let mut line = String::new();
    tokio::time::timeout(timeout, srv.stdout.read_line(&mut line)).await??;
    let noted = serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    srv.stdin
        .write_all(format!("{}\n", serde_json::to_string(&noted)?).as_bytes())
        .await?;

    // tools/list following pagination cursors until no nextCursor.
    let mut tools = vec![];
    let mut cursor: Option<String> = None;
    loop {
        let mut params = serde_json::json!({});
        if let Some(c) = cursor {
            params["cursor"] = serde_json::Value::String(c);
        }
        let id = srv.next_id;
        srv.next_id += 1;
        let req = serde_json::json!({"jsonrpc": "2.0", "id": id, "method": "tools/list", "params": params});
        srv.stdin
            .write_all(format!("{}\n", serde_json::to_string(&req)?).as_bytes())
            .await?;
        let mut resp_line = String::new();
        tokio::time::timeout(timeout, srv.stdout.read_line(&mut resp_line)).await??;
        let v: serde_json::Value = serde_json::from_str(&resp_line)?;
        if let Some(err) = v.get("error") {
            anyhow::bail!("tools/list: {err}");
        }
        let result = v.get("result").cloned().unwrap_or_default();
        if let Some(arr) = result.get("tools").and_then(|t| t.as_array()) {
            for t in arr {
                let original = t
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                if original.is_empty() {
                    continue;
                }
                tools.push(PendingTool {
                    registered_name: sanitise_tool_name(name, &original),
                    original_name: original,
                    desc: t
                        .get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string(),
                    schema: t
                        .get("inputSchema")
                        .cloned()
                        .unwrap_or(serde_json::json!({"type": "object"})),
                });
            }
        }
        cursor = result
            .get("nextCursor")
            .and_then(|c| c.as_str())
            .map(|s| s.to_string());
        if cursor.is_none() {
            break;
        }
    }
    Ok((srv, tools))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn null_config_gives_no_tools() {
        let mgr = McpManager::setup(&serde_json::Value::Null).await;
        assert!(mgr.tools.is_empty());
    }

    #[test]
    fn flatten_marks_errors() {
        let v = serde_json::json!({"content": [{"type": "text", "text": "bad"}]});
        assert_eq!(flatten_result(&v, true), "Error: bad");
    }
}
