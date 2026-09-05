//! ============================================================================
//! Module: engine::llm::spec
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Model-spec parsing plus provider tables. Handles `provider:model`
//!   and `provider/model` spellings with optional `@base-url` override,
//!   per-provider default base URLs and API-key env lookup (including the
//!   short alias env var and the shared gateway key), plus context-window,
//!   cost, and retryable-error tables.
//!
//! DESIGN PATTERNS USED:
//!   * Factory — `resolve_model()` manufactures a ready `ModelSpec`.
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `ModelSpec`           — model_id plus base_url plus api_key triple.
//!   * `resolve_model()`     — parse a spec string into a `ModelSpec`.
//!   * `context_window()`    — tokens of context per model family.
//!   * `cost_per_mtok()`     — (input, output) USD per million tokens.
//!   * `is_transient_error()`— retryable vs fatal error classifier.
//!
//! HOW IT WORKS:
//!   * Colon form splits provider at the first `:`; slash form (no colon)
//!     splits at the first `/`. A trailing `@http...` segment or the model
//!     base URL env var overrides the default endpoint. Keys resolve from
//!     `<PROVIDER>_API_KEY` with gateway fallbacks; empty means keyless
//!     local endpoints. Missing separators panic (fail fast on bad config).
//!   * Only chat-completions style endpoints are callable here; models
//!     living on other protocols surface as clean failure messages.
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::llm::resolve_model;
//! let s = resolve_model("ollama:gemma3:4b");
//! assert!(s.base_url.contains("11434"));
//! ```
//! ============================================================================

use serde::{Deserialize, Serialize};

/// A resolved model endpoint: everything needed to place a call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    /// Provider prefix (`openai`, `anthropic`, `ollama`, `lyzr`, ...).
    pub provider: String,
    /// Model id sent as `model` in the request body.
    pub model_id: String,
    /// OpenAI-compatible base URL (no trailing slash).
    pub base_url: String,
    /// Bearer token (may be empty for keyless local endpoints).
    pub api_key: String,
}

/// Parse `provider:model[@base-url]` into a `ModelSpec` (Factory).
///
/// # Description
/// Provider defaults: `ollama` maps to localhost:11434, `anthropic` to the
/// Anthropic endpoint, `opencode` to the Zen gateway and `opencode-go` to
/// the Go plan path, both keyed from the shared gateway key, else OpenAI.
/// Both `provider:model` and `provider/model` spellings are accepted — with
/// no `:` present, the first `/` splits provider from model. `@base-url`
/// or the model base URL env var overrides. The API key comes from
/// `<PROVIDER>_API_KEY` (uppercased, `-` mapped to `_`) with alias and
/// gateway fallbacks. Missing key yields an empty string for local
/// endpoints. Panics when neither `:` nor `/` is present.
///
/// Zen/Go note: only the chat-completions family speaks OpenAI chat SSE
/// here (DeepSeek, Kimi, GLM, MiniMax, and similar). Models living on other
/// endpoint families with different protocols are NOT callable through this
/// client — the API error surfaces as a clean failure message.
///
/// # Example
/// ```rust
/// use engine::llm::resolve_model;
/// let s = resolve_model("openai:gpt-4o-mini@http://localhost:8090/v1");
/// assert_eq!(s.model_id, "gpt-4o-mini");
/// let z = resolve_model("opencode/kimi-k2.6");
/// assert_eq!(z.base_url, "https://opencode.ai/zen/v1");
/// ```
pub fn resolve_model(spec: &str) -> ModelSpec {
    let (provider, rest) = match spec.find(':') {
        Some(colon) => {
            let (p, r) = spec.split_at(colon);
            (p, &r[1..])
        }
        None => {
            // OpenCode-style `provider/model` (no colon): split on first `/`.
            // (Safe: base URLs only ever appear AFTER a colon `@base` part.)
            let slash = spec.find('/').unwrap_or_else(|| {
                panic!("model spec must be provider:model or provider/model, got {spec:?}")
            });
            let (p, r) = spec.split_at(slash);
            (p, &r[1..])
        }
    };
    let (model_id, at_base) = match rest.rsplit_once('@') {
        Some((m, b)) if b.starts_with("http") => (m.to_string(), Some(b.to_string())),
        _ => (rest.to_string(), None),
    };
    let base_url = at_base
        .or_else(|| std::env::var("GITAGENT_MODEL_BASE_URL").ok())
        .unwrap_or_else(|| default_base_url(provider));
    let env_key = format!("{}_API_KEY", provider.to_uppercase().replace('-', "_"));
    let mut api_key = std::env::var(&env_key).unwrap_or_default();
    if api_key.is_empty() && provider == "lyzr" {
        api_key = std::env::var("LYZR_API_KEY").unwrap_or_default();
    }
    if api_key.is_empty() && (provider == "opencode" || provider == "opencode-go") {
        // `opencode-go` would otherwise map to a suffixed key name, which
        // does not exist — both plans authenticate with the shared key.
        api_key = std::env::var("OPENCODE_API_KEY").unwrap_or_default();
    }
    ModelSpec {
        provider: provider.to_string(),
        model_id,
        base_url,
        api_key,
    }
}

fn default_base_url(provider: &str) -> String {
    match provider {
        "ollama" => "http://localhost:11434/v1".to_string(),
        "anthropic" => "https://api.anthropic.com/v1".to_string(),
        // OpenCode Zen gateway (OpenAI-compatible chat-completions family;
        // key from the shared gateway key via the rule below) and the Go plan,
        // which lives under a separate path segment.
        "opencode" => "https://opencode.ai/zen/v1".to_string(),
        "opencode-go" => "https://opencode.ai/zen/go/v1".to_string(),
        "google" | "gemini" => {
            "https://generativelanguage.googleapis.com/v1beta/openai".to_string()
        }
        "xai" => "https://api.x.ai/v1".to_string(),
        "groq" => "https://api.groq.com/openai/v1".to_string(),
        "mistral" => "https://api.mistral.ai/v1".to_string(),
        _ => "https://api.openai.com/v1".to_string(),
    }
}

/// Context window (tokens) per model family.
///
/// # Description
/// Built-in table: Gemini and GPT-4.1 class at 1M, Claude and o-series
/// reasoning models at 200k, default 128k. Unknown names yield 128k.
///
/// # Example
/// ```rust
/// use engine::llm::context_window;
/// assert_eq!(context_window("gpt-4o-mini"), 128_000);
/// ```
pub fn context_window(model_id: &str) -> usize {
    let m = model_id.to_lowercase();
    if m.contains("gemini") || m.contains("gpt-4.1") {
        1_000_000
    } else if m.contains("claude")
        || m.contains("gpt-5")
        || m.starts_with("o1")
        || m.starts_with("o3")
        || m.starts_with("o4")
    {
        200_000
    } else {
        128_000
    }
}

/// (input, output) USD cost per million tokens.
///
/// # Description
/// Small built-in table; unknown models yield (0,0) and the caller falls
/// back to usage-based estimation.
///
/// # Example
/// ```rust
/// use engine::llm::cost_per_mtok;
/// let (i, o) = cost_per_mtok("gpt-4o-mini");
/// assert!(i > 0.0 && o > 0.0);
/// ```
pub fn cost_per_mtok(model_id: &str) -> (f64, f64) {
    let m = model_id.to_lowercase();
    if m.contains("gpt-4o-mini") {
        (0.15, 0.60)
    } else if m.contains("gpt-4o") {
        (2.50, 10.0)
    } else if m.contains("claude-sonnet") {
        (3.0, 15.0)
    } else if m.contains("claude-haiku") {
        (0.80, 4.0)
    } else if m.contains("claude-opus") {
        (15.0, 75.0)
    } else {
        (0.0, 0.0)
    }
}

/// True for retryable failures (429/5xx/timeout/connect/DNS...).
///
/// # Description
/// Only transient errors are retried — retrying a 401/400 duplicates nothing
/// but wastes time. Message sniffing (not status codes) because the error
/// surfaces as a string from several transports.
///
/// # Example
/// ```rust
/// use engine::llm::is_transient_error;
/// assert!(is_transient_error("429 Too Many Requests"));
/// assert!(!is_transient_error("401 Unauthorized"));
/// ```
pub fn is_transient_error(msg: &str) -> bool {
    let m = msg.to_lowercase();
    m.contains("429")
        || m.contains("500")
        || m.contains("502")
        || m.contains("503")
        || m.contains("504")
        || m.contains("timeout")
        || m.contains("timed out")
        || m.contains("connection")
        || m.contains("dns")
        || m.contains("reset")
        || m.contains("temporarily")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_provider_model_and_base() {
        let s = resolve_model("openai:gpt-4o-mini@http://localhost:8090/v1");
        assert_eq!(
            (s.provider.as_str(), s.model_id.as_str()),
            ("openai", "gpt-4o-mini")
        );
        assert_eq!(s.base_url, "http://localhost:8090/v1");
    }

    #[test]
    fn ollama_defaults_to_localhost() {
        assert!(resolve_model("ollama:gemma3:4b").base_url.contains("11434"));
    }

    #[test]
    fn opencode_slash_form_resolves_to_zen() {
        let s = resolve_model("opencode/kimi-k2.6");
        assert_eq!(s.provider.as_str(), "opencode");
        assert_eq!(s.model_id, "kimi-k2.6");
        assert_eq!(s.base_url, "https://opencode.ai/zen/v1");
    }

    #[test]
    fn opencode_colon_form_also_works() {
        let s = resolve_model("opencode:deepseek-v4-flash");
        assert_eq!(s.model_id, "deepseek-v4-flash");
        assert_eq!(s.base_url, "https://opencode.ai/zen/v1");
    }

    #[test]
    fn opencode_go_resolves_to_go_endpoint() {
        let s = resolve_model("opencode-go/kimi-k2.6");
        assert_eq!(s.provider.as_str(), "opencode-go");
        assert_eq!(s.base_url, "https://opencode.ai/zen/go/v1");
    }

    #[test]
    fn transient_classifier() {
        assert!(is_transient_error("503 Service Unavailable"));
        assert!(!is_transient_error("401 bad key"));
    }
}
