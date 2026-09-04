//! ============================================================================
//! Module: cli::integrations_cmd (src/integrations_cmd.rs)
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   `gitagent integrations ...` — harness interop from the terminal:
//!   `list` (all five harnesses + what each exports), `detect` (env/binary
//!   heuristics), `export --format <slug> --out <dir>` (render native files
//!   for OpenCode / NanoBot / OpenClaw / Claude Code / Lyzr).
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `run()` — dispatch list|detect|export.
//!
//! HOW TO USE (examples):
//! ```bash
//! gitagent integrations list
//! gitagent integrations detect --dir ./my-agent
//! gitagent integrations export --format opencode --out ./interop --dir ./my-agent
//! ```
//! ============================================================================

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Dispatch one `gitagent integrations <action>` command.
pub fn run(
    dir: &Path,
    action: Option<&str>,
    format: Option<&str>,
    out: Option<PathBuf>,
) -> Result<()> {
    match action.unwrap_or("list") {
        "list" => {
            for h in engine::integrations::all_harnesses() {
                println!(
                    "{} — {}\n  exports: {} — {}",
                    h.id(),
                    h.label(),
                    h.config_file(),
                    h.description()
                );
            }
            Ok(())
        }
        "detect" => {
            let found = engine::integrations::detect_available();
            if found.is_empty() {
                println!("no harness detected (set e.g. LYZR_API_KEY or ANTHROPIC_API_KEY)");
            } else {
                for h in found {
                    println!("detected: {} ({})", h.label(), h.id());
                }
            }
            Ok(())
        }
        "export" => {
            let slug = format.context("usage: integrations export --format <opencode|nanobot|openclaw|claude-code|lyzr> --out <dir>")?;
            // `slug` is `&str` (Copy): reuse in the error below is free.
            let harness = engine::integrations::Harness::parse(slug)
                .with_context(|| format!("unknown harness: {slug}"))?;
            let out_dir = out.context("usage: integrations export --format <slug> --out <dir>")?;
            let loaded = engine::loader::load_agent(dir, None, None).context("loading agent")?;
            let tools: Vec<String> = loaded.manifest.tools.clone();
            let skills: Vec<String> = engine::loader::discover_skills(dir)
                .into_iter()
                .map(|s| s.name)
                .collect();
            let model = loaded.model_specs.first().cloned().unwrap_or_default();
            let files = engine::integrations::export_for(
                harness,
                &loaded.manifest.name,
                &loaded.system_prompt,
                &tools,
                &model,
                &skills,
                &loaded.manifest.mcp_servers,
                &out_dir,
            )?;
            for f in files {
                println!("wrote {}", out_dir.join(f).display());
            }
            Ok(())
        }
        other => anyhow::bail!("unknown integrations action: {other} (list|detect|export)"),
    }
}
