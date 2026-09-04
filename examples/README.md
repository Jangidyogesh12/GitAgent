# examples/

Runnable content for this workspace (this folder was empty scaffolding before).

| Path | What it is |
|---|---|
| `minimal-agent/` | Smallest working agent (`agent.yaml` + `SOUL.md` + `memory/`) |
| `demo-agent/` | Full showcase: skills, declarative tool, knowledge (always-load + on-demand), workflow, hook, few-shot, sub-agent, plugin |
| `mock-llm.py` | Mock OpenAI-SSE server for offline end-to-end runs (no API key) |

## Run with a real key

```bash
export OPENAI_API_KEY="sk-..."
cargo run -p cli -- --dir examples/minimal-agent "hello"
cargo run -p cli -- --dir examples/demo-agent          # REPL, try /skills /plugins /memory
```

## Run fully offline (mock LLM, no key, no network)

```bash
python3 examples/mock-llm.py &                          # turn 1: read call, turn 2: final text
OPENAI_API_KEY=dummy cargo run -p cli -- --dir examples/demo-agent \
  --model "openai:mock@http://127.0.0.1:8090/v1" -p "read the soul file"
# expect: banner → ⚙ read(...) → ✓ result → final text → (session_end), exit 0
kill %1
```

## Inspect without running

```bash
cargo run -p cli -- plugin list --dir examples/demo-agent
# → greeter [local] (enabled) — Adds a greeting section…

cargo run -p cli -- integrations export --format opencode \
  --out /tmp/interop --dir examples/demo-agent
# → /tmp/interop/opencode.json (model + full system prompt + tools)
```

## SDK demo (needs a key or mock)

```bash
cargo run -p sdk --example demo -- examples/demo-agent "hello"
```
