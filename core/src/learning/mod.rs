//! ============================================================================
//! Crate: learning
//! ----------------------------------------------------------------------------
//! WHAT THIS CRATE IS FOR:
//!   The "always learning" half of GitAgent. Covers skill confidence math
//!   (`reinforcement`), the multi-step task lifecycle plus skill matching
//!   (`tasks`: begin/update/end/list), and turning finished tasks into
//!   reusable skills (`skills_learn`: evaluate/crystallize/status/review/
//!   update/delete).
//!
//! DESIGN PATTERNS USED:
//!   * Strategy — both tools implement `AgentTool`.
//!   * State — tasks move active → succeeded|failed (explicit transitions in
//!     `tasks.rs`, invalid transitions rejected).
//!   * Observer — `end` with `skill_used` notifies reinforcement, which
//!     updates the skill's confidence via `adjust_confidence`.
//!
//! MODULES PRESENT IN THIS CRATE:
//!   * `reinforcement` — confidence math + SKILL.md frontmatter updates.
//!   * `tasks`         — TaskTracker tool + JSON store + keyword matching.
//!   * `skills_learn`  — SkillLearner tool + worthiness heuristic.
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::learning::{adjust_confidence, Outcome};
//! assert!(adjust_confidence(0.5, Outcome::Success) > 0.5);
//! ```
//! ============================================================================

pub mod reinforcement;
pub mod skills_learn;
pub mod tasks;

pub use reinforcement::{adjust_confidence, confidence_of, record_outcome, Outcome};
pub use skills_learn::SkillLearner;
pub use tasks::{match_skills, TaskTracker};
