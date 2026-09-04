//! ============================================================================
//! Module: engine::manifest::model
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Model configuration types from `agent.yaml → model:` plus the
//!   provider→API-key-env mapping from `src/index.ts` (the startup key check).
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `ModelConstraints` — temperature/max_tokens/top_p/top_k/stop_sequences.
//!   * `ModelConfig`      — preferred + fallback + constraints.
//!   * `provider_api_key()` — provider name → required env var ("" = none).
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::manifest::provider_api_key;
//! assert_eq!(provider_api_key("anthropic"), "ANTHROPIC_API_KEY");
//! ```
//! ============================================================================

use serde::{Deserialize, Serialize};

/// Generation constraints from `model.constraints` (all optional).
///
/// # Description
/// Mirrors the TS manifest constraints; the SDK accepts both snake_case and
/// camelCase (`max_tokens`/`maxTokens`) — here serde aliases cover both.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelConstraints {
    /// Sampling temperature.
    #[serde(default)]
    pub temperature: Option<f64>,
    /// Max output tokens per turn.
    #[serde(default, alias = "maxTokens")]
    pub max_tokens: Option<u64>,
    /// Nucleus sampling cutoff.
    #[serde(default, alias = "topP")]
    pub top_p: Option<f64>,
    /// Top-k sampling cutoff.
    #[serde(default, alias = "topK")]
    pub top_k: Option<u64>,
    /// Stop sequences.
    #[serde(default)]
    pub stop_sequences: Option<Vec<String>>,
}

/// The `model:` section: preferred spec + fallbacks + constraints.
///
/// # Description
/// `preferred` format is `provider:model` or `provider:model@base-url`
/// (Lyzr looks like `lyzr:<agent-id>@https://agent-prod.studio.lyzr.ai/v4`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Preferred model (`provider:model[@base-url]`).
    #[serde(default)]
    pub preferred: String,
    /// Ordered fallbacks tried after the preferred fails.
    #[serde(default)]
    pub fallback: Vec<String>,
    /// Optional generation constraints.
    #[serde(default)]
    pub constraints: Option<ModelConstraints>,
}

/// Map a provider prefix to its required API-key env var.
///
/// # Description
/// Ports the TS startup key check (`anthropic`→`ANTHROPIC_API_KEY`, ...).
/// Returns `""` for keyless providers (`ollama`, `mock`) — the caller treats
/// empty as "no key required".
///
/// # Example
/// ```rust
/// use engine::manifest::provider_api_key;
/// assert_eq!(provider_api_key("openai"), "OPENAI_API_KEY");
/// assert_eq!(provider_api_key("ollama"), "");
/// ```
pub fn provider_api_key(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "ANTHROPIC_API_KEY",
        "openai" => "OPENAI_API_KEY",
        "google" | "gemini" => "GEMINI_API_KEY",
        "opencode" | "opencode-go" => "OPENCODE_API_KEY",
        "xai" => "XAI_API_KEY",
        "groq" => "GROQ_API_KEY",
        "mistral" => "MISTRAL_API_KEY",
        "lyzr" => "LYZR_API_KEY",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_map_matches_ts() {
        assert_eq!(provider_api_key("anthropic"), "ANTHROPIC_API_KEY");
        assert_eq!(provider_api_key("ollama"), "");
    }

    #[test]
    fn parses_constraints_with_camel_alias() {
        let c: ModelConfig =
            serde_yaml::from_str("preferred: openai:gpt-4o-mini\nconstraints:\n  maxTokens: 100\n")
                .unwrap();
        assert_eq!(c.constraints.unwrap().max_tokens, Some(100));
    }
}
