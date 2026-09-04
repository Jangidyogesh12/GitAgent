//! ============================================================================
//! Module: engine::observe::telemetry
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Env-gated telemetry sink (`.gitagent/telemetry.jsonl`). Ports the
//!   gating half of `src/telemetry.ts` (init only when
//!   `OTEL_EXPORTER_OTLP_ENDPOINT` / console-exporter is set and
//!   `GITAGENT_OTEL_ENABLED != false`; here the gate is
//!   `GITAGENT_TELEMETRY=1`). A full OTLP exporter is a documented
//!   extension point (Study.md) — the call sites below are where its
//!   spans would attach.
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `Telemetry` — `new()` (reads env) + `event()` + `is_enabled()`.
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::observe::Telemetry;
//! use std::path::PathBuf;
//! let t = Telemetry::new(PathBuf::from("/tmp/g"));
//! assert!(!t.is_enabled()); // unless GITAGENT_TELEMETRY=1
//! ```
//! ============================================================================

use std::path::PathBuf;

/// Env-gated JSONL telemetry sink.
#[derive(Debug, Clone)]
pub struct Telemetry {
    enabled: bool,
    path: PathBuf,
}

impl Telemetry {
    /// Create the sink; enabled iff `GITAGENT_TELEMETRY=1`.
    ///
    /// # Example
    /// ```rust
    /// use engine::observe::Telemetry;
    /// use std::path::PathBuf;
    /// let t = Telemetry::new(PathBuf::from("/tmp/g"));
    /// let _ = t.is_enabled();
    /// ```
    pub fn new(gitagent_dir: PathBuf) -> Self {
        Self {
            enabled: std::env::var("GITAGENT_TELEMETRY")
                .map(|v| v == "1")
                .unwrap_or(false),
            path: gitagent_dir.join("telemetry.jsonl"),
        }
    }

    /// Whether emission is active.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Emit one named event with fields (no-op when disabled).
    ///
    /// # Example
    /// ```rust,no_run
    /// use engine::observe::Telemetry;
    /// use std::path::PathBuf;
    /// Telemetry::new(PathBuf::from("/tmp/g")).event("session.start", serde_json::json!({}));
    /// ```
    pub fn event(&self, name: &str, fields: serde_json::Value) {
        if !self.enabled {
            return;
        }
        let v = serde_json::json!({"event": name, "at": chrono::Utc::now().to_rfc3339(), "fields": fields});
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(f, "{v}");
        }
    }
}
