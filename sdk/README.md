# sdk/ — public programmatic API (crate `sdk`)

Ports `src/sdk.ts` + `src/exports.ts`, built on `engine` (from `../core`).

Modules: `types` (`SdkMessage` stream union, Builder `QueryOptions`),
`permissions` (Claude-Code-style `PermissionGate`), `fns` (`tool()`
closure tools), `query` (`query()` Facade + allow-then-deny filters),
`session` (multi-turn `Session`).

```bash
cargo run -p sdk --example demo -- ./my-agent "hello"
cargo test -p sdk
```
