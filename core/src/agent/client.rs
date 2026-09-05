//! ============================================================================
//! Module: engine::agent::client
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The Strategy-pattern seam for LLM providers. The engine (`agent.rs`)
//!   calls `LlmClient::complete()`; `llm` plugs in the real
//!   OpenAI-compatible streaming implementation. This keeps `engine`
//!   provider-agnostic: one trait replaces all provider-specific registries.
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `LlmClient`   — async `complete(system, messages, tools, params)`.
//!   * `GenParams`   — temperature/max_tokens/top_p (model constraints).
//!   * `NoopClient`  — canned-reply client for unit tests (no network).
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::agent::{NoopClient, LlmClient, GenParams};
//! // NoopClient::new("hello") implements LlmClient with a canned reply.
//! ```
//! ============================================================================

use crate::agent::message::AssistantMessage;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Generation constraints (from `agent.yaml → model.constraints` or CLI).
///
/// # Description
/// All-optional so manifests only set what they need; `None` means "let the
/// provider decide". Covers temperature, max_tokens, top_p, top_k and stop
/// sequences (top_k / stop are passed through to providers that support them).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GenParams {
    /// Sampling temperature.
    pub temperature: Option<f64>,
    /// Max output tokens for one turn.
    pub max_tokens: Option<u64>,
    /// Nucleus sampling cutoff.
    pub top_p: Option<f64>,
}

/// A language-model backend (Strategy pattern).
///
/// # Description
/// One method by design: given the system prompt, the transcript so far, the
/// tool JSON-schemas and the params, produce the next assistant turn.
/// Streaming deltas are an llm-crate internal detail — the trait only sees
/// the finished turn (plus optional progress events via the returned message).
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Produce the next assistant turn. Errors are encoded as
    /// `AssistantMessage::failure(...)` by robust implementations, not `Err`.
    ///
    /// # Example
    /// ```rust,no_run
    /// use engine::agent::{LlmClient, NoopClient, GenParams};
    /// // async { NoopClient::new("hi").complete("s", &[], &[], &GenParams::default()).await };
    /// ```
    async fn complete(
        &self,
        system_prompt: &str,
        messages: &[crate::agent::message::AgentMessage],
        tools: &[serde_json::Value],
        params: &GenParams,
    ) -> AssistantMessage;
}

/// Canned-reply client for tests and offline demos (no network).
///
/// # Description
/// Always returns `text` as a `Stop` turn. Lets `run_loop()` be tested
/// without API keys — the same role `spawn_mock_llm` played in the
/// `rust/gitagent-rs/tests/slice.rs` harness.
#[derive(Debug, Clone)]
pub struct NoopClient {
    /// The text every turn returns.
    pub text: String,
}

impl NoopClient {
    /// Create a client that always replies `text`.
    ///
    /// # Example
    /// ```rust
    /// use engine::agent::NoopClient;
    /// let c = NoopClient::new("done");
    /// assert_eq!(c.text, "done");
    /// ```
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

#[async_trait]
impl LlmClient for NoopClient {
    async fn complete(
        &self,
        _system: &str,
        _messages: &[crate::agent::message::AgentMessage],
        _tools: &[serde_json::Value],
        _params: &GenParams,
    ) -> AssistantMessage {
        AssistantMessage::text_reply(self.text.clone())
    }
}
