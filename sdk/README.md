# sdk/ — public programmatic API (crate `sdk`)

Ports `src/sdk.ts` + `src/exports.ts`, built on `engine` (from `../core`).
Modules: `types` (`SdkMessage`, `QueryOptions`), `permissions`
(`PermissionGate`), `fns` (`tool()`), `query` (`query()` Facade),
`session` (multi-turn `Session`).

```bash
cargo run -p sdk --example demo -- ./my-agent "hello"
cargo test -p sdk
```

## One-shot runs

```rust
use sdk::{query, QueryOptions};

let mut rx = query(QueryOptions::new("./my-agent".into(), "hi"));
while let Some(msg) = rx.recv().await {
    match msg {
        sdk::SdkMessage::Delta(t) => print!("{t}"),
        sdk::SdkMessage::Assistant(t) => println!("\nDone: {t}"),
        sdk::SdkMessage::ToolUse(_, name, _) => println!("\ncalling {name}"),
        sdk::SdkMessage::ToolResult(_, n, c, e) => println!("[{n} err={e}] {c}"),
        sdk::SdkMessage::System(s) => println!("[{s}]"),
        sdk::SdkMessage::Error(e) => eprintln!("error: {e}"),
    }
}
```

## Providers — per-call model selection

```rust
// OpenAI / Anthropic / Ollama (keyless) / custom gateway
let opts = QueryOptions::new(dir, prompt).with_model("anthropic:claude-sonnet-4-6");
let opts = QueryOptions::new(dir, prompt).with_model("ollama:qwen3:8b");
let opts = QueryOptions::new(dir, prompt).with_model("openai:mymodel@http://host:port/v1");

// OpenCode Zen / Go (OPENCODE_API_KEY)
let opts = QueryOptions::new(dir, prompt).with_model("opencode/kimi-k2.6");
let opts = QueryOptions::new(dir, prompt).with_model("opencode-go/kimi-k2.6");
```

Same spec language as the CLI (`provider:model`, `provider/model`,
`provider:model@base-url`); CLI flag beats `QueryOptions.model`, which
beats `agent.yaml → model.preferred`, then `fallback[]`. Only
`chat/completions`-family models work (see `core/README.md` provider
table); failures arrive as `SdkMessage::Error`.

## Multi-turn sessions + custom tools

```rust
use sdk::{tool, QueryOptions, Session};

let session = Session::open(QueryOptions::new("./my-agent".into(), ""))?;
let mut rx = session.send("hello".to_string()).await?;
session.abort(); // cooperative cancel

let shout = tool("shout", "Uppercase text",
    serde_json::json!({"type": "object"}),
    |args| Ok(args.to_string().to_uppercase()));
// attach via QueryOptions { extra_tools: vec![Arc::new(shout)], .. }
```

## Permissions (Claude-Code-style gate)

```rust
// mode + ordered rules; deny wins, then allow, then the mode default
opts.permission_mode = Some(sdk::PermissionMode::Plan); // read-only planning
opts.permission_rules = vec!["deny:cli(rm -rf)".into(), "allow:read".into()];
```

Modes: `Default` (balanced) · `Plan` (blocks mutating tools) ·
`AcceptEdits` (auto-approve writes) · `Bypass` (allow all, denies still win).
