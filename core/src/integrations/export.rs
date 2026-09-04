//! ============================================================================
//! Module: engine::integrations::export
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The export Facade + model-string helpers. `export_for()` picks the
//!   adapter (Factory) and runs it; `lyzr_model_string()` builds the
//!   `lyzr:<agent-id>@<base>` convention from any model string;
//!   `normalise_model_string()` applies harness-specific tweaks (currently
//!   Lyzr-aware, pass-through otherwise).
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `export_for()`              — harness + fields + out_dir → files.
//!   * `lyzr_model_string()`       — ensure `lyzr:<id>@<base>` shape.
//!   * `normalise_model_string()`  — per-harness model tweaks.
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::integrations::lyzr_model_string;
//! assert_eq!(lyzr_model_string("abc", "https://base"), "lyzr:abc@https://base");
//! ```
//! ============================================================================

use anyhow::Result;
use std::path::Path;

use crate::integrations::adapters::ExportInput;
use crate::integrations::harness::{adapter_for, Harness};

/// Export one agent to one harness (Facade).
///
/// # Description
/// Thin wrapper over `adapter_for(harness).export(...)` so callers never
/// touch adapters directly. `model` should be the preferred spec
/// (`provider:model[@base]`); `mcp_servers` passes through where supported.
///
/// # Example
/// ```rust,no_run
/// use engine::integrations::{Harness, export_for};
/// use std::path::Path;
/// let files = export_for(Harness::NanoBot, "demo", "prompt", &["cli".to_string()], "m", &[], &serde_json::Value::Null, Path::new("/tmp/o")).unwrap();
/// ```
#[allow(clippy::too_many_arguments)]
pub fn export_for(
    harness: Harness,
    name: &str,
    system_prompt: &str,
    tools: &[String],
    model: &str,
    skills: &[String],
    mcp_servers: &serde_json::Value,
    out_dir: &Path,
) -> Result<Vec<String>> {
    let input = ExportInput {
        name: name.to_string(),
        system_prompt: system_prompt.to_string(),
        tools: tools.to_vec(),
        model: normalise_model_string(harness, model),
        skills: skills.to_vec(),
        mcp_servers: mcp_servers.clone(),
    };
    adapter_for(harness).export(&input, out_dir)
}

/// Build the Lyzr model string `lyzr:<agent-id>@<base>`.
///
/// # Description
/// Already-shaped strings pass through; a bare id gets wrapped; any other
/// `provider:model` is returned unchanged (the caller chose a non-Lyzr
/// model — don't mangle it).
///
/// # Example
/// ```rust
/// use engine::integrations::lyzr_model_string;
/// assert_eq!(lyzr_model_string("abc", "https://b"), "lyzr:abc@https://b");
/// assert_eq!(lyzr_model_string("openai:gpt-4o-mini", "https://b"), "openai:gpt-4o-mini");
/// ```
pub fn lyzr_model_string(model: &str, base: &str) -> String {
    if model.starts_with("lyzr:") && model.contains('@') {
        return model.to_string();
    }
    if !model.contains(':') {
        return format!("lyzr:{model}@{base}");
    }
    model.to_string()
}

/// Apply harness-specific model tweaks (Lyzr-aware, else pass-through).
///
/// # Example
/// ```rust
/// use engine::integrations::{normalise_model_string, Harness};
/// assert_eq!(normalise_model_string(Harness::OpenCode, "m"), "m");
/// ```
pub fn normalise_model_string(harness: Harness, model: &str) -> String {
    match harness {
        Harness::Lyzr => lyzr_model_string(model, "https://agent-prod.studio.lyzr.ai/v4"),
        _ => model.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lyzr_wrapping() {
        assert_eq!(lyzr_model_string("abc", "https://b"), "lyzr:abc@https://b");
        assert_eq!(
            lyzr_model_string("lyzr:abc@https://b", "https://x"),
            "lyzr:abc@https://b"
        );
    }
}
