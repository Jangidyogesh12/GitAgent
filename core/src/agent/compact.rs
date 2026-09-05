//! ============================================================================
//! Module: engine::agent::compact
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Context-window budgeting for the agent loop. Estimates transcript
//!   tokens with a cheap char heuristic, flags transcripts past 75% of the
//!   window, and rewrites oversized tool results in place for the model
//!   view while the stored transcript stays complete. The `Compactor` is a
//!   first-class loop seam applied to messages SENT to the model.
//!
//! DESIGN PATTERNS USED:
//!   * Strategy — the loop calls `compact()` polymorphically; this file is
//!     the default head-plus-tail truncation strategy (model summarisation
//!     lives one layer up).
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `Compactor` — `new(context_window)`; `needs_compaction()`;
//!     `compact()`; `estimate()`.
//!
//! HOW IT WORKS:
//!   * Budget is 75% of the window; per-tool-result cap is 10_000 chars,
//!     split evenly across head and tail with a char-count marker.
//!   * Messages are never dropped: dropping a result would orphan its call
//!     and providers would reject the turn, so only content shrinks.
//!   * Token math is `ceil(chars/4)` plus a per-tool-call constant to
//!     account for call envelope overhead.
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

/// Budgets the context window (default budget 75% of window).
///
/// # Description
/// Usable budget is 75% of the window so headroom remains for the reply.
/// `compact()` never drops a message — it truncates oversized tool-result
/// contents in place, because dropping a result orphans its call and
/// providers reject the turn.
#[derive(Debug, Clone)]
pub struct Compactor {
    /// Full context window of the model.
    pub context_window: usize,
    /// Usable budget = 75% of the window.
    pub budget: usize,
    /// Per-tool-result truncation cap (10k chars, head plus tail kept).
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

    /// Rough token estimate (`chars/4` rounded up).
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
