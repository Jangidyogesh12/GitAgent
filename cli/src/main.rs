//! ============================================================================
//! Binary: gitagent (crate cli, src/main.rs)
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The `gitagent` command entry: clap CLI surface + dispatch. Flag parsing
//!   decides the run mode: `plugin` / `integrations` subcommands run first
//!   and exit; otherwise `--repo <url>` opens a remote session branch while
//!   the default `--dir` path scaffolds/validates a local agent dir. Then it
//!   loads `.env` files, resolves the model, runs the startup API-key check,
//!   prints the banner, and either executes a single prompt (`--prompt` /
//!   trailing arg) or drops into the interactive REPL. Presentation only —
//!   the engine lives in `sdk` and `engine`.
//!
//! HOW IT WORKS:
//!   * Flag parsing: clap `Cli` derives `--dir/--model/--prompt/--env/--repo/
//!     --pat/--session/--permission-mode/--allow-tool/--deny-tool` plus the
//!     `plugin`, `integrations`, `update` and `uninstall` subcommands.
//!   * Subcommand shortcut: `plugin`, `integrations`, `update` and `uninstall`
//!     dispatch to their handler modules immediately, before any agent loading.
//!   * Env stack: `helpers::load_env_stack()` loads `~/.gitagent/.env` then
//!     `<dir>/.env`, with the agent-local file winning.
//!   * Session vs scaffold: `--repo` requires a token (`--pat` or
//!     `GITHUB_TOKEN`/`GIT_TOKEN`), clones into the work dir, and creates a
//!     session branch via `init_local_session`; without `--repo` the dir is
//!     scaffolded in place via `ensure_repo()`.
//!   * Startup: `loader::load_agent()` resolves manifest + system prompt +
//!     model list; `api_key_check()` maps the model provider prefix to its
//!     required env var and bails with a hint when missing; `print_banner()`
//!     shows name/version/model/skills/session.
//!   * One-shot vs REPL: `--prompt`/`prompt_arg` builds `QueryOptions` via
//!     `query_options()` and streams through `sdk::query()` + `render_stream`
//!     returning an exit code; otherwise `repl::run()` owns a multi-turn
//!     `Session`. A `--repo` session is finalised (push/notes) on exit.
//!
//! DESIGN PATTERNS USED:
//!   * Command — clap `Cli`/`Commands` types; each subcommand maps to one
//!     handler module (`plugin_cmd`, `integrations_cmd`).
//!   * Builder — clap derive + `QueryOptions` chained builders.
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `main()`        — async entry: parse → dispatch → run.
//!   * `run_once()`    — one-shot `--prompt` mode (render + exit code).
//!   * `api_key_check()` — provider→env-var startup check.
//!   * `print_banner()`— name/version/model/tools/skills summary.
//!
//! HOW TO USE (examples):
//! ```bash
//! gitagent --dir ./my-agent "Explain this project"   # one-shot
//! gitagent --dir ./my-agent                          # REPL
//! gitagent --model anthropic:claude-sonnet-4-6 -p "hi" --dir .
//! ```
//! ============================================================================

mod integrations_cmd;
mod plugin_cmd;
mod render;
mod repl;
mod scaffold;
mod uninstall_cmd;
mod update_cmd;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// GitAgent — a git-native AI agent: your agent lives inside a git repo.
#[derive(Debug, Parser)]
#[command(
    name = "gitagent",
    version,
    about = "Git-native AI agent (chat in a terminal)"
)]
struct Cli {
    /// Agent directory (default: current folder; with --repo: ./<repo-name> derived from URL).
    #[arg(long, short = 'd', global = true)]
    dir: Option<PathBuf>,
    /// Model override, e.g. anthropic:claude-sonnet-4-6.
    #[arg(long, short = 'm')]
    model: Option<String>,
    /// One-shot prompt (ask once, exit — no REPL).
    #[arg(long, short = 'p')]
    prompt: Option<String>,
    /// Same as --prompt, as a bare trailing argument:
    /// `gitagent --dir DIR "do something"` (--prompt wins if both given).
    #[arg(index = 1)]
    prompt_arg: Option<String>,
    /// Config environment name (config/<env>.yaml).
    #[arg(long, short = 'e')]
    env: Option<String>,
    /// Clone a remote repo and work on a session branch.
    #[arg(long, short = 'r')]
    repo: Option<String>,
    /// Personal access token (or GITHUB_TOKEN / GIT_TOKEN).
    #[arg(long)]
    pat: Option<String>,
    /// Resume a session branch (with --repo).
    #[arg(long)]
    session: Option<String>,
    /// Permission mode: default | plan | acceptEdits | bypass.
    #[arg(long, default_value = "default")]
    permission_mode: String,
    /// Allow rules, e.g. --allow-tool "read" (repeatable).
    #[arg(long)]
    allow_tool: Vec<String>,
    /// Deny rules, e.g. --deny-tool "cli(rm -rf)" (repeatable).
    #[arg(long)]
    deny_tool: Vec<String>,
    /// Subcommand (plugin / integrations).
    #[command(subcommand)]
    command: Option<Commands>,
}

/// Subcommands (each = one Command-pattern handler).
#[derive(Debug, Subcommand)]
enum Commands {
    /// Manage plugins (install/list/remove/enable/disable/init).
    Plugin {
        /// Action to perform.
        action: String,
        /// Plugin source (git URL or path) for install/init.
        target: Option<String>,
        /// Force reinstall.
        #[arg(long)]
        force: bool,
    },
    /// Harness interop: list, detect, export agents for OpenCode & co.
    Integrations {
        /// Action: list | detect | export.
        action: Option<String>,
        /// Harness slug for export (opencode|nanobot|openclaw|claude-code|lyzr).
        #[arg(long)]
        format: Option<String>,
        /// Output dir for export.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Update gitagent to the latest release (reinstalls in place).
    Update {
        /// Release tag (default: latest). Also via GITAGENT_VERSION.
        #[arg(long)]
        version: Option<String>,
        /// Build from source with cargo instead of the prebuilt binary.
        #[arg(long)]
        from_source: bool,
        /// Releases repo `owner/name` (default: Jangidyogesh12/GitAgent).
        #[arg(long)]
        repo: Option<String>,
    },
    /// Uninstall gitagent (removes the binary).
    Uninstall {
        /// Also remove the global ~/.gitagent dir (keys, plugins, caches).
        #[arg(long)]
        purge: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    // Effective dir: explicit --dir wins; else with --repo derive ./<repo-name>
    // (git-clone style) so bare `gitagent --repo <url>` never tries `clone ... .`;
    // else "." (local mode / subcommands).
    let repo_derived_dir: Option<PathBuf> = cli
        .repo
        .as_deref()
        .map(default_repo_dir)
        .filter(|_| cli.dir.is_none());
    let cli_dir: PathBuf = cli
        .dir
        .clone()
        .or(repo_derived_dir.clone())
        .unwrap_or_else(|| PathBuf::from("."));
    // `gitagent plugin|integrations|update|uninstall ...` shortcuts first.
    match &cli.command {
        Some(Commands::Update {
            version,
            from_source,
            repo,
        }) => {
            return update_cmd::run(version.as_deref(), *from_source, repo.as_deref());
        }
        Some(Commands::Uninstall { purge }) => {
            return uninstall_cmd::run(*purge);
        }
        Some(Commands::Plugin {
            action,
            target,
            force,
        }) => {
            return plugin_cmd::run(cli_dir.as_path(), action, target.as_deref(), *force);
        }
        Some(Commands::Integrations {
            action,
            format,
            out,
        }) => {
            return integrations_cmd::run(
                cli_dir.as_path(),
                action.as_deref(),
                format.as_deref(),
                out.clone(),
            );
        }
        None => {}
    }

    // Env stack: ~/.gitagent/.env then <dir>/.env (later wins).
    if let Some(derived) = &repo_derived_dir {
        println!("using dir: {}", derived.display());
    }
    engine::helpers::load_env_stack(cli_dir.as_path());

    // Repo mode XOR local dir mode.
    let (work_dir, session) = match &cli.repo {
        Some(url) => {
            let token = cli.pat.clone().or_else(|| {
                std::env::var("GITHUB_TOKEN")
                    .or_else(|_| std::env::var("GIT_TOKEN"))
                    .ok()
            });
            if token.is_none() {
                anyhow::bail!("--repo needs --pat (or GITHUB_TOKEN / GIT_TOKEN)");
            }
            let dir = cli_dir.clone();
            let opts = engine::session::SessionOptions {
                url: url.clone(),
                token,
                dir,
                session: cli.session.clone(),
            };
            let s = engine::session::init_local_session(&opts)?;
            println!("repo session: {} on {}", s.session_id, s.branch);
            (s.dir.clone(), Some(s))
        }
        None => {
            scaffold::ensure_repo(cli_dir.as_path(), cli.model.as_deref())?;
            (cli_dir.clone(), None)
        }
    };

    // Resolve model + startup API-key check.
    let loaded = engine::loader::load_agent(
        work_dir.as_path(),
        cli.model.as_deref(),
        cli.session.as_deref(),
    )
    .context("loading agent")?;
    api_key_check(&loaded.model_specs)?;
    print_banner(&loaded);

    if let Some(prompt) = cli.prompt.clone().or_else(|| cli.prompt_arg.clone()) {
        let code = run_once(work_dir, prompt, &cli).await;
        if let Some(s) = session {
            s.finalize().ok();
        }
        std::process::exit(code);
    }

    repl::run(work_dir, &cli).await;
    if let Some(s) = session {
        s.finalize().ok();
    }
    Ok(())
}

/// One-shot `--prompt` mode: render the stream, return exit code.
///
/// # Description
/// Streams deltas to stdout; any `Error` message → exit 1 so shell callers
/// can detect failure (success → 0).
async fn run_once(work_dir: PathBuf, prompt: String, cli: &Cli) -> i32 {
    let opts = query_options(work_dir, prompt, cli);
    let mut rx = sdk::query(opts);
    render::render_stream(&mut rx).await
}

/// Build SDK options from CLI flags (single place both modes share).
fn query_options(work_dir: PathBuf, prompt: String, cli: &Cli) -> sdk::QueryOptions {
    let mode = match cli.permission_mode.to_lowercase().replace('_', "").as_str() {
        "plan" => sdk::PermissionMode::Plan,
        "acceptedits" => sdk::PermissionMode::AcceptEdits,
        "bypass" => sdk::PermissionMode::Bypass,
        _ => sdk::PermissionMode::Default,
    };
    let mut rules: Vec<String> = cli
        .allow_tool
        .iter()
        .map(|t| format!("allow:{t}"))
        .collect();
    rules.extend(cli.deny_tool.iter().map(|t| format!("deny:{t}")));
    let mut opts = sdk::QueryOptions::new(work_dir, prompt);
    opts.model = cli.model.clone();
    opts.permission_mode = Some(mode);
    opts.permission_rules = rules;
    opts
}

/// Provider→env-var startup key check (missing key = exit 1 with hint).
fn api_key_check(specs: &[String]) -> Result<()> {
    let Some(first) = specs.first() else {
        anyhow::bail!("no model configured")
    };
    let provider = first.split(':').next().unwrap_or("");
    // Local / test providers need no key.
    if matches!(provider, "ollama" | "mock" | "") {
        return Ok(());
    }
    // Custom base URLs (incl. Lyzr) authenticate via OPENAI_API_KEY/LYZR_API_KEY.
    if (std::env::var("GITAGENT_MODEL_BASE_URL").is_ok() || first.contains('@'))
        && (std::env::var("OPENAI_API_KEY").is_ok() || std::env::var("LYZR_API_KEY").is_ok())
    {
        return Ok(());
    }
    let need = engine::manifest::provider_api_key(provider);
    if need.is_empty() {
        return Ok(());
    }
    if std::env::var(need).is_err() {
        anyhow::bail!("missing API key: set {need} for provider `{provider}`");
    }
    Ok(())
}

/// Print the startup banner (name/version/model/tools/skills).
fn print_banner(loaded: &engine::loader::LoadedAgent) {
    println!(
        "{} v{} — {}",
        loaded.manifest.name, loaded.manifest.version, loaded.manifest.description
    );
    println!("model: {}", loaded.model_specs.join(", "));
    println!(
        "skills: {}  session: {}",
        loaded.skill_count, loaded.session_id
    );
}

/// Derive a clone dir from a repo URL (git-clone style): last path segment,
/// trailing `/` + `.git` stripped, `:` handled for SSH, sanitised, fallback
/// `repo-session`. Relative so it resolves under cwd (e.g. `./my-agent`).
///
/// # Example
/// ```rust,ignore
/// assert_eq!(default_repo_dir("https://github.com/o/my-agent.git"), PathBuf::from("my-agent"));
/// ```
fn default_repo_dir(url: &str) -> PathBuf {
    let s = url.trim().trim_end_matches('/');
    let after_scheme = s.rsplit("://").next().unwrap_or(s);
    let after_host = after_scheme
        .rsplit_once(':')
        .map(|(_, p)| p)
        .filter(|p| !p.contains('/'))
        .unwrap_or(after_scheme);
    let last = after_host.rsplit('/').next().unwrap_or(after_host);
    let base = last.strip_suffix(".git").unwrap_or(last);
    let clean: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let clean = clean.trim_matches(|c| c == '-' || c == '.');
    if clean.is_empty() {
        PathBuf::from("repo-session")
    } else {
        PathBuf::from(clean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_repo_basename() {
        assert_eq!(
            default_repo_dir("https://github.com/Jangidyogesh12/gitagent-webscraper.git"),
            PathBuf::from("gitagent-webscraper")
        );
        assert_eq!(
            default_repo_dir("https://github.com/org/my-agent"),
            PathBuf::from("my-agent")
        );
        assert_eq!(
            default_repo_dir("https://github.com/org/my-agent/"),
            PathBuf::from("my-agent")
        );
        assert_eq!(
            default_repo_dir("git@github.com:org/my-agent.git"),
            PathBuf::from("my-agent")
        );
        assert_eq!(default_repo_dir(""), PathBuf::from("repo-session"));
    }
}
