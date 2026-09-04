//! ============================================================================
//! Module: engine::agent::message
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The conversation transcript model. Ports the `AgentMessage` family from
//!   pi-agent-core (which the TS `sdk.ts` event-mapping code consumes):
//!   user turns, assistant turns (text / thinking / tool calls), tool results,
//!   stop reasons, and token/cost usage.
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `StopReason`       — why the model stopped (Stop/Length/ToolUse/...).
//!   * `Usage`            — input/output/total tokens + USD cost.
//!   * `ContentBlock`     — Text | Thinking | ToolCall.
//!   * `AssistantMessage` — one assistant turn + `text()`/`tool_calls()`.
//!   * `ToolResultMessage`— tool_call_id + content + is_error flag.
//!   * `AgentMessage`     — User | Assistant | ToolResult (the transcript).
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::agent::message::{AgentMessage, AssistantMessage};
//! let m = AgentMessage::User("hello".into());
//! let a = AssistantMessage::text_reply("hi there");
//! assert!(a.text().contains("hi"));
//! ```
//! ============================================================================

use serde::{Deserialize, Serialize};

/// Why the model stopped generating this turn.
///
/// # Description
/// Mirrors pi-agent-core's stop reasons. `Length` triggers the bounded
/// auto-continue nudge in the loop; `Error`/`Aborted` stop cleanly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopReason {
    /// Natural end of turn (final answer, no tool calls).
    Stop,
    /// Hit the output token limit (loop nudges "continue").
    Length,
    /// Produced tool calls the loop must execute.
    ToolUse,
    /// Provider/transport failure (carried as data, never panicked).
    Error,
    /// Cancelled via abort handle.
    Aborted,
}

/// Token + cost accounting for one assistant turn.
///
/// # Description
/// Mirrors `Usage` in pi-agent-core; aggregated by `CostTracker` in the
/// observe crate. `cost_usd` is 0 when the provider reports no pricing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    /// Prompt tokens consumed.
    pub input: u64,
    /// Completion tokens generated.
    pub output: u64,
    /// input + output (convenience copy).
    pub total: u64,
    /// Estimated USD cost of this turn.
    pub cost_usd: f64,
}

impl Usage {
    /// Add another turn's usage into this accumulator.
    ///
    /// # Example
    /// ```rust
    /// use engine::agent::message::Usage;
    /// let mut a = Usage { input: 10, output: 5, total: 15, cost_usd: 0.1 };
    /// a.add(&Usage { input: 1, output: 1, total: 2, cost_usd: 0.01 });
    /// assert_eq!(a.total, 17);
    /// ```
    pub fn add(&mut self, other: &Usage) {
        self.input += other.input;
        self.output += other.output;
        self.total += other.total;
        self.cost_usd += other.cost_usd;
    }
}

/// One block inside an assistant message.
///
/// # Description
/// A turn can mix plain text, chain-of-thought (`Thinking`, from DeepSeek /
/// reasoning models), and any number of tool calls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ContentBlock {
    /// Visible answer text.
    Text(String),
    /// Reasoning trace (shown dimmed in the CLI, never sent as final text).
    Thinking(String),
    /// A request to run a tool: stable `id`, tool `name`, JSON `arguments`.
    ToolCall {
        /// Provider-assigned call id (echoed back in the ToolResult).
        id: String,
        /// Tool name as registered.
        name: String,
        /// Raw JSON arguments object.
        arguments: serde_json::Value,
    },
}

/// One assistant turn: blocks + stop reason + optional error + usage.
///
/// # Description
/// Constructors + accessors keep call sites readable; `failure()` builds the
/// error-as-value message the loop uses instead of throwing (a lesson ported
/// from the TS harness, where thrown provider errors tore down sessions).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssistantMessage {
    /// Ordered content blocks of this turn.
    pub content: Vec<ContentBlock>,
    /// Why generation stopped.
    pub stop_reason: StopReason,
    /// Human-readable error when `stop_reason == Error`.
    pub error_message: Option<String>,
    /// Token/cost usage of this turn.
    pub usage: Usage,
}

impl AssistantMessage {
    /// Build a plain text reply with `Stop` reason and empty usage.
    ///
    /// # Example
    /// ```rust
    /// use engine::agent::message::AssistantMessage;
    /// let m = AssistantMessage::text_reply("done");
    /// assert_eq!(m.text(), "done");
    /// ```
    pub fn text_reply(text: impl Into<String>) -> Self {
        Self {
            content: vec![ContentBlock::Text(text.into())],
            stop_reason: StopReason::Stop,
            error_message: None,
            usage: Usage::default(),
        }
    }

    /// Build an error turn (stop_reason = Error, no blocks).
    ///
    /// # Example
    /// ```rust
    /// use engine::agent::message::AssistantMessage;
    /// let m = AssistantMessage::failure("boom");
    /// assert!(m.error_message.is_some());
    /// ```
    pub fn failure(msg: impl Into<String>) -> Self {
        Self {
            content: vec![],
            stop_reason: StopReason::Error,
            error_message: Some(msg.into()),
            usage: Usage::default(),
        }
    }

    /// Concatenated visible text of this turn (ignores Thinking/ToolCall).
    ///
    /// # Example
    /// ```rust
    /// use engine::agent::message::AssistantMessage;
    /// assert_eq!(AssistantMessage::text_reply("a").text(), "a");
    /// ```
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text(t) => Some(t.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// All `(id, name, arguments)` tool calls requested this turn.
    ///
    /// # Example
    /// ```rust
    /// use engine::agent::message::{AssistantMessage, ContentBlock};
    /// let m = AssistantMessage::text_reply("x");
    /// assert!(m.tool_calls().is_empty());
    /// ```
    pub fn tool_calls(&self) -> Vec<(String, String, serde_json::Value)> {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolCall {
                    id,
                    name,
                    arguments,
                } => Some((id.clone(), name.clone(), arguments.clone())),
                _ => None,
            })
            .collect()
    }
}

/// The result of executing one tool call, fed back to the model.
///
/// # Description
/// `is_error` marks failures as *data* (the model sees "Error: ...") instead
/// of control flow — the TS `flattenToolResult` convention.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResultMessage {
    /// Must match the originating ToolCall `id`.
    pub tool_call_id: String,
    /// Tool name (for logging; pairing uses the id).
    pub tool_name: String,
    /// Text (or JSON-stringified) result content.
    pub content: String,
    /// True when the tool failed / was denied.
    pub is_error: bool,
}

/// One transcript entry: user turn, assistant turn, or tool result.
///
/// # Description
/// The transcript is the agent's memory of the session; the compactor may
/// *summarise* it for the model but (invariant) never drops entries, because
/// an orphaned tool_use without its tool_result makes providers return 400.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AgentMessage {
    /// Human (or system-injected follow-up) text.
    User(String),
    /// One assistant turn.
    Assistant(AssistantMessage),
    /// One tool execution result.
    ToolResult(ToolResultMessage),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_joins_only_text_blocks() {
        let m = AssistantMessage {
            content: vec![
                ContentBlock::Thinking("hmm".into()),
                ContentBlock::Text("a".into()),
                ContentBlock::Text("b".into()),
            ],
            stop_reason: StopReason::Stop,
            error_message: None,
            usage: Usage::default(),
        };
        assert_eq!(m.text(), "ab");
        assert!(m.tool_calls().is_empty());
    }

    #[test]
    fn usage_adds_up() {
        let mut u = Usage {
            input: 1,
            output: 2,
            total: 3,
            cost_usd: 0.5,
        };
        u.add(&Usage {
            input: 1,
            output: 1,
            total: 2,
            cost_usd: 0.5,
        });
        assert_eq!((u.total, u.cost_usd), (5, 1.0));
    }
}
