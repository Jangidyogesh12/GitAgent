//! ============================================================================
//! Crate: llm
//! ----------------------------------------------------------------------------
//! WHAT THIS CRATE IS FOR:
//!   The LLM provider layer — the Rust replacement for `@mariozechner/pi-ai`
//!   (the model registry the TS code imported). ONE OpenAI-compatible
//!   streaming client covers OpenAI, Anthropic-via-gateway, Ollama, Lyzr and
//!   any `--base-url` endpoint, because they all speak the same wire format.
//!
//! DESIGN PATTERNS USED:
//!   * Strategy — `OpenAiCompat` implements `engine::agent::LlmClient`, so
//!     the engine stays provider-agnostic.
//!   * Factory — `resolve_model()` builds a `ModelSpec` from a
//!     `provider:model[@base-url]` string (the TS `getModel` replacement).
//!   * Chain of Responsibility — `complete_with_fallback()` tries preferred
//!     then fallbacks in order, with bounded retries on transient errors.
//!   * Decorator — retry/backoff wraps the raw SSE call without changing it.
//!
//! MODULES PRESENT IN THIS CRATE:
//!   * `spec`     — ModelSpec + resolve_model() + context/cost tables.
//!   * `compat`   — OpenAiCompat Strategy implementation (SSE streaming).
//!   * `fallback` — resilient multi-spec driver (retries + failover).
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::llm::{resolve_model, OpenAiCompat};
//! let spec = resolve_model("openai:gpt-4o-mini");
//! assert_eq!(spec.base_url, "https://api.openai.com/v1");
//! ```
//! ============================================================================

pub mod compat;
pub mod fallback;
pub mod spec;

pub use compat::OpenAiCompat;
pub use fallback::complete_with_fallback;
pub use spec::{context_window, cost_per_mtok, is_transient_error, resolve_model, ModelSpec};
