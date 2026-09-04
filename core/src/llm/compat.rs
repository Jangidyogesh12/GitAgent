//! ============================================================================
//! Module: engine::llm::compat
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The OpenAI-compatible streaming chat client (Strategy implementation of
//!   `engine::agent::LlmClient`). Sends `POST {base}/chat/completions` with
//!   `stream:true`, parses SSE (`data:` lines until `[DONE]`), accumulates
//!   text / reasoning / indexed tool-call deltas, converts message history,
//!   and recovers text-form tool calls (`{"name":..}` fenced JSON) that small
//!   local models emit instead of native tool calls.
//!
//! DESIGN PATTERNS USED:
//!   * Strategy — implements `LlmClient::complete()` for the engine.
//!   * Adapter — translates our transcript into OpenAI wire messages and the
//!     SSE stream back into `AssistantMessage`.
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `OpenAiCompat`           — client holding specs + http + retries.
//!   * `OpenAiCompat::new()`    — build from spec strings.
//!   * `recover_text_tool_call()` — fenced-JSON tool-call recovery.
//!   * `to_wire_messages()`    — transcript → OpenAI message list.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::llm::OpenAiCompat;
//! let client = OpenAiCompat::new(&["openai:gpt-4o-mini"]);
//! ```
//! ============================================================================

use crate::agent::client::{GenParams, LlmClient};
use crate::agent::message::{AgentMessage, AssistantMessage, ContentBlock, StopReason, Usage};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use crate::llm::spec::{cost_per_mtok, resolve_model, ModelSpec};

/// OpenAI-compatible chat client (Strategy for the engine).
#[derive(Clone)]
pub struct OpenAiCompat {
    /// Preferred + fallback specs, tried in order.
    pub specs: Vec<ModelSpec>,
    /// Shared HTTP client (identifies as `gitagent/<version>`).
    pub http: Client,
    /// Retries per spec on transient errors.
    pub max_retries: u32,
    /// Stable id sent as `x-opencode-session` on opencode.ai endpoints
    /// (their docs ask third-party clients to send it for prompt-cache
    /// optimization + abuse monitoring). One per client ≈ one agent run.
    pub session_key: String,
}

/// Shared HTTP client builder (identifying User-Agent, no broad default).
///
/// # Example
/// ```rust,no_run
/// // let http = engine::llm::compat::http_client();
/// ```
pub fn http_client() -> Client {
    Client::builder()
        .user_agent(concat!("gitagent/", env!("CARGO_PKG_VERSION")))
        .build()
        .unwrap_or_else(|_| Client::new())
}

impl OpenAiCompat {
    /// Build from `provider:model[@base]` strings (first = preferred).
    ///
    /// # Example
    /// ```rust
    /// use engine::llm::OpenAiCompat;
    /// let c = OpenAiCompat::new(&["openai:gpt-4o-mini"]);
    /// assert_eq!(c.specs.len(), 1);
    /// ```
    pub fn new(specs: &[&str]) -> Self {
        Self {
            specs: specs.iter().map(|s| resolve_model(s)).collect(),
            http: http_client(),
            max_retries: 3,
            session_key: uuid::Uuid::new_v4().to_string(),
        }
    }

    /// Build from already-resolved specs.
    pub fn from_specs(specs: Vec<ModelSpec>) -> Self {
        Self {
            specs,
            http: http_client(),
            max_retries: 3,
            session_key: uuid::Uuid::new_v4().to_string(),
        }
    }
}

#[async_trait]
impl LlmClient for OpenAiCompat {
    async fn complete(
        &self,
        system: &str,
        messages: &[AgentMessage],
        tools: &[serde_json::Value],
        params: &GenParams,
    ) -> AssistantMessage {
        crate::llm::fallback::complete_with_fallback(
            &self.http,
            &self.specs,
            system,
            messages,
            tools,
            params,
            self.max_retries,
            &self.session_key,
        )
        .await
    }
}

/// Convert our transcript into OpenAI wire messages (Adapter).
///
/// # Description
/// User → `{role:user}`, Assistant text → `{role:assistant}`, ToolCall →
/// `tool_calls:[{id,type:function,function:{name,arguments}}]`, ToolResult →
/// `{role:tool, tool_call_id, content}`. Assistant content is ALWAYS a
/// string (never null) because Ollama rejects null content.
///
/// # Example
/// ```rust
/// use engine::llm::compat::to_wire_messages;
/// use engine::agent::message::AgentMessage;
/// let w = to_wire_messages(&[AgentMessage::User("hi".into())]);
/// assert_eq!(w[0]["role"], "user");
/// ```
pub fn to_wire_messages(messages: &[AgentMessage]) -> Vec<serde_json::Value> {
    let mut out = vec![];
    for m in messages {
        match m {
            AgentMessage::User(t) => out.push(json!({"role": "user", "content": t})),
            AgentMessage::Assistant(a) => {
                let text = a.text();
                let calls: Vec<serde_json::Value> = a
                    .tool_calls()
                    .into_iter()
                    .map(|(id, name, args)| {
                        json!({"id": id, "type": "function",
                               "function": {"name": name, "arguments": args.to_string()}})
                    })
                    .collect();
                let mut msg = json!({"role": "assistant", "content": text});
                if !calls.is_empty() {
                    msg["tool_calls"] = json!(calls);
                }
                out.push(msg);
            }
            AgentMessage::ToolResult(r) => out.push(
                json!({"role": "tool", "tool_call_id": r.tool_call_id, "content": r.content}),
            ),
        }
    }
    out
}

/// Recover a text-form tool call from fenced/braced JSON.
///
/// # Description
/// Small local models (Gemma/Muse via Ollama templates) print
/// `{"name": "<tool>", "arguments": {...}}` as TEXT instead of using native
/// tool calls. When the reply holds exactly one such object (optionally in a
/// ```json fence), promote it to a real ToolCall block. Returns None when
/// the text is a normal answer — never steal user-visible text.
///
/// # Example
/// ```rust
/// use engine::llm::compat::recover_text_tool_call;
/// let b = recover_text_tool_call("```json\n{\"name\":\"read\",\"arguments\":{\"path\":\"a\"}}\n```", "c1").unwrap();
/// assert!(matches!(b, engine::agent::message::ContentBlock::ToolCall { .. }));
/// assert!(recover_text_tool_call("just a normal answer", "c1").is_none());
/// ```
pub fn recover_text_tool_call(text: &str, call_id: &str) -> Option<ContentBlock> {
    let t = text.trim();
    let inner = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```"))
        .and_then(|s| s.strip_suffix("```").map(|s| s.trim()))
        .unwrap_or(t);
    if !(inner.starts_with('{') && inner.ends_with('}')) {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(inner).ok()?;
    let name = v.get("name")?.as_str()?;
    if name.is_empty() || name.contains(' ') || name.contains('\n') {
        return None;
    }
    // Guard: a real answer that merely CONTAINS an object has extra keys/text.
    let allowed = v
        .as_object()?
        .keys()
        .all(|k| k == "name" || k == "arguments" || k == "args");
    if !allowed {
        return None;
    }
    let args = v
        .get("arguments")
        .or_else(|| v.get("args"))
        .cloned()
        .unwrap_or(json!({}));
    Some(ContentBlock::ToolCall {
        id: call_id.to_string(),
        name: name.to_string(),
        arguments: args,
    })
}

/// Parse one SSE `data:` payload into (text, thinking, tool-delta, finish).
pub(crate) fn parse_sse_data(
    data: &str,
    acc_text: &mut String,
    acc_think: &mut String,
    tool_acc: &mut std::collections::HashMap<u32, (String, String, String)>,
    usage: &mut Usage,
    model_id: &str,
) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    if let Some(u) = v.get("usage") {
        let inp = u.get("prompt_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
        let out = u
            .get("completion_tokens")
            .and_then(|x| x.as_u64())
            .unwrap_or(0);
        usage.input += inp;
        usage.output += out;
        usage.total += inp + out;
        let (pi, po) = cost_per_mtok(model_id);
        if usage.cost_usd == 0.0 && (pi > 0.0 || po > 0.0) {
            usage.cost_usd += inp as f64 / 1e6 * pi + out as f64 / 1e6 * po;
        }
    }
    let choice = v.get("choices")?.get(0)?;
    let finish = choice
        .get("finish_reason")
        .and_then(|f| f.as_str())
        .map(|s| s.to_string());
    let delta = choice.get("delta").or_else(|| choice.get("message"))?;
    if let Some(t) = delta.get("content").and_then(|c| c.as_str()) {
        acc_text.push_str(t);
    }
    for key in ["reasoning_content", "reasoning"] {
        if let Some(t) = delta.get(key).and_then(|c| c.as_str()) {
            acc_think.push_str(t);
        }
    }
    if let Some(calls) = delta.get("tool_calls").and_then(|c| c.as_array()) {
        for c in calls {
            let idx = c.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
            let e = tool_acc
                .entry(idx)
                .or_insert((String::new(), String::new(), String::new()));
            if let Some(id) = c.get("id").and_then(|x| x.as_str()) {
                if !id.is_empty() {
                    e.0 = id.to_string();
                }
            }
            if let Some(f) = c.get("function") {
                if let Some(n) = f.get("name").and_then(|x| x.as_str()) {
                    if !n.is_empty() {
                        e.1 = n.to_string();
                    }
                }
                if let Some(a) = f.get("arguments").and_then(|x| x.as_str()) {
                    e.2.push_str(a);
                }
            }
        }
    }
    finish
}

/// Map OpenAI finish_reason → our StopReason.
pub(crate) fn map_finish(finish: Option<&str>, has_calls: bool) -> StopReason {
    match finish {
        Some("tool_calls") => StopReason::ToolUse,
        Some("length") => StopReason::Length,
        Some("stop") | None => {
            if has_calls {
                StopReason::ToolUse
            } else {
                StopReason::Stop
            }
        }
        _ => StopReason::Stop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_fenced_tool_call() {
        let b = recover_text_tool_call(
            "```json\n{\"name\":\"read\",\"arguments\":{\"path\":\"a\"}}\n```",
            "c1",
        )
        .unwrap();
        assert!(matches!(b, ContentBlock::ToolCall { .. }));
    }

    #[test]
    fn ignores_normal_answers() {
        assert!(recover_text_tool_call("The answer is 42, see {\"a\":1} docs.", "c1").is_none());
    }

    #[test]
    fn wire_messages_always_string_content() {
        let w = to_wire_messages(&[AgentMessage::Assistant(AssistantMessage::text_reply(""))]);
        assert_eq!(w[0]["content"], "");
    }
}
