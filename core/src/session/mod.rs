//! ============================================================================
//! Crate: session
//! ----------------------------------------------------------------------------
//! WHAT THIS CRATE IS FOR:
//!   "Local repo mode" — clone a GitHub repo, work on a session branch, push
//!   when done. Ports `src/session.ts`: `initLocalSession()` (authed URL,
//!   clone `--depth 1 --no-single-branch` or reuse dir, default-branch
//!   detect via `symbolic-ref refs/remotes/origin/HEAD` with main→master
//!   fallback, `reset --hard`, resume vs new `gitagent/session-<8hex>`
//!   branch, scaffold agent.yaml + memory), `commitChanges()`
//!   (skip-if-clean via `diff --cached --quiet`), `push()`, and `finalize()`
//!   (commit + push + PAT SCRUB from the remote URL — the security rule).
//!
//! DESIGN PATTERNS USED:
//!   * Facade — `init_local_session()` hides the whole git dance.
//!   * RAII — `LocalSession` owns the branch; `finalize()` must run even on
//!     error paths (the SDK/CLI call it in `finally`-equivalent code; a
//!     prior TS leak of sandbox VMs + PATs on blocked-hook paths is
//!     documented in Study.md).
//!
//! MODULES PRESENT IN THIS CRATE:
//!   * `local` — LocalSession + init/commit/push/finalize + url helpers.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! use engine::session::SessionOptions;
//! let opts = SessionOptions { url: "https://github.com/org/repo".into(), token: None, dir: "/tmp/w".into(), session: None };
//! ```
//! ============================================================================

pub mod local;

pub use local::{authed_url, clean_url, init_local_session, LocalSession, SessionOptions};
