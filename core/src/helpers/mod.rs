//! ============================================================================
//! Crate: helpers
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   This is the crate root (`lib.rs`). It declares and re-exports every
//!   shared utility module used by ALL other gitagent crates, so the rest of
//!   the workspace never duplicates helpers.
//!
//! DESIGN PATTERNS USED:
//!   * Facade — this file is a single front door (`use engine::helpers::...`)
//!     hiding the five sub-modules below.
//!   * Utility Module (refactoring.guru "no pattern", organised as pure
//!     functions) — every helper is stateless and independently testable.
//!
//! FUNCTIONS / MODULES PRESENT IN THIS FILE:
//!   * `pub mod env`         — re-exported; `.env` loading + `${VAR}` expansion
//!   * `pub mod fsx`         — re-exported; file helpers (read/write/paginate)
//!   * `pub mod text`        — re-exported; truncation + token estimation
//!   * `pub mod frontmatter` — re-exported; `--- yaml ---` markdown parsing
//!   * `pub mod jsonl`       — re-exported; append-only `.jsonl` log writer
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::helpers::{interpolate_env, estimate_tokens, truncate_tail};
//! let out = interpolate_env("hello ${USER}", &|k| std::env::var(k).ok());
//! assert!(estimate_tokens("four chars = one token (roughly)") > 0);
//! ```
//! ============================================================================

pub mod env;
pub mod frontmatter;
pub mod fsx;
pub mod jsonl;
pub mod text;

pub use env::{interpolate_env, interpolate_value, load_dotenv_file, load_env_stack};
pub use frontmatter::{parse_frontmatter, split_frontmatter};
pub use fsx::{ensure_dir, paginate_lines, read_file_lossy, resolve_path, write_file_create_dirs};
pub use jsonl::append_jsonl;
pub use text::{estimate_tokens, truncate_head, truncate_middle, truncate_tail};
pub use text::{FACTORY_TRUNCATE, MAX_OUTPUT_CHARS, MAX_READ_BYTES, MAX_READ_LINES};
