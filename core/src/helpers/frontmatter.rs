//! ============================================================================
//! Module: engine::helpers::frontmatter
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Parse `---`-fenced markdown files (skill docs, workflows, sub-agent
//!   definitions) into a YAML metadata block plus a body string. Only the
//!   first fence pair is consumed; files without a leading fence return no
//!   metadata and the whole text as body.
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `split_frontmatter()` — split raw text into (yaml, body).
//!   * `parse_frontmatter()`  — split plus deserialise the yaml into `T`.
//!
//! HOW IT WORKS:
//!   * Line scan: first line must trim to `---`; collects until the next
//!     fence line, then joins the remainder as body. Unterminated fences
//!     fall back to no-metadata. Typed parsing errors on missing blocks or
//!     invalid YAML; callers accepting fence-less files use the splitter.
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::helpers::frontmatter::parse_frontmatter;
//! use serde::Deserialize;
//! #[derive(Deserialize)] struct M { name: String }
//! let (meta, body) = parse_frontmatter::<M>("---\nname: x\n---\nhello").unwrap();
//! assert_eq!(meta.name, "x"); assert_eq!(body, "hello");
//! ```
//! ============================================================================

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;

/// Split `---`-fenced frontmatter from the markdown body.
///
/// # Description
/// Returns `(Some(yaml), body)` when the file starts with `---`, else
/// `(None, whole_text)`. Only the FIRST fence pair is consumed.
///
/// # Example
/// ```rust
/// use engine::helpers::frontmatter::split_frontmatter;
/// let (fm, body) = split_frontmatter("---\na: 1\n---\nrest");
/// assert_eq!(fm.unwrap().trim(), "a: 1");
/// assert_eq!(body, "rest");
/// ```
pub fn split_frontmatter(text: &str) -> (Option<String>, String) {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (None, text.to_string());
    }
    let mut yaml = String::new();
    for line in lines.by_ref() {
        if line.trim() == "---" {
            let body: Vec<&str> = lines.collect();
            return (Some(yaml), body.join("\n"));
        }
        yaml.push_str(line);
        yaml.push('\n');
    }
    (None, text.to_string())
}

/// Split frontmatter and deserialise it into `T`.
///
/// # Description
/// Errors when there is no frontmatter block OR the yaml is invalid —
/// callers that accept fm-less files should use [`split_frontmatter`].
///
/// # Example
/// ```rust
/// use engine::helpers::frontmatter::parse_frontmatter;
/// use serde::Deserialize;
/// #[derive(Deserialize)] struct M { name: String }
/// let (m, _) = parse_frontmatter::<M>("---\nname: demo\n---\nbody").unwrap();
/// assert_eq!(m.name, "demo");
/// ```
pub fn parse_frontmatter<T: DeserializeOwned>(text: &str) -> Result<(T, String)> {
    let (yaml, body) = split_frontmatter(text);
    let yaml = yaml.context("missing frontmatter block (expected leading ---)")?;
    let meta: T = serde_yaml::from_str(&yaml).context("parsing frontmatter yaml")?;
    Ok((meta, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct M {
        name: String,
    }

    #[test]
    fn splits_and_parses() {
        let (m, body) = parse_frontmatter::<M>("---\nname: demo\n---\nhello").unwrap();
        assert_eq!(m.name, "demo");
        assert_eq!(body.trim(), "hello");
    }

    #[test]
    fn no_fence_returns_none() {
        let (fm, body) = split_frontmatter("plain text");
        assert!(fm.is_none());
        assert_eq!(body, "plain text");
    }
}
