//! ============================================================================
//! Crate: manifest
//! ----------------------------------------------------------------------------
//! WHAT THIS CRATE IS FOR:
//!   The `agent.yaml` schema — pure data types with NO I/O except one loader.
//!   Ports the `AgentManifest` interface from `src/loader.ts` (§2.11 of
//!   study.md): spec_version, name/version/description, model (preferred +
//!   fallbacks + constraints), tools, skills allowlist, runtime, extends,
//!   dependencies, delegation, compliance, plugins, mcp_servers.
//!
//! DESIGN PATTERNS USED:
//!   * Builder — `AgentManifest::scaffold()` manufactures the default
//!     first-run manifest (mirrors `ensureRepo()` in `src/index.ts`).
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
