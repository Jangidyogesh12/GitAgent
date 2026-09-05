//! ============================================================================
//! Module: engine::learning::skills_learn
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The `skill_learner` tool — turn finished tasks into reusable skills.
//!   `evaluate` runs the 4-check worthiness heuristic, `crystallize`
//!   writes a success-gated SKILL.md plus a git commit, `status` lists
//!   learned skills, `review` flags confidence < 0.4, `update` replaces
//!   instructions, `delete` removes the skill dir.
//!
//! WORTHINESS HEURISTIC:
//!   multi_step (≥3 steps) · non_trivial (≥2 steps) · novel (no existing
//!   skill description with Jaccard > 0.5) · generalizable (<30% of steps
//!   match project-specific patterns: absolute paths, UUIDs, 3+-part
//!   PascalCase). Worthy if `override || pass>=3 || (multi_step && novel)`.
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `SkillLearner`       — the tool (Sequential).
//!   * `worthiness()`       — pure heuristic → (worthy, check results).
//!   * `jaccard()`          — keyword-set similarity helper.
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::learning::skills_learn::{worthiness, jaccard};
//! assert!(jaccard("a b c", "a b d") > 0.3);
//! ```
//! ============================================================================

use crate::agent::{AgentTool, ExecutionMode, ToolOutput};
use async_trait::async_trait;
use std::path::PathBuf;

/// Skill-crystallisation tool (Sequential — writes skills/ + commits).
pub struct SkillLearner {
    /// Agent dir (reads tasks store, writes `skills/`).
    pub agent_dir: PathBuf,
}

impl SkillLearner {
    /// Create the tool for an agent directory.
    ///
    /// # Example
    /// ```rust
    /// use engine::learning::SkillLearner;
    /// use std::path::PathBuf;
    /// assert_eq!(SkillLearner::new(PathBuf::from(".")).name(), "skill_learner");
    /// ```
    pub fn new(agent_dir: PathBuf) -> Self {
        Self { agent_dir }
    }

    /// Registry name (kept for tests).
    pub fn name(&self) -> &str {
        "skill_learner"
    }

    fn load_task(&self, task_id: &str) -> Option<crate::learning::tasks::TaskRecord> {
        std::fs::read_to_string(self.agent_dir.join(".gitagent/learning/tasks.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v.get("tasks").cloned())
            .and_then(|v| serde_json::from_value::<Vec<crate::learning::tasks::TaskRecord>>(v).ok())
            .and_then(|ts| ts.into_iter().find(|t| t.id == task_id))
    }
}

/// Jaccard similarity of keyword sets (novelty check helper).
///
/// # Description
/// Same keywordisation as `match_skills` (lowercase, >2 chars); similarity
/// = |A∩B| / |A∪B|. Tasks count as "not novel" when similarity > 0.5.
///
/// # Example
/// ```rust
/// use engine::learning::skills_learn::jaccard;
/// assert!((jaccard("a b c", "a b c") - 1.0).abs() < 1e-9);
/// assert_eq!(jaccard("aaa bbb", "ccc ddd"), 0.0);
/// ```
pub fn jaccard(a: &str, b: &str) -> f64 {
    fn kw(s: &str) -> std::collections::HashSet<String> {
        s.to_lowercase()
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| w.len() > 2)
            .map(|w| w.to_string())
            .collect()
    }
    let (sa, sb) = (kw(a), kw(b));
    if sa.is_empty() && sb.is_empty() {
        return 1.0;
    }
    let inter = sa.intersection(&sb).count() as f64;
    let union = sa.union(&sb).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Evaluate worthiness: (worthy, multi_step, non_trivial, novel, generalizable).
///
/// # Description
/// Pure function implementing the `evaluate` heuristic. `existing_descs`
/// are the current skill descriptions (novelty compares against each).
///
/// # Example
/// ```rust
/// use engine::learning::skills_learn::worthiness;
/// let steps = vec!["one two three".to_string(); 4];
/// let (worthy, _, _, _, _) = worthiness(&steps, &[], false);
/// assert!(worthy);
/// ```
pub fn worthiness(
    steps: &[String],
    existing_descs: &[String],
    override_heuristic: bool,
) -> (bool, bool, bool, bool, bool) {
    let multi_step = steps.len() >= 3;
    let non_trivial = steps.len() >= 2;
    let text = steps.join(" ");
    let novel = existing_descs.iter().all(|d| jaccard(&text, d) <= 0.5);
    // Project-specific patterns: absolute paths, UUIDs, 3+-part PascalCase.
    let uuid_re = regex::Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}").unwrap();
    let pascal_re = regex::Regex::new(r"\b[A-Z][a-z]+(?:[A-Z][a-z]+){2,}\b").unwrap();
    let specific = steps
        .iter()
        .filter(|s| {
            s.contains(" /")
                || s.contains('/') && s.contains("/home")
                || s.starts_with('/')
                || uuid_re.is_match(s)
                || pascal_re.is_match(s)
        })
        .count();
    let generalizable = steps.is_empty() || specific as f64 / steps.len() as f64 <= 0.3;
    let pass = [multi_step, non_trivial, novel, generalizable]
        .iter()
        .filter(|&&b| b)
        .count();
    let worthy = override_heuristic || pass >= 3 || (multi_step && novel);
    (worthy, multi_step, non_trivial, novel, generalizable)
}

#[async_trait]
impl AgentTool for SkillLearner {
    fn name(&self) -> &str {
        "skill_learner"
    }
    fn description(&self) -> &str {
        "Turn finished tasks into reusable skills: evaluate, crystallize, status, review, update, delete"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {"type": "string"},
                "task_id": {"type": "string"},
                "skill_name": {"type": "string"},
                "skill_description": {"type": "string"},
                "instructions": {"type": "string"},
                "override_heuristic": {"type": "boolean"}
            },
            "required": ["action"]
        })
    }
    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Sequential
    }

    async fn execute(&self, _id: &str, args: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        match action {
            "evaluate" => {
                let tid = args.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
                let ov = args
                    .get("override_heuristic")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let Some(task) = self.load_task(tid) else {
                    return Ok(ToolOutput::err("Error: unknown task_id"));
                };
                let existing: Vec<String> = crate::loader::discover_skills(&self.agent_dir)
                    .into_iter()
                    .map(|s| s.description)
                    .collect();
                let (worthy, ms, nt, nov, gen) = worthiness(&task.steps, &existing, ov);
                Ok(ToolOutput::ok(format!(
                    "Worthiness for task {tid}: worthy={worthy} \
                     [multi_step={ms}, non_trivial={nt}, novel={nov}, generalizable={gen}]"
                )))
            }
            "crystallize" => {
                let tid = args.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
                let name = args
                    .get("skill_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let desc = args
                    .get("skill_description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if name.is_empty() || desc.is_empty() {
                    return Ok(ToolOutput::err(
                        "Error: skill_name + skill_description required",
                    ));
                }
                let Some(task) = self.load_task(tid) else {
                    return Ok(ToolOutput::err("Error: unknown task_id"));
                };
                // Success-gated: only succeeded tasks crystallise.
                if task.status != crate::learning::tasks::TaskStatus::Succeeded {
                    return Ok(ToolOutput::err(
                        "Error: only tasks with outcome success can be crystallized",
                    ));
                }
                let dir = self.agent_dir.join(format!("skills/{name}"));
                std::fs::create_dir_all(&dir).ok();
                let steps_md = task
                    .steps
                    .iter()
                    .map(|s| format!("- {s}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                let body = format!(
                    "# {}\n\n{}\n\n## Steps\n{}\n\n## What Worked\n{}\n\n## What Did NOT Work\n(none recorded)\n",
                    name, desc, steps_md, task.objective
                );
                let fm = format!(
                    "name: {name}\ndescription: \"{}\"\nlearned_from: task:{tid}\nconfidence: 1.0\nusage_count: 0\nsuccess_count: 0\nfailure_count: 0\nnegative_examples: []\n",
                    desc.replace('"', "'")
                );
                std::fs::write(dir.join("SKILL.md"), format!("---\n{fm}---\n{body}")).ok();
                let _ = std::process::Command::new("git")
                    .args(["add", &format!("skills/{name}/SKILL.md")])
                    .current_dir(&self.agent_dir)
                    .output();
                let _ = std::process::Command::new("git")
                    .args([
                        "commit",
                        "-m",
                        &format!("learn: crystallize skill {name} from task {tid}"),
                    ])
                    .current_dir(&self.agent_dir)
                    .output();
                Ok(ToolOutput::ok(format!(
                    "Skill crystallized: skills/{name}/SKILL.md"
                )))
            }
            "status" => {
                let skills = crate::loader::discover_skills(&self.agent_dir);
                if skills.is_empty() {
                    return Ok(ToolOutput::ok("No learned skills yet."));
                }
                Ok(ToolOutput::ok(
                    skills
                        .iter()
                        .map(|s| format!("{} (confidence {:.2})", s.name, s.confidence))
                        .collect::<Vec<_>>()
                        .join("\n"),
                ))
            }
            "review" => {
                let low: Vec<String> = crate::loader::discover_skills(&self.agent_dir)
                    .into_iter()
                    .filter(|s| s.confidence < crate::learning::reinforcement::FLAG_THRESHOLD)
                    .map(|s| format!("{} (confidence {:.2})", s.name, s.confidence))
                    .collect();
                Ok(ToolOutput::ok(if low.is_empty() {
                    "No low-confidence skills. All skills healthy.".into()
                } else {
                    format!("Skills needing review:\n{}", low.join("\n"))
                }))
            }
            "update" => {
                let name = args
                    .get("skill_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let instructions = args
                    .get("instructions")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let file = self.agent_dir.join(format!("skills/{name}/SKILL.md"));
                if !file.is_file() || instructions.is_empty() {
                    return Ok(ToolOutput::err(
                        "Error: unknown skill or empty instructions",
                    ));
                }
                let text = std::fs::read_to_string(&file).unwrap_or_default();
                let (fm, _) = crate::helpers::split_frontmatter(&text);
                let head = fm.map(|y| format!("---\n{y}---\n")).unwrap_or_default();
                std::fs::write(&file, format!("{head}{instructions}")).ok();
                Ok(ToolOutput::ok(format!("Skill {name} updated")))
            }
            "delete" => {
                let name = args
                    .get("skill_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let dir = self.agent_dir.join(format!("skills/{name}"));
                if !dir.is_dir() {
                    return Ok(ToolOutput::err("Error: unknown skill"));
                }
                std::fs::remove_dir_all(&dir).ok();
                Ok(ToolOutput::ok(format!("Skill {name} deleted")))
            }
            _ => Ok(ToolOutput::err(
                "Error: `action` must be evaluate|crystallize|status|review|update|delete",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heuristic_marks_good_tasks_worthy() {
        let steps = vec!["first generic step one".to_string(); 4];
        let (worthy, _, _, _, _) = worthiness(&steps, &[], false);
        assert!(worthy);
    }

    #[test]
    fn trivial_tasks_not_worthy() {
        let steps = vec!["do it".to_string()];
        let (worthy, _, _, _, _) = worthiness(&steps, &[], false);
        assert!(!worthy);
    }
}
