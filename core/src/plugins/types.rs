//! ============================================================================
//! Module: engine::plugins::types
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Plugin manifest types. Defines the `plugin.yaml` shape and its
//!   validation: required id/name/version/description, kebab-case id,
//!   `provides` (tools/hooks/skills/prompt), `config` (properties with
//!   type/description/default/env plus a required list), optional `entry`
//!   (programmatic module, informational) and `engine` (version range,
//!   only `>=` ranges interpreted, informational).
//!
//! TYPES PRESENT IN THIS FILE:
//!   * `PluginManifest`   — plugin.yaml shape (+ `validate()`).
//!   * `LoadedPlugin`     — dir + manifest + enabled + scope.
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::plugins::PluginManifest;
//! let m: PluginManifest = serde_yaml::from_str("id: demo\nname: D\nversion: 1\ndescription: d\n").unwrap();
//! assert!(m.validate().is_ok());
//! ```
//! ============================================================================

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One config property declaration (`config.properties.<key>`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigProperty {
    /// string | number | boolean.
    #[serde(default)]
    pub prop_type: Option<String>,
    /// Human description.
    #[serde(default)]
    pub description: String,
    /// Default value (JSON).
    #[serde(default)]
    pub default: serde_json::Value,
    /// Env var fallback name.
    #[serde(default)]
    pub env: Option<String>,
}

/// The `config:` section: schema + required keys.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginConfigSchema {
    /// Declared properties.
    #[serde(default)]
    pub properties: std::collections::HashMap<String, ConfigProperty>,
    /// Required keys (missing → warning only, non-fatal).
    #[serde(default)]
    pub required: Vec<String>,
}

/// The `provides:` section: what the plugin contributes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginProvides {
    /// Contributes declarative `tools/*.yaml`.
    #[serde(default)]
    pub tools: bool,
    /// Event → script list (resolved relative to the plugin dir).
    #[serde(default)]
    pub hooks: std::collections::HashMap<String, Vec<PluginHookDef>>,
    /// Contributes `skills/`.
    #[serde(default)]
    pub skills: bool,
    /// Extra prompt markdown file (relative path).
    #[serde(default)]
    pub prompt: Option<String>,
}

/// One plugin-declared script hook.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginHookDef {
    /// Script path relative to the plugin dir.
    #[serde(default)]
    pub script: String,
    /// Human description.
    #[serde(default)]
    pub description: String,
}

/// `plugin.yaml` manifest: identity + contributions + config schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Kebab-case id (enforced).
    #[serde(default)]
    pub id: String,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Version string.
    #[serde(default)]
    pub version: String,
    /// One-line description.
    #[serde(default)]
    pub description: String,
    /// Contributions.
    #[serde(default)]
    pub provides: PluginProvides,
    /// Config schema.
    #[serde(default)]
    pub config: PluginConfigSchema,
    /// Programmatic entry module (informational in this port).
    #[serde(default)]
    pub entry: Option<String>,
    /// Engine range (only `>=` understood; informational).
    #[serde(default)]
    pub engine: Option<String>,
}

impl PluginManifest {
    /// Validate required fields + kebab-case id.
    ///
    /// # Example
    /// ```rust
    /// use engine::plugins::PluginManifest;
    /// let m: PluginManifest = serde_yaml::from_str("id: Bad Name\nname: x\nversion: 1\ndescription: d\n").unwrap();
    /// assert!(m.validate().is_err());
    /// ```
    pub fn validate(&self) -> Result<()> {
        for (field, val) in [
            ("id", &self.id),
            ("name", &self.name),
            ("version", &self.version),
            ("description", &self.description),
        ] {
            if val.trim().is_empty() {
                anyhow::bail!("plugin.yaml: `{field}` is required");
            }
        }
        let kebab = !self.id.is_empty()
            && self
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !kebab {
            anyhow::bail!("plugin.yaml: `id` must be kebab-case, got {:?}", self.id);
        }
        Ok(())
    }
}

/// A discovered, validated plugin ready to contribute.
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    /// Plugin id (= directory name).
    pub name: String,
    /// Where it was found (local | global | installed).
    pub scope: String,
    /// Absolute plugin directory.
    pub dir: PathBuf,
    /// Validated manifest.
    pub manifest: PluginManifest,
    /// Whether it is enabled (manifest `plugins.<name>.enabled`, default true).
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_kebab_id() {
        let m: PluginManifest =
            serde_yaml::from_str("id: Bad\nname: x\nversion: 1\ndescription: d\n").unwrap();
        assert!(m.validate().is_err());
    }
}
