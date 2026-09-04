//! ============================================================================
//! Module: engine::tools::edit
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The `edit` tool — surgical find-and-replace. Ports `src/tools/edit.ts`:
//!   rejects identical old/new + empty old_string + 0 matches + ambiguous
//!   multi-match without replace_all; `regex` mode via the `regex` crate
//!   (Rust equivalent of JS RegExp with identical group-reference syntax);
//!   `replace_all` uses split/join; final "identical content" guard.
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `EditTool` — `new(cwd)`; pure helper `apply_edit()`.
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::tools::edit::apply_edit;
//! let out = apply_edit("a b c", "b", "z", false, false, "").unwrap();
//! assert_eq!(out, ("a z c".to_string(), 1));
//! ```
//! ============================================================================

use crate::agent::{AgentTool, ExecutionMode, ToolOutput};
use async_trait::async_trait;
use std::path::PathBuf;

/// Surgical file-edit tool (Sequential — mutates files).
pub struct EditTool {
    /// Base dir for relative paths.
    pub cwd: PathBuf,
}

impl EditTool {
    /// Create the tool rooted at `cwd`.
    ///
    /// # Example
    /// ```rust
    /// use engine::tools::EditTool;
    /// use std::path::PathBuf;
    /// assert_eq!(EditTool::new(PathBuf::from(".")).name(), "edit");
    /// ```
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }

    /// Registry name (kept for tests).
    pub fn name(&self) -> &str {
        "edit"
    }
}

/// Apply one edit to `text`; returns (new_text, replacements).
///
/// # Description
/// Pure core of the tool (unit-testable without files). Rules mirror TS:
/// empty `old` → Err; identical old/new → Err; 0 matches → Err; >1 match
/// without `replace_all` → ambiguity Err; regex mode compiles `old` with
/// `flags` (`i`/`m`/`s` mapped; `g` implied by `replace_all`).
///
/// # Example
/// ```rust
/// use engine::tools::edit::apply_edit;
/// assert!(apply_edit("aaa", "a", "b", false, false, "").is_err()); // ambiguous
/// assert_eq!(apply_edit("aaa", "a", "b", true, false, "").unwrap().1, 3);
/// ```
pub fn apply_edit(
    text: &str,
    old: &str,
    new: &str,
    replace_all: bool,
    use_regex: bool,
    flags: &str,
) -> anyhow::Result<(String, usize)> {
    if old.is_empty() {
        anyhow::bail!("`old_string` must not be empty");
    }
    if old == new && !use_regex {
        anyhow::bail!("`old_string` and `new_string` are identical — nothing to do");
    }
    if use_regex {
        // Map JS-style flags onto regex-crate inline flags.
        let mut prefix = String::new();
        if flags.contains('i') {
            prefix.push_str("(?i)");
        }
        if flags.contains('m') {
            prefix.push_str("(?m)");
        }
        if flags.contains('s') {
            prefix.push_str("(?s)");
        }
        let pattern = format!("{prefix}{old}");
        let re = regex::Regex::new(&pattern).map_err(|e| anyhow::anyhow!("invalid regex: {e}"))?;
        let count = re.find_iter(text).count();
        if count == 0 {
            anyhow::bail!("no match found for pattern");
        }
        if count > 1 && !replace_all {
            anyhow::bail!(
                "pattern matches {count} times — set replace_all=true or narrow the pattern"
            );
        }
        // `$1`-style group refs work in the regex crate the same as JS.
        let out = if replace_all {
            re.replace_all(text, new).into_owned()
        } else {
            re.replace(text, new).into_owned()
        };
        if out == text {
            anyhow::bail!("replacement produced identical content");
        }
        Ok((out, if replace_all { count } else { 1 }))
    } else {
        let count = text.matches(old).count();
        if count == 0 {
            anyhow::bail!("`old_string` not found in file");
        }
        if count > 1 && !replace_all {
            anyhow::bail!("`old_string` matches {count} times — set replace_all=true or narrow it");
        }
        let out = if replace_all {
            text.replace(old, new)
        } else {
            text.replacen(old, new, 1)
        };
        if out == text {
            anyhow::bail!("replacement produced identical content");
        }
        Ok((out, if replace_all { count } else { 1 }))
    }
}

#[async_trait]
impl AgentTool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }
    fn description(&self) -> &str {
        "Surgical find-and-replace in a file (exact match, replace_all, or regex mode)"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to the file to edit"},
                "old_string": {"type": "string", "description": "Exact text to find (must be unique unless replace_all)"},
                "new_string": {"type": "string", "description": "Replacement text"},
                "replace_all": {"type": "boolean", "description": "Replace every occurrence (default: false)"},
                "regex": {"type": "boolean", "description": "Treat old_string as a regular expression"},
                "flags": {"type": "string", "description": "Regex flags, e.g. 'i', 'm', 's'"}
            },
            "required": ["path", "old_string", "new_string"]
        })
    }
    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Sequential
    }

    async fn execute(&self, _id: &str, args: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let raw = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let old = args
            .get("old_string")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let new = args
            .get("new_string")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let replace_all = args
            .get("replace_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let use_regex = args.get("regex").and_then(|v| v.as_bool()).unwrap_or(false);
        let flags = args.get("flags").and_then(|v| v.as_str()).unwrap_or("");
        if raw.trim().is_empty() {
            return Ok(ToolOutput::err("Error: `path` is required"));
        }
        let path = crate::helpers::resolve_path(&self.cwd, raw);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                return Ok(ToolOutput::err(format!(
                    "Error: cannot read {}: {e}",
                    path.display()
                )))
            }
        };
        match apply_edit(&text, old, new, replace_all, use_regex, flags) {
            Ok((out, n)) => match std::fs::write(&path, out) {
                Ok(()) => Ok(ToolOutput::ok(format!(
                    "Edited {} — {n} replacement(s) applied",
                    path.display()
                ))),
                Err(e) => Ok(ToolOutput::err(format!(
                    "Error: cannot write {}: {e}",
                    path.display()
                ))),
            },
            Err(e) => Ok(ToolOutput::err(format!("Error: {e:#}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_replace_works() {
        assert_eq!(
            apply_edit("a b c", "b", "B", false, false, "").unwrap().1,
            1
        );
    }

    #[test]
    fn ambiguous_without_flag_errors() {
        assert!(apply_edit("a a", "a", "b", false, false, "").is_err());
        assert_eq!(apply_edit("a a", "a", "b", true, false, "").unwrap().1, 2);
    }

    #[test]
    fn regex_mode_with_groups() {
        let (out, _) = apply_edit("foo 123", r"foo (\d+)", "bar $1", false, true, "").unwrap();
        assert_eq!(out, "bar 123");
    }
}
