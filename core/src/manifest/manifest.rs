//! ============================================================================
//! Module: engine::manifest::manifest
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The full `agent.yaml` manifest type + loading/saving. Ports the
//!   `AgentManifest` interface from `src/loader.ts` (study.md §2.11):
//!   identity, model, tools, skills allowlist, runtime, extends,
//!   dependencies, delegation, compliance, plugins, mcp_servers.
//!
//! DESIGN PATTERNS USED:
//!   * Builder — `AgentManifest::scaffold()` builds the default first-run
//!     manifest (model `openai:gpt-4o-mini`, max_turns 50, tools
//!     [cli, read, write, memory]) mirroring `ensureRepo()`.
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `Dependency`      — {name, source, version, mount} git dependency.
//!   * `AgentManifest`   — the whole manifest (+ `scaffold()`, `model_spec()`).
//!   * `load_manifest()` — read + parse `agent.yaml`.
//!   * `save_manifest()` — serialise back to `agent.yaml`.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::manifest::{load_manifest, AgentManifest};
//! use std::path::Path;
//! let m = load_manifest(Path::new("agent.yaml")).unwrap();
//! let specs = m.model_spec(None); // CLI flag wins over manifest
//! ```
//! ============================================================================

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::manifest::model::ModelConfig;

/// One `dependencies[]` entry: a git repo cloned into `.gitagent/deps/<name>`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Dependency {
    /// Folder name under `.gitagent/deps/`.
    #[serde(default)]
    pub name: String,
    /// Git URL to clone.
    #[serde(default)]
    pub source: String,
    /// Branch/tag passed as `--branch`.
    #[serde(default)]
    pub version: String,
    /// Mount point hint (informational).
    #[serde(default)]
    pub mount: String,
}

/// The whole `agent.yaml` document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    /// Manifest spec version, e.g. `"0.1.0"`.
    #[serde(default)]
    pub spec_version: String,
    /// Agent name (goes in the prompt header `# {name} v{version}`).
    #[serde(default)]
    pub name: String,
    /// Agent version string.
    #[serde(default)]
    pub version: String,
    /// One-line description (prompt header).
    #[serde(default)]
    pub description: String,
    /// Author (informational).
    #[serde(default)]
    pub author: Option<String>,
    /// License (informational).
    #[serde(default)]
    pub license: Option<String>,
    /// Tags (informational).
    #[serde(default)]
    pub tags: Vec<String>,
    /// Model selection + constraints.
    #[serde(default)]
    pub model: ModelConfig,
    /// Tool name list (informational/for inheritance union in TS).
    #[serde(default)]
    pub tools: Vec<String>,
    /// Allowlist filter on discovered skills (absent = all skills).
    #[serde(default)]
    pub skills: Option<Vec<String>>,
    /// Runtime knobs (max_turns, timeout...). Free-form to stay forwards-
    /// compatible; typed accessors below read the known keys.
    #[serde(default)]
    pub runtime: serde_json::Value,
    /// Git URL of a parent agent (deep-merge, child wins).
    #[serde(default)]
    pub extends: Option<String>,
    /// Git dependencies cloned into `.gitagent/deps/`.
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    /// Delegation config (mode auto|explicit|router). Free-form.
    #[serde(default)]
    pub delegation: serde_json::Value,
    /// Compliance config (risk_level, human_in_the_loop...). Free-form.
    #[serde(default)]
    pub compliance: serde_json::Value,
    /// Plugin table `{name: {enabled, source, version, config}}`. Free-form.
    #[serde(default)]
    pub plugins: serde_json::Value,
    /// MCP server table. Free-form here; typed in `mcp`.
    #[serde(default)]
    pub mcp_servers: serde_json::Value,
}

impl AgentManifest {
    /// Build the default first-run manifest (Builder pattern).
    ///
    /// # Description
    /// Mirrors the `ensureRepo()` template in `src/index.ts`: model
    /// `openai:gpt-4o-mini`, `max_turns: 50`, tools `[cli, read, write,
    /// memory]`. Used by `gitagent --dir <empty>` scaffolding and tests.
    ///
    /// # Example
    /// ```rust
    /// use engine::manifest::AgentManifest;
    /// let m = AgentManifest::scaffold("demo");
    /// assert_eq!(m.max_turns(), 50);
    /// ```
    pub fn scaffold(name: &str) -> Self {
        Self {
            spec_version: "0.1.0".into(),
            name: name.into(),
            version: "0.1.0".into(),
            description: "A git-native AI agent".into(),
            author: None,
            license: None,
            tags: vec![],
            model: ModelConfig {
                preferred: "openai:gpt-4o-mini".into(),
                fallback: vec![],
                constraints: None,
            },
            tools: vec!["cli".into(), "read".into(), "write".into(), "memory".into()],
            skills: None,
            runtime: serde_json::json!({"max_turns": 50}),
            extends: None,
            dependencies: vec![],
            delegation: serde_json::Value::Null,
            compliance: serde_json::Value::Null,
            plugins: serde_json::Value::Null,
            mcp_servers: serde_json::Value::Null,
        }
    }

    /// Max think→act turns (default 50 when unset).
    ///
    /// # Example
    /// ```rust
    /// use engine::manifest::AgentManifest;
    /// assert_eq!(AgentManifest::scaffold("x").max_turns(), 50);
    /// ```
    pub fn max_turns(&self) -> u32 {
        self.runtime
            .get("max_turns")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as u32
    }

    /// CLI `timeout` for the `cli` tool (seconds), if the manifest sets one.
    ///
    /// # Example
    /// ```rust
    /// use engine::manifest::AgentManifest;
    /// assert!(AgentManifest::scaffold("x").cli_timeout().is_none());
    /// ```
    pub fn cli_timeout(&self) -> Option<u64> {
        self.runtime.get("timeout").and_then(|v| v.as_u64())
    }

    /// Ordered model specs: CLI flag > manifest preferred, then fallbacks.
    ///
    /// # Description
    /// Mirrors the TS precedence `envConfig.model_override > --model flag >
    /// manifest.model.preferred`. Empty preferred + no flag → empty vec and
    /// the caller reports "no model configured" (fail-fast, like TS).
    ///
    /// # Example
    /// ```rust
    /// use engine::manifest::AgentManifest;
    /// let m = AgentManifest::scaffold("x");
    /// assert_eq!(m.model_spec(None), vec!["openai:gpt-4o-mini"]);
    /// ```
    pub fn model_spec(&self, cli_override: Option<&str>) -> Vec<String> {
        let mut out = vec![];
        if let Some(flag) = cli_override {
            if !flag.is_empty() {
                out.push(flag.to_string());
            }
        } else if !self.model.preferred.is_empty() {
            out.push(self.model.preferred.clone());
        }
        out.extend(self.model.fallback.iter().cloned());
        out
    }
}

/// Read + YAML-parse `agent.yaml`.
///
/// # Description
/// Fails with the file path in context (the CLI turns this into exit 1 with
/// a hint, like TS `loadAgent` errors).
///
/// # Example
/// ```rust,no_run
/// use engine::manifest::load_manifest;
/// use std::path::Path;
/// let m = load_manifest(Path::new("./agent.yaml")).unwrap();
/// ```
pub fn load_manifest(path: &Path) -> Result<AgentManifest> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading manifest {}", path.display()))?;
    serde_yaml::from_str(&text).with_context(|| format!("parsing manifest {}", path.display()))
}

/// Serialise a manifest back to `agent.yaml`.
///
/// # Example
/// ```rust,no_run
/// use engine::manifest::{save_manifest, AgentManifest};
/// use std::path::Path;
/// save_manifest(Path::new("/tmp/a/agent.yaml"), &AgentManifest::scaffold("demo")).unwrap();
/// ```
pub fn save_manifest(path: &Path, manifest: &AgentManifest) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let text = serde_yaml::to_string(manifest)?;
    std::fs::write(path, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_defaults_match_ts_template() {
        let m = AgentManifest::scaffold("demo");
        assert_eq!(m.model.preferred, "openai:gpt-4o-mini");
        assert_eq!(m.max_turns(), 50);
        assert_eq!(m.model_spec(Some("x:y")), vec!["x:y"]);
    }

    #[test]
    fn round_trips_through_yaml() {
        let m = AgentManifest::scaffold("demo");
        let y = serde_yaml::to_string(&m).unwrap();
        let back: AgentManifest = serde_yaml::from_str(&y).unwrap();
        assert_eq!(back.name, "demo");
    }
}
