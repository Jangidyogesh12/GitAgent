//! ============================================================================
//! Module: sdk::query
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The `query()` Facade — the whole pipeline in one call. Ports
//!   `src/sdk.ts`: load agent → system-prompt suffix → builtin (+learning)
//!   tools → declarative tools → plugin tools (collision-skipped) → MCP
//!   tools → SDK extra tools → allowed/disallowed filters → gates
//!   (permissions first, then script hooks) → model client (preferred +
//!   fallbacks) → engine loop → `SdkMessage` stream. Setup failures arrive
//!   as `SdkMessage::Error` (never a panic), mirroring the TS `.catch` that
//!   emits `system/error` + finish.
//!
//! FUNCTIONS / TYPES PRESENT IN THIS FILE:
//!   * `QueryError`      — setup-failure type (also sent as SdkMessage).
//!   * `query()`         — run one prompt → message receiver (Observer).
//!   * `build_registry()`— assemble the tool registry (filter order:
//!     allowlist THEN denylist, like TS).
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use sdk::{query, QueryOptions};
//! use std::path::PathBuf;
//! # async fn demo() {
//! let mut rx = query(QueryOptions::new(PathBuf::from("./my-agent"), "hi"));
//! while let Some(m) = rx.recv().await { println!("{m:?}"); }
//! # }
//! ```
//! ============================================================================

use std::sync::Arc;
use tokio::sync::mpsc;

use engine::agent::{Agent, AgentEvent, AgentTool, GenParams, StopReason};
use engine::hooks::HookGate;

use crate::permissions::{PermissionGate, PermissionMode};
use crate::types::{QueryOptions, SdkMessage};

/// Setup failure (also delivered to the stream as `SdkMessage::Error`).
#[derive(Debug)]
pub struct QueryError(pub String);

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "query setup failed: {}", self.0)
    }
}
impl std::error::Error for QueryError {}

/// Run one prompt through the full pipeline; stream `SdkMessage`s.
///
/// # Description
/// Spawns a background task (Observer): the receiver yields Delta /
/// Assistant / ToolUse / ToolResult / System messages, then closes. Dropping
/// the receiver mid-stream aborts nothing by itself — but the loop ends when
/// its event receiver drops (the engine task is bound to the channel).
/// `System("session_end ...")` always closes a successful run; `Error(...)`
/// closes a failed setup.
///
/// # Example
/// ```rust,no_run
/// use sdk::{query, QueryOptions};
/// use std::path::PathBuf;
/// # async fn demo() {
/// let mut rx = query(QueryOptions::new(PathBuf::from("."), "hello"));
/// while let Some(msg) = rx.recv().await { let _ = msg; }
/// # }
/// ```
pub fn query(opts: QueryOptions) -> mpsc::UnboundedReceiver<SdkMessage> {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        if let Err(e) = run_query(opts, tx.clone()).await {
            let _ = tx.send(SdkMessage::Error(e.0));
        }
    });
    rx
}

async fn run_query(
    opts: QueryOptions,
    tx: mpsc::UnboundedSender<SdkMessage>,
) -> Result<(), QueryError> {
    let fail = |m: String| QueryError(m);
    // 1. Load agent (manifest + system prompt + model specs).
    let mut loaded =
        engine::loader::load_agent(&opts.dir, opts.model.as_deref(), opts.session_id.as_deref())
            .map_err(|e| fail(format!("load agent: {e:#}")))?;
    if let Some(suffix) = &opts.system_prompt_suffix {
        loaded.system_prompt.push_str("\n\n");
        loaded.system_prompt.push_str(suffix);
    }
    let _ = tx.send(SdkMessage::System(format!(
        "session_start {}",
        loaded.session_id
    )));

    // 2. Registry: builtin → learning → declarative → plugin → MCP → extra.
    let cli_timeout = loaded.manifest.cli_timeout().unwrap_or(120);
    let mut tools: Vec<Arc<dyn AgentTool>> =
        engine::tools::builtin_tools(&opts.dir, cli_timeout, None)
            .into_iter()
            .collect();
    // Learning tools (need the skill registry for matching).
    let skill_pairs: Vec<(String, String)> = engine::loader::discover_skills(&opts.dir)
        .into_iter()
        .map(|s| (s.name, s.description))
        .collect();
    tools.push(Arc::new(engine::learning::TaskTracker::new(
        opts.dir.clone(),
        skill_pairs,
    )));
    tools.push(Arc::new(engine::learning::SkillLearner::new(
        opts.dir.clone(),
    )));
    for t in engine::tools::load_declarative_tools(&opts.dir) {
        tools.push(Arc::new(t));
    }
    // Plugins: declarative tools of enabled plugins (collision-skipped).
    let plugins = engine::plugins::discover_plugins(&opts.dir, &loaded.manifest.plugins);
    for p in &plugins {
        if !p.manifest.provides.tools {
            continue;
        }
        for t in engine::tools::load_declarative_tools(&p.dir) {
            if tools.iter().any(|e| e.name() == t.name()) {
                eprintln!("warning: plugin {} tool clash: {}", p.name, t.name());
                continue;
            }
            // NOTE: plugin tool scripts resolve under the PLUGIN dir (the
            // DeclarativeTool already carries its own agent_dir = plugin dir
            // because load_declarative_tools(&p.dir) rooted it there).
            tools.push(Arc::new(t));
        }
    }
    // MCP tools (fail-soft inside setup).
    let mut mcp = engine::mcp::McpManager::setup(&loaded.manifest.mcp_servers).await;
    for t in mcp.tools.drain(..) {
        if tools.iter().any(|e| e.name() == t.name()) {
            eprintln!("warning: MCP tool clash: {}", t.name());
            continue;
        }
        tools.push(t);
    }
    tools.extend(opts.extra_tools.iter().cloned());
    tools = build_registry(tools, opts.allowed_tools.as_deref(), &opts.disallowed_tools);

    // 3. Gates: permissions first, then script hooks (agent + plugin merged).
    let mut gates: Vec<Arc<dyn engine::agent::ToolGate>> = vec![];
    gates.push(Arc::new(PermissionGate::new(
        opts.permission_mode.unwrap_or(PermissionMode::Default),
        &opts.permission_rules,
    )));
    let agent_hooks = engine::hooks::load_hooks_config(&opts.dir);
    let merged = engine::hooks::merge_hooks(
        &agent_hooks,
        &engine::plugins::plugin_hook_configs(&plugins),
    );
    if !merged.pre_tool_use.is_empty() {
        gates.push(Arc::new(HookGate::new(
            opts.dir.clone(),
            loaded.session_id.clone(),
            merged.pre_tool_use,
        )));
    }

    // 4. Model client (preferred + fallbacks) + engine agent.
    let specs: Vec<String> = loaded.model_specs.clone();
    let client = Arc::new(engine::llm::OpenAiCompat::new(
        &specs.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
    ));
    let max_turns = opts
        .max_turns
        .unwrap_or_else(|| loaded.manifest.max_turns());
    let params = GenParams {
        temperature: loaded
            .manifest
            .model
            .constraints
            .as_ref()
            .and_then(|c| c.temperature),
        max_tokens: loaded
            .manifest
            .model
            .constraints
            .as_ref()
            .and_then(|c| c.max_tokens),
        top_p: loaded
            .manifest
            .model
            .constraints
            .as_ref()
            .and_then(|c| c.top_p),
    };
    let agent = Agent::new(loaded.system_prompt.clone(), tools, &specs.join(","))
        .with_max_turns(max_turns)
        .with_params(params)
        .with_gates(gates);

    // 5. Prompt → map engine events → SdkMessages (the TS event-mapping table).
    let mut ev_rx = agent.prompt(opts.prompt.clone(), client).await;
    while let Some(ev) = ev_rx.recv().await {
        let msg = match ev {
            AgentEvent::MessageDelta { text, .. } => SdkMessage::Delta(text),
            // Error turns carry no text blocks — surface them as errors
            // instead of an invisible empty message (a silent session_end
            // with no output means "the provider call failed, look here").
            AgentEvent::MessageEnd(m) if m.stop_reason == StopReason::Error => SdkMessage::Error(
                m.error_message
                    .clone()
                    .unwrap_or_else(|| "model call failed".to_string()),
            ),
            AgentEvent::MessageEnd(m) => SdkMessage::Assistant(m.text()),
            AgentEvent::ToolExecutionStart {
                tool_call_id,
                tool_name,
                args,
            } => SdkMessage::ToolUse(tool_call_id, tool_name, args),
            AgentEvent::ToolExecutionEnd {
                tool_call_id,
                tool_name,
                content,
                is_error,
            } => SdkMessage::ToolResult(tool_call_id, tool_name, content, is_error),
            AgentEvent::AgentStart => SdkMessage::System("agent_start".into()),
            AgentEvent::AgentEnd => SdkMessage::System("session_end".into()),
            _ => continue,
        };
        if tx.send(msg).is_err() {
            break; // consumer went away → stop draining (loop ends with channel)
        }
    }
    mcp.cleanup().await;
    Ok(())
}

/// Apply allowlist THEN denylist to a registry (pure, testable).
///
/// # Description
/// Ports the TS filter order: `allowedTools` allowlist first, then
/// `disallowedTools` denylist. `None` allowlist = keep all.
///
/// # Example
/// ```rust
/// use sdk::build_registry;
/// use engine::tools::ReadTool;
/// use std::sync::Arc;
/// let tools: Vec<Arc<dyn engine::agent::AgentTool>> = vec![Arc::new(ReadTool::new("/tmp".into()))];
/// assert!(build_registry(tools, Some(&["read".to_string()]), &[]).len() == 1);
/// ```
pub fn build_registry(
    tools: Vec<Arc<dyn AgentTool>>,
    allowed: Option<&[String]>,
    disallowed: &[String],
) -> Vec<Arc<dyn AgentTool>> {
    tools
        .into_iter()
        .filter(|t| match allowed {
            Some(list) => list.iter().any(|a| a == t.name()),
            None => true,
        })
        .filter(|t| !disallowed.iter().any(|d| d == t.name()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::tools::ReadTool;

    #[test]
    fn filters_apply_allow_then_deny() {
        let tools: Vec<Arc<dyn AgentTool>> = vec![Arc::new(ReadTool::new("/tmp".into()))];
        assert_eq!(
            build_registry(tools.clone(), Some(&["read".into()]), &[]).len(),
            1
        );
        assert_eq!(
            build_registry(tools.clone(), Some(&["cli".into()]), &[]).len(),
            0
        );
        assert_eq!(build_registry(tools, None, &["read".into()]).len(), 0);
    }
}
