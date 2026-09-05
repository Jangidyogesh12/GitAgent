//! ============================================================================
//! Module: cli::repl (src/repl.rs)
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   The interactive chat loop: read lines on stdin, run each as an agent
//!   turn, and render the streamed result. Lines starting with `/` are
//!   local slash commands (process info / skill lookup); everything else is
//!   sent as a prompt. Multi-turn state lives in `sdk::Session`, which owns
//!   the transcript — this module is presentation + input routing only.
//!
//! HOW IT WORKS:
//!   * Startup: builds shared `QueryOptions` from CLI flags via
//!     `query_options()`, opens one `sdk::Session` (loads manifest, tools,
//!     gates, model client once), and prints the session id plus hints.
//!   * Input: `read_line("→ ")` flushes the prompt, blocks on stdin, and
//!     returns `None` on EOF/Ctrl-D to exit. Empty lines are skipped.
//!   * Dispatch: slash lines go to `handle_slash()` (`false` breaks the
//!     loop); normal lines call `session.send(text).await` and drain the
//!     returned channel through `render::render_stream()`. Turn errors print
//!     to stderr and the loop continues; only `/quit`/`/exit`/EOF exits.
//!   * Slash commands: `/quit|/exit` (leave), `/help` (command list),
//!     `/memory` (print `memory/MEMORY.md`), `/skills` (discover + list with
//!     confidence), `/tasks` (active objectives from
//!     `.gitagent/learning/tasks.json`), `/learned` (skill confidences),
//!     `/plugins` (discovered plugins with scope from the `agent.yaml`
//!     plugins table), `/skill:<name> [args]` (print the matching
//!     `skills/<name>/SKILL.md` inline plus args with a hint to send it as
//!     the next prompt).
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `run()`             — the loop (blocking stdin reader + async turns).
//!   * `handle_slash()`    — dispatch one slash command (true = keep going).
//!
//! HOW TO USE (example):
//! ```bash
//! gitagent --dir ./my-agent
//! → /skills
//! → /skill:review-docs ./README.md
//! → /quit
//! ```
//! ============================================================================

use std::path::PathBuf;

use crate::{query_options, Cli};

/// Run the interactive loop until /quit (or EOF / Ctrl-D).
///
/// # Description
/// Blocking-stdin design: each line is read on a blocking task, then the
/// turn runs async through `Session::send()` + `render_stream()`. Slash
/// lines go to [`handle_slash`]; everything else is a prompt. Errors from a
/// turn print + continue (only /quit exits).
pub async fn run(work_dir: PathBuf, cli: &Cli) {
    let opts = query_options(work_dir.clone(), String::new(), cli);
    let session = match sdk::Session::open(opts.clone()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e:#}");
            return;
        }
    };
    println!(
        "chat with {} (type /quit to exit, /help for commands)",
        session.session_id()
    );
    loop {
        let line = read_line("→ ");
        let Some(line) = line else { break };
        let text = line.trim().to_string();
        if text.is_empty() {
            continue;
        }
        if text.starts_with('/') {
            if !handle_slash(&work_dir, &text) {
                break;
            }
            continue;
        }
        match session.send(text).await {
            Ok(mut rx) => {
                crate::render::render_stream(&mut rx).await;
            }
            Err(e) => eprintln!("error: {e:#}"),
        }
    }
}

/// Handle one slash command; false = exit the REPL.
///
/// # Description
/// Dispatch table: /quit|/exit (leave), /memory (print MEMORY.md), /skills
/// (refresh + list with confidence), /tasks (active from tasks.json),
/// /learned (learned skills + ratios), /plugins (loaded plugins +
/// contributions), /skill:<name> [args] (inline SKILL.md + args as a
/// one-shot prompt through the session), /help (this table).
///
/// # Example
/// ```rust,no_run
/// // crate::repl::handle_slash(Path::new("./my-agent"), "/skills");
/// ```
pub fn handle_slash(work_dir: &std::path::Path, line: &str) -> bool {
    let cmd = line.split_whitespace().next().unwrap_or("");
    match cmd {
        "/quit" | "/exit" => false,
        "/help" => {
            println!(
                "/quit|/exit /memory /skills /tasks /learned /plugins /skill:<name> [args] /help"
            );
            true
        }
        "/memory" => {
            let p = work_dir.join("memory/MEMORY.md");
            println!(
                "{}",
                std::fs::read_to_string(p).unwrap_or_else(|_| "(no memory yet)".into())
            );
            true
        }
        "/skills" => {
            for s in engine::loader::discover_skills(work_dir) {
                println!(
                    "{} — {} (confidence {:.2})",
                    s.name, s.description, s.confidence
                );
            }
            true
        }
        "/tasks" => {
            let v: serde_json::Value =
                std::fs::read_to_string(work_dir.join(".gitagent/learning/tasks.json"))
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok())
                    .unwrap_or(serde_json::json!({"tasks": []}));
            let active: Vec<String> = v
                .get("tasks")
                .and_then(|t| t.as_array())
                .map(|ts| {
                    ts.iter()
                        .filter(|t| t.get("status").and_then(|s| s.as_str()) == Some("active"))
                        .map(|t| {
                            t.get("objective")
                                .and_then(|o| o.as_str())
                                .unwrap_or("?")
                                .to_string()
                        })
                        .collect()
                })
                .unwrap_or_default();
            if active.is_empty() {
                println!("no active tasks");
            } else {
                for a in active {
                    println!("• {a}");
                }
            }
            true
        }
        "/learned" => {
            for s in engine::loader::discover_skills(work_dir) {
                println!("{} — confidence {:.2}", s.name, s.confidence);
            }
            true
        }
        "/plugins" => {
            // Manifest table is the source of truth for enablement display.
            let manifest = engine::manifest::load_manifest(&work_dir.join("agent.yaml"));
            let table = manifest
                .map(|m| m.plugins)
                .unwrap_or(serde_json::Value::Null);
            for p in engine::plugins::discover_plugins(work_dir, &table) {
                println!("{} [{}] — {}", p.name, p.scope, p.manifest.description);
            }
            true
        }
        _ if cmd.starts_with("/skill:") => {
            let name = cmd.trim_start_matches("/skill:");
            let args = line.split_once(' ').map(|x| x.1).unwrap_or("");
            let file = work_dir.join(format!("skills/{name}/SKILL.md"));
            match std::fs::read_to_string(&file) {
                Ok(text) => {
                    println!("<skill name=\"{name}\">\n{text}\n</skill>\nargs: {args}\n(hint: send the above as your next prompt to run it)");
                }
                Err(_) => println!("unknown skill: {name}"),
            }
            true
        }
        _ => {
            println!("unknown command: {cmd} (try /help)");
            true
        }
    }
}

fn read_line(prompt: &str) -> Option<String> {
    use std::io::{BufRead, Write};
    print!("{prompt}");
    std::io::stdout().flush().ok()?;
    let mut buf = String::new();
    let n = std::io::stdin().lock().read_line(&mut buf).ok()?;
    if n == 0 {
        return None; // EOF / Ctrl-D
    }
    Some(buf)
}
