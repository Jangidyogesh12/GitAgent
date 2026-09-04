//! ============================================================================
//! Module: engine::helpers::text
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Text budgeting helpers ported from `src/tools/shared.ts` (`truncateOutput`),
//!   `src/tool-factory.ts` (50k truncation) and `src/compact.ts` (token math):
//!   keep giant tool outputs / histories inside LLM context windows.
//!
//! DESIGN PATTERNS USED:
//!   * Strategy — three interchangeable truncation strategies (keep head,
//!     keep tail, keep both ends) selected by the caller per situation.
//!
//! CONSTANTS (mirror the TypeScript originals):
//!   * `MAX_OUTPUT_CHARS` = 100_000 — cli/declarative output cap (tail kept).
//!   * `MAX_READ_LINES`   = 2000     — read tool default page size.
//!   * `MAX_READ_BYTES`   = 100_000  — read tool page byte cap.
//!   * `FACTORY_TRUNCATE` = 50_000   — tool-factory result cap (head kept).
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `estimate_tokens()` — `ceil(chars/4)` heuristic (matches compact.ts).
//!   * `truncate_tail()`   — keep the LAST n chars (for shell output).
//!   * `truncate_head()`   — keep the FIRST n chars (for tool results).
//!   * `truncate_middle()` — keep head+tail with a `...` marker (for history).
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::helpers::text::{estimate_tokens, truncate_tail, MAX_OUTPUT_CHARS};
//! assert!(estimate_tokens("abcd") >= 1);
//! assert!(truncate_tail("hello world", 5).ends_with("world"));
//! ```
//! ============================================================================

/// Max tool output chars sent to the LLM (~100KB). Mirrors `MAX_OUTPUT`.
pub const MAX_OUTPUT_CHARS: usize = 100_000;
/// Default page size of the `read` tool. Mirrors `MAX_LINES`.
pub const MAX_READ_LINES: usize = 2000;
/// Byte cap of one `read` page. Mirrors `MAX_BYTES`.
pub const MAX_READ_BYTES: usize = 100_000;
/// Tool-factory result cap. Mirrors `buildTool()` truncation (50k).
pub const FACTORY_TRUNCATE: usize = 50_000;

/// Rough token estimate: `ceil(chars / 4)`.
///
/// # Description
/// Same heuristic as `compact.ts`/`context.ts` in TypeScript. Deliberately
/// pessimistic so compaction triggers early rather than late.
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
/// Prefixes a `[output truncated, showing last ~N chars]` marker when
/// truncation happened — same marker text family as `cli.ts`.
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

/// Keep `head` + `tail` halves with an omission marker (for long histories).
///
/// # Description
/// Mirrors the 10k tool-result compaction in `compact.ts` (half head, half
/// tail). `budget` is split evenly between both ends.
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
    fn token_math_matches_ts_heuristic() {
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
