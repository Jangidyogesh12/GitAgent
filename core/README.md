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
cargo test -p engine   # unit + doc tests + mock-SSE provider suite (core/tests/)
cargo doc -p engine --open
```
