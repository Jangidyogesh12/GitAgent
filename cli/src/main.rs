//! ============================================================================
//! Binary: gitagent (crate cli, src/main.rs)
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The `gitagent` command entry: clap CLI surface + dispatch. Ports
//!   `src/index.ts`: `plugin` subcommand shortcut → flag parsing → `--repo`
//!   session / `--dir` scaffold → `.env` loading → compliance warnings →
//!   one-shot (`--prompt`) or interactive REPL. Presentation only — the
//!   engine lives in `sdk`.
//!
//! DESIGN PATTERNS USED:
//!   * Command — clap `Cli`/`Commands` types; each subcommand maps to one
//!     handler module (`plugin_cmd`, `integrations_cmd`).
//!   * Builder — clap derive + `QueryOptions` chained builders.
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `main()`        — async entry: parse → dispatch → run.
//!   * `run_once()`    — one-shot `--prompt` mode (render + exit code).
//!   * `api_key_check()` — provider→env-var startup check (TS parity).
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
    /// Agent directory (default: current folder).
    #[arg(long, short = 'd', default_value = ".", global = true)]
    dir: PathBuf,
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    // `gitagent plugin ...` / `gitagent integrations ...` shortcut first.
    match &cli.command {
        Some(Commands::Plugin {
            action,
            target,
            force,
        }) => {
            return plugin_cmd::run(&cli.dir, action, target.as_deref(), *force);
        }
        Some(Commands::Integrations {
            action,
            format,
            out,
        }) => {
            return integrations_cmd::run(
                &cli.dir,
                action.as_deref(),
                format.as_deref(),
                out.clone(),
            );
        }
        None => {}
    }

    // Env stack: ~/.gitagent/.env then <dir>/.env (later wins) — TS parity.
    engine::helpers::load_env_stack(&cli.dir);

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
            let dir = cli.dir.clone();
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
            scaffold::ensure_repo(&cli.dir, cli.model.as_deref())?;
            (cli.dir.clone(), None)
        }
    };

    // Resolve model + startup API-key check (TS index.ts parity).
    let loaded =
        engine::loader::load_agent(&work_dir, cli.model.as_deref(), cli.session.as_deref())
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
/// Streams deltas to stdout; any `Error` message → exit 1 (like TS
/// single-shot which propagates failures to the shell).
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
    // Local / test providers need no key (matches TS behaviour + llm crate).
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
