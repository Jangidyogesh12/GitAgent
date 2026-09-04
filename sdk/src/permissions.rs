//! ============================================================================
//! Module: sdk::permissions
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Claude-Code-style permission gate. Ports the DESIGN (not the protocol)
//!   of Claude Code's permissions plus `rust/gitagent-rs/.../permissions.rs`:
//!   modes Default | Plan | AcceptEdits | Bypass, and ordered rules
//!   `allow|deny:tool` or `allow|deny:tool(substring)` with `*` wildcards,
//!   matched against the `command`/`path`/`prompt` arg field else the whole
//!   JSON. Precedence: Bypass short-circuit → deny → allow → mode default;
//!   Plan blocks mutating tools (anything Sequential except read).
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `PermissionMode` — Default | Plan | AcceptEdits | Bypass.
//!   * `PermissionGate` — `new()` + `parse_rule()` + ToolGate impl.
//!
//! HOW TO USE (example):
//! ```rust
//! use sdk::{PermissionGate, PermissionMode};
//! let g = PermissionGate::new(PermissionMode::Default, &["deny:cli(rm -rf)".to_string()]);
//! ```
//! ============================================================================

use engine::agent::{GateDecision, ToolGate};

/// Policy mode (mirrors Claude Code's modes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    /// Ask-equivalent: allow reads, allow listed, deny listed (default).
    Default,
    /// Read-only planning: block all mutating (Sequential, non-read) tools.
    Plan,
    /// Auto-approve edits: reads + file writes allowed unless denied.
    AcceptEdits,
    /// Allow everything (deny rules still win — explicit beats blanket).
    Bypass,
}

/// One parsed rule: allow|deny × tool pattern × optional substring.
///
/// Returned by [`parse_rule`]; matched inside [`PermissionGate`].
#[derive(Debug, Clone)]
pub struct Rule {
    allow: bool,
    tool_pat: String,
    substr: Option<String>,
}

/// Claude-Code-style permission gate (Chain of Responsibility link).
pub struct PermissionGate {
    /// Active mode.
    pub mode: PermissionMode,
    /// Ordered rules (first match wins within allow/deny precedence).
    pub rules: Vec<Rule>,
}

impl PermissionGate {
    /// Build from a mode + raw rule strings (bad rules warn + skip).
    ///
    /// # Example
    /// ```rust
    /// use sdk::{PermissionGate, PermissionMode};
    /// let g = PermissionGate::new(PermissionMode::Plan, &[]);
    /// assert_eq!(g.mode, PermissionMode::Plan);
    /// ```
    pub fn new(mode: PermissionMode, raw_rules: &[String]) -> Self {
        let rules = raw_rules.iter().filter_map(|r| parse_rule(r)).collect();
        Self { mode, rules }
    }

    fn decide(&self, tool: &str, args: &serde_json::Value) -> GateDecision {
        // 1. Explicit deny rules first (even in Bypass — explicit beats blanket).
        for r in &self.rules {
            if !r.allow && rule_matches(r, tool, args) {
                return GateDecision::Deny(format!("permission rule denied {tool}"));
            }
        }
        // 2. Bypass allows the rest.
        if self.mode == PermissionMode::Bypass {
            return GateDecision::Allow;
        }
        // 3. Explicit allow rules.
        for r in &self.rules {
            if r.allow && rule_matches(r, tool, args) {
                return GateDecision::Allow;
            }
        }
        // 4. Mode default.
        match self.mode {
            PermissionMode::Plan if is_mutating(tool) => {
                GateDecision::Deny(format!("plan mode blocks mutating tool {tool}"))
            }
            _ => GateDecision::Allow,
        }
    }
}

/// Parse `allow:tool`, `deny:tool(sub)`, `tool` (allow shorthand).
///
/// # Example
/// ```rust
/// use sdk::permissions::parse_rule;
/// assert!(parse_rule("deny:cli(rm -rf)").is_some());
/// assert!(parse_rule("").is_none());
/// ```
pub fn parse_rule(raw: &str) -> Option<Rule> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let (allow, rest) = match raw.split_once(':') {
        Some(("allow", r)) => (true, r),
        Some(("deny", r)) => (false, r),
        _ => (true, raw),
    };
    let (tool_pat, substr) = match rest.find('(') {
        Some(i) => {
            let sub = rest[i + 1..].strip_suffix(')')?.to_string();
            (rest[..i].to_string(), Some(sub))
        }
        None => (rest.to_string(), None),
    };
    if tool_pat.is_empty() {
        return None;
    }
    Some(Rule {
        allow,
        tool_pat,
        substr,
    })
}

fn wildcard_match(pat: &str, text: &str) -> bool {
    // Tiny `*`-wildcard matcher (no regex dep needed here).
    if pat == "*" {
        return true;
    }
    let parts: Vec<&str> = pat.split('*').collect();
    if parts.len() == 1 {
        return pat == text;
    }
    let mut pos = 0;
    if !parts[0].is_empty() {
        if !text.starts_with(parts[0]) {
            return false;
        }
        pos = parts[0].len();
    }
    for (i, part) in parts.iter().enumerate().skip(1) {
        if part.is_empty() {
            continue;
        }
        match text[pos..].find(*part) {
            Some(j) => pos += j + part.len(),
            None => return false,
        }
        if i == parts.len() - 1 && !pat.ends_with('*') && pos != text.len() {
            return false;
        }
    }
    true
}

fn rule_matches(r: &Rule, tool: &str, args: &serde_json::Value) -> bool {
    if !wildcard_match(&r.tool_pat, tool) {
        return false;
    }
    match &r.substr {
        None => true,
        Some(sub) => {
            // Match against the most relevant arg field, else whole JSON.
            let hay = args
                .get("command")
                .or_else(|| args.get("path"))
                .or_else(|| args.get("prompt"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| args.to_string());
            hay.contains(sub.as_str())
        }
    }
}

fn is_mutating(tool: &str) -> bool {
    // Read-only tools pass Plan mode; everything else is "mutating".
    !matches!(tool, "read")
}

/// Manual `Future` return (no `async_trait`): downstream of the `core`
/// package, where that macro's `engine::…` paths would resolve to our crate
/// instead of std. `std::` paths used explicitly throughout.
impl ToolGate for PermissionGate {
    fn check<'life0, 'life1, 'life2, 'async_trait>(
        &'life0 self,
        tool_name: &'life1 str,
        args: &'life2 serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = GateDecision> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        Self: 'async_trait,
    {
        let verdict = self.decide(tool_name, args);
        Box::pin(async move { verdict })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_beats_bypass() {
        let g = PermissionGate::new(PermissionMode::Bypass, &["deny:cli(rm -rf)".into()]);
        assert!(matches!(
            g.decide("cli", &serde_json::json!({"command": "rm -rf /"})),
            GateDecision::Deny(_)
        ));
        assert!(matches!(
            g.decide("read", &serde_json::json!({})),
            GateDecision::Allow
        ));
    }

    #[test]
    fn plan_blocks_mutating() {
        let g = PermissionGate::new(PermissionMode::Plan, &[]);
        assert!(matches!(
            g.decide("write", &serde_json::json!({})),
            GateDecision::Deny(_)
        ));
        assert!(matches!(
            g.decide("read", &serde_json::json!({})),
            GateDecision::Allow
        ));
    }
}
