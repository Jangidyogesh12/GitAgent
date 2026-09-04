//! ============================================================================
//! Module: engine::integrations::adapters
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The five harness Adapters. Each implements `HarnessAdapter::export()`:
//!   given (agent name, system prompt, tool names, model spec, manifest
//!   extras) it writes the harness-native files into `out_dir` and returns
//!   the list of files written. Mappings are best-effort and DOCUMENTED per
//!   adapter below (each harness owns its schema; we track the stable core).
//!
//! ADAPTER MAP (what each export writes):
//!   * OpenCode    → `opencode.json`: {model, instructions(system prompt),
//!     tools, mcp passthrough}.
//!   * NanoBot     → `nanobot.yaml` + `SYSTEM.md`: agent/model/tools/memory.
//!   * OpenClaw    → `openclaw.json`: {agents:[{name, model, instructions,
//!     skills}], tools}.
//!   * ClaudeCode  → `CLAUDE.md` (persona+rules+memory guidance) +
//!     `.claude/settings.json` (tool permissions allowlist).
//!   * Lyzr        → `.env.lyzr`: LYZR_API_KEY placeholder + model string
//!     (`lyzr:<agent-id>@<base>`) + base-url var.
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `ExportInput`     — everything adapters need (no loader dependency).
//!   * `HarnessAdapter`  — `harness()` + `export()` (+ `model_hint()`).
//!   * `*Adapter` × 5    — the concrete adapters.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::integrations::{adapter_for, Harness};
//! use engine::integrations::adapters::ExportInput;
//! use std::path::Path;
//! let input = ExportInput { name: "demo".into(), system_prompt: "be nice".into(), tools: vec!["cli".into()], model: "openai:gpt-4o-mini".into(), skills: vec![], mcp_servers: serde_json::Value::Null };
//! let files = adapter_for(Harness::OpenCode).export(&input, Path::new("/tmp/out")).unwrap();
//! ```
//! ============================================================================

use anyhow::Result;
use std::path::Path;

use crate::integrations::harness::Harness;

/// Everything an adapter needs (kept free of loader types to avoid cycles).
#[derive(Debug, Clone)]
pub struct ExportInput {
    /// Agent name.
    pub name: String,
    /// Assembled system prompt (adapters slice/quote as needed).
    pub system_prompt: String,
    /// Registered tool names.
    pub tools: Vec<String>,
    /// Preferred model spec (`provider:model[@base]`).
    pub model: String,
    /// Skill names (for harnesses with a skills concept).
    pub skills: Vec<String>,
    /// Raw `mcp_servers` table (passed through where supported).
    pub mcp_servers: serde_json::Value,
}

/// A harness interop adapter (Adapter pattern).
pub trait HarnessAdapter: Send + Sync {
    /// Which harness this adapts.
    fn harness(&self) -> Harness;
    /// Write native files into `out_dir`; return files written (relative).
    ///
    /// # Example
    /// See module docs — each impl takes `&ExportInput` + `out_dir`.
    fn export(&self, input: &ExportInput, out_dir: &Path) -> Result<Vec<String>>;
    /// Model-string hint for running UNDER this harness ("" = unchanged).
    fn model_hint(&self, model: &str) -> String {
        model.to_string()
    }
}

fn write(out_dir: &Path, rel: &str, content: &str) -> Result<String> {
    let p = out_dir.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&p, content)?;
    Ok(rel.to_string())
}

/// OpenCode adapter → `opencode.json`.
pub struct OpenCodeAdapter;

impl HarnessAdapter for OpenCodeAdapter {
    fn harness(&self) -> Harness {
        Harness::OpenCode
    }
    fn export(&self, input: &ExportInput, out_dir: &Path) -> Result<Vec<String>> {
        // Maps: model → model; system prompt → instructions; tools → tools
        // allowlist; mcp_servers passed through verbatim.
        let doc = serde_json::json!({
            "$schema": "https://opencode.ai/config.json",
            "model": input.model,
            "instructions": [input.system_prompt.clone()],
            "tools": input.tools,
            "mcp": input.mcp_servers,
        });
        Ok(vec![write(
            out_dir,
            "opencode.json",
            &serde_json::to_string_pretty(&doc)?,
        )?])
    }
}

/// NanoBot adapter → `nanobot.yaml` + `SYSTEM.md`.
pub struct NanoBotAdapter;

impl HarnessAdapter for NanoBotAdapter {
    fn harness(&self) -> Harness {
        Harness::NanoBot
    }
    fn export(&self, input: &ExportInput, out_dir: &Path) -> Result<Vec<String>> {
        // Maps: name/model/tools + memory path; full prompt → SYSTEM.md.
        let doc = serde_json::json!({
            "agent": {"name": input.name, "model": input.model},
            "system_prompt_file": "SYSTEM.md",
            "tools": input.tools,
            "memory": {"path": "memory/MEMORY.md", "git_backed": true},
            "skills": input.skills,
        });
        let yaml = serde_yaml::to_string(&doc)?;
        Ok(vec![
            write(out_dir, "nanobot.yaml", &yaml)?,
            write(out_dir, "SYSTEM.md", &input.system_prompt)?,
        ])
    }
}

/// OpenClaw adapter → `openclaw.json`.
pub struct OpenClawAdapter;

impl HarnessAdapter for OpenClawAdapter {
    fn harness(&self) -> Harness {
        Harness::OpenClaw
    }
    fn export(&self, input: &ExportInput, out_dir: &Path) -> Result<Vec<String>> {
        // Maps: one agents[] entry (name/model/instructions/skills) + tools.
        let doc = serde_json::json!({
            "agents": [{
                "name": input.name,
                "model": input.model,
                "instructions": input.system_prompt,
                "skills": input.skills,
            }],
            "tools": input.tools,
        });
        Ok(vec![write(
            out_dir,
            "openclaw.json",
            &serde_json::to_string_pretty(&doc)?,
        )?])
    }
}

/// Claude Code adapter → `CLAUDE.md` + `.claude/settings.json`.
pub struct ClaudeCodeAdapter;

impl HarnessAdapter for ClaudeCodeAdapter {
    fn harness(&self) -> Harness {
        Harness::ClaudeCode
    }
    fn export(&self, input: &ExportInput, out_dir: &Path) -> Result<Vec<String>> {
        // Maps: whole system prompt → CLAUDE.md (Claude Code's instruction
        // file); tools → allowlist permissions (best-effort naming).
        let perms: Vec<String> = input.tools.iter().map(|t| format!("Tool:{t}")).collect();
        let settings = serde_json::json!({
            "permissions": {"allow": perms},
            "model": input.model,
        });
        Ok(vec![
            write(
                out_dir,
                "CLAUDE.md",
                &format!("# {}\n\n{}", input.name, input.system_prompt),
            )?,
            write(
                out_dir,
                ".claude/settings.json",
                &serde_json::to_string_pretty(&settings)?,
            )?,
        ])
    }
}

/// Lyzr adapter → `.env.lyzr` (+ model-string conventions).
pub struct LyzrAdapter;

impl HarnessAdapter for LyzrAdapter {
    fn harness(&self) -> Harness {
        Harness::Lyzr
    }
    fn export(&self, input: &ExportInput, out_dir: &Path) -> Result<Vec<String>> {
        // Maps: model string convention lyzr:<agent-id>@<base> + key vars.
        // Mirrors the TS install.sh "Install with LYZR" flow values.
        let base = "https://agent-prod.studio.lyzr.ai/v4";
        let body = format!(
            "# Lyzr Studio backend for agent `{}` (generated by gitagent)\n\
             # 1. Put your Studio API key here.\n\
             LYZR_API_KEY=${{LYZR_API_KEY}}\n\
             # 2. Model string: lyzr:<agent-id>@<base-url>\n\
             GITAGENT_MODEL={}\n\
             GITAGENT_MODEL_BASE_URL={base}\n",
            input.name,
            crate::integrations::export::lyzr_model_string(&input.model, base),
        );
        Ok(vec![write(out_dir, ".env.lyzr", &body)?])
    }
    fn model_hint(&self, model: &str) -> String {
        crate::integrations::export::lyzr_model_string(
            model,
            "https://agent-prod.studio.lyzr.ai/v4",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo() -> ExportInput {
        ExportInput {
            name: "demo".into(),
            system_prompt: "be nice".into(),
            tools: vec!["cli".into()],
            model: "openai:gpt-4o-mini".into(),
            skills: vec![],
            mcp_servers: serde_json::Value::Null,
        }
    }

    #[test]
    fn opencode_writes_config() {
        let d = std::env::temp_dir().join(format!("ga-oc-{}", std::process::id()));
        let files = OpenCodeAdapter.export(&demo(), &d).unwrap();
        assert_eq!(files, vec!["opencode.json"]);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn claude_writes_two_files() {
        let d = std::env::temp_dir().join(format!("ga-cc-{}", std::process::id()));
        let files = ClaudeCodeAdapter.export(&demo(), &d).unwrap();
        assert_eq!(files.len(), 2);
        std::fs::remove_dir_all(&d).ok();
    }
}
