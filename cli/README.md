# `gitagent` CLI — user manual

The `gitagent` binary (crate `cli`) is the terminal front end.
All agent logic lives in `sdk` and `engine`; this page covers install,
flags, providers, REPL, plugins, and harness export.

## Install

```bash
# from this repo (needs Rust 1.75+ and git)
./installer/install.sh
# or manually:
cargo install --path cli
```

## Quick start

```bash
# One-liner: scaffolds agent.yaml, SOUL.md, memory/ on first run
# (bare prompt and --prompt/-p are equivalent; the flag wins if both given)
gitagent --dir ~/my-project "Explain this project"

# Interactive chat loop (REPL)
gitagent --dir ~/my-agent
```

## Providers — how to run with each one

Pick with `--model` (or `agent.yaml → model.preferred`). Accepted forms:
`provider:model`, `provider/model`, `provider:model@base-url`.

| Provider | Command | Key |
|---|---|---|
| OpenAI | `--model "openai:gpt-4o-mini"` | `OPENAI_API_KEY` |
| Anthropic | `--model "anthropic:claude-sonnet-4-6"` | `ANTHROPIC_API_KEY` |
| Google | `--model "google:gemini-2.0-flash"` | `GEMINI_API_KEY` |
| xAI / Groq / Mistral | `--model "xai:grok-4"`, `"groq:…"`, `"mistral:…"` | `XAI_API_KEY` / `GROQ_API_KEY` / `MISTRAL_API_KEY` |
| Ollama (local, keyless) | `--model "ollama:llama3.2:3b"` | none (needs `ollama serve`) |
| Lyzr | `--model "lyzr:<agent-id>@https://agent-prod.studio.lyzr.ai/v4"` | `LYZR_API_KEY` |
| OpenCode Zen | `--model "opencode/kimi-k2.6"` | `OPENCODE_API_KEY` |
| OpenCode Go | `--model "opencode-go/kimi-k2.6"` | `OPENCODE_API_KEY` (Go key) |
| Any gateway | `--model "openai:<id>@http://host:port/v1"` | key via `OPENAI_API_KEY` |

```bash
export OPENCODE_API_KEY="..."   # Zen key from opencode.ai/auth
gitagent --dir ./my-agent --model "opencode/kimi-k2.6" "Explain this project"
```

Only OpenAI-`chat/completions`-speaking models work: Zen/Go GPT, Claude,
Gemini and MiniMax/Qwen-on-`/messages` use other protocols — use those
providers' native keys instead. A missing key exits 1 naming the variable;
a rejected call prints `error: …` (never silent).

## Flags

| Flag | Short | Meaning |
|---|---|---|
| `--dir <path>` | `-d` | Agent directory (default: cwd; global — before or after subcommands) |
| `--model <spec>` | `-m` | Model override (see table above) |
| `--prompt "..."` | `-p` | One-shot mode (ask once, exit — no REPL) |
| `--env <name>` | `-e` | Reserved for `config/<name>.yaml` environments |
| `--repo <url>` + `--pat <token>` | `-r` | Clone a repo, work on `gitagent/session-<id>`, push on exit (token also via `GITHUB_TOKEN`/`GIT_TOKEN`) |
| `--session <branch>` | | Resume a session branch (with `--repo`) |
| `--permission-mode <m>` | | `default` \| `plan` (read-only) \| `acceptEdits` \| `bypass` |
| `--allow-tool <rule>` / `--deny-tool <rule>` | | Repeatable, e.g. `--deny-tool "cli(rm -rf)"` |

## REPL slash commands

| Command | What it does |
|---|---|
| `/quit`, `/exit` | Leave (finalises repo sessions) |
| `/help` | List commands |
| `/memory` | Show `memory/MEMORY.md` |
| `/skills` | List installed skills with confidence |
| `/tasks` | Show in-progress tasks |
| `/learned` | Show learned skills with confidence |
| `/plugins` | Show loaded plugins |
| `/skill:<name> [args]` | Show a skill's instructions inline (send as your next prompt to run it) |

## Plugins

```bash
gitagent plugin install <git-url-or-path> --dir ./my-agent
gitagent plugin list --dir ./my-agent
gitagent plugin enable|disable <name> --dir ./my-agent
gitagent plugin remove <name> --dir ./my-agent
gitagent plugin init my-plugin --dir ./my-agent   # scaffold a new plugin
```

A plugin is a folder with `plugin.yaml` (`id`, `name`, `version`,
`description`, `provides: {tools, hooks, skills, prompt}`, `config:`) plus
optional `tools/*.yaml`, `hooks/`, `skills/`. Discovery order: agent
`plugins/` → global `~/.gitagent/plugins/` → installed
`.gitagent/plugins/`.

## Harness interop (OpenCode, NanoBot, OpenClaw, Claude Code, Lyzr)

```bash
gitagent integrations list                        # all five + what each exports
gitagent integrations detect                      # env/binary heuristics
gitagent integrations export --format opencode --out ./interop --dir ./my-agent
```

| `--format` | Writes |
|---|---|
| `opencode` | `opencode.json` (`model` + `agent.<name>.prompt` + `mcp`) |
| `nanobot` | `nanobot.yaml` + `SYSTEM.md` |
| `openclaw` | `openclaw.json` (agents + tools) |
| `claude-code` | `CLAUDE.md` + `.claude/settings.json` |
| `lyzr` | `.env.lyzr` (`LYZR_API_KEY` + `lyzr:<id>@<base>` model string) |

Then run it under OpenCode from the export dir (`opencode` / `opencode run "…"`).

## Exit codes

`0` = ok (tool errors are model-visible data, not shell failures);
`1` = setup/LLM failure, missing key, or blocked session start.
