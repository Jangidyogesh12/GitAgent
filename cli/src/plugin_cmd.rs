//! ============================================================================
//! Module: cli::plugin_cmd (src/plugin_cmd.rs)
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   `gitagent plugin ...` — install/list/remove/enable/disable/init.
//!   Ports `src/plugin-cli.ts`: git-URL → clone into `.gitagent/plugins/`,
//!   local path → copy into `plugins/`, manifest edits via comment-friendly
//!   YAML round-trip (we re-serialise only the `plugins:` table, preserving
//!   the rest byte-for-byte where possible), init scaffolds plugin.yaml +
//!   tools/ + hooks/ + skills/ + README.
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `run()` — dispatch install|list|remove|enable|disable|init.
//!
//! HOW TO USE (examples):
//! ```bash
//! gitagent plugin install https://github.com/org/my-plugin --dir ./my-agent
//! gitagent plugin list --dir ./my-agent
//! gitagent plugin init my-plugin --dir ./my-agent
//! ```
//! ============================================================================

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Dispatch one `gitagent plugin <action> [target]` command.
///
/// # Description
/// Actions: `install <src>` (git URL or local path), `list|ls`, `remove|rm
/// <name>`, `enable <name>`, `disable <name>`, `init|create <name>`.
/// Enablement lives in `agent.yaml → plugins.<name>.enabled`; install
/// records `source` too. Errors are user-facing messages (the CLI prints
/// them + exits non-zero).
pub fn run(dir: &Path, action: &str, target: Option<&str>, force: bool) -> Result<()> {
    match action {
        "install" => {
            let src = target.context("usage: gitagent plugin install <git-url-or-path>")?;
            install(dir, src, force)
        }
        "list" | "ls" => list(dir),
        "remove" | "rm" => {
            let name = target.context("usage: gitagent plugin remove <name>")?;
            remove(dir, name)
        }
        "enable" => {
            let name = target.context("usage: gitagent plugin enable <name>")?;
            set_enabled(dir, name, true)
        }
        "disable" => {
            let name = target.context("usage: gitagent plugin disable <name>")?;
            set_enabled(dir, name, false)
        }
        "init" | "create" => {
            let name = target.context("usage: gitagent plugin init <name>")?;
            init(dir, name)
        }
        _ => anyhow::bail!(
            "unknown plugin action: {action} (install|list|remove|enable|disable|init)"
        ),
    }
}

fn manifest_path(dir: &Path) -> PathBuf {
    dir.join("agent.yaml")
}

fn load_table(dir: &Path) -> serde_json::Value {
    engine::manifest::load_manifest(&manifest_path(dir))
        .map(|m| m.plugins)
        .unwrap_or(serde_json::Value::Null)
}

/// Install from a git URL (→ `.gitagent/plugins/`) or local path (→ `plugins/`).
fn install(dir: &Path, src: &str, force: bool) -> Result<()> {
    let is_url = src.starts_with("http") || src.starts_with("git@");
    let name = target_name(src);
    if is_url {
        let dest = dir.join(format!(".gitagent/plugins/{name}"));
        if dest.exists() {
            if !force {
                anyhow::bail!("{name} already installed (use --force to reinstall)");
            }
            std::fs::remove_dir_all(&dest).ok();
        }
        let out = std::process::Command::new("git")
            .args(["clone", "--depth", "1", src, &dest.to_string_lossy()])
            .output()?;
        if !out.status.success() {
            anyhow::bail!("clone failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        upsert_table(
            dir,
            &name,
            serde_json::json!({"enabled": true, "source": src}),
        )?;
    } else {
        let src_path = Path::new(src);
        let dest = dir.join(format!("plugins/{name}"));
        copy_dir(src_path, &dest)?;
        upsert_table(dir, &name, serde_json::json!({"enabled": true}))?;
    }
    println!("installed plugin {name}");
    Ok(())
}

/// List discovered plugins with scope + enabled state.
fn list(dir: &Path) -> Result<()> {
    let table = load_table(dir);
    let plugins = engine::plugins::discover_plugins(dir, &table);
    if plugins.is_empty() {
        println!("no plugins installed");
        return Ok(());
    }
    for p in plugins {
        let on = if p.enabled { "enabled" } else { "disabled" };
        println!(
            "{} [{}] ({on}) — {}",
            p.name, p.scope, p.manifest.description
        );
    }
    Ok(())
}

/// Remove from local + installed dirs and drop the manifest entry.
fn remove(dir: &Path, name: &str) -> Result<()> {
    for scope in ["plugins", ".gitagent/plugins"] {
        let p = dir.join(format!("{scope}/{name}"));
        if p.is_dir() {
            std::fs::remove_dir_all(&p).ok();
        }
    }
    remove_table_entry(dir, name)?;
    println!("removed plugin {name}");
    Ok(())
}

/// Flip `plugins.<name>.enabled` in agent.yaml.
fn set_enabled(dir: &Path, name: &str, enabled: bool) -> Result<()> {
    let mut table = load_table(dir);
    let obj = table
        .as_object_mut()
        .context("plugins table is not an object")?;
    let entry = obj.entry(name.to_string()).or_insert(serde_json::json!({}));
    entry["enabled"] = serde_json::Value::Bool(enabled);
    write_table(dir, &table)?;
    println!(
        "plugin {name} {}",
        if enabled { "enabled" } else { "disabled" }
    );
    Ok(())
}

/// Scaffold a new plugin skeleton (plugin.yaml + tools/ + hooks/ + skills/ + README).
fn init(dir: &Path, name: &str) -> Result<()> {
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        anyhow::bail!("plugin name must be kebab-case");
    }
    let base = dir.join(format!("plugins/{name}"));
    std::fs::create_dir_all(base.join("tools")).ok();
    std::fs::create_dir_all(base.join("hooks")).ok();
    std::fs::create_dir_all(base.join("skills")).ok();
    std::fs::write(
        base.join("plugin.yaml"),
        format!(
            "id: {name}\nname: {name}\nversion: 0.1.0\ndescription: TODO — describe this plugin\n\
             provides:\n  tools: true\n  skills: false\nconfig: {{}}\n"
        ),
    )?;
    std::fs::write(
        base.join("README.md"),
        format!("# {name}\n\nTODO — document this plugin.\n"),
    )?;
    println!("scaffolded plugin at {}", base.display());
    Ok(())
}

fn target_name(src: &str) -> String {
    let s = src.trim_end_matches('/').trim_end_matches(".git");
    s.rsplit('/').next().unwrap_or("plugin").to_string()
}

fn copy_dir(src: &Path, dest: &Path) -> Result<()> {
    if !src.is_dir() {
        anyhow::bail!("not a directory: {}", src.display());
    }
    std::fs::create_dir_all(dest)?;
    for e in walkdir::WalkDir::new(src).min_depth(1) {
        let e = e?;
        let rel = e.path().strip_prefix(src)?;
        let target = dest.join(rel);
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(p) = target.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::copy(e.path(), &target)?;
        }
    }
    Ok(())
}

fn upsert_table(dir: &Path, name: &str, entry: serde_json::Value) -> Result<()> {
    let mut table = load_table(dir);
    if table.is_null() {
        table = serde_json::json!({});
    }
    table[name] = entry;
    write_table(dir, &table)
}

fn remove_table_entry(dir: &Path, name: &str) -> Result<()> {
    let mut table = load_table(dir);
    if let Some(obj) = table.as_object_mut() {
        obj.remove(name);
        write_table(dir, &table)?;
    }
    Ok(())
}

/// Rewrite ONLY the `plugins:` table, preserving the rest of agent.yaml.
///
/// # Description
/// Round-trips the whole manifest through serde_yaml (key order stable for
/// mapping output) — the pragmatic equivalent of the TS comment-preserving
/// `yaml.parseDocument` edit. Documented as approximate in Study.md.
fn write_table(dir: &Path, table: &serde_json::Value) -> Result<()> {
    let path = manifest_path(dir);
    let text = std::fs::read_to_string(&path).context("reading agent.yaml")?;
    let mut doc: serde_yaml::Value = serde_yaml::from_str(&text)?;
    doc["plugins"] = serde_yaml::to_value(table)?;
    std::fs::write(&path, serde_yaml::to_string(&doc)?)?;
    Ok(())
}
