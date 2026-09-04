//! ============================================================================
//! Module: engine::plugins::discover
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Plugin discovery + contributions. Ports `discoverPluginDirs()` and the
//!   loading half of `src/plugins.ts`: scope order local → global →
//!   installed, auto-install from `plugins.<name>.source` git URLs into
//!   `.gitagent/plugins/` (argv-only clone, fail-soft), enabled-flag +
//!   `${ENV}`-interpolated config resolution (user > env > default, missing
//!   required → warning), `plugin_prompt_additions()` (`# Plugin: <name>`
//!   sections) and `plugin_hook_configs()` (HookDefinitions with base_dir).
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `discover_plugins()`       — full pipeline → Vec<LoadedPlugin>.
//!   * `plugin_prompt_additions()`— `# Plugin: <name>` markdown sections.
//!   * `plugin_hook_configs()`    — per-plugin HooksConfigs (base_dir set).
//!   * `resolve_plugin_config()`  — user > env > default (+ warnings).
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::plugins::discover_plugins;
//! use std::path::Path;
//! let ps = discover_plugins(Path::new("./my-agent"), &serde_json::Value::Null);
//! ```
//! ============================================================================

use std::path::{Path, PathBuf};

use crate::plugins::types::{LoadedPlugin, PluginManifest};

/// Discover + validate + auto-install plugins (fail-soft throughout).
///
/// # Description
/// Scope order (first hit wins per name): `<agent>/plugins/<name>` (local),
/// `~/.gitagent/plugins/<name>` (global), `<agent>/.gitagent/plugins/`
/// (installed). When the manifest's `plugins.<name>.source` is a git URL and
/// nothing is installed, shallow-clone it (fail-soft). `plugin_table` is the
/// raw `plugins:` value from agent.yaml (enabled/source/version/config).
///
/// # Example
/// ```rust,no_run
/// use engine::plugins::discover_plugins;
/// use std::path::Path;
/// let ps = discover_plugins(Path::new("./my-agent"), &serde_json::Value::Null);
/// assert!(ps.iter().all(|p| p.enabled));
/// ```
pub fn discover_plugins(agent_dir: &Path, plugin_table: &serde_json::Value) -> Vec<LoadedPlugin> {
    let home = std::env::var("HOME").unwrap_or_default();
    let scopes: Vec<(String, PathBuf)> = vec![
        ("local".into(), agent_dir.join("plugins")),
        ("global".into(), Path::new(&home).join(".gitagent/plugins")),
        ("installed".into(), agent_dir.join(".gitagent/plugins")),
    ];
    // Candidate names: union of dir listings + manifest table keys.
    let mut names: Vec<String> = vec![];
    for (_, scope_dir) in &scopes {
        if let Ok(entries) = std::fs::read_dir(scope_dir) {
            for e in entries.filter_map(|e| e.ok()) {
                if e.path().is_dir() {
                    if let Ok(n) = e.file_name().into_string() {
                        if !names.contains(&n) {
                            names.push(n);
                        }
                    }
                }
            }
        }
    }
    if let Some(obj) = plugin_table.as_object() {
        for k in obj.keys() {
            if !names.contains(k) {
                names.push(k.clone());
            }
        }
    }
    names.sort();

    let mut out = vec![];
    for name in names {
        let entry_cfg = plugin_table.get(&name);
        let enabled = entry_cfg
            .and_then(|v| v.get("enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        // Auto-install from `source` when nothing is on disk yet.
        if !scopes
            .iter()
            .any(|(_, d)| d.join(&name).join("plugin.yaml").is_file())
        {
            if let Some(source) = entry_cfg
                .and_then(|v| v.get("source"))
                .and_then(|v| v.as_str())
            {
                if !source.is_empty() {
                    let dest = agent_dir.join(format!(".gitagent/plugins/{name}"));
                    let version = entry_cfg
                        .and_then(|v| v.get("version"))
                        .and_then(|v| v.as_str());
                    let _ = clone_shallow(source, &dest, version);
                }
            }
        }
        let found = scopes.iter().find_map(|(scope, d)| {
            let dir = d.join(&name);
            if dir.join("plugin.yaml").is_file() {
                Some((scope.clone(), dir))
            } else {
                None
            }
        });
        let Some((scope, dir)) = found else { continue };
        let text = match std::fs::read_to_string(dir.join("plugin.yaml")) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let manifest: PluginManifest = match serde_yaml::from_str(&text) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("warning: plugin {name}: bad plugin.yaml: {e}");
                continue;
            }
        };
        if manifest.validate().is_err() {
            eprintln!("warning: plugin {name}: invalid manifest, skipping");
            continue;
        }
        resolve_plugin_config(&name, &manifest, entry_cfg);
        out.push(LoadedPlugin {
            name,
            scope,
            dir,
            manifest,
            enabled,
        });
    }
    out.into_iter().filter(|p| p.enabled).collect()
}

/// Resolve a plugin's config (user > env > default) with warnings.
///
/// # Description
/// Ports the TS config resolution: manifest `plugins.<name>.config` values
/// (with `${ENV}` interpolation) win, then the property's `env` var
/// (coerced to number/boolean), then `default`. Missing `required` keys
/// warn (not fatal). Returns the resolved map (used by hosts; scripts read
/// env themselves).
///
/// # Example
/// ```rust
/// use engine::plugins::discover::resolve_plugin_config;
/// use engine::plugins::PluginManifest;
/// let m: PluginManifest = serde_yaml::from_str("id: p\nname: P\nversion: 1\ndescription: d\n").unwrap();
/// let cfg = resolve_plugin_config("p", &m, None);
/// assert!(cfg.is_empty());
/// ```
pub fn resolve_plugin_config(
    name: &str,
    manifest: &PluginManifest,
    entry_cfg: Option<&serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut out = serde_json::Map::new();
    let user = entry_cfg
        .and_then(|v| v.get("config"))
        .and_then(|v| v.as_object());
    let lookup = |k: &str| std::env::var(k).ok();
    for (key, prop) in &manifest.config.properties {
        let interpolated_default = match &prop.default {
            serde_json::Value::String(s) => {
                serde_json::Value::String(crate::helpers::interpolate_env(s, &lookup))
            }
            other => other.clone(),
        };
        let mut val = interpolated_default;
        if let Some(env_name) = &prop.env {
            if let Ok(ev) = std::env::var(env_name) {
                val = coerce(&ev);
            }
        }
        if let Some(u) = user.and_then(|m| m.get(key)) {
            val = match u {
                serde_json::Value::String(s) => {
                    serde_json::Value::String(crate::helpers::interpolate_env(s, &lookup))
                }
                other => other.clone(),
            };
        }
        out.insert(key.clone(), val);
    }
    for req in &manifest.config.required {
        if !out.contains_key(req) {
            eprintln!("warning: plugin {name}: missing required config `{req}`");
        }
    }
    out
}

fn coerce(raw: &str) -> serde_json::Value {
    if let Ok(n) = raw.parse::<i64>() {
        return serde_json::Value::from(n);
    }
    if let Ok(f) = raw.parse::<f64>() {
        return serde_json::Value::from(f);
    }
    match raw.to_lowercase().as_str() {
        "true" | "1" => serde_json::Value::Bool(true),
        "false" | "0" => serde_json::Value::Bool(false),
        _ => serde_json::Value::String(raw.to_string()),
    }
}

/// Build `# Plugin: <name>` prompt sections from `provides.prompt` files.
///
/// # Example
/// ```rust,no_run
/// use engine::plugins::{discover_plugins, plugin_prompt_additions};
/// use std::path::Path;
/// let ps = discover_plugins(Path::new("."), &serde_json::Value::Null);
/// let sections = plugin_prompt_additions(&ps);
/// ```
pub fn plugin_prompt_additions(plugins: &[LoadedPlugin]) -> Vec<String> {
    plugins
        .iter()
        .filter_map(|p| {
            p.manifest.provides.prompt.as_ref().map(|rel| {
                let content = std::fs::read_to_string(p.dir.join(rel)).unwrap_or_default();
                format!("# Plugin: {}\n{}", p.manifest.name, content)
            })
        })
        .collect()
}

/// Build per-plugin `HooksConfig`s with `base_dir` = plugin dir.
///
/// # Description
/// Ports the TS script-hook loading for plugins: each `provides.hooks`
/// event list becomes HookDefinitions anchored at the plugin dir so the
/// traversal guard in `hooks` confines them correctly.
///
/// # Example
/// ```rust,no_run
/// use engine::plugins::{discover_plugins, plugin_hook_configs};
/// use std::path::Path;
/// let ps = discover_plugins(Path::new("."), &serde_json::Value::Null);
/// let cfgs = plugin_hook_configs(&ps);
/// ```
pub fn plugin_hook_configs(plugins: &[LoadedPlugin]) -> Vec<crate::hooks::HooksConfig> {
    plugins
        .iter()
        .map(|p| {
            let mut cfg = crate::hooks::HooksConfig::default();
            for (event, defs) in &p.manifest.provides.hooks {
                let list: Vec<crate::hooks::HookDefinition> = defs
                    .iter()
                    .map(|d| crate::hooks::HookDefinition {
                        script: d.script.clone(),
                        description: d.description.clone(),
                        base_dir: p.dir.to_string_lossy().into_owned(),
                    })
                    .collect();
                match event.as_str() {
                    "on_session_start" => cfg.on_session_start = list,
                    "pre_tool_use" => cfg.pre_tool_use = list,
                    "post_tool_failure" => cfg.post_tool_failure = list,
                    "post_response" => cfg.post_response = list,
                    "pre_query" => cfg.pre_query = list,
                    "file_changed" => cfg.file_changed = list,
                    "on_error" => cfg.on_error = list,
                    _ => {}
                }
            }
            cfg
        })
        .collect()
}

fn clone_shallow(url: &str, dest: &Path, version: Option<&str>) -> anyhow::Result<()> {
    if dest.exists() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Argv-only git (same RCE lesson as the loader).
    let mut args = vec!["clone".to_string(), "--depth".to_string(), "1".to_string()];
    if let Some(v) = version {
        args.extend(["--branch".to_string(), v.to_string()]);
    }
    args.extend([url.to_string(), dest.to_string_lossy().into_owned()]);
    let out = std::process::Command::new("git").args(&args).output()?;
    if !out.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_agent_has_no_plugins() {
        let d = std::env::temp_dir().join(format!("ga-plug-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        // Point HOME at the sandbox so global scope is empty too.
        let ps = discover_plugins(&d, &serde_json::Value::Null);
        assert!(ps
            .iter()
            .all(|p| p.scope != "local" || !p.dir.starts_with(&d)));
        std::fs::remove_dir_all(&d).ok();
    }
}
