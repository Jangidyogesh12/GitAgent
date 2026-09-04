//! ============================================================================
//! Module: cli::scaffold (src/scaffold.rs)
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   First-run agent scaffolding. Ports `ensureRepo()` from `src/index.ts`:
//!   mkdir + `git init` + `.gitignore` + initial empty commit + agent.yaml
//!   template (default model openai:gpt-4o-mini, max_turns 50, tools
//!   [cli, read, write, memory]) + workspace/ + memory/MEMORY.md + SOUL.md
//!   + scaffold commit. Existing agent dirs pass through untouched.
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `ensure_repo()` — scaffold when `agent.yaml` is missing, else Ok.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! // crate::scaffold::ensure_repo(std::path::Path::new("./my-agent"), None).unwrap();
//! ```
//! ============================================================================

use anyhow::Result;
use std::path::Path;

/// Scaffold a fresh agent dir when `agent.yaml` is missing (else no-op).
///
/// # Description
/// Mirrors `ensureRepo()`: creates the dir, git-inits it, writes the
/// template files, and commits. Idempotent — existing agents are returned
/// as-is so repeated runs never clobber user files.
///
/// # Example
/// ```rust,no_run
/// // ensure_repo(Path::new("/tmp/new-agent"), None).unwrap();
/// ```
pub fn ensure_repo(dir: &Path, model: Option<&str>) -> Result<()> {
    if dir.join("agent.yaml").is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    // git init (best-effort: agent works without git, memory just won't commit).
    let _ = std::process::Command::new("git")
        .arg("init")
        .current_dir(dir)
        .output();
    let gi = dir.join(".gitignore");
    if !gi.is_file() {
        std::fs::write(&gi, "node_modules/\ndist/\n.gitagent/\n").ok();
    }
    let mut manifest = engine::manifest::AgentManifest::scaffold(
        dir.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("my-agent"),
    );
    if let Some(m) = model {
        if !m.is_empty() {
            manifest.model.preferred = m.to_string();
        }
    }
    engine::manifest::save_manifest(&dir.join("agent.yaml"), &manifest)?;
    std::fs::create_dir_all(dir.join("workspace")).ok();
    std::fs::create_dir_all(dir.join("memory")).ok();
    if !dir.join("memory/MEMORY.md").is_file() {
        std::fs::write(dir.join("memory/MEMORY.md"), "# Memory\n").ok();
    }
    if !dir.join("SOUL.md").is_file() {
        std::fs::write(
            dir.join("SOUL.md"),
            "# Soul\nYou are a helpful git-native agent. Be concise, verify with tools, remember durably.\n",
        )
        .ok();
    }
    // Initial commits (best-effort).
    let _ = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(dir)
        .output();
    let _ = std::process::Command::new("git")
        .args(["commit", "--allow-empty", "-m", "gitagent: scaffold agent"])
        .current_dir(dir)
        .output();
    println!("scaffolded new agent at {}", dir.display());
    Ok(())
}
