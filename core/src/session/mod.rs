//! ============================================================================
//! Crate: session
//! ----------------------------------------------------------------------------
//! WHAT THIS CRATE IS FOR:
//!   "Local repo mode" — clone a GitHub repo, work on a session branch, push
//!   when done. Inputs: `SessionOptions` (repo URL, optional token, work
//!   dir, optional session id). Steps: `init_local_session()` builds an
//!   authed URL, clones `--depth 1 --no-single-branch` (or reuses the dir),
//!   detects the default branch via `symbolic-ref refs/remotes/origin/HEAD`
//!   with main→master fallback, runs `reset --hard`, then resumes the given
//!   session branch or creates `gitagent/session-<8hex>` and scaffolds
//!   agent.yaml + memory; `commitChanges()` skips when clean (checked via
//!   `diff --cached --quiet`); `push()` publishes; `finalize()` commits +
//!   pushes and scrubs the token out of the stored remote URL. Outputs:
//!   a `LocalSession` bound to the branch. Invariant: the token never
//!   persists in the remote URL after finalize.
//!
//! DESIGN PATTERNS USED:
//!   * Facade — `init_local_session()` hides the whole git dance.
//!   * RAII — `LocalSession` owns the branch; `finalize()` must run even on
//!     error paths (the SDK/CLI call it in `finally`-equivalent code so
//!     sandbox resources and tokens are never leaked on blocked paths).
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
