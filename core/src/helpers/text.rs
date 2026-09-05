//! ============================================================================
//! Module: engine::helpers::text
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Text budgeting helpers that keep giant tool outputs and histories
//!   inside model context windows. Three truncation directions plus a
//!   cheap token estimator share one char-based counting approach.
//!
//! DESIGN PATTERNS USED:
//!   * Strategy — three interchangeable truncation strategies (keep head,
//!     keep tail, keep both ends) selected by the caller per situation.
//!
//! CONSTANTS (budgets + defaults):
//!   * `MAX_OUTPUT_CHARS` = 100_000 — shell/script output cap (tail kept).
//!   * `MAX_READ_LINES`   = 2000     — read tool default page size.
//!   * `MAX_READ_BYTES`   = 100_000  — read tool page char cap.
//!   * `FACTORY_TRUNCATE` = 50_000   — generic tool-result cap (head kept).
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `estimate_tokens()` — `ceil(chars/4)` heuristic.
//!   * `truncate_tail()`   — keep the LAST n chars (for shell output).
//!   * `truncate_head()`   — keep the FIRST n chars (for tool results).
//!   * `truncate_middle()` — keep head plus tail with a marker (history).
//!
//! HOW IT WORKS (why each direction):
//!   * Tail for shell output: the most recent lines carry the exit status
//!     and error summary, so older head content is dropped first.
//!   * Head for tool results: the start usually holds the schema or lead
//!     answer, so the tail is dropped first.
//!   * Middle for history/compaction: both the opening context and the
//!     latest turn matter, so the budget splits evenly across both ends.
//!   * Token math `ceil(chars/4)` approximates one token per four chars;
//!     it deliberately over-estimates so budgeting triggers early.
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::helpers::text::{estimate_tokens, truncate_tail, MAX_OUTPUT_CHARS};
//! assert!(estimate_tokens("abcd") >= 1);
//! assert!(truncate_tail("hello world", 5).ends_with("world"));
//! ```
//! ============================================================================

/// Max tool output chars sent to the model (~100KB, tail kept).
pub const MAX_OUTPUT_CHARS: usize = 100_000;
/// Default page size of the `read` tool (lines per page).
pub const MAX_READ_LINES: usize = 2000;
/// Char cap of one `read` page (~100KB).
pub const MAX_READ_BYTES: usize = 100_000;
/// Generic tool-result cap (head kept, ~50KB).
pub const FACTORY_TRUNCATE: usize = 50_000;

/// Rough token estimate: `ceil(chars / 4)`.
///
/// # Description
/// Cheap char-based heuristic. Deliberately pessimistic so compaction
/// triggers early rather than late.
///
/// # Example
/// ```rust
/// use engine::helpers::text::estimate_tokens;
/// assert_eq!(estimate_tokens("abcd"), 1);
/// assert_eq!(estimate_tokens("abcde"), 2);
/// ```
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

/// Keep the LAST `max_chars` characters (for streaming shell output).
///
/// # Description
/// Prefixes a truncation marker naming the kept budget when content was
/// dropped, so the model knows it sees the tail only.
///
/// # Example
/// ```rust
/// use engine::helpers::text::truncate_tail;
/// assert_eq!(truncate_tail("abcdef", 3), "[output truncated, showing last ~3 chars]\n...def");
/// assert_eq!(truncate_tail("abc", 10), "abc");
/// ```
pub fn truncate_tail(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let tail: String = text.chars().skip(count - max_chars).collect();
    format!("[output truncated, showing last ~{max_chars} chars]\n...{tail}")
}

/// Keep the FIRST `max_chars` characters (for tool results).
///
/// # Example
/// ```rust
/// use engine::helpers::text::truncate_head;
/// assert!(truncate_head("abcdef", 3).starts_with("abc"));
/// ```
pub fn truncate_head(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars).collect();
    format!("{head}\n...[truncated, showing first ~{max_chars} chars]")
}

/// Keep `head` plus `tail` halves with an omission marker (long histories).
///
/// # Description
/// The `budget` is split evenly between both ends; the marker records the
/// total char count so the model can gauge how much was omitted.
///
/// # Example
/// ```rust
/// use engine::helpers::text::truncate_middle;
/// let s: String = "x".repeat(100);
/// let out = truncate_middle(&s, 20);
/// assert!(out.contains("omitted"));
/// ```
pub fn truncate_middle(text: &str, budget: usize) -> String {
    if text.chars().count() <= budget {
        return text.to_string();
    }
    let half = budget / 2;
    let head: String = text.chars().take(half).collect();
    let count = text.chars().count();
    let tail: String = text.chars().skip(count - half).collect();
    format!("{head}\n...[{count} chars total, middle omitted]...\n{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_math_uses_ceil_div_four() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
    }

    #[test]
    fn tail_keeps_end_and_marks() {
        let out = truncate_tail("abcdef", 3);
        assert!(out.ends_with("def") && out.contains("truncated"));
    }

    #[test]
    fn middle_keeps_both_ends() {
        let out = truncate_middle(&"x".repeat(100), 20);
        assert!(out.contains("omitted"));
    }
}
