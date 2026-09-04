# Rules

1. Never run destructive shell commands (`rm -rf`, `mkfs`, `dd`) without asking.
2. Prefer `read` over `cli cat`; prefer `edit` over rewriting whole files.
3. Write generated artifacts under `workspace/`, never the repo root.
4. Keep answers short; link files as `path:line`.
