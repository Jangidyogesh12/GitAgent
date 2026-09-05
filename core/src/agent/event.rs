//! ============================================================================
//! Module: engine::agent::event
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The Observer-pattern payload: every significant moment of the agent loop
//!   is published as an `AgentEvent` over a tokio mpsc channel. The CLI
//!   renders them (streaming deltas, tool previews); the SDK maps them to
//!   `SdkMessage`s. The loop never knows who is listening.
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `DeltaKind`  — Text vs Thinking streaming fragments.
//!   * `AgentEvent` — AgentStart/TurnStart/UserMessage/MessageDelta/
//!     MessageEnd/ToolExecutionStart/ToolExecutionEnd/TurnEnd/AgentEnd.
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::agent::event::{AgentEvent, DeltaKind};
//! let ev = AgentEvent::MessageDelta { kind: DeltaKind::Text, text: "hi".into() };
//! assert!(matches!(ev, AgentEvent::MessageDelta { .. }));
//! ```
//! ============================================================================

use crate::agent::message::AssistantMessage;
use serde::{Deserialize, Serialize};

/// Which stream a `MessageDelta` fragment belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeltaKind {
    /// Visible answer tokens.
    Text,
    /// Reasoning-trace tokens (render dimmed).
    Thinking,
}

/// Lifecycle events emitted by `run_loop()` (Observer pattern).
///
/// # Description
/// Fixed event vocabulary consumed by the CLI renderer and the SDK message
/// mapper: streaming deltas, tool start/end, turn boundaries.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Session started (carries nothing; CLI prints the banner instead).
    AgentStart,
    /// A new think→act iteration began (1-indexed turn number).
    TurnStart(u32),
    /// A user/follow-up message entered the transcript.
    UserMessage(String),
    /// One streaming fragment from the model.
    MessageDelta {
        /// Text vs reasoning stream.
        kind: DeltaKind,
        /// The fragment text.
        text: String,
    },
    /// The model finished one assistant turn (full message attached).
    MessageEnd(AssistantMessage),
    /// A tool call is about to run (CLI prints a preview from this).
    ToolExecutionStart {
        /// Stable call id (pairs with the End event).
        tool_call_id: String,
        /// Registered tool name.
        tool_name: String,
        /// Raw JSON args.
        args: serde_json::Value,
    },
    /// A tool call finished (result already appended to the transcript).
    ToolExecutionEnd {
        /// Pairs with the Start event.
        tool_call_id: String,
        /// Tool name (for display).
        tool_name: String,
        /// Result text (possibly truncated for display by the consumer).
        content: String,
        /// True when the tool failed / was denied.
        is_error: bool,
    },
    /// The think→act iteration finished.
    TurnEnd,
    /// The whole session finished (max turns / final answer / abort).
    AgentEnd,
}
