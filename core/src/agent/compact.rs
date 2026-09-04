//! ============================================================================
//! Module: engine::agent::compact
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Context-window budgeting. Ports `src/compact.ts` (token estimation,
//!   `needsCompaction` > 75% rule, tool-result truncation) AND fixes its
//!   biggest flaw: in TS `compact.ts` exported helpers but nothing called
//!   them in the loop. Here the `Compactor` is a first-class loop seam
//!   (`LoopConfig.compactor`), applied to the messages SENT to the model.
//!
//! DESIGN PATTERNS USED:
//!   * Strategy — the loop calls `compact()` polymorphically; this file is
//!     the default "truncate middle of oversized tool results" strategy
//!     (LLM-summarisation lives one layer up, in the SDK, like TS).
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `Compactor` — `new(context_window)`; `needs_compaction()`;
//!     `compact()`; `estimate()`.
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::agent::Compactor;
//! use engine::agent::message::AgentMessage;
//! let c = Compactor::new(200_000);
//! assert!(!c.needs_compaction(&[AgentMessage::User("hi".into())]));
//! ```
//! ============================================================================

use crate::agent::message::AgentMessage;

/// Budgets the context window (default 200k like `compact.ts`).
///
/// # Description
/// `budget = 75% of window` mirrors the TS `needsCompaction` ratio (0.75).
/// `compact()` never DROPS a message — it truncates oversized tool-result
/// contents in place, because dropping a ToolResult orphans its ToolCall and
/// providers answer 400 (the pairing invariant from `rust/gitagent-rs`).
#[derive(Debug, Clone)]
pub struct Compactor {
    /// Full context window of the model.
    pub context_window: usize,
    /// Usable budget = 75% of the window.
    pub budget: usize,
    /// Per-tool-result truncation cap (mirrors the TS 10k rule).
    pub tool_result_cap: usize,
}

impl Compactor {
    /// Create a compactor for a `context_window`-sized model.
    ///
    /// # Example
    /// ```rust
    /// use engine::agent::Compactor;
    /// let c = Compactor::new(200_000);
    /// assert_eq!(c.budget, 150_000);
    /// ```
    pub fn new(context_window: usize) -> Self {
        Self {
            context_window,
            budget: (context_window as f64 * 0.75) as usize,
            tool_result_cap: 10_000,
        }
    }

    /// Rough token estimate (`chars/4`, same heuristic as TS `compact.ts`).
    ///
    /// # Example
    /// ```rust
    /// use engine::agent::Compactor;
    /// assert_eq!(Compactor::estimate("abcd"), 1);
    /// ```
    pub fn estimate(text: &str) -> usize {
        text.chars().count().div_ceil(4)
    }

    fn transcript_tokens(&self, messages: &[AgentMessage]) -> usize {
        messages
            .iter()
            .map(|m| match m {
                AgentMessage::User(t) => Self::estimate(t),
                AgentMessage::Assistant(a) => {
                    Self::estimate(&a.text()) + a.tool_calls().len() * 100
                }
                AgentMessage::ToolResult(r) => Self::estimate(&r.content),
            })
            .sum()
    }

    /// True when the transcript exceeds 75% of the context window.
    ///
    /// # Example
    /// ```rust
    /// use engine::agent::{Compactor, AgentMessage};
    /// let c = Compactor::new(100);
    /// assert!(c.needs_compaction(&[AgentMessage::User("x".repeat(1000))]));
    /// ```
    pub fn needs_compaction(&self, messages: &[AgentMessage]) -> bool {
        self.transcript_tokens(messages) > self.budget
    }

    /// Truncate oversized tool results (head+tail kept), never drop messages.
    ///
    /// # Description
    /// Returns the (possibly rewritten) message list actually sent to the
    /// model. The stored transcript is untouched — the loop keeps the full
    /// history while the model sees the budgeted view.
    ///
    /// # Example
    /// ```rust
    /// use engine::agent::{Compactor, AgentMessage, ToolResultMessage};
    /// let c = Compactor::new(200_000);
    /// let msgs = vec![AgentMessage::User("hi".into())];
    /// assert_eq!(c.compact(&msgs).len(), 1);
    /// ```
    pub fn compact(&self, messages: &[AgentMessage]) -> Vec<AgentMessage> {
        if !self.needs_compaction(messages) {
            return messages.to_vec();
        }
        messages
            .iter()
            .map(|m| match m {
                AgentMessage::ToolResult(r) if r.content.chars().count() > self.tool_result_cap => {
                    let half = self.tool_result_cap / 2;
                    let head: String = r.content.chars().take(half).collect();
                    let n = r.content.chars().count();
                    let tail: String = r.content.chars().skip(n - half).collect();
                    let mut r2 = r.clone();
                    r2.content = format!("{head}\n...[compacted {n} chars]...\n{tail}");
                    AgentMessage::ToolResult(r2)
                }
                other => other.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::message::ToolResultMessage;

    #[test]
    fn small_transcript_needs_nothing() {
        let c = Compactor::new(200_000);
        assert!(!c.needs_compaction(&[AgentMessage::User("hi".into())]));
    }

    #[test]
    fn oversized_tool_result_is_truncated_not_dropped() {
        let c = Compactor::new(100);
        let big = "y".repeat(50_000);
        let msgs = vec![AgentMessage::ToolResult(ToolResultMessage {
            tool_call_id: "1".into(),
            tool_name: "cli".into(),
            content: big,
            is_error: false,
        })];
        assert!(c.needs_compaction(&msgs));
        let out = c.compact(&msgs);
        assert_eq!(out.len(), 1);
    }
}
