# GitAgent (Rust workspace)

<p align="center">
  <img src="https://img.shields.io/badge/rust-2021-orange?style=flat-square&logo=rust" alt="rust edition" />
  <img src="https://img.shields.io/badge/cargo-workspace-blue?style=flat-square&logo=cargo" alt="cargo workspace" />
  <img src="https://img.shields.io/github/license/open-gitagent/gitagent?style=flat-square" alt="license" />
</p>

<h1 align="center">GitAgent — Rust port</h1>

<p align="center">
  <strong>A universal git-native always-learning AI agent framework, reimplemented in Rust.</strong><br/>
  Your agent lives inside a git repo — identity, rules, memory, tools and
  skills are all version-controlled files.
</p>

<p align="center">
  <a href="#install">Install</a> &bull;
  <a href="#quick-start">Quick Start</a> &bull;
  <a href="#sdk">SDK</a> &bull;
  <a href="#architecture">Architecture</a> &bull;
  <a href="#tools">Tools</a> &bull;
  <a href="#hooks">Hooks</a> &bull;
  <a href="#skills">Skills</a> &bull;
  <a href="#plugins">Plugins</a> &bull;
  <a href="#harness-interop">Harness interop</a>
</p>

Ported from the TypeScript [`gitagent`](https://github.com/open-gitagent/gitagent)
(`@open-gitagent/gitagent` v2.2.0, pi-agent-core + pi-ai engine). This port
re-implements **both** the agent loop and the model layer in Rust — no
Node.js required at runtime. Agent directories are fully compatible: an
agent written for the TS version loads here unchanged.

---

## Why GitAgent?

Most agent frameworks treat configuration as code scattered across your application. GitAgent flips this — **your agent IS a git repository**:

- **`agent.yaml`** — model, tools, runtime config
- **`SOUL.md`** — personality and identity
- **`RULES.md`** — behavioral constraints
- **`memory/`** — git-committed memory with full history
- **`tools/`** — declarative YAML tool definitions
- **`skills/`** — composable skill modules
- **`hooks/`** — lifecycle hook scripts

Fork an agent. Branch a personality. `git log` your agent's memory. Diff its rules. This is **agents as repos**.

## Install

```bash
# Prebuilt binary, no Rust toolchain needed (linux-x64, mac-arm64):
curl -fsSL https://raw.githubusercontent.com/Jangidyogesh12/GitAgent/main/installer/install-remote.sh | bash

# Or the full local setup (build + backend wizard):
./installer/install.sh
# binary only: GITAGENT_NO_SETUP=1 ./installer/install.sh
# or: cargo install --path cli
```

Releases are tag-triggered: push freely to `main` (only CI runs), and
when ready bump `cli/Cargo.toml`, commit, then
`git tag vX.Y.Z && git push origin main vX.Y.Z` — the tag builds
per-OS binaries and publishes them with checksums, which is what the
remote installer downloads. See `.github/workflows/` (commented for
learning).

The installer checks prerequisites, builds the release binary, installs it
to `$HOME/.cargo/bin` (override with `GITAGENT_PREFIX=...`), then walks you
through backend setup (Lyzr / Anthropic / OpenAI / Ollama / OpenCode Zen /
custom endpoint) and scaffolds your first agent.

### Or install manually

```bash
cargo install --path cli
```

> **Requirements:** Rust 1.75+, git. No Node.js, no voice mode in this port.

## Quick Start

**Run your first agent in one line:**

```bash
export OPENAI_API_KEY="sk-..."
gitagent --dir ~/my-project "Explain this project and suggest improvements"
```

That's it. GitAgent auto-scaffolds everything on first run — `agent.yaml`, `SOUL.md`, `memory/`, `workspace/`, `git init` — and drops you into the agent.

### Local Repo Mode

Clone a GitHub repo, run an agent on it, auto-commit and push to a session branch:

```bash
gitagent --repo https://github.com/org/repo --pat ghp_xxx "Fix the login bug"
```

Resume an existing session:

```bash
gitagent --repo https://github.com/org/repo --pat ghp_xxx --session gitagent/session-a1b2c3d4 "Continue"
```

Token can come from env instead of `--pat`:

```bash
export GITHUB_TOKEN=ghp_xxx
gitagent --repo https://github.com/org/repo "Add unit tests"
```

On exit the session commits, pushes `gitagent/session-<id>`, and **scrubs the
token** from the remote URL.

### CLI Options

| Flag | Short | Description |
|---|---|---|
| `--dir <path>` | `-d` | Agent directory (default: cwd; global flag) |
| `--repo <url>` | `-r` | Git repo URL to clone and work on |
| `--pat <token>` | | Token (or set `GITHUB_TOKEN` / `GIT_TOKEN`) |
| `--session <branch>` | | Resume an existing session branch |
| `--model <spec>` | `-m` | Override model (`provider:model`, `provider/model`, or `provider:model@base-url`) |
| `--prompt <text>` | `-p` | Single-shot prompt (skip REPL; a bare trailing argument works too) |
| `--env <name>` | `-e` | Accepted, reserved for future `config/<env>.yaml` support |
| `--permission-mode <m>` | | `default` \| `plan` (read-only) \| `acceptEdits` \| `bypass` |
| `--allow-tool <rule>` | | Repeatable allow rule, e.g. `read` |
| `--deny-tool <rule>` | | Repeatable deny rule, e.g. `cli(rm -rf)` |

Subcommands: `gitagent plugin …` (see [Plugins](#plugins)),
`gitagent integrations …` (see [Harness interop](#harness-interop)).

### REPL commands

| Command | Description |
|---|---|
| `/quit`, `/exit` | Leave (finalises repo sessions) |
| `/help` | List commands |
| `/memory` | Print `memory/MEMORY.md` |
| `/skills` | List skills with confidence |
| `/tasks` | Show in-progress tasks |
| `/learned` | Show learned skills with confidence |
| `/plugins` | Show loaded plugins |
| `/skill:<name> [args]` | Show a skill's instructions inline (send as your next prompt to run it) |

Full terminal manual: [`cli/README.md`](cli/README.md).

## SDK

The SDK provides a programmatic interface. It mirrors the Claude Agent SDK
pattern but runs **in-process** — no subprocesses, no IPC.

### `query(opts)` — one-shot streaming run

```rust
use sdk::{query, QueryOptions};
use std::path::PathBuf;

let mut rx = query(QueryOptions::new(
    PathBuf::from("./my-agent"),
    "List the tools you have and summarize this repo",
));
while let Some(msg) = rx.recv().await {
    match msg {
        sdk::SdkMessage::Delta(t) => print!("{t}"),
        sdk::SdkMessage::Assistant(t) => println!("\nDone: {t}"),
        sdk::SdkMessage::ToolUse(_, name, _) => println!("\ncalling {name}"),
        sdk::SdkMessage::ToolResult(_, name, content, err) => {
            println!("[{name} err={err}] {}", &content[..content.len().min(120)])
        }
        sdk::SdkMessage::System(s) => println!("[{s}]"),
        sdk::SdkMessage::Error(e) => eprintln!("error: {e}"),
    }
}
```

### `Session` — multi-turn handle

```rust
use sdk::{QueryOptions, Session};

let s = Session::open(QueryOptions::new("./my-agent".into(), ""))?;
let mut rx = s.send("hello".to_string()).await?;
while let Some(m) = rx.recv().await { /* same messages */ }
s.abort(); // cooperative cancel of the running turn
```

### `tool(name, description, schema, handler)` — custom tools

```rust
use sdk::tool;

let search = tool(
    "search_docs",
    "Search the documentation",
    serde_json::json!({
        "type": "object",
        "properties": { "q": { "type": "string" } },
        "required": ["q"]
    }),
    |args| Ok(format!("hits for {}", args["q"].as_str().unwrap_or(""))),
);
// pass via QueryOptions { extra_tools: vec![Arc::new(search)], .. }
```

Runnable version: `cargo run -p sdk --example demo -- ./my-agent "hello"`.

### `QueryOptions` reference

| Field | Type | Description |
|---|---|---|
| `dir` | `PathBuf` | Agent directory |
| `prompt` | `String` | User prompt (single-shot; use `Session` for multi-turn) |
| `model` | `Option<String>` | Override spec (wins over the manifest) |
| `max_turns` | `Option<u32>` | Override `runtime.max_turns` |
| `allowed_tools` | `Option<Vec<String>>` | Allowlist (applied first) |
| `disallowed_tools` | `Vec<String>` | Denylist (applied second) |
| `system_prompt_suffix` | `Option<String>` | Appended to the assembled prompt |
| `extra_tools` | `Vec<Arc<dyn AgentTool>>` | Custom tools (`tool()` above) |
| `permission_mode` | `Option<PermissionMode>` | `Default`/`Plan`/`AcceptEdits`/`Bypass` |
| `permission_rules` | `Vec<String>` | `allow:tool` / `deny:tool(substring)` |
| `session_id` | `Option<String>` | Override (fresh UUID when `None`) |

### Message types

| Variant | Carries | Meaning |
|---|---|---|
| `Delta(String)` | text fragment | streaming chunk |
| `Assistant(String)` | full text | finished turn |
| `ToolUse(id, name, args)` | call triple | tool started |
| `ToolResult(id, name, content, is_error)` | result tuple | tool finished |
| `System(String)` | note | `session_start`, `agent_start`, `session_end` |
| `Error(String)` | message | setup or provider failure (also exits CLI non-zero) |

Provider failures arrive as `Error`, never panics — an empty turn with an
instant `session_end` always means "read the error line above it".

## Architecture

```
my-agent/
├── agent.yaml          # Model, tools, runtime config
├── SOUL.md             # Agent identity & personality
├── RULES.md            # Behavioral rules & constraints
├── DUTIES.md           # Role-specific responsibilities
├── memory/
│   └── MEMORY.md       # Git-committed agent memory
├── tools/
│   └── *.yaml          # Declarative tool definitions
├── skills/
│   └── <name>/
│       └── SKILL.md    # Skill instructions (YAML frontmatter + body)
├── workflows/
│   └── *.yaml|*.md     # Multi-step workflows (`@name` flows + docs)
├── agents/
│   └── <name>/         # Sub-agent definitions
├── plugins/
│   └── <name>/         # Local plugins (plugin.yaml + tools/hooks/skills)
├── hooks/
│   └── hooks.yaml      # Lifecycle hook scripts
├── knowledge/
│   └── index.yaml      # Knowledge base entries
├── examples/
│   └── *.md            # Few-shot examples
└── .gitagent/          # Runtime state (sessions, plugins, tasks, logs)
```

### Agent manifest (`agent.yaml`)

```yaml
spec_version: "0.1.0"
name: my-agent
version: 1.0.0
description: An agent that does things

model:
  preferred: "anthropic:claude-sonnet-4-6"   # or opencode/kimi-k2.6, ollama:qwen3:8b …
  fallback: ["openai:gpt-4o-mini"]
  constraints:
    temperature: 0.7
    max_tokens: 4096

tools: [cli, read, write, memory]

runtime:
  max_turns: 50
  timeout: 120            # default timeout (s) for the cli tool

# Optional
extends: "https://github.com/org/base-agent.git"
skills: [code-review, deploy]                 # allowlist (absent = all)
delegation:
  mode: auto
compliance:
  recordkeeping:
    audit_logging: true                       # → .gitagent/audit.jsonl
```

Loader order (joined with blank lines, empties skipped): manifest header →
`SOUL.md` → `RULES.md` → parent `RULES.md` → `DUTIES.md` → `AGENTS.md` →
Memory → Knowledge → Skills (mandatory-check block) → Workflows →
Sub-Agents → Examples → `# Plugin:` sections → Workspace → Learning.

## Tools

### Built-in tools

| Tool | Mode | Description |
|---|---|---|
| `cli` | sequential | Shell commands (timeout, process-group kill, ~100KB tail) |
| `read` | parallel | Files with binary sniff + 2000-line / 100KB pagination |
| `write` | sequential | Create/overwrite (parents auto-created) |
| `edit` | sequential | Exact / `replace_all` / regex find-and-replace |
| `memory` | sequential | Load/save git-committed layered memory |
| `task_tracker` | sequential | begin/update/end/list multi-step tasks + skill matching |
| `skill_learner` | sequential | evaluate/crystallize/status/review/update/delete skills |

`read` is the only parallel-safe builtin; everything mutating runs
sequentially. Declarative, plugin and MCP tools join the same registry
(`<server>__<tool>` for MCP).

### Declarative tools

Define tools as YAML in `tools/` (args arrive as JSON on stdin, stdout is
the result; 120s timeout):

```yaml
# tools/shout.yaml
name: shout
description: Uppercase some text
input_schema:
  type: object
  properties:
    text: { type: string }
  required: [text]
implementation:
  script: shout.sh
  runtime: sh
```

## Hooks

Script hooks in `hooks/hooks.yaml` (10s timeout each; stdout parsed as the
verdict; hook errors never block — fail-open):

```yaml
hooks:
  on_session_start:
    - script: validate-env.sh
      description: Check environment is ready
  pre_tool_use:
    - script: audit-tools.sh
      description: Log and gate tool usage
  post_response:
    - script: notify.sh
  on_error:
    - script: alert.sh
```

Events: `on_session_start` (block aborts), `pre_tool_use` (block/modify
enforced), `post_tool_failure`, `post_response`, `pre_query`,
`file_changed` (fires for `write`/`edit`), `on_error`. Scripts resolve
under `hooks/` (or the plugin dir) with `../` escapes rejected:

```json
{ "action": "allow" }
{ "action": "block", "reason": "Not permitted" }
{ "action": "modify", "args": { "modified": "args" } }
```

Permission modes (`--permission-mode` / `PermissionMode`) add a
Claude-Code-style gate on top: `plan` blocks mutating tools,
`deny:cli(rm -rf)`-style rules beat everything except explicit allows.

## Skills

Composable instruction modules in `skills/<name>/SKILL.md` (kebab-case name
matching the directory, `name` + `description` required):

```markdown
---
name: code-review
description: Review code for quality and security
confidence: 1.0
---

# Code Review

When reviewing code:
1. Check for security vulnerabilities
2. Verify error handling
3. Run the lint script for style checks
```

Invoke via REPL (`/skill:code-review Review the auth module`), or let the
agent pick skills itself — the prompt mandates checking `<available_skills>`
first, `task_tracker` matches by keyword overlap, and `skill_learner`
crystallizes wins back into `skills/` with reinforcement
(success `+0.1·(1−c)`, failure `−0.2`, partial `−0.05`, flagged `< 0.4`).

## Plugins

Reusable file-based extensions (tools, hooks, skills, prompts). No
programmatic entry execution in this port — the file-based 90% is fully
supported.

### CLI commands

```bash
gitagent plugin install https://github.com/org/my-plugin.git --dir ./my-agent
gitagent plugin install ./path/to/plugin --dir ./my-agent
gitagent plugin list --dir ./my-agent
gitagent plugin enable|disable <name> --dir ./my-agent
gitagent plugin remove <name> --dir ./my-agent
gitagent plugin init my-plugin --dir ./my-agent   # scaffold
```

| Flag | Description |
|---|---|
| `--force` | Reinstall even if already present |

### Plugin manifest (`plugin.yaml`)

```yaml
id: my-plugin                    # Required, kebab-case
name: My Plugin
version: 0.1.0
description: What this plugin does
author: Your Name
license: MIT
engine: ">=0.3.0"                # Informational in this port

provides:
  tools: true                    # Load tools from tools/*.yaml
  skills: true                   # Load skills from skills/
  prompt: prompt.md              # Inject into system prompt
  hooks:
    pre_tool_use:
      - script: hooks/audit.sh
        description: Audit tool calls

config:
  properties:
    api_key:
      type: string
      description: API key
      env: MY_API_KEY            # Env var fallback
    timeout:
      type: number
      default: 30
  required: [api_key]
```

### Plugin config in `agent.yaml`

```yaml
plugins:
  my-plugin:
    enabled: true
    source: https://github.com/org/my-plugin.git  # Auto-install on load
    version: main                                   # Git branch/tag
    config:
      api_key: "${MY_API_KEY}"                      # Supports env interpolation
      timeout: 60
```

Config resolution: `agent.yaml config` > `env var` > `manifest default`
(missing required keys warn, never fail).

### Discovery order (first match wins)

1. **Local** — `<agent-dir>/plugins/<name>/`
2. **Global** — `~/.gitagent/plugins/<name>/`
3. **Installed** — `<agent-dir>/.gitagent/plugins/<name>/`

### Plugin structure

```
my-plugin/
├── plugin.yaml          # Manifest (required)
├── tools/               # Declarative tool definitions
│   └── *.yaml
├── hooks/               # Hook scripts
├── skills/              # Skill modules
├── prompt.md            # System prompt addition
└── README.md
```

## MCP (Model Context Protocol)

GitAgent is an **MCP client** over **stdio**: point it at a server and its
tools register as `<server>__<tool>` (sanitised, capped at 64 chars),
fail-soft per server, cursor pagination followed, connections closed on
every exit path.

```yaml
# agent.yaml
mcp_servers:
  filesystem:
    command: npx
    args: ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/data"]
    env:
      LOG_LEVEL: "${MCP_LOG_LEVEL}"   # ${VAR} interpolated
    cwd: /path/to/data                # optional
    timeoutMs: 30000                  # connect/list timeout (default 30000)
```

> HTTP/SSE server entries parse but are skipped with a warning in this
> port — stdio is fully implemented. Like upstream v1: tools only, no
> resources/prompts.

## Multi-Model Support

One OpenAI-compatible client covers every provider. Spec formats:
`provider:model`, OpenCode-style `provider/model`, and
`provider:model@base-url` (or `GITAGENT_MODEL_BASE_URL`) for any gateway:

```yaml
# agent.yaml
model:
  preferred: "anthropic:claude-sonnet-4-6"
  fallback:
    - "openai:gpt-4o-mini"
    - "ollama:qwen3:8b"
```

| Provider prefix | Key env var | Notes |
|---|---|---|
| `openai` | `OPENAI_API_KEY` | |
| `anthropic` | `ANTHROPIC_API_KEY` | via OpenAI-compat gateway shape |
| `google`, `gemini` | `GEMINI_API_KEY` | |
| `xai` | `XAI_API_KEY` | |
| `groq` | `GROQ_API_KEY` | |
| `mistral` | `MISTRAL_API_KEY` | |
| `ollama` | — (none) | `http://localhost:11434/v1`, needs `ollama serve` |
| `lyzr` | `LYZR_API_KEY` | `lyzr:<agent-id>@https://agent-prod.studio.lyzr.ai/v4` |
| `opencode` | `OPENCODE_API_KEY` | Zen chat-completions models (DeepSeek, Kimi, GLM, MiniMax, …) |
| `opencode-go` | `OPENCODE_API_KEY` | Go plan: separate `/zen/go/` endpoint, `opencode-go/<id>` |

Zen/Go GPT/Claude/Gemini models live on `/responses`, `/messages`,
`/models/…` endpoints with different protocols and are **not** callable
here — use those providers' native keys instead. The client identifies as
`gitagent/<version>` and sends `x-opencode-session` on opencode endpoints
(their docs ask third-party clients to).

## Inheritance & Composition

```yaml
# agent.yaml
extends: "https://github.com/org/base-agent.git"   # shallow-cloned, deep-merged (child wins)

dependencies:
  - name: shared-tools
    source: "https://github.com/org/shared-tools.git"
    version: main
    mount: tools

delegation:
  mode: auto
```

`extends`/dependencies clone with argv-only git (a malicious
`extends: "$(cmd)"` fails to clone instead of executing). `delegation` is
preserved for forward compatibility.

## Compliance & Audit

```yaml
# agent.yaml
compliance:
  recordkeeping:
    audit_logging: true     # → append-only .gitagent/audit.jsonl (results sliced to 1000 chars)
    retention_days: 90
```

Audit logging is honored; compliance *rule validation* is not yet ported
(see `Study.md` gaps).

## Observability

No OTLP exporter in this port — instead, gated local sinks:

| Sink | Gate | Location |
|---|---|---|
| Cost tracker | always | per-model tokens/USD, printed per turn |
| Audit log | `compliance.recordkeeping.audit_logging` | `.gitagent/audit.jsonl` |
| Chat history | always | `.gitagent/chat-history/<branch>.jsonl` |
| Telemetry | `GITAGENT_TELEMETRY=1` | `.gitagent/telemetry.jsonl` |

A full OTLP exporter attaches at `core/src/observe/telemetry.rs`
(documented extension point in `Study.md`).

## Harness interop

Export any agent to five external harnesses (best-effort mappings):

```bash
gitagent integrations list     # all five + what each exports
gitagent integrations detect   # env/binary heuristics
gitagent integrations export --format opencode --out ./interop --dir ./my-agent
```

| `--format` | Writes |
|---|---|
| `opencode` | `opencode.json` (`model` + `agent.<name>.prompt` + `mcp`) |
| `nanobot` | `nanobot.yaml` + `SYSTEM.md` |
| `openclaw` | `openclaw.json` (agents + tools) |
| `claude-code` | `CLAUDE.md` + `.claude/settings.json` |
| `lyzr` | `.env.lyzr` (`LYZR_API_KEY` + `lyzr:<id>@<base>`) |

Then run it under OpenCode from the export dir:

```bash
cd ./interop && opencode          # TUI picks up opencode.json
opencode run "Summarize this repo"  # headless one-shot
```

Or skip exporting and point `gitagent` itself at OpenCode's gateway:

```bash
export OPENCODE_API_KEY="..."   # Zen key from opencode.ai/auth
gitagent --dir ./my-agent --model "opencode/kimi-k2.6" "do something"
# Go plan ($10/mo) instead: --model "opencode-go/kimi-k2.6"
```

## Repository layout

Three folders, three crates (`core/` holds a crate named `engine` — a
package literally called `core` breaks every proc macro emitting
`core::…` paths; see `core/src/lib.rs`):

```
core/                 backbone library (13 modules: helpers, agent, llm,
                      manifest, loader, tools, learning, hooks, plugins,
                      mcp, session, observe, integrations)
sdk/                  public query()/Session/tool() API
cli/                  the gitagent binary (+ README manual)
installer/            install.sh (binary install + backend setup wizard)
examples/             runnable agents + offline mock LLM (see its README)
Study.md              deep-dive porting log: patterns, gaps, test commands
```

## Development

```bash
cargo build --workspace
cargo test --workspace        # 285+ tests, offline: no keys, no network
cargo test -p engine          # incl. mock-SSE provider regression suite
cargo clippy --workspace --all-targets   # zero-warning gate
cargo fmt --all -- --check
python3 examples/mock-llm.py &           # offline end-to-end (see Study.md §9)
```

Every source file carries a header (purpose, ported-from, patterns,
contents) and every public function a runnable doctest. Conventions and
known gaps live in [`Study.md`](Study.md); per-folder manuals in
[`cli/README.md`](cli/README.md) and [`examples/README.md`](examples/README.md).

## FAQ

### General

**What is GitAgent?**
A git-native AI agent framework where the agent IS a git repository —
identity, rules, memory, tools, skills version-controlled; `git log` your
agent's memory, diff its rules.

**How does it differ from the TypeScript original?**
Agent directories are compatible both ways, but this port re-implements the
loop (`core::agent`) and the model layer (`core::llm`) in Rust — no Node.js,
single `gitagent` binary, same `agent.yaml` schema.

### Installation & Setup

**What are the requirements?**
Rust 1.75+, git. `./installer/install.sh` (or `cargo install --path cli`).
No Node.js, no voice mode.

**How do I set up API keys?**
Run the installer wizard, or export them manually (`OPENAI_API_KEY`,
`ANTHROPIC_API_KEY`, `OPENCODE_API_KEY`, `OLLAMA_*` needs none — just
`ollama serve`).

**Which models work?**
Anything OpenAI-compatible: OpenAI, Anthropic-via-gateway, Google, xAI,
Groq, Mistral, Ollama (local, keyless), Lyzr, OpenCode Zen/Go
(chat-completions families). Override with
`gitagent --model provider:model` (also `provider/model`,
`provider:model@base-url`).

### Core Concepts

**How do I use the SDK?**
```rust
use sdk::{query, QueryOptions};
let mut rx = query(QueryOptions::new("./my-agent".into(), "hello"));
while let Some(m) = rx.recv().await { /* Delta/Assistant/ToolUse/… */ }
```

**How do local repo sessions work?**
`gitagent --repo URL --pat TOKEN "task"` clones, branches
`gitagent/session-<id>`, and on exit commits + pushes + scrubs the token.
Resume with `--session <branch>`.

**What hooks fire when?**
`on_session_start`, `pre_tool_use`, `post_tool_failure`, `post_response`,
`pre_query`, `file_changed` (write/edit only), `on_error` — scripts get
JSON on stdin, reply `{allow|block|modify}`, 10s timeout, fail-open.

### Development

**How do I add a tool?**
Declarative `tools/<name>.yaml` + script (args JSON on stdin), or
`sdk::tool()` closures in-process, or an MCP stdio server in
`mcp_servers:`.

**How do I add a skill?**
`skills/<name>/SKILL.md` with `name`+`description` frontmatter; finished
tasks crystallize into new skills via `skill_learner`.

### Troubleshooting

**Silent `session_end` with no output?**
That was a bug and is fixed: provider failures now print
`error: …` (stderr, exit 1). If you still see silence, paste the full
output — it means the failure happens before the loop starts (bad
`agent.yaml`, missing dir).

**Missing key?**
Startup exits 1 naming the exact variable (`OPENCODE_API_KEY` for
`opencode*`, none needed for `ollama`).

**Mock SSE rejected?**
`finish_reason` must sit *inside* the choice object (valid OpenAI shape);
the client drops malformed payloads rather than hallucinating tool calls.

**Where can I get help?**
`Study.md` (§7 gaps, §9 test recipes), `examples/` (runnable agents +
mock server), GitHub Issues: https://github.com/open-gitagent/gitagent/issues

## License

This project is licensed under the [MIT License](./LICENSE) — same as the
original TypeScript project.
