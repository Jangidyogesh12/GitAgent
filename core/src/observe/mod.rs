//! ============================================================================
//! Crate: observe
//! ----------------------------------------------------------------------------
//! WHAT THIS CRATE IS FOR:
//!   Observability: cost tracking, audit logging, chat history, telemetry.
//!   Ports `src/cost-tracker.ts` (per-model token/USD accumulator),
//!   `src/audit.ts` (`.gitagent/audit.jsonl`, 1000-char result slices),
//!   `src/chat-history.ts` (per-branch JSONL + summarise trigger), and
//!   `src/telemetry.ts` (here: JSONL event sink gated by
//!   `GITAGENT_TELEMETRY=1`, mirroring the env-gated OTel init — a full OTLP
//!   exporter is a documented extension point, see Study.md).
//!
//! DESIGN PATTERNS USED:
//!   * Observer — `AuditLogger`/`Telemetry` subscribe to session events by
//!     having small `record_*` methods called from the SDK/CLI (no callbacks
//!     needed at this scale; the call sites are the subjects).
//!
//! MODULES PRESENT IN THIS CRATE:
//!   * `cost`     — CostTracker (per-model accumulation).
//!   * `audit`    — AuditLogger (JSONL session/tool/error records).
//!   * `history`  — ChatHistory (per-branch JSONL store).
//!   * `telemetry`— Telemetry (env-gated JSONL event sink).
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::observe::CostTracker;
//! let mut c = CostTracker::new();
//! c.add("openai:gpt-4o-mini", 100, 50, 0.001);
//! assert_eq!(c.total_usd(), 0.001);
//! ```
//! ============================================================================

pub mod audit;
pub mod cost;
pub mod history;
pub mod telemetry;

pub use audit::AuditLogger;
pub use cost::CostTracker;
pub use history::ChatHistory;
pub use telemetry::Telemetry;
