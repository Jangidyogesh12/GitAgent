//! ============================================================================
//! Module: engine::session::local
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Git session management (see crate docs). ALL git runs through argv
//!   arrays — never a shell — so tokens/URLs cannot inject commands.
//!
//! TYPES / FUNCTIONS PRESENT IN THIS FILE:
//!   * `SessionOptions`     — {url, token, dir, session} inputs.
//!   * `LocalSession`       — {dir, branch, session_id} + methods.
//!   * `init_local_session()` — clone/reuse → branch → scaffold.
//!   * `authed_url()`       — inject token into https URL.
//!   * `clean_url()`        — strip token back out (PAT scrub).
//!
//! HOW TO USE (example):
//! ```rust
//! use engine::session::{authed_url, clean_url};
//! let a = authed_url("https://github.com/o/r", "TOK");
//! assert_eq!(a, "https://TOK@github.com/o/r");
//! assert_eq!(clean_url(&a), "https://github.com/o/r");
//! ```
//! ============================================================================

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Inputs to `init_local_session()` (mirrors the TS opts object).
#[derive(Debug, Clone)]
pub struct SessionOptions {
    /// Remote repo URL (https).
    pub url: String,
    /// Personal access token (or corrupts nothing when None).
    pub token: Option<String>,
    /// Local working dir (clone target / reuse dir).
    pub dir: PathBuf,
    /// Existing session branch to resume (None = new branch).
    pub session: Option<String>,
}

/// An active repo session: owns the branch until `finalize()`.
#[derive(Debug, Clone)]
pub struct LocalSession {
    /// Working directory.
    pub dir: PathBuf,
    /// Session branch (`gitagent/session-<id>`).
    pub branch: String,
    /// Short session id (8 hex for new sessions).
    pub session_id: String,
    /// Clean (token-free) remote URL, for scrubbing on finalize.
    pub clean: String,
}

impl LocalSession {
    /// Commit all changes unless the tree is clean (skip-if-clean rule).
    ///
    /// # Description
    /// `git add -A`, then `git diff --cached --quiet` (success = clean =
    /// skip); else commit with `msg` or the default
    /// `gitagent: auto-commit (<branch>)`.
    ///
    /// # Example
    /// ```rust,no_run
    /// use engine::session::{SessionOptions, init_local_session};
    /// # async fn demo(s: engine::session::LocalSession) { s.commit_changes(None).unwrap(); }
    /// ```
    pub fn commit_changes(&self, msg: Option<&str>) -> Result<bool> {
        git(&self.dir, &["add", "-A"])?;
        // `diff --cached --quiet`: exit 0 → nothing staged → skip.
        let clean = std::process::Command::new("git")
            .args(["diff", "--cached", "--quiet"])
            .current_dir(&self.dir)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if clean {
            return Ok(false);
        }
        let m = msg
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("gitagent: auto-commit ({})", self.branch));
        git(&self.dir, &["commit", "-m", &m])?;
        Ok(true)
    }

    /// Push the session branch to origin.
    ///
    /// # Example
    /// ```rust,no_run
    /// # async fn demo(s: engine::session::LocalSession) { s.push().unwrap(); }
    /// ```
    pub fn push(&self) -> Result<()> {
        git(&self.dir, &["push", "origin", &self.branch])?;
        Ok(())
    }

    /// Commit + push + SCRUB the PAT from the remote URL (RAII end).
    ///
    /// # Description
    /// The security-critical step: `git remote set-url origin <clean>` runs
    /// even when commit/push fail (best-effort), so tokens never linger in
    /// `.git/config`. Mirrors TS `finalize()`.
    ///
    /// # Example
    /// ```rust,no_run
    /// # async fn demo(s: engine::session::LocalSession) { s.finalize().unwrap(); }
    /// ```
    pub fn finalize(&self) -> Result<()> {
        let _ = self.commit_changes(None);
        let _ = self.push();
        // PAT scrub ALWAYS runs (best-effort) — even if push failed.
        let _ = std::process::Command::new("git")
            .args(["remote", "set-url", "origin", &self.clean])
            .current_dir(&self.dir)
            .output();
        Ok(())
    }
}

/// Clone/reuse the repo and check out the session branch (Facade).
///
/// # Description
///
/// New dir clones shallow; existing dirs are re-authed, fetched and reset.
/// Resume checks out or tracks the branch (best-effort pull); new sessions
/// get `gitagent/session-<8hex>` via `checkout -b`. Agent files are
/// scaffolded when missing (same template as `ensureRepo`).
///
/// # Example
/// ```rust,no_run
/// use engine::session::{init_local_session, SessionOptions};
/// use std::path::PathBuf;
/// let opts = SessionOptions { url: "https://github.com/o/r".into(), token: None, dir: PathBuf::from("/tmp/w"), session: None };
/// // let s = init_local_session(&opts).unwrap();
/// ```
pub fn init_local_session(opts: &SessionOptions) -> Result<LocalSession> {
    let token = opts.token.clone().unwrap_or_default();
    let authed = authed_url(&opts.url, &token);
    let clean = clean_url(&opts.url);
    std::fs::create_dir_all(&opts.dir).ok();

    if !opts.dir.join(".git").exists() {
        let out = std::process::Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                "--no-single-branch",
                &authed,
                &opts.dir.to_string_lossy(),
            ])
            .output()
            .context("git clone")?;
        if !out.status.success() {
            anyhow::bail!("git clone failed: {}", String::from_utf8_lossy(&out.stderr));
        }
    } else {
        git(&opts.dir, &["remote", "set-url", "origin", &authed])?;
        git(&opts.dir, &["fetch", "origin"])?;
        let default = default_branch(&opts.dir);
        git(&opts.dir, &["checkout", &default])?;
        let _ = git(
            &opts.dir,
            &["reset", "--hard", &format!("origin/{default}")],
        );
    }

    let (branch, session_id) = match &opts.session {
        Some(resume) => {
            // Resume: local branch or track the remote one; best-effort pull.
            let has_local = std::process::Command::new("git")
                .args(["rev-parse", "--verify", resume])
                .current_dir(&opts.dir)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if has_local {
                git(&opts.dir, &["checkout", resume])?;
            } else {
                git(
                    &opts.dir,
                    &["checkout", "-b", resume, &format!("origin/{resume}")],
                )?;
            }
            let _ = git(&opts.dir, &["pull", "--ff-only"]);
            let id = resume
                .strip_prefix("gitagent/session-")
                .unwrap_or(resume)
                .to_string();
            (resume.clone(), id)
        }
        None => {
            let id = hex8();
            let branch = format!("gitagent/session-{id}");
            git(&opts.dir, &["checkout", "-b", &branch])?;
            (branch, id)
        }
    };

    // Scaffold agent files on the session branch when missing.
    if !opts.dir.join("agent.yaml").is_file() {
        manifest_scaffold(&opts.dir)?;
    }
    if !opts.dir.join("memory/MEMORY.md").is_file() {
        std::fs::create_dir_all(opts.dir.join("memory")).ok();
        std::fs::write(opts.dir.join("memory/MEMORY.md"), "# Memory\n").ok();
    }
    Ok(LocalSession {
        dir: opts.dir.clone(),
        branch,
        session_id,
        clean,
    })
}

/// Inject a token into an https URL (`https://h/o/r` → `https://TOK@h/o/r`).
///
/// # Example
/// ```rust
/// use engine::session::authed_url;
/// assert_eq!(authed_url("https://h.o/r", "T"), "https://T@h.o/r");
/// assert_eq!(authed_url("https://T@h.o/r", "T"), "https://T@h.o/r");
/// ```
pub fn authed_url(url: &str, token: &str) -> String {
    if token.is_empty() || !url.starts_with("https://") {
        return url.to_string();
    }
    let rest = &url["https://".len()..];
    if rest.contains('@') {
        return url.to_string(); // already authed
    }
    format!("https://{token}@{rest}")
}

/// Strip credentials from a URL (`https://TOK@h/o/r` → `https://h/o/r`).
///
/// # Description
/// The PAT-scrub primitive: everything between `https://` and the LAST `@`
/// before the first `/` is credentials. Non-https URLs pass through.
///
/// # Example
/// ```rust
/// use engine::session::clean_url;
/// assert_eq!(clean_url("https://TOK@h.o/r"), "https://h.o/r");
/// ```
pub fn clean_url(url: &str) -> String {
    if !url.starts_with("https://") {
        return url.to_string();
    }
    let rest = &url["https://".len()..];
    let host_end = rest.find('/').unwrap_or(rest.len());
    if let Some(at) = rest[..host_end].rfind('@') {
        format!("https://{}", &rest[at + 1..])
    } else {
        url.to_string()
    }
}

fn default_branch(dir: &std::path::Path) -> String {
    // symbolic-ref refs/remotes/origin/HEAD → origin/<name>; fallbacks main→master.
    let out = std::process::Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .current_dir(dir)
        .output();
    if let Ok(o) = out {
        if o.status.success() {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if let Some(name) = s.rsplit('/').next() {
                if !name.is_empty() {
                    return name.to_string();
                }
            }
        }
    }
    for guess in ["main", "master"] {
        let ok = std::process::Command::new("git")
            .args(["rev-parse", "--verify", &format!("origin/{guess}")])
            .current_dir(dir)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            return guess.to_string();
        }
    }
    "main".to_string()
}

fn hex8() -> String {
    // 8 hex chars from time+pid (no rand dep needed for a branch suffix).
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0)
        ^ (std::process::id().wrapping_mul(0x9E3779B1));
    format!("{n:08x}")
}

fn git(dir: &std::path::Path, args: &[&str]) -> Result<()> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()?;
    if !out.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

fn manifest_scaffold(dir: &std::path::Path) -> Result<()> {
    // Same template family as ensureRepo (scaffold model, max_turns 50).
    let text = "spec_version: \"0.1.0\"\nname: session-agent\nversion: 0.1.0\n\
        description: Agent working on a repo session\nmodel:\n  preferred: openai:gpt-4o-mini\n  fallback: []\n\
        tools: [cli, read, write, memory]\nruntime:\n  max_turns: 50\n";
    std::fs::write(dir.join("agent.yaml"), text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_helpers_round_trip() {
        let a = authed_url("https://github.com/o/r", "TOK");
        assert_eq!(a, "https://TOK@github.com/o/r");
        assert_eq!(clean_url(&a), "https://github.com/o/r");
        assert_eq!(
            clean_url("git@github.com:o/r.git"),
            "git@github.com:o/r.git"
        );
    }
}
