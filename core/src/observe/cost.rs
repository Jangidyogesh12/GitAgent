//! ============================================================================
//! Module: engine::observe::cost
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Per-model token/USD accumulation across a session. `add()` folds one
//!   turn's input/output tokens and USD cost into the per-model bucket;
//!   `total_usd()` sums all buckets and `summary()` renders the CLI usage
//!   lines plus a total.
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `ModelCost`    — one model's {input, output, usd}.
//!   * `CostTracker`  — `add()` / `total_usd()` / `summary()`.
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::observe::CostTracker;
//! let mut c = CostTracker::new();
//! c.add("m", 10, 5, 0.002);
//! assert!(c.summary().contains("m"));
//! ```
//! ============================================================================

use std::collections::HashMap;

/// One model's accumulated spend.
#[derive(Debug, Clone, Default)]
pub struct ModelCost {
    /// Prompt tokens.
    pub input: u64,
    /// Completion tokens.
    pub output: u64,
    /// USD.
    pub usd: f64,
}

/// Session-wide cost accumulator keyed by model name.
#[derive(Debug, Clone, Default)]
pub struct CostTracker {
    per_model: HashMap<String, ModelCost>,
}

impl CostTracker {
    /// Create an empty tracker.
    ///
    /// # Example
    /// ```rust
    /// use engine::observe::CostTracker;
    /// assert_eq!(CostTracker::new().total_usd(), 0.0);
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one turn's usage under `model`.
    ///
    /// # Example
    /// ```rust
    /// use engine::observe::CostTracker;
    /// let mut c = CostTracker::new();
    /// c.add("m", 100, 50, 0.01);
    /// assert_eq!(c.total_usd(), 0.01);
    /// ```
    pub fn add(&mut self, model: &str, input: u64, output: u64, usd: f64) {
        let e = self.per_model.entry(model.to_string()).or_default();
        e.input += input;
        e.output += output;
        e.usd += usd;
    }

    /// Total USD across all models.
    ///
    /// # Example
    /// ```rust
    /// use engine::observe::CostTracker;
    /// assert_eq!(CostTracker::new().total_usd(), 0.0);
    /// ```
    pub fn total_usd(&self) -> f64 {
        self.per_model.values().map(|m| m.usd).sum()
    }

    /// Human-readable multi-line summary (for the CLI usage line).
    ///
    /// # Example
    /// ```rust
    /// use engine::observe::CostTracker;
    /// let mut c = CostTracker::new();
    /// c.add("m", 1, 1, 0.5);
    /// assert!(c.summary().contains("$0.50"));
    /// ```
    pub fn summary(&self) -> String {
        let mut lines: Vec<String> = self
            .per_model
            .iter()
            .map(|(m, c)| format!("{m}: {} in / {} out / ${:.4}", c.input, c.output, c.usd))
            .collect();
        lines.sort();
        lines.push(format!("total: ${:.4}", self.total_usd()));
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_per_model() {
        let mut c = CostTracker::new();
        c.add("a", 10, 5, 0.1);
        c.add("a", 10, 5, 0.1);
        c.add("b", 1, 1, 0.05);
        assert!((c.total_usd() - 0.25).abs() < 1e-9);
        assert!(c.summary().contains('a'));
    }
}
