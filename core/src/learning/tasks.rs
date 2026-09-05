//! ============================================================================
//! Module: engine::learning::tasks
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The `task_tracker` tool — multi-step task lifecycle. Store lives at
//!   `.gitagent/learning/tasks.json`. `begin` resumes a matching active
//!   task (attempts++) or creates one (attempts = prior failures + 1 plus
//!   a list of prior failure reasons); `update` appends steps; `end`
//!   applies the status transition and reinforces `skill_used`; `list`
//!   shows active tasks. Skill suggestions come from a keyword-overlap
//!   matcher (score > 0.1, words > 2 chars).
//!
//! DESIGN PATTERNS USED:
//!   * State — TaskStatus active → succeeded|failed; illegal transitions Err.
//!   * Observer — `end` notifies reinforcement (`record_outcome`).
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `TaskStatus`   — Active | Succeeded | Failed.
//!   * `TaskRecord`   — stored task shape.
//!   * `match_skills()` — keyword-overlap matcher (pure, testable).
//!   * `TaskTracker`  — the tool (Sequential).
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::learning::match_skills;
//! let hits = match_skills("deploy docs site", &[("docs-helper".into(), "helps deploy documentation".into())]);
//! assert_eq!(hits, vec!["docs-helper"]);
//! ```
//! ============================================================================

use crate::agent::{AgentTool, ExecutionMode, ToolOutput};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::learning::reinforcement::{record_outcome, Outcome};

/// Task lifecycle state (State pattern).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    /// In progress.
    Active,
    /// Ended well.
    Succeeded,
    /// Ended badly.
    Failed,
}

/// One stored task with lifecycle state, step log, and outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    /// UUID.
    pub id: String,
    /// What the task is about.
    pub objective: String,
    /// Step log.
    #[serde(default)]
    pub steps: Vec<String>,
    /// Attempt counter (resumes bump it).
    #[serde(default = "one")]
    pub attempts: u32,
    /// Current state.
    pub status: TaskStatus,
    /// success | failure | partial (set at end).
    #[serde(default)]
    pub outcome: Option<String>,
    /// Why it failed (end+failure).
    #[serde(default)]
    pub failure_reason: Option<String>,
    /// Skill credited (triggers reinforcement).
    #[serde(default)]
    pub skill_used: Option<String>,
    /// RFC3339 start time.
    #[serde(default)]
    pub started_at: String,
    /// RFC3339 end time.
    #[serde(default)]
    pub ended_at: Option<String>,
}

fn one() -> u32 {
    1
}

/// Keyword-overlap skill matcher (pure function).
///
/// # Description
/// Lowercases, strips non `[a-z0-9\s-]`, keeps words > 2 chars, then
/// scores overlap / max(lenA, lenB) > 0.1. Returns matching skill names
/// in registry order.
///
/// # Example
/// ```rust
/// use engine::learning::match_skills;
/// let hits = match_skills("fix login bug", &[("auth".into(), "login session handling".into())]);
/// assert_eq!(hits, vec!["auth"]);
/// ```
pub fn match_skills(objective: &str, skills: &[(String, String)]) -> Vec<String> {
    fn keywords(s: &str) -> Vec<String> {
        s.to_lowercase()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == ' ' || c == '-' {
                    c
                } else {
                    ' '
                }
            })
            .collect::<String>()
            .split_whitespace()
            .filter(|w| w.len() > 2)
            .map(|w| w.to_string())
            .collect()
    }
    let obj = keywords(objective);
    if obj.is_empty() {
        return vec![];
    }
    skills
        .iter()
        .filter(|(_, desc)| {
            let d = keywords(desc);
            if d.is_empty() {
                return false;
            }
            let overlap = obj.iter().filter(|w| d.contains(w)).count();
            overlap as f64 / obj.len().max(d.len()) as f64 > 0.1
        })
        .map(|(name, _)| name.clone())
        .collect()
}

/// Multi-step task tracker tool (Sequential — owns the JSON store).
pub struct TaskTracker {
    /// Agent dir (store at `.gitagent/learning/tasks.json`).
    pub agent_dir: PathBuf,
    /// (name, description) skill registry for `begin` matching.
    pub skills: Vec<(String, String)>,
}

impl TaskTracker {
    /// Create the tool; `skills` feeds the begin-time matcher.
    ///
    /// # Example
    /// ```rust
    /// use engine::learning::TaskTracker;
    /// use std::path::PathBuf;
    /// let t = TaskTracker::new(PathBuf::from("."), vec![]);
    /// assert_eq!(t.name(), "task_tracker");
    /// ```
    pub fn new(agent_dir: PathBuf, skills: Vec<(String, String)>) -> Self {
        Self { agent_dir, skills }
    }

    /// Registry name (kept for tests).
    pub fn name(&self) -> &str {
        "task_tracker"
    }

    fn store_path(&self) -> PathBuf {
        self.agent_dir.join(".gitagent/learning/tasks.json")
    }

    fn load(&self) -> Vec<TaskRecord> {
        std::fs::read_to_string(self.store_path())
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v.get("tasks").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    }

    fn save(&self, tasks: &[TaskRecord]) {
        let v = serde_json::json!({"tasks": tasks});
        if let Some(p) = self.store_path().parent() {
            std::fs::create_dir_all(p).ok();
        }
        std::fs::write(
            self.store_path(),
            serde_json::to_string_pretty(&v).unwrap_or_default(),
        )
        .ok();
    }
}

#[async_trait]
impl AgentTool for TaskTracker {
    fn name(&self) -> &str {
        "task_tracker"
    }
    fn description(&self) -> &str {
        "Track multi-step tasks: begin, update, end, list"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {"type": "string"},
                "objective": {"type": "string"},
                "task_id": {"type": "string"},
                "step": {"type": "string"},
                "outcome": {"type": "string"},
                "failure_reason": {"type": "string"},
                "skill_used": {"type": "string"}
            },
            "required": ["action"]
        })
    }
    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Sequential
    }

    async fn execute(&self, _id: &str, args: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let mut tasks = self.load();
        match action {
            "begin" => {
                let objective = args
                    .get("objective")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if objective.is_empty() {
                    return Ok(ToolOutput::err("Error: `objective` is required for begin"));
                }
                // Resume a matching ACTIVE task (State: stays Active, attempts++).
                if let Some(t) = tasks
                    .iter_mut()
                    .find(|t| t.status == TaskStatus::Active && t.objective == objective)
                {
                    t.attempts += 1;
                    let id = t.id.clone();
                    let attempts = t.attempts;
                    self.save(&tasks);
                    return Ok(ToolOutput::ok(format!(
                        "Resumed task {id} (attempt {attempts})"
                    )));
                }
                let prior_failures: Vec<&TaskRecord> = tasks
                    .iter()
                    .filter(|t| t.status == TaskStatus::Failed && t.objective == objective)
                    .collect();
                let attempts = prior_failures.len() as u32 + 1;
                let mut msg = String::new();
                if !prior_failures.is_empty() {
                    msg.push_str("Prior failures on this objective:\n");
                    for f in &prior_failures {
                        msg.push_str(&format!(
                            "- {}\n",
                            f.failure_reason.clone().unwrap_or_default()
                        ));
                    }
                }
                let id = uuid::Uuid::new_v4().to_string();
                tasks.push(TaskRecord {
                    id: id.clone(),
                    objective: objective.clone(),
                    steps: vec![],
                    attempts,
                    status: TaskStatus::Active,
                    outcome: None,
                    failure_reason: None,
                    skill_used: None,
                    started_at: chrono::Utc::now().to_rfc3339(),
                    ended_at: None,
                });
                self.save(&tasks);
                let hits = match_skills(&objective, &self.skills);
                if !hits.is_empty() {
                    msg.push_str(&format!(
                        "SKILL MATCH FOUND — YOU MUST USE IT: {}\n",
                        hits.join(", ")
                    ));
                }
                msg.push_str(&format!("Task begun: {id} (attempt {attempts})"));
                Ok(ToolOutput::ok(msg))
            }
            "update" => {
                let tid = args.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
                let step = args
                    .get("step")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                match tasks.iter_mut().find(|t| t.id == tid) {
                    Some(t) if t.status == TaskStatus::Active => {
                        t.steps.push(step);
                        let n = t.steps.len();
                        self.save(&tasks);
                        Ok(ToolOutput::ok(format!("Task {tid} updated ({n} steps)")))
                    }
                    Some(_) => Ok(ToolOutput::err("Error: task is already ended")),
                    None => Ok(ToolOutput::err("Error: unknown task_id")),
                }
            }
            "end" => {
                let tid = args.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
                let outcome = args.get("outcome").and_then(|v| v.as_str()).unwrap_or("");
                let pos = tasks.iter().position(|t| t.id == tid);
                let Some(i) = pos else {
                    return Ok(ToolOutput::err("Error: unknown task_id"));
                };
                if tasks[i].status != TaskStatus::Active {
                    return Ok(ToolOutput::err("Error: task is already ended"));
                }
                // State transition (State pattern: only Active → terminal).
                let (status, out_str) = match outcome {
                    "success" => (TaskStatus::Succeeded, "success"),
                    "failure" => (TaskStatus::Failed, "failure"),
                    "partial" => (TaskStatus::Failed, "partial"),
                    _ => {
                        return Ok(ToolOutput::err(
                            "Error: `outcome` must be success|failure|partial",
                        ))
                    }
                };
                tasks[i].status = status;
                tasks[i].outcome = Some(out_str.into());
                tasks[i].failure_reason = args
                    .get("failure_reason")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                tasks[i].skill_used = args
                    .get("skill_used")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                tasks[i].ended_at = Some(chrono::Utc::now().to_rfc3339());
                // Observer: notify reinforcement when a skill is credited.
                if let Some(skill) = tasks[i].skill_used.clone() {
                    let file = self.agent_dir.join(format!("skills/{skill}/SKILL.md"));
                    let oc = match out_str {
                        "success" => Outcome::Success,
                        "partial" => Outcome::Partial,
                        _ => Outcome::Failure,
                    };
                    record_outcome(&file, oc, tasks[i].failure_reason.as_deref()).ok();
                }
                self.save(&tasks);
                Ok(ToolOutput::ok(format!("Task {tid} ended: {out_str}")))
            }
            "list" => {
                let active: Vec<String> = tasks
                    .iter()
                    .filter(|t| t.status == TaskStatus::Active)
                    .map(|t| format!("{}: {} ({} steps)", t.id, t.objective, t.steps.len()))
                    .collect();
                Ok(ToolOutput::ok(if active.is_empty() {
                    "No active tasks.".into()
                } else {
                    active.join("\n")
                }))
            }
            _ => Ok(ToolOutput::err(
                "Error: `action` must be begin|update|end|list",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matcher_finds_overlap() {
        let hits = match_skills(
            "deploy docs site",
            &[("docs-helper".into(), "helps deploy documentation".into())],
        );
        assert_eq!(hits, vec!["docs-helper"]);
    }

    #[tokio::test]
    async fn begin_update_end_flow() {
        let d = std::env::temp_dir().join(format!("ga-tasks-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let t = TaskTracker::new(d.clone(), vec![]);
        let out = t
            .execute(
                "x",
                serde_json::json!({"action": "begin", "objective": "demo"}),
            )
            .await
            .unwrap();
        assert!(out.content.contains("Task begun"));
        let id = out.content.split_whitespace().nth(2).unwrap().to_string();
        t.execute(
            "x",
            serde_json::json!({"action": "update", "task_id": id, "step": "s1"}),
        )
        .await
        .unwrap();
        let end = t
            .execute(
                "x",
                serde_json::json!({"action": "end", "task_id": id, "outcome": "success"}),
            )
            .await
            .unwrap();
        assert!(end.content.contains("success"));
        std::fs::remove_dir_all(&d).ok();
    }
}
