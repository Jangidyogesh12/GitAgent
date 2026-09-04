# `gitagent` CLI — user manual

The `gitagent` binary (crate `cli`) is the terminal front end.
All agent logic lives in `sdk` and the library crates; this page
covers installation, flags, the REPL, plugins, and harness export.

## Install

```bash
# from this repo (needs Rust 1.75+ and git)
./installer/install.sh
# or manually:
cargo install --path cli
```

Requires an LLM key, e.g. `export OPENAI_API_KEY="sk-..."` (or Anthropic /
Lyzr equivalents — see "Models").

## Quick start

```bash
# One-liner: scaffolds agent.yaml, SOUL.md, memory/ on first run
# (bare prompt and --prompt/-p are equivalent; the flag wins if both given)
gitagent --dir ~/my-project "Explain this project"

# Interactive chat loop (REPL)
gitagent --dir ~/my-agent
```

## Flags

| Flag | Short | Meaning |
|---|---|---|
| `--dir <path>` | `-d` | Agent directory (default: current folder; global — works before or after subcommands) |
| `--model provider:model` | `-m` | Override the model, e.g. `anthropic:claude-sonnet-4-6` or `lyzr:<id>@https://agent-prod.studio.lyzr.ai/v4` |
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
| `opencode` | `opencode.json` (model + instructions + tools + mcp) |
| `nanobot` | `nanobot.yaml` + `SYSTEM.md` |
| `openclaw` | `openclaw.json` (agents + model + skills) |
| `claude-code` | `CLAUDE.md` + `.claude/settings.json` |
| `lyzr` | `.env.lyzr` (`LYZR_API_KEY` + `lyzr:<id>@<base>` model string) |

## Models

`--model provider:model[@base-url]`; OpenCode style `provider/model` also
works (`opencode/kimi-k2.6`). `@base-url` (or `GITAGENT_MODEL_BASE_URL`)
targets any OpenAI-compatible endpoint (Ollama, Lyzr, gateways). Startup key
check: `anthropic`→`ANTHROPIC_API_KEY`, `openai`→`OPENAI_API_KEY`,
`google`→`GEMINI_API_KEY`, `xai`→`XAI_API_KEY`, `groq`→`GROQ_API_KEY`,
`mistral`→`MISTRAL_API_KEY`, `lyzr`→`LYZR_API_KEY`,
`opencode`→`OPENCODE_API_KEY`; `ollama` needs no key.

### OpenCode Zen (hosted gateway)

Zen exposes an OpenAI-compatible `chat/completions` family at
`https://opencode.ai/zen/v1` — get a key at opencode.ai/auth, then:

```bash
export OPENCODE_API_KEY="..."
gitagent --dir ./my-agent --model "opencode/kimi-k2.6" "Explain this project"
```

Compatible Zen models are the chat-completions ones (DeepSeek V4,
Kimi K2/K3, GLM, MiniMax, …). Zen GPT/Claude/Gemini models live on
`/responses`, `/messages`, `/models/…` endpoints with different protocols
and are NOT callable through gitagent — use those providers' native keys
instead (e.g. `--model anthropic:claude-sonnet-4-6`).

### OpenCode Go ($10/mo subscription — different endpoint!)

Go keys are scoped to `https://opencode.ai/zen/go/v1` (note `/go/`) and
use the model format `opencode-go/<id>` — a Zen-style `opencode/<id>`
will be rejected. Same `OPENCODE_API_KEY` env var:

```bash
export OPENCODE_API_KEY="<Go key from opencode.ai/auth>"
gitagent --dir ./my-agent --model "opencode-go/kimi-k2.6" "Explain this project"
```

Go chat-completions models include Kimi K2.6/K2.7-Code/K3, DeepSeek V4
Pro/Flash, GLM-5.x, MiMo-V2.5, Hy3/Hy4, Omen Alpha, LongCat-2.0.
(Go's MiniMax/Qwen/Muse/GPT models use other protocols — unsupported,
same rule as Zen.) Verify a key any time with zero cost:

```bash
curl -s https://opencode.ai/zen/go/v1/models \
  -H "Authorization: Bearer $OPENCODE_API_KEY" | head -c 300
# → {"object":"list","data":[…]} means the key works
```

The client identifies as `gitagent/<version>` and sends
`x-opencode-session` on opencode endpoints (their docs ask third-party
clients to send it, so Go accounts aren't flagged for unidentified
traffic).

## Exit codes

`0` = ok (tool errors are model-visible data, not shell failures);
`1` = setup/LLM failure, missing key, or blocked session start.
