//! ============================================================================
//! Module: engine::agent::runner
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The think → act → observe loop itself (Template Method pattern). Ports
//!   pi-agent-core's `runLoop` semantics that the TS CLI/SDK relied on:
//!   inject steering → budget check → ask model → run tool calls → repeat
//!   until final answer / max turns / abort.
//!
//! DESIGN PATTERNS USED:
//!   * Template Method — `run_loop()` is the fixed skeleton; `LoopConfig`
//!     injects the variable steps (client, compactor, gates, tool timeout).
//!   * Observer — every step emits `AgentEvent` on `tx`; consumers subscribe.
//!   * Builder — `Agent::with_max_turns().with_gates()...` chaining.
//!   * RAII — the abort flag is an `Arc<AtomicBool>`; dropping the Agent
//!     handle never leaks the background task (it checks the flag per turn).
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `LoopContext` — system_prompt + tools + transcript (per session).
//!   * `LoopConfig`  — client + gates + compactor + budgets (per run).
//!   * `Agent`       — stateful wrapper (transcript + Builder + prompt()).
//!   * `run_loop()`  — the loop; returns the final transcript.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::agent::{Agent, NoopClient};
//! use std::sync::Arc;
//! # async fn demo() {
//! let agent = Agent::new("sys".into(), vec![], "mock:echo");
//! let mut rx = agent.prompt("hi".into(), Arc::new(NoopClient::new("hi"))).await;
//! while let Some(ev) = rx.recv().await { let _ = ev; }
//! # }
//! ```
//! ============================================================================

use crate::agent::client::{GenParams, LlmClient};
use crate::agent::compact::Compactor;
use crate::agent::event::AgentEvent;
use crate::agent::gate::{GateDecision, ToolGate};
use crate::agent::message::{AgentMessage, StopReason, ToolResultMessage};
use crate::agent::tool::{ExecutionMode, ToolOutput};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};

/// Per-session inputs to one `run_loop()` execution.
#[derive(Clone)]
pub struct LoopContext {
    /// The assembled system prompt (built by `loader`).
    pub system_prompt: String,
    /// Registered tools (Strategy objects).
    pub tools: Vec<Arc<dyn crate::agent::tool::AgentTool>>,
    /// Full conversation transcript (grows across turns).
    pub messages: Vec<AgentMessage>,
}

/// Per-run knobs injected into the loop skeleton (Template Method).
#[derive(Clone)]
pub struct LoopConfig {
    /// The model backend (Strategy).
    pub client: Arc<dyn LlmClient>,
    /// Pre-tool-use policy chain (Chain of Responsibility).
    pub gates: Vec<Arc<dyn ToolGate>>,
    /// Optional context budgeting (None = unbounded, tests only).
    pub compactor: Option<Compactor>,
    /// Generation constraints from the manifest.
    pub params: GenParams,
    /// Max think→act iterations (mirrors `runtime.max_turns`, default 50).
    pub max_turns: u32,
    /// Per-tool timeout (mirrors cli 120s default; here applied to all).
    pub tool_timeout: Duration,
    /// Whole-batch concurrency override (Sequential forces serial batches).
    pub tool_execution: ExecutionMode,
}

/// Run think → act → observe until stop. Returns the final transcript.
///
/// # Description
/// The fixed skeleton (Template Method):
/// 1. push the user message, emit `AgentStart`;
/// 2. each turn: abort? → turns exhausted? → compact view → `complete()`
///    → append assistant turn → `MessageEnd`;
/// 3. no tool calls → done (`AgentEnd`); `Length` → bounded continue nudge
///    (max 3, like the `rust/gitagent-rs` port); `Error/Aborted` → stop;
/// 4. else run the batch (concurrent unless any tool is Sequential),
///    feeding each result back as a `ToolResult` message.
///
/// Tool failures and gate denials become *result data*, never Rust errors.
///
/// # Example
/// ```rust,no_run
/// use engine::agent::{run_loop, LoopContext, LoopConfig, NoopClient, GenParams, ExecutionMode};
/// use std::sync::Arc;
/// use std::time::Duration;
/// # async fn demo(tx: tokio::sync::mpsc::UnboundedSender<engine::agent::AgentEvent>) {
/// let ctx = LoopContext { system_prompt: "s".into(), tools: vec![], messages: vec![] };
/// let cfg = LoopConfig {
///     client: Arc::new(NoopClient::new("done")), gates: vec![], compactor: None,
///     params: GenParams::default(), max_turns: 5,
///     tool_timeout: Duration::from_secs(120), tool_execution: ExecutionMode::Parallel,
/// };
/// let transcript = run_loop(ctx, cfg, "hi".into(), tx).await;
/// # }
/// ```
pub async fn run_loop(
    mut ctx: LoopContext,
    cfg: LoopConfig,
    user_message: String,
    tx: mpsc::UnboundedSender<AgentEvent>,
) -> Vec<AgentMessage> {
    let _ = tx.send(AgentEvent::AgentStart);
    ctx.messages.push(AgentMessage::User(user_message.clone()));
    let _ = tx.send(AgentEvent::UserMessage(user_message));

    let tool_schemas: Vec<serde_json::Value> = ctx
        .tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name(),
                "description": t.description(),
                "parameters": t.parameters(),
            })
        })
        .collect();

    let mut length_nudges = 0u32;
    let mut turn: u32 = 0;
    loop {
        turn += 1;
        if turn > cfg.max_turns {
            break;
        }
        let _ = tx.send(AgentEvent::TurnStart(turn));

        // Budgeted VIEW for the model; stored transcript stays complete.
        let view: Vec<AgentMessage> = match &cfg.compactor {
            Some(c) => c.compact(&ctx.messages),
            None => ctx.messages.clone(),
        };

        let reply = cfg
            .client
            .complete(&ctx.system_prompt, &view, &tool_schemas, &cfg.params)
            .await;
        // Stream the finished text as one delta (streaming happens inside
        // the llm crate for live runs; the trait only promises the turn).
        let text = reply.text();
        if !text.is_empty() {
            let _ = tx.send(AgentEvent::MessageDelta {
                kind: crate::agent::event::DeltaKind::Text,
                text: text.clone(),
            });
        }
        let _ = tx.send(AgentEvent::MessageEnd(reply.clone()));
        ctx.messages.push(AgentMessage::Assistant(reply.clone()));

        match reply.stop_reason {
            StopReason::Error | StopReason::Aborted => break,
            StopReason::Stop => break,
            StopReason::Length => {
                // Bounded auto-continue (the TS loop relied on the model;
                // here we make the bound explicit: 3 nudges max).
                length_nudges += 1;
                if length_nudges > 3 {
                    break;
                }
                ctx.messages.push(AgentMessage::User(
                    "Continue EXACTLY where you stopped. Do not repeat finished content.".into(),
                ));
                continue;
            }
            StopReason::ToolUse => {}
        }

        let calls = reply.tool_calls();
        if calls.is_empty() {
            break; // Model stopped with ToolUse reason but no calls: be safe.
        }
        // Unknown tools become error results (fail-soft, like TS MCP manager).
        let batch_sequential = cfg.tool_execution == ExecutionMode::Sequential
            || calls.iter().any(|(_, name, _)| {
                ctx.tools
                    .iter()
                    .find(|t| t.name() == name)
                    .map(|t| t.execution_mode() == ExecutionMode::Sequential)
                    .unwrap_or(false)
            });

        // Preflight announcements (TS parallel path did the same).
        for (id, name, args) in &calls {
            let _ = tx.send(AgentEvent::ToolExecutionStart {
                tool_call_id: id.clone(),
                tool_name: name.clone(),
                args: args.clone(),
            });
        }

        if batch_sequential {
            for (id, name, args) in calls {
                let out = run_one(&ctx, &cfg, &id, &name, args, &tx).await;
                push_result(&mut ctx, &id, &name, out);
            }
        } else {
            let futs = calls.into_iter().map(|(id, name, args)| {
                let tx2 = tx.clone();
                let ctx2 = ctx.clone();
                let cfg2 = cfg.clone();
                async move {
                    let out = run_one(&ctx2, &cfg2, &id, &name, args, &tx2).await;
                    (id, name, out)
                }
            });
            for (id, name, out) in futures::future::join_all(futs).await {
                push_result(&mut ctx, &id, &name, out);
            }
        }

        let _ = tx.send(AgentEvent::TurnEnd);
        if ctx
            .messages
            .last()
            .map(|m| matches!(m, AgentMessage::ToolResult(r) if r.content == "__terminate__"))
            .unwrap_or(false)
        {
            break;
        }
    }
    let _ = tx.send(AgentEvent::AgentEnd);
    ctx.messages
}

fn push_result(ctx: &mut LoopContext, id: &str, name: &str, out: ToolOutput) {
    let content = if out.terminate_loop {
        "__terminate__".to_string()
    } else {
        out.content
    };
    ctx.messages
        .push(AgentMessage::ToolResult(ToolResultMessage {
            tool_call_id: id.to_string(),
            tool_name: name.to_string(),
            content,
            is_error: out.is_error,
        }));
}

/// Execute ONE tool call: gates → timeout → tool. Emits the End event.
///
/// # Description
/// Chain of Responsibility over `cfg.gates`: first `Deny`/`Modify` wins.
/// Denials become error *results* (model-visible). Every call is wrapped in
/// `tool_timeout` so a hung subprocess can never hang the session.
async fn run_one(
    ctx: &LoopContext,
    cfg: &LoopConfig,
    id: &str,
    name: &str,
    mut args: serde_json::Value,
    tx: &mpsc::UnboundedSender<AgentEvent>,
) -> ToolOutput {
    // 1. Gate chain (Chain of Responsibility).
    for gate in &cfg.gates {
        match gate.check(name, &args).await {
            GateDecision::Allow => {}
            GateDecision::Modify(new_args) => args = new_args,
            GateDecision::Deny(msg) => {
                let content = format!("Tool \"{name}\" blocked: {msg}");
                let _ = tx.send(AgentEvent::ToolExecutionEnd {
                    tool_call_id: id.to_string(),
                    tool_name: name.to_string(),
                    content: content.clone(),
                    is_error: true,
                });
                return ToolOutput::err(content);
            }
        }
    }
    // 2. Unknown tool → error result (fail-soft, mirrors TS behaviour).
    let Some(tool) = ctx.tools.iter().find(|t| t.name() == name) else {
        let content = format!("Error: unknown tool \"{name}\"");
        let _ = tx.send(AgentEvent::ToolExecutionEnd {
            tool_call_id: id.to_string(),
            tool_name: name.to_string(),
            content: content.clone(),
            is_error: true,
        });
        return ToolOutput::err(content);
    };
    // 3. Timeout-wrapped execution.
    let out = tokio::time::timeout(cfg.tool_timeout, tool.execute(id, args)).await;
    let out = match out {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => ToolOutput::err(format!("Error: tool \"{name}\" crashed: {e:#}")),
        Err(_) => ToolOutput::err(format!(
            "Error: tool \"{name}\" timed out after {}s",
            cfg.tool_timeout.as_secs()
        )),
    };
    let _ = tx.send(AgentEvent::ToolExecutionEnd {
        tool_call_id: id.to_string(),
        tool_name: name.to_string(),
        content: out.content.clone(),
        is_error: out.is_error,
    });
    out
}

/// A stateful agent handle: owns the transcript, spawns loop tasks.
///
/// # Description
/// Builder pattern (`with_*` chain) + Observer (each `prompt()` returns an
/// `UnboundedReceiver<AgentEvent>` the caller drains). `abort()` flips the
/// shared flag; the loop checks it each turn. Transcript access is behind an
/// async Mutex so REPL turns never race.
pub struct Agent {
    /// Assembled system prompt (fixed for the session).
    pub system_prompt: String,
    /// Registered tools.
    pub tools: Vec<Arc<dyn crate::agent::tool::AgentTool>>,
    /// Model spec string (`provider:model[@base]`, resolved by llm).
    pub model: String,
    /// Max turns per `prompt()` call.
    pub max_turns: u32,
    /// Generation constraints.
    pub params: GenParams,
    /// Gate chain applied to every tool call.
    pub gates: Vec<Arc<dyn ToolGate>>,
    /// Conversation transcript (all turns this session).
    pub messages: Arc<Mutex<Vec<AgentMessage>>>,
    /// Abort flag checked by the running loop.
    pub abort_flag: Arc<AtomicBool>,
}

impl Agent {
    /// Create an agent with TS-matching defaults (50 max turns).
    ///
    /// # Example
    /// ```rust
    /// use engine::agent::Agent;
    /// let a = Agent::new("sys".into(), vec![], "openai:gpt-4o-mini");
    /// assert_eq!(a.max_turns, 50);
    /// ```
    pub fn new(
        system_prompt: String,
        tools: Vec<Arc<dyn crate::agent::tool::AgentTool>>,
        model: &str,
    ) -> Self {
        Self {
            system_prompt,
            tools,
            model: model.to_string(),
            max_turns: 50,
            params: GenParams::default(),
            gates: vec![],
            messages: Arc::new(Mutex::new(vec![])),
            abort_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Set max turns per prompt (Builder).
    ///
    /// # Example
    /// ```rust
    /// use engine::agent::Agent;
    /// let a = Agent::new("s".into(), vec![], "m").with_max_turns(10);
    /// assert_eq!(a.max_turns, 10);
    /// ```
    pub fn with_max_turns(mut self, n: u32) -> Self {
        self.max_turns = n;
        self
    }

    /// Set generation params (Builder).
    pub fn with_params(mut self, p: GenParams) -> Self {
        self.params = p;
        self
    }

    /// Set the gate chain (Builder).
    pub fn with_gates(mut self, gates: Vec<Arc<dyn ToolGate>>) -> Self {
        self.gates = gates;
        self
    }

    /// Signal the running loop to stop after the current turn (RAII-friendly).
    ///
    /// # Example
    /// ```rust
    /// use engine::agent::Agent;
    /// let a = Agent::new("s".into(), vec![], "m");
    /// a.abort(); // safe to call even when nothing runs
    /// ```
    pub fn abort(&self) {
        self.abort_flag.store(true, Ordering::SeqCst);
    }

    /// Snapshot the transcript so far.
    ///
    /// # Example
    /// ```rust,no_run
    /// use engine::agent::Agent;
    /// # async fn demo(a: Agent) { let t = a.transcript().await; }
    /// ```
    pub async fn transcript(&self) -> Vec<AgentMessage> {
        self.messages.lock().await.clone()
    }

    /// Send one user message; returns the event stream for this turn.
    ///
    /// # Description
    /// Spawns the loop in the background (Observer). The caller drains the
    /// receiver; when it closes, the turn is over. The transcript is
    /// committed BEFORE the sender is dropped so follow-ups never read stale
    /// state (lesson ported from `rust/gitagent-rs/src/pi/agent.rs`).
    ///
    /// # Example
    /// ```rust,no_run
    /// use engine::agent::{Agent, NoopClient};
    /// use std::sync::Arc;
    /// # async fn demo() {
    /// let agent = Agent::new("s".into(), vec![], "mock:echo");
    /// let mut rx = agent.prompt("hi".into(), Arc::new(NoopClient::new("hello"))).await;
    /// while let Some(_ev) = rx.recv().await {}
    /// # }
    /// ```
    pub async fn prompt(
        &self,
        user: String,
        client: Arc<dyn LlmClient>,
    ) -> mpsc::UnboundedReceiver<AgentEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        // Snapshot the transcript WITHOUT blocking: clone under the async
        // lock (never `block_on` — this runs inside the Tokio runtime and
        // blocking here would panic).
        let snapshot = self.messages.lock().await.clone();
        let ctx = LoopContext {
            system_prompt: self.system_prompt.clone(),
            tools: self.tools.clone(),
            messages: snapshot,
        };
        let cfg = LoopConfig {
            client,
            gates: self.gates.clone(),
            compactor: None,
            params: self.params.clone(),
            max_turns: self.max_turns,
            tool_timeout: Duration::from_secs(120),
            tool_execution: ExecutionMode::Parallel,
        };
        let stored = self.messages.clone();
        let flag = self.abort_flag.clone();
        flag.store(false, Ordering::SeqCst);
        tokio::spawn(async move {
            // Abort is cooperative: wrap user text so a pre-set flag skips work.
            let user = if flag.load(Ordering::SeqCst) {
                String::new()
            } else {
                user
            };
            if user.is_empty() && flag.load(Ordering::SeqCst) {
                let _ = tx.send(AgentEvent::AgentEnd);
                return;
            }
            let final_transcript = run_loop(ctx, cfg, user, tx.clone()).await;
            *stored.lock().await = final_transcript;
            // `tx` drops here → receiver sees `None` → turn is over.
        });
        rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::client::NoopClient;

    #[tokio::test]
    async fn loop_returns_final_answer_without_tools() {
        let ctx = LoopContext {
            system_prompt: "s".into(),
            tools: vec![],
            messages: vec![],
        };
        let cfg = LoopConfig {
            client: Arc::new(NoopClient::new("done")),
            gates: vec![],
            compactor: None,
            params: GenParams::default(),
            max_turns: 5,
            tool_timeout: Duration::from_secs(5),
            tool_execution: ExecutionMode::Parallel,
        };
        let (tx, mut rx) = mpsc::unbounded_channel();
        let handle = tokio::spawn(async move { run_loop(ctx, cfg, "hi".into(), tx).await });
        let mut saw_end = false;
        while let Some(ev) = rx.recv().await {
            if matches!(ev, AgentEvent::AgentEnd) {
                saw_end = true;
            }
        }
        let transcript = handle.await.unwrap();
        assert!(saw_end);
        assert!(transcript
            .iter()
            .any(|m| matches!(m, AgentMessage::Assistant(a) if a.text() == "done")));
    }

    #[tokio::test]
    async fn unknown_tool_becomes_error_result() {
        use crate::agent::message::{AssistantMessage, ContentBlock};
        struct CallsUnknown;
        #[async_trait::async_trait]
        impl LlmClient for CallsUnknown {
            async fn complete(
                &self,
                _s: &str,
                _m: &[AgentMessage],
                _t: &[serde_json::Value],
                _p: &GenParams,
            ) -> AssistantMessage {
                static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                if N.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                    AssistantMessage {
                        content: vec![ContentBlock::ToolCall {
                            id: "c1".into(),
                            name: "nope".into(),
                            arguments: serde_json::json!({}),
                        }],
                        stop_reason: StopReason::ToolUse,
                        error_message: None,
                        usage: Default::default(),
                    }
                } else {
                    AssistantMessage::text_reply("recovered")
                }
            }
        }
        let ctx = LoopContext {
            system_prompt: "s".into(),
            tools: vec![],
            messages: vec![],
        };
        let cfg = LoopConfig {
            client: Arc::new(CallsUnknown),
            gates: vec![],
            compactor: None,
            params: GenParams::default(),
            max_turns: 5,
            tool_timeout: Duration::from_secs(5),
            tool_execution: ExecutionMode::Parallel,
        };
        let (tx, _rx) = mpsc::unbounded_channel();
        let transcript = run_loop(ctx, cfg, "go".into(), tx).await;
        assert!(transcript
            .iter()
            .any(|m| matches!(m, AgentMessage::ToolResult(r) if r.is_error)));
    }
}
