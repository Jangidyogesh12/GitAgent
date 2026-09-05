//! ============================================================================
//! Module: engine::llm::fallback
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Resilient completion driver: try the preferred spec, retry transient
//!   errors with exponential backoff, then fail over to fallback specs in
//!   order. Single attempts stream chat-completions SSE into text, thinking,
//!   tool-call, and usage accumulators until the done marker.
//!
//! DESIGN PATTERNS USED:
//!   * Chain of Responsibility — specs tried in order; first success wins.
//!   * Decorator — backoff/retry wraps the single-attempt `stream_once`.
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `complete_with_fallback()` — resilient driver (errors as VALUES).
//!   * `stream_once()`             — one SSE attempt against one spec.
//!
//! HOW IT WORKS:
//!   * Backoff is 400ms times 2^attempt, shift capped at 5. Fatal errors
//!     (auth, bad request) skip retries and move to the next spec.
//!   * Tool-less endpoints degrade gracefully: when the endpoint rejects
//!     the `tools` field, the driver retries once without tools.
//!   * SSE folding buffers partial lines, skips non-data lines, merges
//!     indexed tool deltas, then recovers text-form tool calls for small
//!     local models that emit calls as prose.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::llm::{resolve_model, fallback::complete_with_fallback};
//! use engine::agent::client::GenParams;
//! # async fn demo(http: reqwest::Client) {
//! let specs = vec![resolve_model("openai:gpt-4o-mini")];
//! let msg = complete_with_fallback(&http, &specs, "sys", &[], &[], &GenParams::default(), 2, "sess-1").await;
//! # }
//! ```
//! ============================================================================

use crate::agent::client::GenParams;
use crate::agent::message::{AgentMessage, AssistantMessage, ContentBlock, Usage};
use futures::StreamExt;
use serde_json::json;

use crate::llm::compat::{map_finish, parse_sse_data, recover_text_tool_call, to_wire_messages};
use crate::llm::spec::{is_transient_error, ModelSpec};

/// Try each spec in order, retrying transient errors (errors as VALUES).
///
/// # Description
/// Never returns `Err`: total failure yields an error assistant message so
/// the engine stops cleanly instead of tearing down the session. Retries
/// happen pre-stream, so they never duplicate visible output. Backoff is
/// 400ms times 2^attempt, capped at shift 5.
///
/// # Example
/// ```rust,no_run
/// use engine::llm::{resolve_model, fallback::complete_with_fallback};
/// use engine::agent::client::GenParams;
/// # async fn demo(http: reqwest::Client) {
/// let specs = vec![resolve_model("openai:gpt-4o-mini")];
/// let m = complete_with_fallback(&http, &specs, "s", &[], &[], &GenParams::default(), 1, "sess-1").await;
/// # }
/// ```
#[allow(clippy::too_many_arguments)] // driver takes the full call context; bundling would churn all callers
pub async fn complete_with_fallback(
    http: &reqwest::Client,
    specs: &[ModelSpec],
    system: &str,
    messages: &[AgentMessage],
    tools: &[serde_json::Value],
    params: &GenParams,
    max_retries: u32,
    session_key: &str,
) -> AssistantMessage {
    if specs.is_empty() {
        return AssistantMessage::failure("no model specs configured");
    }
    let mut last_err = String::from("unknown error");
    for spec in specs {
        for attempt in 0..=max_retries {
            match stream_once(http, spec, system, messages, tools, params, session_key).await {
                Ok(msg) => return msg,
                Err(e) => {
                    last_err = e.to_string();
                    if !is_transient_error(&last_err) {
                        break; // fatal for THIS spec → try next spec immediately
                    }
                    let shift = attempt.min(5);
                    tokio::time::sleep(std::time::Duration::from_millis(400 * (1 << shift))).await;
                }
            }
        }
    }
    AssistantMessage::failure(format!("all models failed; last error: {last_err}"))
}

/// One SSE streaming attempt against one spec.
///
/// # Description
/// POSTs `{model, messages:[system, ...history], tools?, temperature?...}`
/// with `stream:true`, then folds `data:` lines into text/thinking/tool
/// accumulators until `[DONE]`. Tool-less providers degrade gracefully: when
/// the endpoint rejects `tools`, we retry once without them.
async fn stream_once(
    http: &reqwest::Client,
    spec: &ModelSpec,
    system: &str,
    messages: &[AgentMessage],
    tools: &[serde_json::Value],
    params: &GenParams,
    session_key: &str,
) -> anyhow::Result<AssistantMessage> {
    let wire = to_wire_messages(messages);
    let wire_tools: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            json!({"type": "function",
                   "function": {"name": t.get("name"), "description": t.get("description"),
                                "parameters": t.get("parameters").unwrap_or(&json!({"type":"object"}))}})
        })
        .collect();

    let mut messages = vec![json!({"role":"system","content": system})];
    messages.extend(wire);
    let mut body = json!({
        "model": spec.model_id,
        "stream": true,
        "messages": messages,
    });
    if !wire_tools.is_empty() {
        body["tools"] = json!(wire_tools);
        body["tool_choice"] = json!("auto");
    }
    if let Some(t) = params.temperature {
        body["temperature"] = json!(t);
    }
    if let Some(m) = params.max_tokens {
        body["max_tokens"] = json!(m);
    }
    if let Some(p) = params.top_p {
        body["top_p"] = json!(p);
    }

    let url = format!("{}/chat/completions", spec.base_url.trim_end_matches('/'));
    let mut req = http.post(&url).json(&body);
    if !spec.api_key.is_empty() {
        req = req.bearer_auth(&spec.api_key);
    }
    // Gateway accounts expect a stable session header (cache plus abuse
    // monitoring), so gateway traffic carries an identifier.
    if spec.base_url.contains("opencode.ai") {
        req = req.header("x-opencode-session", session_key);
    }
    let resp = req.send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        // Graceful degradation: provider without tool support → retry bare.
        if status.as_u16() == 400
            && text.to_lowercase().contains("tool")
            && body.get("tools").is_some()
        {
            body.as_object_mut().unwrap().remove("tools");
            body.as_object_mut().unwrap().remove("tool_choice");
            let mut req2 = http.post(&url).json(&body);
            if !spec.api_key.is_empty() {
                req2 = req2.bearer_auth(&spec.api_key);
            }
            if spec.base_url.contains("opencode.ai") {
                req2 = req2.header("x-opencode-session", session_key);
            }
            let resp2 = req2.send().await?;
            if !resp2.status().is_success() {
                anyhow::bail!(
                    "{} {}",
                    resp2.status(),
                    resp2.text().await.unwrap_or_default()
                );
            }
            return collect_sse(resp2, &spec.model_id).await;
        }
        anyhow::bail!("{status} {text}");
    }
    collect_sse(resp, &spec.model_id).await
}

async fn collect_sse(resp: reqwest::Response, model_id: &str) -> anyhow::Result<AssistantMessage> {
    let mut acc_text = String::new();
    let mut acc_think = String::new();
    let mut tool_acc: std::collections::HashMap<u32, (String, String, String)> = Default::default();
    let mut usage = Usage::default();
    let mut finish: Option<String> = None;
    let mut buf: Vec<u8> = vec![];
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buf.extend_from_slice(&chunk);
        // Split complete lines; keep the partial tail buffered.
        loop {
            let pos = buf.iter().position(|&b| b == b'\n');
            let Some(p) = pos else { break };
            let line: Vec<u8> = buf.drain(..=p).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim();
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                buf.clear();
                break;
            }
            if data.is_empty() {
                continue;
            }
            if let Some(f) = parse_sse_data(
                data,
                &mut acc_text,
                &mut acc_think,
                &mut tool_acc,
                &mut usage,
                model_id,
            ) {
                finish = Some(f);
            }
        }
    }
    // Native tool calls first: fold indexed deltas into ToolCall blocks.
    let mut idxs: Vec<u32> = tool_acc.keys().copied().collect();
    idxs.sort_unstable();
    let mut blocks = vec![];
    if !acc_think.is_empty() {
        blocks.push(ContentBlock::Thinking(acc_think));
    }
    for i in idxs {
        let (id, name, args_s) = tool_acc.remove(&i).unwrap();
        if name.is_empty() {
            continue;
        }
        let args: serde_json::Value = serde_json::from_str(&args_s).unwrap_or(json!({}));
        blocks.push(ContentBlock::ToolCall {
            id: if id.is_empty() {
                format!("call_{i}")
            } else {
                id
            },
            name,
            arguments: args,
        });
    }
    if !acc_text.is_empty() {
        blocks.push(ContentBlock::Text(acc_text.clone()));
    }
    // ...then text-form recovery for small local models.
    if blocks.iter().all(|b| matches!(b, ContentBlock::Text(_))) && !acc_text.trim().is_empty() {
        if let Some(recovered) = recover_text_tool_call(&acc_text, "call_text_0") {
            blocks = vec![recovered];
        }
    }
    let has_calls = blocks
        .iter()
        .any(|b| matches!(b, ContentBlock::ToolCall { .. }));
    Ok(AssistantMessage {
        content: blocks,
        stop_reason: map_finish(finish.as_deref(), has_calls),
        error_message: None,
        usage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::client::GenParams;
    use crate::agent::message::StopReason;

    #[tokio::test]
    async fn empty_specs_fail_as_value_not_err() {
        let http = reqwest::Client::new();
        let m = complete_with_fallback(
            &http,
            &[],
            "s",
            &[],
            &[],
            &GenParams::default(),
            0,
            "test-session",
        )
        .await;
        assert_eq!(m.stop_reason, StopReason::Error);
    }
}
