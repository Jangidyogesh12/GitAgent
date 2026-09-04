//! ============================================================================
//! Module: engine::loader::load
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   `load_agent()` — the Facade that turns an agent directory into a
//!   `LoadedAgent`. Ports `loadAgent()` from `src/loader.ts`: manifest →
//!   `.gitagent/` setup → session state → extends/dependencies (argv-only git
//!   clones, NEVER a shell — the TS `extends: "$(cmd)"` RCE lesson) →
//!   identity files → prompt assembly (TS section order) → model specs.
//!
//! SECURITY NOTE (ported lesson):
//!   `clone_git_repo()` uses argv-array `git` (no shell), so a malicious
//!   `extends: "$(evil)"` is treated as a literal URL and fails to clone
//!   instead of executing.
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `LoadedAgent`      — manifest + system_prompt + dir + session_id +
//!     model_specs + skills count.
//!   * `load_agent()`     — full assembly pipeline (Facade).
//!   * `clone_git_repo()` — argv-only shallow clone (depth 1 + branch).
//!   * `ensure_gitagent_dir()` — create `.gitagent/`, gitignore it.
//!   * `write_session_state()` — `.gitagent/state.json` {session_id,...}.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::loader::load_agent;
//! use std::path::Path;
//! let a = load_agent(Path::new("./my-agent"), None, None).unwrap();
//! println!("{} skills", a.skill_count);
//! ```
//! ============================================================================

use crate::manifest::{load_manifest, AgentManifest};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::loader::discover::{
    discover_agents, discover_examples, discover_skills, discover_workflows, load_knowledge,
    read_optional,
};
use crate::loader::prompt::{
    learning_block, memory_block, skills_block, workspace_block, PromptBuilder,
};

/// Everything a session needs: who the agent is + what it knows.
#[derive(Debug, Clone)]
pub struct LoadedAgent {
    /// Parsed `agent.yaml`.
    pub manifest: AgentManifest,
    /// The one big assembled system prompt.
    pub system_prompt: String,
    /// Absolute agent directory.
    pub dir: PathBuf,
    /// Session id (override or fresh UUID).
    pub session_id: String,
    /// Ordered model specs (CLI flag > preferred, then fallbacks).
    pub model_specs: Vec<String>,
    /// Number of discovered skills (for the CLI banner).
    pub skill_count: usize,
}

/// Load + assemble an agent directory (Facade over the whole pipeline).
///
/// # Description
/// Mirrors `loadAgent(agentDir, modelFlag?, envFlag?, sessionIdOverride?)`
/// minus env-config merging (handled by `manifest` runtime table +
/// the SDK layer). Steps: manifest → `.gitagent/` → state → extends →
/// dependencies → identity files → knowledge/skills/workflows/agents/
/// examples → plugin discovery + `# Plugin:` prompt additions → compliance →
/// workspace → learning. Clone failures are NON-fatal (warn + continue).
///
/// # Example
/// ```rust,no_run
/// use engine::loader::load_agent;
/// use std::path::Path;
/// let a = load_agent(Path::new("./my-agent"), Some("openai:gpt-4o-mini"), None).unwrap();
/// assert!(!a.model_specs.is_empty());
/// ```
pub fn load_agent(
    agent_dir: &Path,
    model_flag: Option<&str>,
    session_override: Option<&str>,
) -> Result<LoadedAgent> {
    let manifest = load_manifest(&agent_dir.join("agent.yaml"))?;
    ensure_gitagent_dir(agent_dir)?;
    let session_id = session_override
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    write_session_state(agent_dir, &session_id)?;

    // extends: shallow clone parent into .gitagent/deps/<name> (fail-soft).
    let mut parent_rules = String::new();
    if let Some(url) = manifest.extends.clone() {
        let dest = agent_dir.join(".gitagent/deps/parent");
        if clone_git_repo(&url, &dest, None).is_ok() {
            parent_rules = read_optional(&dest.join("RULES.md"));
        }
    }
    // dependencies: same treatment per entry (fail-soft each).
    for dep in &manifest.dependencies {
        let dest = agent_dir.join(format!(".gitagent/deps/{}", dep.name));
        let branch = if dep.version.is_empty() {
            None
        } else {
            Some(dep.version.as_str())
        };
        let _ = clone_git_repo(&dep.source, &dest, branch);
    }

    let soul = read_optional(&agent_dir.join("SOUL.md"));
    let rules = read_optional(&agent_dir.join("RULES.md"));
    let duties = read_optional(&agent_dir.join("DUTIES.md"));
    let agents_md = read_optional(&agent_dir.join("AGENTS.md"));

    let (know_inline, know_listed) = load_knowledge(agent_dir);
    let mut know_block = String::from("# Knowledge\n");
    for (path, content) in &know_inline {
        know_block.push_str(&format!(
            "<knowledge path=\"{path}\">\n{content}\n</knowledge>\n"
        ));
    }
    if !know_listed.is_empty() {
        know_block.push_str("<available_knowledge>\n");
        for e in &know_listed {
            know_block.push_str(&format!(
                "<doc path=\"{}\" priority=\"{}\" />\n",
                e.path, e.priority
            ));
        }
        know_block.push_str("</available_knowledge>");
    }

    let mut skills = discover_skills(agent_dir);
    if let Some(allow) = &manifest.skills {
        skills.retain(|s| allow.iter().any(|a| a == &s.name));
    }
    let skill_count = skills.len();
    let entries = skills
        .iter()
        .map(|s| {
            format!(
                "<skill><name>{}</name><description>{}</description>\
                 <location>{}</location><confidence>{}</confidence></skill>",
                s.name, s.description, s.location, s.confidence
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let skills_section = if entries.is_empty() {
        String::new()
    } else {
        skills_block(&entries)
    };

    let workflows = discover_workflows(agent_dir);
    let wf_section = if workflows.is_empty() {
        String::new()
    } else {
        let list = workflows
            .iter()
            .map(|w| {
                format!(
                    "<workflow name=\"{}\" type=\"{}\" description=\"{}\" />",
                    w.name, w.kind, w.description
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("# Workflows\nTrigger SkillFlows with @name in chat.\n<available_workflows>\n{list}\n</available_workflows>")
    };

    let agents = discover_agents(agent_dir);
    let agents_section = if agents.is_empty() {
        String::new()
    } else {
        let list = agents
            .iter()
            .map(|a| {
                format!(
                    "<agent name=\"{}\" path=\"{}\" description=\"{}\" />",
                    a.name, a.path, a.description
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("# Sub-Agents\nDelegate via `gitagent --dir {{path}} -p \"task\"`.\n<available_agents>\n{list}\n</available_agents>")
    };

    let examples = discover_examples(agent_dir);
    let examples_section = if examples.is_empty() {
        String::new()
    } else {
        let list = examples
            .iter()
            .map(|(n, c)| format!("<example name=\"{n}\">\n{c}\n</example>"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("# Examples\n{list}")
    };

    let cloud = std::env::var("GITAGENT_CLOUD").is_ok()
        || std::env::var("KUBERNETES_SERVICE_HOST").is_ok()
        || std::env::var("RENDER").is_ok()
        || std::env::var("FLY_APP_NAME").is_ok();

    // Plugins: discover (auto-installs from `source` URLs, fail-soft) and
    // collect `# Plugin: <name>` prompt additions in manifest position
    // (after examples — the TS section order). Tool contributions are wired
    // separately by the SDK layer, which reuses this same discovery.
    let found_plugins = crate::plugins::discover_plugins(agent_dir, &manifest.plugins);
    let plugin_sections = crate::plugins::plugin_prompt_additions(&found_plugins);

    let mut builder = PromptBuilder::new(&manifest.name, &manifest.version, &manifest.description)
        .section(soul)
        .section(rules)
        .section(parent_rules)
        .section(duties)
        .section(agents_md)
        .section(memory_block())
        .section(know_block)
        .section(skills_section)
        .section(wf_section)
        .section(agents_section)
        .section(examples_section);
    for section in plugin_sections {
        builder = builder.section(section);
    }
    let system_prompt = builder
        .section(workspace_block(cloud))
        .section(learning_block())
        .build();

    Ok(LoadedAgent {
        model_specs: {
            let v = manifest.model_spec(model_flag);
            if v.is_empty() {
                anyhow::bail!("no model configured: set model.preferred in agent.yaml or pass --model provider:model");
            }
            v
        },
        manifest,
        system_prompt,
        dir: agent_dir.to_path_buf(),
        session_id,
        skill_count,
    })
}

/// Shallow-clone a git repo with argv-only `git` (NO shell).
///
/// # Description
/// `git clone --depth 1 [--branch b] <url> <dest>`. Skips when `dest`
/// already exists. SECURITY: argv array means `extends: "$(cmd)"` can never
/// execute — it just fails to clone (fail-soft upstream).
///
/// # Example
/// ```rust,no_run
/// use engine::loader::clone_git_repo;
/// use std::path::Path;
/// clone_git_repo("https://github.com/org/parent-agent", Path::new("/tmp/parent"), None).unwrap();
/// ```
pub fn clone_git_repo(url: &str, dest: &Path, branch: Option<&str>) -> Result<()> {
    if dest.exists() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut args = vec!["clone", "--depth", "1"];
    let branch_owned;
    if let Some(b) = branch {
        branch_owned = b.to_string();
        args.extend(["--branch", &branch_owned]);
    }
    let dest_s = dest.to_string_lossy().into_owned();
    let url_owned = url.to_string();
    let mut cmd_args = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    cmd_args.push(url_owned);
    cmd_args.push(dest_s);
    let out = std::process::Command::new("git")
        .args(&cmd_args)
        .output()
        .context("spawning git clone")?;
    if !out.status.success() {
        anyhow::bail!("git clone failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}

/// Create `.gitagent/` and ensure it is git-ignored.
///
/// # Example
/// ```rust,no_run
/// use engine::loader::load::ensure_gitagent_dir;
/// use std::path::Path;
/// ensure_gitagent_dir(Path::new("./my-agent")).unwrap();
/// ```
pub fn ensure_gitagent_dir(agent_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(agent_dir.join(".gitagent"))?;
    let gi = agent_dir.join(".gitignore");
    let cur = std::fs::read_to_string(&gi).unwrap_or_default();
    if !cur.lines().any(|l| l.trim() == ".gitagent/") {
        let mut next = cur;
        if !next.is_empty() && !next.ends_with('\n') {
            next.push('\n');
        }
        next.push_str(".gitagent/\n");
        std::fs::write(&gi, next)?;
    }
    Ok(())
}

/// Write `.gitagent/state.json` with the current session id + start time.
///
/// # Example
/// ```rust,no_run
/// use engine::loader::load::write_session_state;
/// use std::path::Path;
/// write_session_state(Path::new("./my-agent"), "sess-1").unwrap();
/// ```
pub fn write_session_state(agent_dir: &Path, session_id: &str) -> Result<()> {
    let state = serde_json::json!({
        "session_id": session_id,
        "started_at": chrono::Utc::now().to_rfc3339(),
    });
    std::fs::write(
        agent_dir.join(".gitagent/state.json"),
        serde_json::to_string_pretty(&state)?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::AgentManifest;

    fn scaffold_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ga-load-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        crate::manifest::save_manifest(&d.join("agent.yaml"), &AgentManifest::scaffold(name))
            .unwrap();
        std::fs::write(d.join("SOUL.md"), "I am TestBot.").unwrap();
        d
    }

    #[test]
    fn assembles_prompt_in_ts_order() {
        let d = scaffold_dir("order");
        let a = load_agent(&d, None, Some("s1")).unwrap();
        assert_eq!(a.session_id, "s1");
        let soul_pos = a.system_prompt.find("TestBot").unwrap();
        let mem_pos = a.system_prompt.find("# Memory").unwrap();
        assert!(soul_pos < mem_pos);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn cli_flag_wins_over_manifest() {
        let d = scaffold_dir("flag");
        let a = load_agent(&d, Some("x:y"), None).unwrap();
        assert_eq!(a.model_specs[0], "x:y");
        std::fs::remove_dir_all(&d).ok();
    }
}
