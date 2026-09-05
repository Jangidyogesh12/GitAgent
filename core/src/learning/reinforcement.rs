//! ============================================================================
//! Module: engine::learning::reinforcement
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Skill confidence math. Success moves confidence toward 1.0 via
//!   `c + 0.1*(1-c)` (diminishing returns near the top), failure drops it
//!   by 0.2, partial drops it by 0.05 (both floored at 0.0); skills below
//!   0.4 are flagged for review. Stats persist in SKILL.md frontmatter
//!   (usage/success/failure counts, negative_examples capped at 10).
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `Outcome`            — Success | Failure | Partial.
//!   * `adjust_confidence()`— pure confidence transition.
//!   * `confidence_of()`    — read a skill's confidence from SKILL.md.
//!   * `record_outcome()`   — update SKILL.md frontmatter counts.
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::learning::{adjust_confidence, Outcome};
//! let c = adjust_confidence(1.0, Outcome::Success);
//! assert!((c - 1.0).abs() < f64::EPSILON); // saturates at 1.0
//! ```
//! ============================================================================

use anyhow::Result;
use std::path::Path;

/// Confidence below which `review` flags a skill (0.4).
pub const FLAG_THRESHOLD: f64 = 0.4;
/// Max stored negative examples per skill (cap: 10).
pub const MAX_NEGATIVE_EXAMPLES: usize = 10;

/// Task outcome driving reinforcement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Task succeeded → confidence rises.
    Success,
    /// Task failed → confidence drops hard.
    Failure,
    /// Partial credit → small dip.
    Partial,
}

/// Pure confidence transition applying the update rules.
///
/// # Description
/// success: `c + 0.1*(1-c)` (diminishing returns near 1.0); failure: `c-0.2`
/// floored at 0.0; partial: `c-0.05` floored at 0.0.
///
/// # Example
/// ```rust
/// use engine::learning::{adjust_confidence, Outcome};
/// assert!(adjust_confidence(0.5, Outcome::Failure) < 0.5);
/// ```
pub fn adjust_confidence(current: f64, outcome: Outcome) -> f64 {
    match outcome {
        Outcome::Success => (current + 0.1 * (1.0 - current)).min(1.0),
        Outcome::Failure => (current - 0.2).max(0.0),
        Outcome::Partial => (current - 0.05).max(0.0),
    }
}

/// Read a skill's `confidence:` from its SKILL.md frontmatter.
///
/// # Description
/// Missing file / missing field → 1.0 (hand-written skills start certain).
///
/// # Example
/// ```rust,no_run
/// use engine::learning::confidence_of;
/// use std::path::Path;
/// let c = confidence_of(Path::new("skills/foo/SKILL.md"));
/// ```
pub fn confidence_of(skill_file: &Path) -> f64 {
    let text = std::fs::read_to_string(skill_file).unwrap_or_default();
    let (fm, _) = crate::helpers::split_frontmatter(&text);
    fm.and_then(|y| serde_yaml::from_str::<serde_yaml::Value>(&y).ok())
        .and_then(|v| v.get("confidence").and_then(|c| c.as_f64()))
        .unwrap_or(1.0)
}

/// Apply an outcome to a skill's SKILL.md frontmatter counts.
///
/// # Description
/// Bumps usage_count + success/failure, recomputes confidence via
/// [`adjust_confidence`], appends `failure_reason` to negative_examples
/// (capped at 10). Writes the file back preserving the body. Missing skill
/// → Ok(false) (fail-soft); updated → Ok(true).
///
/// # Example
/// ```rust,no_run
/// use engine::learning::{record_outcome, Outcome};
/// use std::path::Path;
/// record_outcome(Path::new("skills/foo/SKILL.md"), Outcome::Success, None).unwrap();
/// ```
pub fn record_outcome(
    skill_file: &Path,
    outcome: Outcome,
    failure_reason: Option<&str>,
) -> Result<bool> {
    let text = match std::fs::read_to_string(skill_file) {
        Ok(t) => t,
        Err(_) => return Ok(false),
    };
    let (fm, body) = crate::helpers::split_frontmatter(&text);
    let Some(yaml) = fm else { return Ok(false) };
    let mut map: serde_yaml::Mapping = serde_yaml::from_str(&yaml).unwrap_or_default();
    let conf = map
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);
    let bump = |map: &mut serde_yaml::Mapping, key: &str| {
        let n = map.get(key).and_then(|v| v.as_u64()).unwrap_or(0) + 1;
        map.insert(serde_yaml::Value::from(key), serde_yaml::Value::from(n));
    };
    bump(&mut map, "usage_count");
    match outcome {
        Outcome::Success => bump(&mut map, "success_count"),
        Outcome::Failure => {
            bump(&mut map, "failure_count");
            if let Some(reason) = failure_reason {
                let mut neg: Vec<String> = map
                    .get("negative_examples")
                    .and_then(|v| serde_json::to_value(v).ok())
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or_default();
                neg.push(reason.to_string());
                neg.truncate(MAX_NEGATIVE_EXAMPLES);
                map.insert(
                    serde_yaml::Value::from("negative_examples"),
                    serde_yaml::to_value(&neg)
                        .map(|v| serde_yaml::to_value(v).unwrap_or_default())
                        .unwrap_or_default(),
                );
            }
        }
        Outcome::Partial => bump(&mut map, "failure_count"),
    }
    map.insert(
        serde_yaml::Value::from("confidence"),
        serde_yaml::Value::from(adjust_confidence(conf, outcome)),
    );
    let new_yaml = serde_yaml::to_string(&map)?;
    std::fs::write(skill_file, format!("---\n{new_yaml}---\n{body}"))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_math_follows_documented_rules() {
        assert!((adjust_confidence(0.5, Outcome::Success) - 0.55).abs() < 1e-9);
        assert!((adjust_confidence(0.5, Outcome::Failure) - 0.3).abs() < 1e-9);
        assert!((adjust_confidence(0.5, Outcome::Partial) - 0.45).abs() < 1e-9);
        assert_eq!(adjust_confidence(1.0, Outcome::Success), 1.0);
        assert_eq!(adjust_confidence(0.1, Outcome::Failure), 0.0);
    }
}
