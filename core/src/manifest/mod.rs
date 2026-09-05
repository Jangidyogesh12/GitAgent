//! ============================================================================
//! Crate: manifest
//! ----------------------------------------------------------------------------
//! WHAT THIS CRATE IS FOR:
//!   The `agent.yaml` schema — pure data types with NO I/O except one
//!   loader. Inputs: `agent.yaml` text. Steps: serde-parse into
//!   `AgentManifest` (spec_version, name/version/description, model with
//!   preferred + fallbacks + constraints, tools, skills allowlist, runtime,
//!   extends, dependencies, delegation, compliance, plugins, mcp_servers)
//!   with defaults for every field. Outputs: a typed manifest plus typed
//!   accessors (`max_turns()`, `model_spec()`, `provider_api_key()`).
//!   Invariant: free-form tables stay `serde_json::Value` so newer keys
//!   never break older binaries.
//!
//! DESIGN PATTERNS USED:
//!   * Builder — `AgentManifest::scaffold()` manufactures the default
//!     first-run manifest for empty-dir scaffolding.
//!   * Data Transfer Object — structs cross every crate boundary unchanged.
//!
//! MODULES PRESENT IN THIS CRATE:
//!   * `model`    — ModelConfig + ModelConstraints + provider_key().
//!   * `types` — AgentManifest + Dependency + load_manifest() + scaffold.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::manifest::load_manifest;
//! use std::path::Path;
//! let m = load_manifest(Path::new("./my-agent/agent.yaml")).unwrap();
//! assert!(!m.name.is_empty());
//! ```
//! ============================================================================

pub mod model;
pub mod types;

pub use model::{provider_api_key, ModelConfig, ModelConstraints};
pub use types::{load_manifest, save_manifest, AgentManifest, Dependency};
