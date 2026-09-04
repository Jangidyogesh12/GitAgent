//! ============================================================================
//! Module: engine::integrations::harness
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The `Harness` enum (the five supported external harnesses), per-harness
//!   metadata (id, label, config file, detection rule), environment-based
//!   detection (`detect_available()`), and the `adapter_for()` Factory.
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `Harness`            — OpenCode | NanoBot | OpenClaw | ClaudeCode | Lyzr.
//!   * `Harness::id()`      — CLI slug (`opencode`, `nanobot`, ...).
//!   * `Harness::label()`   — display name.
//!   * `Harness::config_file()` — primary native config file produced.
//!   * `Harness::parse()`   — slug → Harness.
//!   * `all_harnesses()`    — every variant (for `--list`).
//!   * `detect_available()` — env/binary heuristics → likely-present harnesses.
//!   * `adapter_for()`      — Factory → boxed HarnessAdapter.
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::integrations::{Harness, all_harnesses};
//! assert_eq!(Harness::parse("opencode"), Some(Harness::OpenCode));
//! assert_eq!(all_harnesses().len(), 5);
//! ```
//! ============================================================================

use crate::integrations::adapters::HarnessAdapter;

/// An external agent harness we interoperate with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Harness {
    /// sst/opencode — `opencode.json` config + instructions.
    OpenCode,
    /// NanoBot — `nanobot.yaml` agent file.
    NanoBot,
    /// OpenClaw — `openclaw.json` config.
    OpenClaw,
    /// Claude Code — `CLAUDE.md` + `.claude/settings.json`.
    ClaudeCode,
    /// Lyzr Studio — model backend (`lyzr:<id>@<base>`) + `.env.lyzr`.
    Lyzr,
}

impl Harness {
    /// CLI slug used in `gitagent integrations export --format <slug>`.
    ///
    /// # Example
    /// ```rust
    /// use engine::integrations::Harness;
    /// assert_eq!(Harness::OpenCode.id(), "opencode");
    /// ```
    pub fn id(&self) -> &'static str {
        match self {
            Harness::OpenCode => "opencode",
            Harness::NanoBot => "nanobot",
            Harness::OpenClaw => "openclaw",
            Harness::ClaudeCode => "claude-code",
            Harness::Lyzr => "lyzr",
        }
    }

    /// Human display name.
    ///
    /// # Example
    /// ```rust
    /// use engine::integrations::Harness;
    /// assert_eq!(Harness::Lyzr.label(), "Lyzr Studio");
    /// ```
    pub fn label(&self) -> &'static str {
        match self {
            Harness::OpenCode => "OpenCode",
            Harness::NanoBot => "NanoBot",
            Harness::OpenClaw => "OpenClaw",
            Harness::ClaudeCode => "Claude Code",
            Harness::Lyzr => "Lyzr Studio",
        }
    }

    /// Primary native config file this harness exports.
    ///
    /// # Example
    /// ```rust
    /// use engine::integrations::Harness;
    /// assert_eq!(Harness::ClaudeCode.config_file(), "CLAUDE.md");
    /// ```
    pub fn config_file(&self) -> &'static str {
        match self {
            Harness::OpenCode => "opencode.json",
            Harness::NanoBot => "nanobot.yaml",
            Harness::OpenClaw => "openclaw.json",
            Harness::ClaudeCode => "CLAUDE.md",
            Harness::Lyzr => ".env.lyzr",
        }
    }

    /// One-line description of the mapping (shown in `--list`).
    pub fn description(&self) -> &'static str {
        match self {
            Harness::OpenCode => "Export opencode.json (model + instructions + tools + mcp) so the agent runs under OpenCode.",
            Harness::NanoBot => "Export nanobot.yaml (agent, model, system prompt file, tools) for NanoBot runners.",
            Harness::OpenClaw => "Export openclaw.json (agents + model + skills) for OpenClaw.",
            Harness::ClaudeCode => "Export CLAUDE.md (SOUL/RULES/memory guidance) + .claude/settings.json permissions.",
            Harness::Lyzr => "Export .env.lyzr (LYZR_API_KEY + model string) to run the agent on a Lyzr Studio backend.",
        }
    }

    /// Parse a CLI slug (case-insensitive, `-`/`_` tolerant).
    ///
    /// # Example
    /// ```rust
    /// use engine::integrations::Harness;
    /// assert_eq!(Harness::parse("Claude_Code"), Some(Harness::ClaudeCode));
    /// assert_eq!(Harness::parse("nope"), None);
    /// ```
    pub fn parse(slug: impl AsRef<str>) -> Option<Self> {
        let n = slug.as_ref().to_lowercase().replace('_', "-");
        match n.as_str() {
            "opencode" => Some(Harness::OpenCode),
            "nanobot" => Some(Harness::NanoBot),
            "openclaw" | "open-claw" => Some(Harness::OpenClaw),
            "claude-code" | "claudecode" | "claude" => Some(Harness::ClaudeCode),
            "lyzr" => Some(Harness::Lyzr),
            _ => None,
        }
    }
}

/// Every supported harness (powers `integrations --list`).
///
/// # Example
/// ```rust
/// use engine::integrations::all_harnesses;
/// assert_eq!(all_harnesses().len(), 5);
/// ```
pub fn all_harnesses() -> Vec<Harness> {
    vec![
        Harness::OpenCode,
        Harness::NanoBot,
        Harness::OpenClaw,
        Harness::ClaudeCode,
        Harness::Lyzr,
    ]
}

/// Heuristic detection of harnesses likely present in this environment.
///
/// # Description
/// Checks well-known env vars / config files / binaries WITHOUT failing
/// when nothing is found (empty vec = "no harness detected"). Heuristics:
/// OpenCode (`OPENCODE_*` env or `opencode.json` in cwd or `opencode` on
/// PATH), NanoBot (`NANOBOT_*` / `nanobot` binary), OpenClaw
/// (`OPENCLAW_*` / `openclaw.json` / binary), Claude Code (`ANTHROPIC_*` /
/// `CLAUDE_CODE_*` / `claude` binary), Lyzr (`LYZR_API_KEY`).
///
/// # Example
/// ```rust,no_run
/// use engine::integrations::detect_available;
/// let found = detect_available(); // e.g. [Lyzr] when LYZR_API_KEY is set
/// ```
pub fn detect_available() -> Vec<Harness> {
    let mut out = vec![];
    let has_bin = |name: &str| {
        std::process::Command::new("which")
            .arg(name)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    if std::env::var("OPENCODE_API_KEY").is_ok()
        || std::env::var("OPENCODE_MODEL").is_ok()
        || std::path::Path::new("opencode.json").is_file()
        || has_bin("opencode")
    {
        out.push(Harness::OpenCode);
    }
    if std::env::var("NANOBOT_API_KEY").is_ok() || has_bin("nanobot") {
        out.push(Harness::NanoBot);
    }
    if std::env::var("OPENCLAW_API_KEY").is_ok()
        || std::env::var("OPENCLAW_STATE_DIR").is_ok()
        || std::path::Path::new("openclaw.json").is_file()
        || has_bin("openclaw")
    {
        out.push(Harness::OpenClaw);
    }
    if std::env::var("ANTHROPIC_API_KEY").is_ok()
        || std::env::var("CLAUDE_CODE_ENTRYPOINT").is_ok()
        || has_bin("claude")
    {
        out.push(Harness::ClaudeCode);
    }
    if std::env::var("LYZR_API_KEY").is_ok() {
        out.push(Harness::Lyzr);
    }
    out
}

/// Build the adapter for a harness (Factory pattern).
///
/// # Example
/// ```rust
/// use engine::integrations::{adapter_for, Harness};
/// let a = adapter_for(Harness::OpenCode);
/// assert_eq!(a.harness(), Harness::OpenCode);
/// ```
pub fn adapter_for(harness: Harness) -> Box<dyn HarnessAdapter> {
    match harness {
        Harness::OpenCode => Box::new(crate::integrations::adapters::OpenCodeAdapter),
        Harness::NanoBot => Box::new(crate::integrations::adapters::NanoBotAdapter),
        Harness::OpenClaw => Box::new(crate::integrations::adapters::OpenClawAdapter),
        Harness::ClaudeCode => Box::new(crate::integrations::adapters::ClaudeCodeAdapter),
        Harness::Lyzr => Box::new(crate::integrations::adapters::LyzrAdapter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_round_trip() {
        for h in all_harnesses() {
            assert_eq!(Harness::parse(h.id()), Some(h));
        }
        assert_eq!(Harness::parse("nope"), None);
    }
}
