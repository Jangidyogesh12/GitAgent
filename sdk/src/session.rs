//! ============================================================================
//! Module: sdk::session
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Multi-turn sessions: one `Agent` (+ client + gates) kept alive across
//!   `send()` calls, with the transcript accumulating. Ports the RE-usable
//!   half of `src/sdk.ts` (the TS `Query` handle's `steer`/`messages`
//!   surface maps to repeated `send()` here; abort via `abort()`).
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `Session` — `open()` + `send()` + `abort()` + `transcript()`.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use sdk::{Session, QueryOptions};
//! use std::path::PathBuf;
//! # async fn demo() {
//! let opts = QueryOptions::new(PathBuf::from("./my-agent"), "");
//! let session = Session::open(opts).unwrap();
//! let mut rx = session.send("hello".to_string()).await.unwrap();
//! while let Some(m) = rx.recv().await { let _ = m; }
//! # }
//! ```
//! ============================================================================

use std::sync::Arc;
use tokio::sync::mpsc;

use engine::agent::{Agent, GenParams};

use crate::permissions::{PermissionGate, PermissionMode};
use crate::types::{QueryOptions, SdkMessage};

/// A multi-turn agent handle (owns transcript + engine config).
pub struct Session {
    agent: Agent,
    client: Arc<dyn engine::agent::LlmClient>,
    session_id: String,
}

impl Session {
    /// Load the agent + build registry/gates/client once (like `query()`).
    ///
    /// # Description
    /// Reuses the same pipeline as `query()` minus the one-shot prompt:
    /// suffix applied, registry filtered, gates attached, client built.
    /// `opts.prompt` is IGNORED (each `send()` supplies its turn).
    ///
    /// # Example
    /// ```rust,no_run
    /// use sdk::{Session, QueryOptions};
    /// use std::path::PathBuf;
    /// let s = Session::open(QueryOptions::new(PathBuf::from("./my-agent"), "")).unwrap();
    /// ```
    pub fn open(opts: QueryOptions) -> anyhow::Result<Self> {
        let mut loaded = engine::loader::load_agent(
            &opts.dir,
            opts.model.as_deref(),
            opts.session_id.as_deref(),
        )?;
        if let Some(suffix) = &opts.system_prompt_suffix {
            loaded.system_prompt.push_str("\n\n");
            loaded.system_prompt.push_str(suffix);
        }
        let cli_timeout = loaded.manifest.cli_timeout().unwrap_or(120);
        let mut tools: Vec<Arc<dyn engine::agent::AgentTool>> =
            engine::tools::builtin_tools(&opts.dir, cli_timeout, None)
                .into_iter()
                .collect();
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
        tools.extend(opts.extra_tools.iter().cloned());
        tools = crate::query::build_registry(
            tools,
            opts.allowed_tools.as_deref(),
            &opts.disallowed_tools,
        );

        let mut gates: Vec<Arc<dyn engine::agent::ToolGate>> = vec![Arc::new(PermissionGate::new(
            opts.permission_mode.unwrap_or(PermissionMode::Default),
            &opts.permission_rules,
        ))];
        let hooks = engine::hooks::load_hooks_config(&opts.dir);
        if !hooks.pre_tool_use.is_empty() {
            gates.push(Arc::new(engine::hooks::HookGate::new(
                opts.dir.clone(),
                loaded.session_id.clone(),
                hooks.pre_tool_use,
            )));
        }
        let specs = loaded.model_specs.clone();
        let client: Arc<dyn engine::agent::LlmClient> = Arc::new(engine::llm::OpenAiCompat::new(
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
        let agent = Agent::new(loaded.system_prompt, tools, &specs.join(","))
            .with_max_turns(max_turns)
            .with_params(params)
            .with_gates(gates);
        Ok(Self {
            agent,
            client,
            session_id: loaded.session_id,
        })
    }

    /// Session id (the loader-assigned UUID or override).
    ///
    /// # Example
    /// ```rust,no_run
    /// use sdk::{Session, QueryOptions};
    /// use std::path::PathBuf;
    /// # fn demo() {
    /// let s = Session::open(QueryOptions::new(PathBuf::from("./my-agent"), "")).unwrap();
    /// println!("{}", s.session_id());
    /// # }
    /// ```
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Send one turn; returns the mapped `SdkMessage` stream.
    ///
    /// # Example
    /// ```rust,no_run
    /// use sdk::{Session, QueryOptions};
    /// use std::path::PathBuf;
    /// # async fn demo() {
    /// let s = Session::open(QueryOptions::new(PathBuf::from("./my-agent"), "")).unwrap();
    /// let mut rx = s.send("hi".to_string()).await.unwrap();
    /// while let Some(m) = rx.recv().await { let _ = m; }
    /// # }
    /// ```
    pub async fn send(
        &self,
        prompt: String,
    ) -> anyhow::Result<mpsc::UnboundedReceiver<SdkMessage>> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut ev_rx = self.agent.prompt(prompt, self.client.clone()).await;
        tokio::spawn(async move {
            while let Some(ev) = ev_rx.recv().await {
                let msg = match ev {
                    engine::agent::AgentEvent::MessageDelta { text, .. } => SdkMessage::Delta(text),
                    // Same rule as query(): error turns become visible
                    // errors, never silent empty messages.
                    engine::agent::AgentEvent::MessageEnd(m)
                        if m.stop_reason == engine::agent::StopReason::Error =>
                    {
                        SdkMessage::Error(
                            m.error_message
                                .clone()
                                .unwrap_or_else(|| "model call failed".to_string()),
                        )
                    }
                    engine::agent::AgentEvent::MessageEnd(m) => SdkMessage::Assistant(m.text()),
                    engine::agent::AgentEvent::ToolExecutionStart {
                        tool_call_id,
                        tool_name,
                        args,
                    } => SdkMessage::ToolUse(tool_call_id, tool_name, args),
                    engine::agent::AgentEvent::ToolExecutionEnd {
                        tool_call_id,
                        tool_name,
                        content,
                        is_error,
                    } => SdkMessage::ToolResult(tool_call_id, tool_name, content, is_error),
                    engine::agent::AgentEvent::AgentEnd => SdkMessage::System("session_end".into()),
                    _ => continue,
                };
                if tx.send(msg).is_err() {
                    break;
                }
            }
        });
        Ok(rx)
    }

    /// Abort the running turn (cooperative; checked each loop iteration).
    ///
    /// # Example
    /// ```rust,no_run
    /// use sdk::{Session, QueryOptions};
    /// use std::path::PathBuf;
    /// # fn demo() {
    /// let s = Session::open(QueryOptions::new(PathBuf::from("./my-agent"), "")).unwrap();
    /// s.abort();
    /// # }
    /// ```
    pub fn abort(&self) {
        self.agent.abort();
    }

    /// Snapshot the transcript so far.
    ///
    /// # Example
    /// ```rust,no_run
    /// use sdk::{Session, QueryOptions};
    /// use std::path::PathBuf;
    /// # async fn demo() {
    /// let s = Session::open(QueryOptions::new(PathBuf::from("./my-agent"), "")).unwrap();
    /// let t = s.transcript().await;
    /// # }
    /// ```
    pub async fn transcript(&self) -> Vec<engine::agent::AgentMessage> {
        self.agent.transcript().await
    }
}
