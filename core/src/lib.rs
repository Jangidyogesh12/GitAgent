//! ============================================================================
//! Crate: engine (lives in the core/ folder)
//! ----------------------------------------------------------------------------
//! WHAT THIS CRATE IS FOR:
//!   The backbone of the workspace: all agent logic lives here as modules,
//!   and both `sdk` and `cli` build on top of it. A faithful Rust port of the
//!   TypeScript GitAgent (`src/`) plus its two npm engine dependencies
//!   (pi-agent-core → `agent`, pi-ai → `llm`).
//!
//! WHY "engine" AND NOT "core": the folder is `core/`, but a library package
//! named `core` breaks every proc macro that emits `core::…` paths
//! (`#[async_trait]`, `#[derive(Parser)]`, `#[tokio::main]` resolve them to
//! our crate instead of std). So the folder is `core/`, the crate is
//! `engine`, and paths read `engine::<module>::…`.
//!
//! DESIGN PATTERNS USED (https://refactoring.guru/design-patterns/rust):
//!   Strategy, Template Method, Observer, Builder, Factory, Adapter, Facade,
//!   Decorator, Command, Chain of Responsibility, State, Plugin, RAII —
//!   each module header names the ones it uses.
//!
//! MODULES PRESENT IN THIS CRATE:
//!   * `helpers`      — shared utils (env, fs, text budgets, frontmatter, jsonl).
//!   * `agent`        — LLM-agnostic agent loop, transcript, tool/gate seams.
//!   * `llm`          — OpenAI-compatible streaming provider + fallback/retry.
//!   * `manifest`     — pure `agent.yaml` schema + scaffold template.
//!   * `loader`       — agent dir → assembled system prompt (`LoadedAgent`).
//!   * `tools`        — cli/read/write/edit/memory + declarative YAML tools.
//!   * `learning`     — task_tracker + skill_learner + reinforcement math.
//!   * `hooks`        — hooks.yaml lifecycle scripts + gate adapter.
//!   * `plugins`      — plugin.yaml discovery/install/config.
//!   * `mcp`          — MCP stdio client (namespaced tools, fail-soft).
//!   * `session`      — repo clone → session branch → push + PAT scrub.
//!   * `observe`      — cost tracker, audit log, chat history, telemetry.
//!   * `integrations` — OpenCode/NanoBot/OpenClaw/ClaudeCode/Lyzr adapters.
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::manifest::AgentManifest;
//! let m = AgentManifest::scaffold("demo");
//! assert_eq!(m.max_turns(), 50);
//! ```
//! ============================================================================

pub mod agent;
pub mod helpers;
pub mod hooks;
pub mod integrations;
pub mod learning;
pub mod llm;
pub mod loader;
pub mod manifest;
pub mod mcp;
pub mod observe;
pub mod plugins;
pub mod session;
pub mod tools;
