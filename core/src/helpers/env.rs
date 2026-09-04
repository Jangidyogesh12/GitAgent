//! ============================================================================
//! Module: engine::helpers::env
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Ports `src/env-utils.ts` + the `.env` loading in `src/index.ts` from the
//!   TypeScript original. Two jobs:
//!     1. Load `KEY=VALUE` files (global `~/.gitagent/.env`, then agent `.env`)
//!        into the process environment (later source wins).
//!     2. Expand `${VAR_NAME}` placeholders inside arbitrary strings and
//!        inside `serde_json::Value` trees (used for MCP + plugin configs).
//!
//! DESIGN PATTERNS USED:
//!   * Strategy — the caller injects a *lookup strategy* (`&dyn Fn(&str)`)
//!     so tests can pass a fake map while production passes `std::env::var`.
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `interpolate_env()`     — expand `${VAR}` in one string.
//!   * `interpolate_value()`   — recursively expand `${VAR}` in JSON values.
//!   * `load_dotenv_file()`    — parse + install one `.env` file.
//!   * `load_env_stack()`      — load global env, then agent-local env.
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::helpers::env::{interpolate_env, load_dotenv_file};
//! use std::collections::HashMap;
//! let mut m = HashMap::new();
//! m.insert("PORT".to_string(), "8080".to_string());
//! assert_eq!(interpolate_env("http://x:${PORT}", &|k| m.get(k).cloned()), "http://x:8080");
//! ```
//! ============================================================================

use anyhow::Context;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

fn var_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").expect("valid regex"))
}

/// Expand every `${VAR_NAME}` placeholder in `input`.
///
/// # Description
/// Looks each variable up with `lookup`. Missing variables become `""`
/// (mirrors the TypeScript behaviour, which also warns — the warning is left
/// to the caller so this stays a pure function).
///
/// # Example
/// ```rust
/// use engine::helpers::env::interpolate_env;
/// let s = interpolate_env("a=${A} b=${MISSING}", &|k| if k == "A" { Some("1".into()) } else { None });
/// assert_eq!(s, "a=1 b=");
/// ```
pub fn interpolate_env(input: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    var_pattern()
        .replace_all(input, |caps: &regex::Captures| {
            lookup(&caps[1]).unwrap_or_default()
        })
        .into_owned()
}

/// Recursively expand `${VAR}` placeholders inside a JSON value.
///
/// # Description
/// Walks objects/arrays and rewrites every string leaf with
/// [`interpolate_env`]. Non-string leaves pass through untouched.
///
/// # Example
/// ```rust
/// use engine::helpers::env::interpolate_value;
/// use serde_json::json;
/// let v = interpolate_value(&json!({"url": "http://${H}/x"}), &|_| Some("h".into()));
/// assert_eq!(v["url"], "http://h/x");
/// ```
pub fn interpolate_value(
    value: &serde_json::Value,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => serde_json::Value::String(interpolate_env(s, lookup)),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(|v| interpolate_value(v, lookup)).collect())
        }
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), interpolate_value(v, lookup)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Parse one `KEY=VALUE` file and install the pairs into the process env.
///
/// # Description
/// Hand-rolled parser mirroring `src/index.ts`: skips blank lines and `#`
/// comments, strips matching single/double quotes. Existing variables are
/// overwritten (later source wins).
///
/// Returns the number of keys installed.
///
/// # Example
/// ```rust,no_run
/// use engine::helpers::env::load_dotenv_file;
/// use std::path::Path;
/// let n = load_dotenv_file(Path::new("/tmp/demo/.env")).unwrap();
/// ```
pub fn load_dotenv_file(path: &Path) -> anyhow::Result<usize> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading dotenv file {}", path.display()))?;
    Ok(install_dotenv_text(&text))
}

/// Parse dotenv text and install into process env. Pure helper, testable.
///
/// # Description
/// Split into its own function so unit tests do not touch the real process
/// environment more than necessary (they still do — env is global — but the
/// parsing itself is validated through the returned map in tests via
/// [`parse_dotenv`]).
pub fn install_dotenv_text(text: &str) -> usize {
    let map = parse_dotenv(text);
    let n = map.len();
    for (k, v) in map {
        std::env::set_var(k, v);
    }
    n
}

/// Parse dotenv text into a map without touching the environment.
///
/// # Description
/// Pure function — easy to unit test. Handles `export KEY=...` prefixes,
/// quotes, and `#` comments the same way the loader does.
///
/// # Example
/// ```rust
/// use engine::helpers::env::parse_dotenv;
/// let m = parse_dotenv("A=1\n# comment\nB=\"hi there\"\n");
/// assert_eq!(m["A"], "1");
/// assert_eq!(m["B"], "hi there");
/// ```
pub fn parse_dotenv(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some(eq) = line.find('=') else { continue };
        let key = line[..eq].trim().to_string();
        if key.is_empty() {
            continue;
        }
        let mut val = line[eq + 1..].trim().to_string();
        // Strip inline comments only when outside quotes (simple scan).
        if !(val.starts_with('"') || val.starts_with('\'')) {
            if let Some(hash) = val.find(" #") {
                val.truncate(hash);
                val = val.trim_end().to_string();
            }
        }
        if val.len() >= 2
            && ((val.starts_with('"') && val.ends_with('"'))
                || (val.starts_with('\'') && val.ends_with('\'')))
        {
            val = val[1..val.len() - 1].to_string();
        }
        out.insert(key, val);
    }
    out
}

/// Load the global `~/.gitagent/.env` and then `<agent_dir>/.env`.
///
/// # Description
/// Mirrors the CLI boot order in `src/index.ts`: global first, agent-local
/// second so the agent file wins. Missing files are silently skipped.
/// Returns `(global_loaded, local_loaded)`.
///
/// # Example
/// ```rust,no_run
/// use engine::helpers::env::load_env_stack;
/// use std::path::Path;
/// let (g, l) = load_env_stack(Path::new("./my-agent"));
/// ```
pub fn load_env_stack(agent_dir: &Path) -> (bool, bool) {
    let home = std::env::var("HOME").unwrap_or_default();
    let global = Path::new(&home).join(".gitagent/.env");
    let g = global.is_file() && load_dotenv_file(&global).is_ok();
    let local = agent_dir.join(".env");
    let l = local.is_file() && load_dotenv_file(&local).is_ok();
    (g, l)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolates_known_and_blanks_unknown() {
        let s = interpolate_env("x=${A}-${ZZZ}", &|k| {
            if k == "A" {
                Some("1".into())
            } else {
                None
            }
        });
        assert_eq!(s, "x=1-");
    }

    #[test]
    fn parses_quotes_and_comments() {
        let m = parse_dotenv("A=1\n# c\nB=\"hi there\"\nC='x'\nexport D=4\n");
        assert_eq!(m["A"], "1");
        assert_eq!(m["B"], "hi there");
        assert_eq!(m["C"], "x");
        assert_eq!(m["D"], "4");
    }

    #[test]
    fn interpolates_nested_json() {
        let v = serde_json::json!({"a": ["${K}"], "b": 1});
        let out = interpolate_value(&v, &|_| Some("v".into()));
        assert_eq!(out["a"][0], "v");
        assert_eq!(out["b"], 1);
    }
}
