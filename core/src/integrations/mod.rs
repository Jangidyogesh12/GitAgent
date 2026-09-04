//! ============================================================================
//! Crate: integrations
//! ----------------------------------------------------------------------------
//! WHAT THIS CRATE IS FOR:
//!   Harness interop adapters — first-class support for **OpenCode** alongside
//!   the NanoBot / OpenClaw / Claude Code / Lyzr adapters. The TypeScript
//!   original only really integrated Lyzr (as a model backend) and merely
//!   *mentioned* Claude Code (design patterns); NanoBot/OpenClaw/OpenCode
//!   had zero support. This crate closes that gap BOTH ways:
//!     (a) EXPORT: render a gitagent agent dir into each harness's native
//!         config format (`opencode.json`, `nanobot.yaml`, `openclaw.json`,
//!         `CLAUDE.md` + settings, `.env.lyzr`);
//!     (b) MODEL: accept each harness's model strings / env vars when
//!         resolving `--model` (Lyzr `lyzr:id@base`, OpenCode providers...).
//!
//!   All mappings are BEST-EFFORT (each harness evolves independently) and
//!   every adapter documents exactly what it maps. Nothing here is
//!   authoritative for the harness itself.
//!
//! DESIGN PATTERNS USED:
//!   * Adapter — each harness gets a `HarnessAdapter` impl translating
//!     gitagent concepts (manifest + system prompt + tools) into native
//!     files and back.
//!   * Facade — `export_for()` / `detect_available()` hide the five adapters.
//!   * Factory — `adapter_for()` builds the right adapter from a `Harness`.
//!
//! MODULES PRESENT IN THIS CRATE:
//!   * `harness` — Harness enum + metadata + detect_available().
//!   * `adapters`— the five Adapter impls (opencode/nanobot/openclaw/...).
//!   * `export`  — export_for() Facade + model-string helpers.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::integrations::{Harness, export_for};
//! use std::path::Path;
//! let files = export_for(Harness::OpenCode, "demo", "sys prompt",
//!     &["cli".to_string()], "openai:gpt-4o-mini", &[],
//!     &serde_json::Value::Null, Path::new("/tmp/out")).unwrap();
//! ```
//! ============================================================================

pub mod adapters;
pub mod export;
pub mod harness;

pub use adapters::HarnessAdapter;
pub use export::{export_for, lyzr_model_string, normalise_model_string};
pub use harness::{adapter_for, all_harnesses, detect_available, Harness};
