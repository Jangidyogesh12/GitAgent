//! ============================================================================
//! Module: engine::helpers::fsx
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Small filesystem helpers shared by every crate: resolve `~/` and relative
//!   paths, read files lossily, write files creating parent dirs, and paginate
//!   long text outputs (mirrors `src/tools/shared.ts` + `src/tools/read.ts`).
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `resolve_path()`          — expand `~/`, resolve relative to a base dir.
//!   * `read_file_lossy()`       — read a file to String (lossy UTF-8).
//!   * `write_file_create_dirs()`— write a file, creating parents as needed.
//!   * `ensure_dir()`            — `mkdir -p` helper.
//!   * `paginate_lines()`        — 1-indexed offset/limit pagination w/ footer.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::helpers::fsx::{resolve_path, paginate_lines};
//! use std::path::Path;
//! let p = resolve_path(Path::new("."), "notes/a.md");
//! let big = (0..3000).map(|i| i.to_string()).collect::<Vec<_>>().join("\n");
//! let (page, footer) = paginate_lines(&big, 1, 2000).unwrap();
//! assert!(page.lines().count() <= 2000);
//! ```
//! ============================================================================

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Resolve a user-supplied path against `base_dir`.
///
/// # Description
/// Expands a leading `~/` to `$HOME`, leaves absolute paths untouched, and
/// joins relative paths onto `base_dir`. Mirrors `read.ts` path resolution.
///
/// # Example
/// ```rust
/// use engine::helpers::fsx::resolve_path;
/// use std::path::Path;
/// assert_eq!(resolve_path(Path::new("/base"), "/abs/x"), std::path::PathBuf::from("/abs/x"));
/// assert_eq!(resolve_path(Path::new("/base"), "rel/x"), std::path::PathBuf::from("/base/rel/x"));
/// ```
pub fn resolve_path(base_dir: &Path, raw: &str) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        return Path::new(&home).join(rest);
    }
    let p = Path::new(raw);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base_dir.join(p)
    }
}

/// Read a whole file into a String, replacing invalid UTF-8.
///
/// # Description
/// Uses lossy conversion so binary-ish files never crash the agent loop;
/// binary *detection* itself lives in the `read` tool (null-byte sniffing).
///
/// # Example
/// ```rust,no_run
/// use engine::helpers::fsx::read_file_lossy;
/// use std::path::Path;
/// let text = read_file_lossy(Path::new("SOUL.md")).unwrap();
/// ```
pub fn read_file_lossy(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Write `content` to `path`, creating parent directories first.
///
/// # Description
/// Mirrors `write.ts` (`mkdir -p` parents by default). Returns bytes written.
///
/// # Example
/// ```rust,no_run
/// use engine::helpers::fsx::write_file_create_dirs;
/// use std::path::Path;
/// write_file_create_dirs(Path::new("/tmp/demo/a/b.txt"), "hi").unwrap();
/// ```
pub fn write_file_create_dirs(path: &Path, content: &str) -> Result<usize> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating dirs for {}", path.display()))?;
        }
    }
    std::fs::write(path, content).with_context(|| format!("writing {}", path.display()))?;
    Ok(content.len())
}

/// Create a directory and all parents (no error if it exists).
///
/// # Example
/// ```rust,no_run
/// use engine::helpers::fsx::ensure_dir;
/// use std::path::Path;
/// ensure_dir(Path::new("/tmp/demo-dir")).unwrap();
/// ```
pub fn ensure_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).with_context(|| format!("creating dir {}", path.display()))?;
    Ok(())
}

/// Paginate text with 1-indexed `offset` and `limit`.
///
/// # Description
/// Mirrors `paginateLines()` in the TS tools: page defaults to 2000 lines,
/// errors when `offset` is past EOF, and returns a footer telling the model
/// how to continue (`footer` is empty when everything fit).
///
/// # Example
/// ```rust
/// use engine::helpers::fsx::paginate_lines;
/// let text = (1..=10).map(|i| format!("l{i}")).collect::<Vec<_>>().join("\n");
/// let (page, footer) = paginate_lines(&text, 1, 5).unwrap();
/// assert!(footer.contains("10"));
/// ```
pub fn paginate_lines(text: &str, offset: usize, limit: usize) -> Result<(String, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let total = lines.len();
    let start = offset.saturating_sub(1);
    if offset > total && total > 0 {
        anyhow::bail!("offset {} is beyond end of file ({} lines)", offset, total);
    }
    let end = (start + limit).min(total);
    let page = lines[start..end].join("\n");
    let footer = if end < total {
        format!(
            "[Showing lines {}-{} of {}. Use offset={} to continue.]",
            offset,
            end,
            total,
            end + 1
        )
    } else {
        String::new()
    };
    Ok((page, footer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paginates_with_footer() {
        let text = (1..=10)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let (page, footer) = paginate_lines(&text, 1, 4).unwrap();
        assert_eq!(page.lines().count(), 4);
        assert!(footer.contains("10"));
        let (page2, footer2) = paginate_lines(&text, 9, 10).unwrap();
        assert_eq!(page2.lines().count(), 2);
        assert!(footer2.is_empty());
    }

    #[test]
    fn rejects_offset_past_eof() {
        assert!(paginate_lines("a\nb", 99, 10).is_err());
    }
}
