//! ============================================================================
//! Module: cli::update_cmd (src/update_cmd.rs)
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   `gitagent update` — reinstall the latest gitagent release in place.
//!   Default path needs no Rust toolchain: it pipes the repo's
//!   `installer/install-remote.sh` (prebuilt binary + checksum verify) into
//!   bash with GITAGENT_* env set. `--from-source` uses cargo instead.
//!
//! HOW IT WORKS:
//!   * Resolve: repo (flag > GITAGENT_REPO env > default), version (flag >
//!     GITAGENT_VERSION env > "latest"), bindir (GITAGENT_BINDIR env >
//!     current-exe parent > ~/.cargo/bin).
//!   * Binary path: `curl -fsSL <script-url> | bash` with env set; the
//!     installer picks the platform asset, verifies sha256, installs.
//!   * Source path: `cargo install --locked --git <url> --rev <rev>
//!     --root <root> --bin gitagent` where root derives from bindir.
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `run()` — dispatch the update.
//!   * `script_url()` — raw URL of install-remote.sh for a repo.
//!   * `install_root_for_bindir()` — cargo --root that lands in bindir.
//!   * `resolve_bindir()` — bindir resolution order.
//!
//! HOW TO USE (examples):
//! ```bash
//! gitagent update                          # latest prebuilt binary
//! gitagent update --version v0.2.0         # pin a release
//! gitagent update --from-source            # cargo build from main
//! ```
//! ============================================================================

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Default releases repo (override with `--repo` or `GITAGENT_REPO`).
pub const DEFAULT_REPO: &str = "Jangidyogesh12/GitAgent";

/// Reinstall gitagent in place (latest release by default).
///
/// # Description
/// - Binary path: shells out to the repo's `install-remote.sh` (needs
///   `curl` + `bash`).
/// - `--from-source`: uses `cargo install --git` (needs `cargo`).
/// - Prints the version before and after when possible; errors are
///   user-facing (the CLI prints them + exits non-zero).
pub fn run(version: Option<&str>, from_source: bool, repo: Option<&str>) -> Result<()> {
    let repo = repo
        .map(|s| s.to_string())
        .or_else(|| std::env::var("GITAGENT_REPO").ok())
        .unwrap_or_else(|| DEFAULT_REPO.to_string());
    let version = version
        .map(|s| s.to_string())
        .or_else(|| std::env::var("GITAGENT_VERSION").ok())
        .unwrap_or_else(|| "latest".to_string());
    let bindir = resolve_bindir();

    if let Ok(exe) = std::env::current_exe() {
        if let Ok(out) = std::process::Command::new(&exe).arg("--version").output() {
            if out.status.success() {
                println!("current: {}", String::from_utf8_lossy(&out.stdout).trim());
            }
        }
    }
    println!(
        "updating gitagent ({repo} @ {version}) → {} …",
        bindir.display()
    );

    if from_source {
        update_from_source(&repo, &version, &bindir)
    } else {
        update_from_release(&repo, &version, &bindir)
    }
}

/// Raw URL of `install-remote.sh` for a `owner/name` repo.
///
/// # Example
/// ```rust,ignore
/// # use cli::update_cmd::script_url;
/// assert_eq!(
///     script_url("acme/agent"),
///     "https://raw.githubusercontent.com/acme/agent/main/installer/install-remote.sh"
/// );
/// ```
pub fn script_url(repo: &str) -> String {
    format!("https://raw.githubusercontent.com/{repo}/main/installer/install-remote.sh")
}

/// Cargo `--root` that installs `gitagent` into `bindir`.
///
/// # Description
/// `cargo install --root <root>` puts the binary at `<root>/bin/gitagent`,
/// so when bindir already ends with `bin` the root is its parent;
/// otherwise the bindir itself is used and the binary lands one level
/// deeper (reported back to the user).
///
/// # Example
/// ```rust,ignore
/// # use cli::update_cmd::install_root_for_bindir;
/// # use std::path::Path;
/// assert_eq!(
///     install_root_for_bindir(Path::new("/home/u/.cargo/bin")),
///     std::path::PathBuf::from("/home/u/.cargo")
/// );
/// ```
pub fn install_root_for_bindir(bindir: &std::path::Path) -> PathBuf {
    if bindir.file_name().is_some_and(|n| n == "bin") {
        bindir
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| bindir.to_path_buf())
    } else {
        bindir.to_path_buf()
    }
}

/// Resolve the install directory for the binary.
///
/// # Description
/// Order: `GITAGENT_BINDIR` (or `GITAGENT_PREFIX/bin`) > parent dir of the
/// running executable > `~/.cargo/bin`. Missing parents are created by the
/// installer, not here.
///
/// # Example
/// ```rust,ignore
/// # use cli::update_cmd::resolve_bindir;
/// let dir = resolve_bindir();
/// assert!(!dir.as_os_str().is_empty());
/// ```
pub fn resolve_bindir() -> PathBuf {
    if let Ok(b) = std::env::var("GITAGENT_BINDIR") {
        if !b.is_empty() {
            return PathBuf::from(b);
        }
    }
    if let Ok(prefix) = std::env::var("GITAGENT_PREFIX") {
        if !prefix.is_empty() {
            return PathBuf::from(prefix).join("bin");
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            if !parent.as_os_str().is_empty() {
                return parent.to_path_buf();
            }
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".cargo/bin")
}

fn update_from_release(repo: &str, version: &str, bindir: &std::path::Path) -> Result<()> {
    let url = script_url(repo);
    // Pipe the installer into bash with env set (needs curl + bash).
    let shell = format!("curl -fsSL {url} | bash");
    let status = std::process::Command::new("bash")
        .arg("-c")
        .arg(&shell)
        .env("GITAGENT_REPO", repo)
        .env("GITAGENT_VERSION", version)
        .env("GITAGENT_BINDIR", bindir)
        .status()
        .context("running the release installer (needs curl + bash)")?;
    if !status.success() {
        anyhow::bail!("update failed (installer exit: {status}) — retry with --from-source");
    }
    verify(bindir)
}

fn update_from_source(repo: &str, version: &str, bindir: &std::path::Path) -> Result<()> {
    let root = install_root_for_bindir(bindir);
    let rev = if version == "latest" { "main" } else { version };
    let git_url = format!("https://github.com/{repo}.git");
    let status = std::process::Command::new("cargo")
        .args([
            "install",
            "--locked",
            "--git",
            &git_url,
            "--rev",
            rev,
            "--root",
            &root.to_string_lossy(),
            "--bin",
            "gitagent",
        ])
        .status()
        .context("running cargo install (needs cargo + git + network)")?;
    if !status.success() {
        anyhow::bail!("update failed (cargo exit: {status})");
    }
    verify(bindir)
}

fn verify(bindir: &std::path::Path) -> Result<()> {
    let bin = bindir.join("gitagent");
    let out = std::process::Command::new(&bin)
        .arg("--version")
        .output()
        .context("installed binary won't run")?;
    if !out.status.success() {
        anyhow::bail!("installed binary won't run: {}", bin.display());
    }
    println!("updated: {}", String::from_utf8_lossy(&out.stdout).trim());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_url_points_at_main() {
        assert_eq!(
            script_url("o/r"),
            "https://raw.githubusercontent.com/o/r/main/installer/install-remote.sh"
        );
    }

    #[test]
    fn root_derives_from_bindir() {
        assert_eq!(
            install_root_for_bindir(std::path::Path::new("/a/.cargo/bin")),
            PathBuf::from("/a/.cargo")
        );
        assert_eq!(
            install_root_for_bindir(std::path::Path::new("/a/tools")),
            PathBuf::from("/a/tools")
        );
    }
}
