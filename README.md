# GitAgent (Rust workspace)

> A git-native AI agent framework, reimplemented in Rust.
> Your agent **lives inside a git repo**: identity, rules, memory, tools and
> skills are version-controlled files. Point `gitagent` at the folder and it
> reads them, builds one big system prompt, and starts a think → act →
> observe loop with an LLM. Memory saves are git commits, so the agent's
> whole life history is in git.

Ported from the TypeScript [`gitagent`](https://github.com/open-gitagent/gitagent)
(`@open-gitagent/gitagent` v2.2.0, pi-agent-core + pi-ai engine). This port
re-implements **both** the agent loop and the model layer in Rust — no
Node.js required at runtime.

## What you get

- **CLI** — `gitagent --dir ./my-agent "do something"` (one-shot) or
  `gitagent --dir ./my-agent` (REPL with `/memory /skills /tasks /plugins
  /skill:name ...`). Full manual: [`cli/README.md`](cli/README.md).
- **SDK** — `query()` streaming API + `Session` multi-turn handle +
  `tool()` closure tools (`sdk`; example: `cargo run -p
  sdk --example demo -- ./my-agent "hello"`).
- **Tools** — `cli`, `read`, `write`, `edit`, `memory`, `task_tracker`,
  `skill_learner` + declarative `tools/*.yaml` script tools + MCP (stdio)
  + plugin tools.
- **Learning** — task lifecycle with skill matching, worthiness heuristic,
  crystallisation into `skills/`, confidence reinforcement (success
  `+0.1(1-c)` / failure `-0.2` / partial `-0.05`, flagged `< 0.4`).
- **Harness interop** — export agents for **OpenCode** (`opencode.json`),
  NanoBot, OpenClaw, Claude Code (`CLAUDE.md`), Lyzr (`.env.lyzr`):
  `gitagent integrations export --format opencode --out ./interop`.
- **Safety** — argv-only git (no shell-injection via `extends:`), PAT scrub
  on session finalize, hook path-traversal guards, permission modes
  (`plan` read-only … `bypass`) + allow/deny rules.

## Layout (Cargo workspace — 3 crates)

```
core/                 the backbone library (crate name: `engine`)
  src/helpers/        shared utils (env, fs, text budgets, frontmatter, jsonl)
  src/agent/          LLM-agnostic engine (loop, transcript, tool/gate seams)
  src/llm/            provider layer (OpenAI-compat SSE + fallback + retry)
  src/manifest/       agent.yaml schema
  src/loader/         dir → system prompt assembly
  src/tools/          cli/read/write/edit/memory + YAML script tools
  src/learning/       task_tracker + skill_learner + reinforcement
  src/hooks/          hooks.yaml lifecycle scripts + gate adapter
  src/plugins/        plugin.yaml discovery/install/config
  src/mcp/            MCP stdio client
  src/session/        repo clone → session branch → push + PAT scrub
  src/observe/        cost tracker, audit log, chat history, telemetry
  src/integrations/   OpenCode/NanoBot/OpenClaw/ClaudeCode/Lyzr adapters
sdk/                  public query()/Session/tool() API (crate: `sdk`)
cli/                  the `gitagent` binary (crate: `cli`, docs in its README)
installer/install.sh  binary install + interactive backend setup
Study.md              deep dive: what was ported, patterns used, gaps
```

Dependency rule: `engine::agent` knows nothing about providers/files/git;
`cli` is presentation only; everything flows through `sdk`. (The `core/`
folder holds a crate named `engine` — a package literally called `core`
would shadow std's `core` inside every proc macro, so the folder keeps
your name and the crate takes the safe one; see `core/src/lib.rs`.)

## Install

```bash
./installer/install.sh
# binary only: GITAGENT_NO_SETUP=1 ./installer/install.sh
# or: cargo install --path cli
```

Needs Rust 1.75+, git, and an LLM key (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`,
`LYZR_API_KEY`, …) or local Ollama (`ollama serve`).

## Use

```bash
export OPENAI_API_KEY="sk-..."
gitagent --dir ~/my-project "Explain this project"   # scaffolds on first run
gitagent --dir ~/my-agent                            # REPL

# repo mode: clone, branch gitagent/session-<id>, push on exit
gitagent --repo https://github.com/org/repo --pat "$GITHUB_TOKEN" -p "fix the flaky test"

# SDK (Rust)
# for await messages with query(QueryOptions::new(dir, prompt)) — see Study.md §7
```

## Examples

Runnable agents + offline mock LLM live in [`examples/`](examples/):

```bash
cargo run -p cli -- --dir examples/demo-agent   # needs OPENAI_API_KEY; try /skills /plugins
python3 examples/mock-llm.py &                  # keyless offline runs (see examples/README.md)
```

## Test

```bash
cargo test --workspace        # 285+ unit + doc tests (no keys, no network)
cargo test -p engine  # incl. mock-SSE-server provider regression tests
```

End-to-end against a mock LLM + all commands are scripted in
**[Study.md](Study.md §8)**.

## Design patterns

Each file header names its patterns (source:
[refactoring.guru/design-patterns/rust](https://refactoring.guru/design-patterns/rust)):
Strategy (`AgentTool`, `ToolGate`, `LlmClient`, `SandboxExec`), Template
Method (`run_loop`), Observer (event/message channels), Builder
(`Agent`, `PromptBuilder`, `QueryOptions`, `AgentManifest::scaffold`),
Factory (`builtin_tools`, `resolve_model`, `adapter_for`), Adapter
(MCP/hook/harness bridges), Facade (`load_agent`, `query`, `McpManager`),
Decorator (filters, gates, retry/backoff), Command (tools, CLI/subcommands),
Chain of Responsibility (gates, fallbacks, hooks), State (task lifecycle),
Plugin (plugins), RAII (finalize/cleanup/kill-on-drop).
