# core/ — the backbone library (crate `engine`)

All agent logic lives here as modules; `sdk/` and `cli/` build on top.
Folder is `core/`, crate is `engine` (a package literally named `core`
breaks proc macros that emit `core::…` paths — see `src/lib.rs`).

| Module | Ports (TS) | Role |
|---|---|---|
| `helpers` | `env-utils.ts`, `tools/shared.ts`, token math, jsonl stores | shared utils |
| `agent` | pi-agent-core (external!) | loop, transcript, tool/gate/client seams |
| `llm` | pi-ai (external!) + model resolution | OpenAI-compat SSE + fallback + retry |
| `manifest` | `AgentManifest` (`loader.ts`) | pure `agent.yaml` schema + scaffold |
| `loader` | `loader.ts` + discovery modules | dir → `LoadedAgent` system prompt |
| `tools` | `tools/*.ts` + `tool-loader.ts` | hands + YAML script tools + registry |
| `learning` | `learning/`, `task-tracker.ts`, `skill-learner.ts` | tasks + skills + confidence |
| `hooks` | `hooks.ts` | lifecycle scripts + `HookGate` |
| `plugins` | `plugins.ts`, `plugin-types.ts` | discovery/install/config |
| `mcp` | `mcp/` | stdio JSON-RPC client, fail-soft |
| `session` | `session.ts` | clone → branch → push + PAT scrub |
| `observe` | cost/audit/history/telemetry | costs, JSONL logs, gated telemetry |
| `integrations` | Lyzr bits (extended ×5) | OpenCode/NanoBot/OpenClaw/ClaudeCode/Lyzr |

Rule: `agent` knows nothing about providers/files/git (pure engine);
everything else plugs into its `AgentTool` / `ToolGate` / `LlmClient` seams.

```bash
cargo test -p engine   # unit + doc tests + mock-SSE provider suite (tests/)
cargo doc -p engine --open
```

## Providers — how the model layer resolves them

One OpenAI-compatible client (`llm::OpenAiCompat`) serves every provider.
`llm::resolve_model()` turns a spec string into endpoint + key:

- **Forms:** `provider:model`, `provider/model` (OpenCode style),
  `provider:model@base-url` (or `GITAGENT_MODEL_BASE_URL` for any gateway).
- **Key lookup:** `<PROVIDER>_API_KEY` (uppercased, `-`→`_`), plus
  `LYZR_API_KEY` for `lyzr` and `OPENCODE_API_KEY` for `opencode*`.
  Missing key → empty string (keyless local endpoints like Ollama).

| Prefix | Base URL | Key |
|---|---|---|
| `openai` | `https://api.openai.com/v1` | `OPENAI_API_KEY` |
| `anthropic` | `https://api.anthropic.com/v1` | `ANTHROPIC_API_KEY` |
| `google`, `gemini` | `…/generativelanguage…/openai` | `GEMINI_API_KEY` |
| `xai` / `groq` / `mistral` | vendor URLs | `XAI_API_KEY` / `GROQ_API_KEY` / `MISTRAL_API_KEY` |
| `ollama` | `http://localhost:11434/v1` | none |
| `lyzr` | per-agent `@base-url` | `LYZR_API_KEY` |
| `opencode` | `https://opencode.ai/zen/v1` (Zen) | `OPENCODE_API_KEY` |
| `opencode-go` | `https://opencode.ai/zen/go/v1` (Go) | `OPENCODE_API_KEY` |
| anything else | `https://api.openai.com/v1` | `<PREFIX>_API_KEY` |

Only `chat/completions`-speaking models work (Zen/Go GPT, Claude, Gemini
and `/messages` models use other protocols — use native keys instead).
Failover: preferred → fallbacks, transient errors (429/5xx/timeout)
retried with capped backoff; total failure becomes an
`AssistantMessage::failure`, never a panic. Requests identify as
`gitagent/<version>` and carry `x-opencode-session` on opencode endpoints.
