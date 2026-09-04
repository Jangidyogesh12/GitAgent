//! ============================================================================
//! Module: engine::loader::discover
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   File discovery for prompt sections. Ports `src/skills.ts`,
//!   `src/knowledge.ts`, `src/workflows.ts`, `src/agents.ts` and
//!   `src/examples.ts`: scan the agent dir, validate frontmatter/names, and
//!   return structured records the prompt builder formats.
//!
//! FUNCTIONS / TYPES PRESENT IN THIS FILE:
//!   * `SkillInfo` + `discover_skills()`   — skills/*/SKILL.md (kebab-case).
//!   * `load_knowledge()`                  — knowledge/index.yaml + entries.
//!   * `WorkflowInfo` + `discover_workflows()` — workflows/*.yaml|md.
//!   * `AgentInfo` + `discover_agents()`   — agents/<n>/agent.yaml | <n>.md.
//!   * `discover_examples()`               — examples/*.md few-shots.
//!   * `read_optional()`                   — read file or "" when missing.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::loader::discover_skills;
//! use std::path::Path;
//! let skills = discover_skills(Path::new("./my-agent"));
//! ```
//! ============================================================================

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Read a file, returning "" when it does not exist.
///
/// # Description
/// Identity files (SOUL/RULES/...) are all optional in TS — this helper
/// encodes that convention once so every caller stays two lines long.
///
/// # Example
/// ```rust
/// use engine::loader::discover::read_optional;
/// use std::path::Path;
/// assert_eq!(read_optional(Path::new("/definitely/missing.md")), "");
/// ```
pub fn read_optional(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// Validate kebab-case (`skills.ts` enforces it for skill names).
///
/// # Example
/// ```rust
/// use engine::loader::discover::is_kebab;
/// assert!(is_kebab("my-skill")); assert!(!is_kebab("My Skill"));
/// ```
pub fn is_kebab(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

/// One discovered skill (SKILL.md frontmatter + location).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    /// Skill name (= directory name, kebab-case).
    pub name: String,
    /// One-line description for the model.
    pub description: String,
    /// Relative location, e.g. `skills/foo/SKILL.md`.
    pub location: String,
    /// Learned confidence 0..1 (default 1.0 for hand-written skills).
    #[serde(default = "default_confidence")]
    pub confidence: f64,
}

fn default_confidence() -> f64 {
    1.0
}

/// Frontmatter subset we accept on SKILL.md files.
#[derive(Debug, Deserialize)]
struct SkillFront {
    name: Option<String>,
    description: Option<String>,
    confidence: Option<f64>,
}

/// Discover `skills/*/SKILL.md` (sorted by name).
///
/// # Description
/// Ports `discoverSkills()`: requires frontmatter `name` + `description`,
/// `name == dir_name`, kebab-case. Invalid entries are SKIPPED (fail-soft),
/// never fatal — one broken skill must not kill the session.
///
/// # Example
/// ```rust,no_run
/// use engine::loader::discover_skills;
/// use std::path::Path;
/// for s in discover_skills(Path::new("./my-agent")) { println!("{}", s.name); }
/// ```
pub fn discover_skills(agent_dir: &Path) -> Vec<SkillInfo> {
    let mut out = vec![];
    let dir = agent_dir.join("skills");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    for name in names {
        if !is_kebab(&name) {
            continue;
        }
        let file = dir.join(&name).join("SKILL.md");
        if !file.is_file() {
            continue;
        }
        let text = read_optional(&file);
        let Ok((fm, _)) = crate::helpers::parse_frontmatter::<SkillFront>(&text) else {
            continue;
        };
        let (Some(n), Some(d)) = (fm.name, fm.description) else {
            continue;
        };
        if n != name {
            continue;
        }
        out.push(SkillInfo {
            name,
            description: d,
            location: format!("skills/{n}/SKILL.md"),
            confidence: fm.confidence.unwrap_or(1.0),
        });
    }
    out
}

/// One knowledge index entry (`knowledge/index.yaml`).
#[derive(Debug, Clone, Deserialize)]
pub struct KnowledgeEntry {
    /// Repo-relative doc path.
    #[serde(default)]
    pub path: String,
    /// high | medium | low.
    #[serde(default)]
    pub priority: String,
    /// Inline into the prompt (vs list for on-demand `read`).
    #[serde(default)]
    pub always_load: bool,
}

/// Load `knowledge/index.yaml` → (inlined blocks, on-demand list).
///
/// # Description
/// Ports `formatKnowledgeForPrompt()`: `always_load` docs are read + wrapped
/// as `<knowledge path="...">`, the rest become `<doc .../>` listings the
/// model can fetch with the `read` tool. Missing index → ([], []).
///
/// # Example
/// ```rust,no_run
/// use engine::loader::discover::load_knowledge;
/// use std::path::Path;
/// let (inline, listed) = load_knowledge(Path::new("./my-agent"));
/// ```
pub fn load_knowledge(agent_dir: &Path) -> (Vec<(String, String)>, Vec<KnowledgeEntry>) {
    let index = agent_dir.join("knowledge/index.yaml");
    let text = read_optional(&index);
    if text.trim().is_empty() {
        return (vec![], vec![]);
    }
    #[derive(Deserialize)]
    struct Index {
        #[serde(default)]
        entries: Vec<KnowledgeEntry>,
    }
    let Ok(idx) = serde_yaml::from_str::<Index>(&text) else {
        return (vec![], vec![]);
    };
    let mut inline = vec![];
    let mut listed = vec![];
    for e in idx.entries {
        if e.always_load {
            let content = read_optional(&agent_dir.join(&e.path));
            inline.push((e.path.clone(), content));
        } else {
            listed.push(e);
        }
    }
    (inline, listed)
}

/// One discovered workflow (yaml flow with steps, or markdown doc).
#[derive(Debug, Clone)]
pub struct WorkflowInfo {
    /// Workflow name (file stem, kebab enforced for yaml flows).
    pub name: String,
    /// "flow" (has steps, triggered via `@name`) or "doc".
    pub kind: String,
    /// Short description for the prompt listing.
    pub description: String,
}

/// Discover `workflows/*.yaml|yml|md`.
///
/// # Description
/// Ports `src/workflows.ts`: yaml files need `name`; files with a `steps`
/// list become triggerable "flows" (`@name` in chat), everything else is a
/// "doc". Sorted by name; invalid files are skipped.
///
/// # Example
/// ```rust,no_run
/// use engine::loader::discover::discover_workflows;
/// use std::path::Path;
/// let flows = discover_workflows(Path::new("./my-agent"));
/// ```
pub fn discover_workflows(agent_dir: &Path) -> Vec<WorkflowInfo> {
    let mut out = vec![];
    let dir = agent_dir.join("workflows");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|x| x.path()))
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    for f in files {
        let ext = f.extension().and_then(|e| e.to_str()).unwrap_or("");
        let stem = f
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if stem.is_empty() {
            continue;
        }
        let text = read_optional(&f);
        match ext {
            "yaml" | "yml" => {
                #[derive(Deserialize)]
                struct Flow {
                    name: Option<String>,
                    description: Option<String>,
                    steps: Option<Vec<serde_yaml::Value>>,
                }
                let Ok(fm) = serde_yaml::from_str::<Flow>(&text) else {
                    continue;
                };
                if fm.name.is_none() {
                    continue;
                }
                let kind = if fm.steps.map(|s| !s.is_empty()).unwrap_or(false) {
                    "flow"
                } else {
                    "doc"
                };
                out.push(WorkflowInfo {
                    name: stem,
                    kind: kind.into(),
                    description: fm.description.unwrap_or_default(),
                });
            }
            "md" => {
                let desc = crate::helpers::split_frontmatter(&text)
                    .0
                    .and_then(|y| {
                        serde_yaml::from_str::<std::collections::HashMap<String, String>>(&y).ok()
                    })
                    .and_then(|m| m.get("description").cloned())
                    .unwrap_or_default();
                out.push(WorkflowInfo {
                    name: stem,
                    kind: "doc".into(),
                    description: desc,
                });
            }
            _ => {}
        }
    }
    out
}

/// One discovered sub-agent (directory form or file form).
#[derive(Debug, Clone)]
pub struct AgentInfo {
    /// Sub-agent name.
    pub name: String,
    /// Description shown to the router model.
    pub description: String,
    /// Absolute path the model delegates to via `gitagent --dir`.
    pub path: String,
}

/// Discover `agents/<name>/agent.yaml` (dir form) or `agents/<name>.md`.
///
/// # Description
/// Ports `src/agents.ts`. The prompt tells the model to delegate with
/// `gitagent --dir {path} -p "task"` (executed through the `cli` tool).
///
/// # Example
/// ```rust,no_run
/// use engine::loader::discover::discover_agents;
/// use std::path::Path;
/// let agents = discover_agents(Path::new("./my-agent"));
/// ```
pub fn discover_agents(agent_dir: &Path) -> Vec<AgentInfo> {
    let mut out = vec![];
    let dir = agent_dir.join("agents");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|x| x.path())).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            let mf = p.join("agent.yaml");
            if !mf.is_file() {
                continue;
            }
            let text = read_optional(&mf);
            #[derive(Deserialize)]
            struct Mini {
                #[serde(default)]
                name: String,
                #[serde(default)]
                description: String,
            }
            let Ok(m) = serde_yaml::from_str::<Mini>(&text) else {
                continue;
            };
            let name = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            out.push(AgentInfo {
                name: if m.name.is_empty() { name } else { m.name },
                description: m.description,
                path: p.to_string_lossy().into_owned(),
            });
        } else if p.extension().and_then(|e| e.to_str()) == Some("md") {
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let text = read_optional(&p);
            let desc = crate::helpers::split_frontmatter(&text)
                .0
                .and_then(|y| {
                    serde_yaml::from_str::<std::collections::HashMap<String, String>>(&y).ok()
                })
                .and_then(|m| m.get("description").cloned())
                .unwrap_or_default();
            if desc.is_empty() {
                continue;
            }
            out.push(AgentInfo {
                name: stem,
                description: desc,
                path: p.to_string_lossy().into_owned(),
            });
        }
    }
    out
}

/// Load `examples/*.md` as (name, content) few-shots, sorted by name.
///
/// # Description
/// Ports `src/examples.ts`: filename minus `.md` becomes
/// `<example name="...">`. Missing dir → empty vec.
///
/// # Example
/// ```rust,no_run
/// use engine::loader::discover::discover_examples;
/// use std::path::Path;
/// let ex = discover_examples(Path::new("./my-agent"));
/// ```
pub fn discover_examples(agent_dir: &Path) -> Vec<(String, String)> {
    let mut out = vec![];
    let dir = agent_dir.join("examples");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|x| x.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
        .collect();
    files.sort();
    for f in files {
        let name = f
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        out.push((name, read_optional(&f)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kebab_check() {
        assert!(is_kebab("my-skill-2"));
        assert!(!is_kebab("My Skill"));
        assert!(!is_kebab(""));
    }

    #[test]
    fn empty_dirs_yield_empty() {
        let d = std::env::temp_dir().join(format!("ga-disc-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert!(discover_skills(&d).is_empty());
        assert!(discover_examples(&d).is_empty());
        std::fs::remove_dir_all(&d).ok();
    }
}
