//! ============================================================================
//! Module: engine::helpers::jsonl
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Append-only `.jsonl` writers. Ports `src/audit.ts` (audit.jsonl),
//!   `src/chat-history.ts` (chat-history/<branch>.jsonl) and
//!   `src/schedule-runner.ts` (schedule-logs) — all three are "append one JSON
//!   object per line" stores in the TypeScript original.
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `append_jsonl()`  — serialise `value` + append one line (creates file).
//!   * `read_jsonl()`    — read back all non-empty lines as JSON values.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::helpers::jsonl::{append_jsonl, read_jsonl};
//! use std::path::Path;
//! append_jsonl(Path::new("/tmp/a.jsonl"), &serde_json::json!({"t": 1})).unwrap();
//! ```
//! ============================================================================

use anyhow::{Context, Result};
use std::path::Path;

/// Append one JSON value as a single line to `path`.
///
/// # Description
/// Creates parent directories and the file itself on demand. File is opened
/// in append mode so concurrent writers only ever add lines (same guarantee
/// the TS `fs.appendFile` gave the audit logger).
///
/// # Example
/// ```rust,no_run
/// use engine::helpers::jsonl::append_jsonl;
/// use std::path::Path;
/// append_jsonl(Path::new("/tmp/log.jsonl"), &serde_json::json!({"ev": "start"})).unwrap();
/// ```
pub fn append_jsonl(path: &Path, value: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating dirs for {}", path.display()))?;
        }
    }
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    writeln!(f, "{}", serde_json::to_string(value)?)?;
    Ok(())
}

/// Read every non-empty line of a `.jsonl` file as JSON.
///
/// # Description
/// Missing file → empty vec (callers treat "no history yet" as normal).
/// Malformed lines are skipped, not fatal — mirrors the fail-soft TS readers.
///
/// # Example
/// ```rust,no_run
/// use engine::helpers::jsonl::read_jsonl;
/// use std::path::Path;
/// let rows = read_jsonl(Path::new("/tmp/log.jsonl")).unwrap();
/// ```
pub fn read_jsonl(path: &Path) -> Result<Vec<serde_json::Value>> {
    if !path.exists() {
        return Ok(vec![]);
    }
    let text = std::fs::read_to_string(path)?;
    Ok(text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_lines() {
        let dir = std::env::temp_dir().join(format!("ga-test-{}", std::process::id()));
        let p = dir.join("a.jsonl");
        append_jsonl(&p, &serde_json::json!({"a": 1})).unwrap();
        append_jsonl(&p, &serde_json::json!({"b": 2})).unwrap();
        let rows = read_jsonl(&p).unwrap();
        assert_eq!(rows.len(), 2);
        std::fs::remove_dir_all(&dir).ok();
    }
}
