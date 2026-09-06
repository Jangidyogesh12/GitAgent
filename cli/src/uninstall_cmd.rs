//! ============================================================================
//! Module: cli::uninstall_cmd (src/uninstall_cmd.rs)
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   `gitagent uninstall` — remove the gitagent binary from disk.
//!   Default removes just the binary; `--purge` also removes the global
//!   `~/.gitagent` dir (global .env, plugins, caches).
//!
//! HOW IT WORKS:
//!   * Locate the binary via `std::env::current_exe()` (falls back to
//!     `<bindir>/gitagent` resolution when that fails).
//!   * Delete the file; report the path. `--purge` deletes the global dir.
//!   * Removing a running binary works on Unix; on failure bail with a
//!     manual `rm` hint.
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `run()` — dispatch the uninstall.
//!   * `global_dir()` — path of the global `~/.gitagent` dir.
//!
//! HOW TO USE (examples):
//! ```bash
//! gitagent uninstall            # remove the binary, keep ~/.gitagent
//! gitagent uninstall --purge    # also remove ~/.gitagent
//! ```
//! ============================================================================

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Remove the gitagent binary (and optionally global data).
///
/// # Description
/// Deletes the running executable's file. With `purge`, also deletes the
/// global `~/.gitagent` dir (keys, plugins, caches) — agent dirs and their
/// `.env` files are never touched. Errors are user-facing (CLI prints
/// them + exits non-zero).
pub fn run(purge: bool) -> Result<()> {
    let exe = std::env::current_exe().context("locating the gitagent binary")?;
    println!("removing {}", exe.display());
    std::fs::remove_file(&exe).with_context(|| {
        format!(
            "removing {} failed — try: rm \"{}\"",
            exe.display(),
            exe.display()
        )
    })?;
    println!("uninstalled gitagent ({})", exe.display());

    if purge {
        let global = global_dir();
        if global.exists() {
            std::fs::remove_dir_all(&global)
                .with_context(|| format!("removing {} failed", global.display()))?;
            println!("purged {}", global.display());
        } else {
            println!("nothing to purge at {}", global.display());
        }
    } else {
        println!("kept {} (use --purge to remove it)", global_dir().display());
    }
    Ok(())
}

/// Path of the global `~/.gitagent` dir (env, plugins, caches).
///
/// # Example
/// ```rust,ignore
/// # use cli::uninstall_cmd::global_dir;
/// assert!(global_dir().ends_with(".gitagent"));
/// ```
pub fn global_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".gitagent")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_dir_name() {
        assert_eq!(
            global_dir().file_name().and_then(|n| n.to_str()),
            Some(".gitagent")
        );
    }
}
