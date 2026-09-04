//! ============================================================================
//! Module: engine::loader::prompt
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Ordered system-prompt assembly. Ports the section ORDER in `loadAgent()`
//!   (study.md §2.1): manifest header → SOUL → RULES → parent RULES → DUTIES
//!   → AGENTS.md → Memory → Knowledge → Skills → Workflows → Sub-Agents →
//!   Examples → Plugins → Compliance → Workspace → Task Learning. Sections
//!   join with "\n\n"; empty ones are skipped.
//!
//! DESIGN PATTERNS USED:
//!   * Builder — `PromptBuilder::new()` + `section()` chaining + `build()`.
//!     Each `section()` is a no-op for empty text, so callers never branch.
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `PromptBuilder` — `new()` / `section()` / `build()`.
//!   * `memory_block()`     — the hardcoded `# Memory` persona text.
//!   * `skills_block()`     — the MANDATORY skill-check block.
//!   * `workspace_block()`  — workspace-dir rules (+ cloud-mode variant).
//!   * `learning_block()`   — task_tracker / skill_learner instructions.
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::loader::PromptBuilder;
//! let p = PromptBuilder::new("demo", "0.1.0", "desc").section("# Extra").build();
//! assert!(p.contains("# demo"));
//! ```
//! ============================================================================

/// Ordered prompt accumulator (Builder pattern).
///
/// # Description
/// Starts with the `# {name} v{version}\n{description}` header (always
/// present, like TS). `section()` appends non-empty text; `build()` joins
/// everything with `"\n\n"`.
#[derive(Debug, Clone)]
pub struct PromptBuilder {
    parts: Vec<String>,
}

impl PromptBuilder {
    /// Start a prompt with the manifest header block.
    ///
    /// # Example
    /// ```rust
    /// use engine::loader::PromptBuilder;
    /// let p = PromptBuilder::new("demo", "0.1.0", "does things").build();
    /// assert!(p.starts_with("# demo v0.1.0"));
    /// ```
    pub fn new(name: &str, version: &str, description: &str) -> Self {
        Self {
            parts: vec![format!("# {name} v{version}\n{description}")],
        }
    }

    /// Append one section (ignored when blank). Chainable.
    ///
    /// # Example
    /// ```rust
    /// use engine::loader::PromptBuilder;
    /// let p = PromptBuilder::new("a", "1", "d").section("").section("## X").build();
    /// assert!(p.contains("## X"));
    /// ```
    pub fn section(mut self, text: impl Into<String>) -> Self {
        let t = text.into();
        if !t.trim().is_empty() {
            self.parts.push(t);
        }
        self
    }

    /// Join all sections with blank lines (final prompt).
    ///
    /// # Example
    /// ```rust
    /// use engine::loader::PromptBuilder;
    /// assert_eq!(PromptBuilder::new("a", "1", "d").build(), "# a v1\nd");
    /// ```
    pub fn build(self) -> String {
        self.parts.join("\n\n")
    }
}

/// The hardcoded `# Memory` persona block (mirrors TS loader text).
///
/// # Example
/// ```rust
/// use engine::loader::prompt::memory_block;
/// assert!(memory_block().contains("MEMORY.md"));
/// ```
pub fn memory_block() -> String {
    "# Memory\nYour long-term memory lives in `memory/MEMORY.md` and is \
     committed to git. You are newly awakened: load memory at the start of \
     important work and save durable facts with the `memory` tool (save \
     action + commit message). Memory persists across sessions."
        .to_string()
}

/// The MANDATORY first-priority skills block wrapping `entries` XML.
///
/// # Description
/// Ports `formatSkillsForPrompt()`: each entry is
/// `<skill><name/><description/><location/><confidence/></skill>`. The model
/// MUST check this registry before acting — that rule is the whole point of
/// the block.
///
/// # Example
/// ```rust
/// use engine::loader::prompt::skills_block;
/// let b = skills_block("<skill><name/>x</skill>");
/// assert!(b.contains("FIRST PRIORITY"));
/// ```
pub fn skills_block(entries: &str) -> String {
    format!(
        "# Skills — FIRST PRIORITY (MANDATORY)\n\
         Before doing ANY task, check <available_skills> for a matching skill. \
         If one matches, you MUST load its SKILL.md instructions and follow them.\n\
         <available_skills>\n{entries}\n</available_skills>"
    )
}

/// Workspace-directory rules, with a cloud-mode variant.
///
/// # Description
/// Mirrors the TS `# Workspace Directory` section: generated artifacts go to
/// `workspace/`. When `cloud` is true (GITAGENT_CLOUD / K8s / Render / Fly
/// env detected), GUI `open` commands are forbidden.
///
/// # Example
/// ```rust
/// use engine::loader::prompt::workspace_block;
/// assert!(workspace_block(false).contains("workspace/"));
/// assert!(workspace_block(true).contains("Cloud Mode"));
/// ```
pub fn workspace_block(cloud: bool) -> String {
    let mut s = "# Workspace Directory\nWrite generated artifacts (reports, \
         exports, scratch files) under `workspace/`, never into the repo root \
         unless the user asks."
        .to_string();
    if cloud {
        s.push_str(
            "\n## Cloud Mode\nYou run in a headless cloud container: \
            never invoke GUI commands (no `open`, no browsers).",
        );
    }
    s
}

/// Task-learning instructions (task_tracker + skill_learner workflow).
///
/// # Example
/// ```rust
/// use engine::loader::prompt::learning_block;
/// assert!(learning_block().contains("task_tracker"));
/// ```
pub fn learning_block() -> String {
    "# Task Learning & Skill Discovery\nFor multi-step work: `task_tracker` \
     begin → update as you go → end with the outcome. After a success, use \
     `skill_learner` evaluate/crystallize to turn the win into a reusable \
     skill for future sessions."
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_empty_sections() {
        let p = PromptBuilder::new("a", "1", "d")
            .section("")
            .section("x")
            .build();
        assert_eq!(p, "# a v1\nd\n\nx");
    }
}
