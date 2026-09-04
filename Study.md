# Study.md — understand this codebase, file by file

> This is a **reading guide**, not a reference dump. It tells you *in which
> order* to review the code so each file makes sense when you reach it,
> then keeps the porting notes (patterns, fidelity, gaps) behind it.
>
> Time budget: ~2 hours cover-to-cover. Each stop says what to read, what
> to notice, and one thing to try.

**The 10-second version:** a prompt enters at `cli/`, gets assembled with
an agent directory in `sdk/` + `core/src/loader`, runs a think→act loop in
`core/src/agent`, and streams back out. Everything else is tools the loop
can call or files the prompt is built from.

---

## 1. Mental model + one prompt's journey (read this first, 10 min)

Before opening any file, fix this loop in your head — it is the entire
product:

```
user prompt → build system prompt → pick model → register tools
    → loop { ask model → run tool calls → feed results back }
    → final answer → stream to user
```

Now trace one real call through exact functions (open each as you go):

| Step | File | Function | What happens |
|---|---|---|---|
| 1 | `cli/src/main.rs` | `main()` → `query_options()` | flags → `sdk::QueryOptions` |
| 2 | `sdk/src/query.rs` | `query()` → `run_query()` | the whole pipeline, one async task |
| 3 | `core/src/loader/load.rs` | `load_agent()` | `agent.yaml` → sections → ONE prompt string |
| 4 | `sdk/src/query.rs` | `build_registry()` | builtin + learning + YAML + plugin + MCP tools, allow→deny filter |
| 5 | `sdk/src/permissions.rs`, `core/src/hooks/gate.rs` | `PermissionGate`, `HookGate` | policy chain for every tool call |
| 6 | `core/src/llm/spec.rs` | `resolve_model()` | `"opencode/kimi-k2.6"` → URL + key |
| 7 | `core/src/agent/agent.rs` | `Agent::prompt()` → `run_loop()` | the loop; emits `AgentEvent`s |
| 8 | `core/src/llm/fallback.rs` | `stream_once()` | SSE → `AssistantMessage` (text / tool calls) |
| 9 | `core/src/agent/agent.rs` | `run_one()` | gates → timeout → `tool.execute()` → `ToolResult` |
| 10 | `cli/src/render.rs` | `render_stream()` | events → terminal output + exit code |

Keep this table open while doing §2 — every stop below is one row zoomed in.

---

## 2. Ordered review path (the main event)

Read in this order. Each stop lists **files → look for → notice → try**.

### Stop 0 — The data: what an agent IS (10 min)
- **Read:** `examples/demo-agent/agent.yaml`, `examples/demo-agent/SOUL.md`,
  `core/src/manifest/manifest.rs` (`AgentManifest`, `scaffold()`,
  `model_spec()`), `core/src/manifest/model.rs` (`provider_api_key()`).
- **Notice:** the manifest is pure data, zero logic; `model_spec()` encodes
  the CLI-flag-beats-manifest precedence; adding a provider is one match arm
  in two tables (`default_base_url` + `provider_api_key`).
- **Try:** `cargo test -p engine manifest` — watch schema round-trips pass.

### Stop 1 — Warm-up: shared helpers (15 min)
- **Read:** `core/src/helpers/text.rs` (constants + 3 truncation strategies),
  `env.rs` (`interpolate_env`, dotenv), `frontmatter.rs`, `fsx.rs`
  (`paginate_lines`), `jsonl.rs`.
- **Notice:** everything is a pure function; the constants
  (`MAX_OUTPUT_CHARS`, `MAX_READ_LINES`, HOOK timeouts live here or next to
  their tool) are the ported TS values, unchanged.
- **Try:** break a doctest expectation (e.g. `estimate_tokens("abcd")`),
  run `cargo test -p engine helpers`, watch it fail, revert.

### Stop 2 — Vocabulary: messages, events, tools, gates (20 min)
- **Read:** `core/src/agent/message.rs` (`AgentMessage`, `ContentBlock`,
  `StopReason`, `Usage`), `event.rs` (`AgentEvent`), `tool.rs`
  (`AgentTool`, `ExecutionMode::Sequential`, `ToolOutput::err` as *data*),
  `gate.rs` (`GateDecision::{Allow, Modify, Deny}`), `client.rs`
  (`LlmClient`, `GenParams`, `NoopClient`).
- **Notice:** the three trait seams (`AgentTool`, `ToolGate`, `LlmClient`)
  are the Strategy pattern — the engine never names a real tool, gate, or
  provider. Failures are *values* (`ToolOutput::err`, `failure()`), never
  panics: that is THE load-bearing design decision of the whole workspace.
- **Try:** implement a 10-line `AgentTool` in a test that returns fixed
  text (copy `FnTool`'s shape from `sdk/src/fns.rs`).

### Stop 3 — The heart: loop + compaction (25 min)
- **Read:** `core/src/agent/compact.rs` (`Compactor`: 75 % budget,
  truncate-in-place, never drop a message), then `core/src/agent/agent.rs`
  (`LoopConfig`/`LoopContext`, `run_loop()`, `run_one()`, `Agent` builder).
- **Notice:** Template Method — `run_loop` is a fixed skeleton, config
  injects the variable parts. Parallel batches serialize if ANY tool is
  `Sequential`. `Length` stop gets ≤3 continue-nudges. Gates run before
  every call; denials become model-visible results.
- **Try:** `cargo test -p engine agent::` — clean finish + unknown-tool
  recovery. Then set `max_turns: 1` mentally and re-trace what stops.

### Stop 4 — The provider: specs, SSE, fallback (20 min)
- **Read:** `core/src/llm/spec.rs` (`resolve_model`, both `:` and `/`
  forms, `is_transient_error`), `compat.rs` (`to_wire_messages`,
  `recover_text_tool_call`, `OpenAiCompat`), `fallback.rs`
  (`complete_with_fallback`, `stream_once`, `collect_sse`).
- **Notice:** errors-as-values again; retries only on *transient* failures
  with capped backoff; tool-less providers degrade to bare chat; small
  local models get fenced-JSON tool-call recovery; `x-opencode-session`
  + `gitagent/<ver>` UA go out on opencode endpoints.
- **Try:** `cargo test -p engine llm` then read `core/tests/mock_sse.rs`
  — a fake TCP server proving text + tool-call parsing with no network.

### Stop 5 — The assembly line: prompt building (20 min)
- **Read:** `core/src/loader/discover.rs` (skills/knowledge/workflows/
  agents/examples readers — all fail-soft), `prompt.rs` (`PromptBuilder`
  + hardcoded memory/workspace/learning blocks), `load.rs` (`load_agent()`,
  argv-only `clone_git_repo`, session state).
- **Notice:** the prompt is just ordered string sections, empties skipped;
  `extends:` clones with NO shell (the TS RCE lesson); plugin `# Plugin:`
  sections slot in after examples; missing files are `""`, never errors.
- **Try:** `./target/debug/gitagent integrations export --format opencode
  --out /tmp/x --dir examples/demo-agent` and grep the JSON for each
  section (skill? plugin? knowledge? workflow?).

### Stop 6 — The hands: tools (20 min)
- **Read:** `core/src/tools/factory.rs` (`builtin_tools`), `cli.rs`
  (timeout → group-kill → tail-cap; `SandboxExec` swap), `read.rs`
  (binary sniff, the only `Parallel` tool), `write.rs`, `edit.rs`
  (`apply_edit` pure core), `memory.rs` (layers + archive overflow +
  best-effort commit), `declarative.rs` (stdin-JSON contract,
  absolutised script paths).
- **Notice:** `ExecutionMode` is a safety contract, not a hint; every
  failure mode returns `ToolOutput::err`, never `Err`.
- **Try:** `echo '{"text":"hi"}' | sh examples/demo-agent/tools/shout.sh`
  — that's literally what the agent executes.

### Stop 7 — The learning loop (15 min)
- **Read:** `core/src/learning/reinforcement.rs` (the exact
  `+0.1(1−c)`/`−0.2`/`−0.05`/`<0.4` math), `tasks.rs` (State transitions +
  skill matcher + Observer into reinforcement), `skills_learn.rs`
  (worthiness heuristic + success-gated crystallize).
- **Notice:** `end` with `skill_used` is the Observer notification that
  closes the loop task → confidence.
- **Try:** `cargo test -p engine learning` — begin/update/end flow.

### Stop 8 — The services: hooks, plugins, MCP, session, observe (20 min)
- **Read:** `core/src/hooks/exec.rs` (10 s timeout, verdict parsing,
  traversal guard, fail-open) → `gate.rs`; `core/src/plugins/discover.rs`
  (3 scopes, user>env>default config); `core/src/mcp/manager.rs`
  (handshake, pagination, `server__tool` names, idempotent cleanup);
  `core/src/session/local.rs` (`authed_url`/`clean_url`, PAT scrub in
  `finalize`); `core/src/observe/` (cost/audit/history/telemetry sinks);
  `core/src/integrations/` (five adapters + `export_for`).
- **Notice:** every service is fail-soft toward the session — a bad hook,
  plugin, MCP server, or clone can warn, never kill the run.
- **Try:** `./target/debug/gitagent plugin list --dir examples/demo-agent`
  and `integrations list`.

### Stop 9 — The Facade: SDK (15 min)
- **Read:** `sdk/src/types.rs` (`QueryOptions` builders),
  `sdk/src/query.rs` (steps 1–5 of the journey table in code),
  `sdk/src/session.rs` (transcript ownership across turns),
  `sdk/src/permissions.rs` (deny → allow → mode), `sdk/src/fns.rs`.
- **Notice:** `query()` is steps 2–9 of §1 in one function; error turns
  map to `SdkMessage::Error` (the fix for silent sessions); dropping the
  receiver ends the turn.
- **Try:** `cargo run -p sdk --example demo -- examples/demo-agent "hi"`
  (needs a key or mock — see `examples/README.md`).

### Stop 10 — The surface: CLI (10 min)
- **Read:** `cli/src/main.rs` (flags → `query_options`, key check, banner,
  one-shot vs REPL), `render.rs` (event → terminal + exit code),
  `repl.rs` (slash table), `scaffold.rs`, `plugin_cmd.rs`,
  `integrations_cmd.rs`.
- **Notice:** presentation only — zero agent logic; `--dir` is global so it
  works before and after subcommands.
- **Try:** `/skills`, `/plugins`, `/memory` in the REPL; `--help`.

### Stop 11 — Tie it together (15 min)
- **Run:** `python3 examples/mock-llm.py &` + the demo-agent mock command
  from `examples/README.md`; watch steps 7–10 of the journey table happen
  live (tool call → result → final text).
- **Read:** `examples/mock-llm.py` (why `finish_reason` sits inside the
  choice object) and `core/tests/mock_sse.rs` once more — now you know
  what both sides of the wire mean.

---

## 3. Workspace map (reference)

| Crate / module | Ports (TS) | Role |
|---|---|---|
| `core/helpers` | `env-utils.ts`, `tools/shared.ts`, token math, jsonl stores | shared utils, zero agent logic |
| `core/agent` | pi-agent-core (external!) | LLM-agnostic engine: transcript, events, tool/gate/client seams, loop |
| `core/llm` | pi-ai (external!) + model resolution | `provider:model[@base]` → SSE streaming + fallback + backoff |
| `core/manifest` | `AgentManifest` interface (`loader.ts`) | pure `agent.yaml` schema + scaffold template |
| `core/loader` | `loader.ts` + discovery modules | dir → `LoadedAgent` (prompt in TS section order) |
| `core/tools` | `tools/*.ts` + `tool-loader.ts` | the hands + declarative YAML tools + registry Factory |
| `core/learning` | `learning/`, `task-tracker.ts`, `skill-learner.ts` | tasks (State) + skills + confidence math |
| `core/hooks` | `hooks.ts` (+ `sdk-hooks.ts` idea) | `hooks.yaml` scripts + `HookGate` adapter |
| `core/plugins` | `plugins.ts`, `plugin-types.ts` | discovery/install/config/prompt+hook contributions |
| `core/mcp` | `mcp/` | stdio JSON-RPC client, namespaced tools, fail-soft setup |
| `core/session` | `session.ts` | clone → `gitagent/session-<8hex>` → push + PAT scrub |
| `core/observe` | cost/audit/history/telemetry | costs, audit JSONL, per-branch history, env-gated telemetry |
| `core/integrations` | Lyzr bits (extended ×5) | OpenCode/NanoBot/OpenClaw/ClaudeCode/Lyzr adapters |
| `sdk` | `sdk.ts`, `sdk-types.ts`, `exports.ts` | `query()` Facade, `Session`, `tool()`, permissions |
| `cli` | `index.ts`, `plugin-cli.ts` | `gitagent` binary: flags, REPL, render, plugin/integrations cmds |

Three folders, three crates: `core/` holds the library (crate `engine`;
the folder keeps the name `core` but a package literally called `core`
breaks every proc macro emitting `core::…` paths — `async_trait`, clap
derive, `tokio::main` — so the crate takes the safe name; see
`core/src/lib.rs`), `sdk/` the public API, `cli/` the binary.

Dependency rule (enforced by the module graph): `engine::agent` depends on
nothing domain-specific; `cli` is presentation only; `sdk` is the only
Facade that touches everything.

---

## 4. Design patterns used (with pointers)

Source: https://refactoring.guru/design-patterns/rust. Every file header
names its patterns; the map below is the complete index.

- **Strategy** — the core seam style. `AgentTool`
  (`core/src/agent/tool.rs`; every tool is interchangeable), `ToolGate`
  (`core/src/agent/gate.rs`; permissions, hooks), `LlmClient`
  (`core/src/agent/client.rs`; `core/src/llm/compat.rs` implements it,
  `NoopClient` fakes it in tests), `SandboxExec`
  (`core/src/tools/cli.rs`; local shell vs remote-VM twin without
  duplicating timeout logic).
- **Template Method** — `run_loop()` (`core/src/agent/agent.rs`) is the
  fixed skeleton (steering → budget → ask model → run batch → repeat);
  `LoopConfig` injects client/compactor/gates/timeouts.
- **Observer** — `AgentEvent` over tokio mpsc (`core/src/agent/event.rs`);
  the loop publishes, CLI renders, SDK maps to `SdkMessage`.
  Session/subject decoupling with zero callbacks.
- **Builder** — `Agent::with_*` (`core/src/agent/agent.rs`),
  `PromptBuilder` (`core/src/loader/prompt.rs`), `QueryOptions::with_*`
  (`sdk/src/types.rs`), `AgentManifest::scaffold()`
  (`core/src/manifest/manifest.rs`).
- **Factory** — `builtin_tools()` (`core/src/tools/factory.rs`),
  `resolve_model()` (`core/src/llm/spec.rs`), `adapter_for()`
  (`core/src/integrations/harness.rs`).
- **Adapter** — `McpTool` (remote→`AgentTool`), `HookGate`
  (scripts→`ToolGate`), five `HarnessAdapter`s (gitagent→native configs),
  `to_wire_messages()` (transcript→OpenAI messages).
- **Facade** — `load_agent()`, `query()`, `McpManager::setup()`,
  `export_for()`, `init_local_session()`.
- **Decorator** — allow/deny registry filters, gate wrapping, retry/backoff
  around the SSE call, usage-accounting stream mapping.
- **Command** — each tool encapsulates name+args as an invocable object;
  clap subcommands + REPL slash commands map 1:1 to handlers.
- **Chain of Responsibility** — gates in order (first Deny/Modify wins),
  model fallback specs, hook definitions, permission deny→allow→mode.
- **State** — `TaskStatus` Active→Succeeded|Failed with illegal transitions
  rejected (`core/src/learning/tasks.rs`).
- **Plugin** — discovery/validation/contributions (`core/src/plugins`).
- **RAII** — `finalize()` (commit+push+PAT scrub), `McpManager::cleanup()`
  (idempotent), `kill_on_drop(true)` on children, abort flag.

---

## 5. Behavioural fidelity notes (constants kept identical)

All TS constants were preserved (see `core/src/helpers/text.rs` + per-file
docs): `MAX_OUTPUT` 100 000 (tail), `MAX_LINES` 2000, `MAX_BYTES` 100 000,
cli default timeout 120 s, hook timeout 10 s, declarative timeout 120 s,
MCP default 30 000 ms + 64-char names, factory truncation 50 000,
skill match > 0.1, Jaccard novelty > 0.5, generalizable ≤ 30 %, confidence
`+0.1(1-c)`/`-0.2`/`-0.05`/flag `< 0.4`/neg-examples cap 10, token
`chars/4`, compaction trigger 75 % of window, tool-result cap 10 000,
scaffold (`openai:gpt-4o-mini`, max_turns 50), branch
`gitagent/session-<8hex>`, audit slice 1000.

Deliberate adaptations (each documented at the code site):
1. **Errors as values** — provider/tool/hook failures become message/result
   data, never panics (the TS crash lessons from `rust/gitagent-rs`). A
   failed provider turn surfaces as a visible `error:` + exit 1, never a
   silent empty session.
2. **Compaction wired into the loop** — TS exported `compact.ts` with zero
   in-loop callers; here `Compactor` is a `LoopConfig` seam.
3. **Declarative tools are Sequential** — TS ran them parallel; scripts can
   touch anything, so fail-safe serialisation (documented in
   `core/src/tools/declarative.rs`).
4. **Prompt order extended** — TS order kept; plugin `# Plugin: <name>`
   sections appended after examples (same position as TS).
5. **`plugin.yaml` edits** — TS used comment-preserving `yaml.parseDocument`;
   here only the `plugins:` table round-trips (rest preserved as parsed —
   key order stable, comments may move; noted in `cli/src/plugin_cmd.rs`).
6. **No sandbox/voice** — e2b `gitmachine` peer and `@open-gitagent/voice`
   have no Rust equivalent here; `SandboxExec` is the seam where a backend
   plugs in (§7).
7. **Absolute script paths for spawned children** — hook and declarative
   scripts are canonicalised before spawn: a relative argv combined with
   the child's `cwd` re-resolves inside the new cwd (exit 127). Covered by
   regression tests in `core/src/hooks/exec.rs` and
   `core/src/tools/declarative.rs`.

---

## 6. New work: OpenCode (+ NanoBot/OpenClaw/ClaudeCode/Lyzr) support

The TS repo had **no** NanoBot/OpenClaw/OpenCode support (verified by grep:
zero matches) and only a Lyzr model-backend convention. This port adds
`core/src/integrations` with two-way interop per harness:

| Harness | Export files | Notes |
|---|---|---|
| `opencode` | `opencode.json` (model + agent prompt + mcp) | first-class |
| `nanobot` | `nanobot.yaml` + `SYSTEM.md` | agent/model/tools + git memory flag |
| `openclaw` | `openclaw.json` (agents[] + tools) | skills passed through |
| `claude-code` | `CLAUDE.md` + `.claude/settings.json` | full prompt → CLAUDE.md; tools → allow perms |
| `lyzr` | `.env.lyzr` | `LYZR_API_KEY` + `lyzr:<id>@<base>` (TS `install.sh` flow values) |

Plus `detect_available()` (env/binary heuristics) and
`normalise_model_string()` (Lyzr-aware). CLI: `gitagent integrations
list|detect|export` (manual in `cli/README.md`). All mappings are
best-effort and labelled as such — harnesses own their schemas.

### Running gitagent ON OpenCode's gateway (provider direction)

Spec formats accepted: `provider:model`, OpenCode-style `provider/model`,
and `provider:model@base-url`. OpenCode entries (verified against
opencode.ai/docs):

| Spec prefix | Base URL | Key env var |
|---|---|---|
| `opencode` | `https://opencode.ai/zen/v1` (Zen) | `OPENCODE_API_KEY` |
| `opencode-go` | `https://opencode.ai/zen/go/v1` (Go plan) | `OPENCODE_API_KEY` |

Only the `.../chat/completions` families speak OpenAI chat SSE
(Zen: DeepSeek, Kimi, GLM, MiniMax, …; Go: Kimi, DeepSeek, GLM, MiMo,
Hy3/Hy4, Omen Alpha, LongCat-2.0, …). GPT/Claude/Gemini models live on
`/responses`, `/messages`, `/models/…` endpoints with different protocols
and are NOT callable here. The client identifies as `gitagent/<version>`
and sends `x-opencode-session` on opencode endpoints (their docs ask
third-party clients to, so Go accounts aren't flagged).

---

## 7. Known gaps (honest list)

1. MCP **HTTP/SSE transports** parsed but not implemented (fail-soft skip;
   stdio is complete). Extension point: `mcp/manager.rs::connect_one`.
2. **Voice mode** (`--voice`, `@open-gitagent/voice`, `capture_photo`
   freshness loop) — no Rust equivalent; the tool is omitted.
3. **Sandbox backends** (e2b `gitmachine`) — seam exists (`SandboxExec`),
   no backend ships.
4. **Schedules/cron** (`schedules.ts`, `schedule-runner.ts`) — YAML CRUD not
   ported; `node-cron` equivalent would be `tokio::time` + `cron` crate.
5. **OTLP telemetry** — env-gated JSONL sink ships; OTLP exporter attach
   point is `observe/telemetry.rs::Telemetry::event`.
6. **Programmatic plugin `entry` modules** — dynamically-imported JS
   `register()` has no Rust equivalent; declarative tools/hooks/skills/
   prompts (the file-based 90 %) are fully supported.
7. **Compliance engine** (`compliance.ts` rules) — manifest field passes
   through; validation warnings not yet ported.

---

## 8. How to use (developer paths)

```bash
# CLI
cargo run -p cli -- --dir ./my-agent "Explain this project"
cargo run -p cli -- --dir ./my-agent          # REPL

# SDK (Rust)
use sdk::{query, QueryOptions};
let mut rx = query(QueryOptions::new("./my-agent".into(), "hi"));
while let Some(m) = rx.recv().await { /* Delta/Assistant/ToolUse/... */ }

# Custom tool (mirrors TS tool())
use sdk::tool;
let t = tool("search", "Search docs",
    serde_json::json!({"type":"object","properties":{"q":{"type":"string"}}}),
    |args| Ok("results".to_string()));

# Harness export
cargo run -p cli -- integrations export --format opencode \
  --out ./interop --dir ./my-agent
```

---

## 9. How to test (commands + what they prove)

```bash
cargo test --workspace
# → 280+ tests (289 at last count), all offline: unit tests per module
#   (behavioural fidelity: confidence math, worthiness heuristic, skill
#   matcher, URL scrub, prompt order, gate precedence, SSE parsing) +
#   doctests (every public API has a runnable example — doctests ARE usage
#   tests) + mock-SSE-server provider regression tests
#   (core/tests/mock_sse.rs: text + tool-call replies over real TCP with
#   zero network dependency).

cargo test -p engine   # all lib tests: loop, SSE suite, tasks, hooks…
cargo test -p sdk      # Facade filters, permissions, closure tools
cargo test -p cli      # renderer previews
cargo clippy --workspace -- -D warnings   # lint gate
cargo fmt --check                 # format gate
```

Manual end-to-end (mock LLM, no keys) — checked in as runnable examples:

```bash
# 1. terminal A: mock server (turn 1: read tool call, turn 2: final text)
python3 examples/mock-llm.py &
# 2. terminal B: full loop against examples/demo-agent (skill + declarative
#    tool + hook + plugin discovery all load first)
OPENAI_API_KEY=dummy cargo run -p cli -- --dir examples/demo-agent \
  --model "openai:mock@http://127.0.0.1:8090/v1" -p "read the soul file"
# expect: banner → ⚙ read(...) → ✓ read: <SOUL.md> → final text → (session_end), exit 0
kill %1
```

See `examples/README.md` for the whole matrix (minimal agent, offline runs,
`plugin list`, `integrations export`, SDK demo).

Other verified commands: scaffold-on-first-run, missing-key exit 1,
`plugin init/list`, `integrations list/export` (opencode.json asserted
valid JSON with model + instructions).

---

## 10. File-comment convention (as requested)

Every source file starts with a `//!` header stating: what the file is for,
which TS file(s) it ports, which patterns it uses, and the list of
types/functions it contains. Every public function carries a `///`
description **plus a runnable example** (compiled as doctests by
`cargo test`, so examples can never rot). Deviations from TS are marked
with `NOTE:`/`SECURITY:` comments at the exact site.
