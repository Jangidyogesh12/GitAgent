//! ============================================================================
//! Module: engine::agent
//! ----------------------------------------------------------------------------
//! WHAT THIS MODULE IS FOR:
//!   The LLM-agnostic agent engine — a faithful Rust port of the
//!   `@mariozechner/pi-agent-core` loop that the TypeScript GitAgent builds on
//!   (see study.md §2 "the agent loop"). NOTHING here knows about OpenAI,
//!   Anthropic, files, or git; those live in sibling modules. This module only
//!   defines the conversation model, the tool/gate seams, and the
//!   think → act → observe loop.
//!
//! DESIGN PATTERNS USED (https://refactoring.guru/design-patterns/rust):
//!   * Strategy  — `AgentTool` + `ToolGate` + `LlmClient` are trait objects;
//!     the loop works against the traits, concrete tools/models plug in.
//!   * Template Method — `run_loop()` is the fixed skeleton (inject steering,
//!     check budget, ask model, run tools, repeat); `LoopConfig` injects the
//!     variable steps (client, compactor, gates).
//!   * Observer  — the loop publishes `AgentEvent`s over a tokio mpsc channel;
//!     CLI/SDK subscribe without the loop knowing who listens.
//!   * Builder   — `Agent::new(...).with_max_turns(...).with_gates(...)`.
//!   * Chain of Responsibility — gates are consulted in order; first
//!     Deny/Modify wins (see `gate.rs`).
//!
//! MODULES PRESENT IN THIS MODULE:
//!   * `message`  — transcript types (AgentMessage, ContentBlock, Usage...).
//!   * `event`    — AgentEvent lifecycle enum (the Observer payload).
//!   * `tool`     — AgentTool trait + ExecutionMode + ToolOutput.
//!   * `gate`     — ToolGate trait + GateDecision (beforeToolCall seam).
//!   * `client`   — LlmClient Strategy trait (implemented by llm).
//!   * `compact`  — Compactor (context-window budgeting, failsafe truncate).
//!   * `runner`   — Agent stateful wrapper + `run_loop()` Template Method.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::agent::{Agent, NoopClient};
//! // `NoopClient` echoes a canned reply — handy for tests without an API key.
//! let agent = Agent::new("You are helpful.".into(), vec![], "mock:echo");
//! ```
//! ============================================================================

pub mod client;
pub mod compact;
pub mod event;
pub mod gate;
pub mod message;
pub mod runner;
pub mod tool;

pub use client::{GenParams, LlmClient, NoopClient};
pub use compact::Compactor;
pub use event::{AgentEvent, DeltaKind};
pub use gate::{AllowAllGate, GateDecision, ToolGate};
pub use message::{
    AgentMessage, AssistantMessage, ContentBlock, StopReason, ToolResultMessage, Usage,
};
pub use runner::{run_loop, Agent, LoopConfig, LoopContext};
pub use tool::{AgentTool, ExecutionMode, ToolOutput};
