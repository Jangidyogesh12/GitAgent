#!/usr/bin/env bash
# ============================================================================
# GitAgent (Rust) installer — install.sh
# ----------------------------------------------------------------------------
# WHAT THIS DOES (mirrors the spirit of the TS install.sh, adapted to Rust):
#   1. Checks prerequisites (cargo/rustc, git).
#   2. Builds + installs the `gitagent` binary (release profile).
#   3. Interactive setup: project dir + model backend choice —
#      (1) Lyzr Studio, (2) Anthropic, (3) OpenAI, (4) Ollama-local, (5) custom.
#   4. Writes the project `.env` + scaffolds the agent on first `gitagent` run.
#
# USAGE:
#   ./installer/install.sh                 # interactive
#   GITAGENT_PREFIX=~/.local installer/install.sh   # custom install prefix
#   GITAGENT_NO_SETUP=1 ./installer/install.sh      # binary only, no wizard
# ============================================================================
set -euo pipefail

PREFIX="${GITAGENT_PREFIX:-$HOME/.cargo}"
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

info()  { printf '\033[1;34m[gitagent]\033[0m %s\n' "$*"; }
warn()  { printf '\033[1;33m[gitagent]\033[0m %s\n' "$*"; }
fatal() { printf '\033[1;31m[gitagent]\033[0m %s\n' "$*"; exit 1; }

echo "=============================================="
echo "  GitAgent (Rust) — installer"
echo "=============================================="

# --- 1. prerequisites -------------------------------------------------------
command -v cargo >/dev/null 2>&1 || fatal "cargo not found — install Rust from https://rustup.rs first"
command -v git   >/dev/null 2>&1 || fatal "git not found — install git first"
rustc --version
info "repo: $REPO_DIR"

# --- 2. build + install ------------------------------------------------------
info "building release binary (this takes a few minutes on first run)..."
cargo install --locked --path "$REPO_DIR/cli" --root "$PREFIX"

BIN="$PREFIX/bin/gitagent"
[ -x "$BIN" ] || fatal "install failed — $BIN not found"
info "installed: $BIN"
"$BIN" --version || "$BIN" --help | head -3

# --- 3. interactive setup (skipped with GITAGENT_NO_SETUP=1) -----------------
if [ "${GITAGENT_NO_SETUP:-0}" = "1" ]; then
  info "setup wizard skipped (GITAGENT_NO_SETUP=1)"
  exit 0
fi

echo
echo "--- Agent setup ------------------------------------------------------"
read -r -p "Project/agent directory [./my-agent]: " PROJECT_DIR
PROJECT_DIR="${PROJECT_DIR:-./my-agent}"
mkdir -p "$PROJECT_DIR"

echo "Choose your model backend:"
echo "  1) Lyzr Studio   (LYZR_API_KEY + lyzr:<agent-id>@base model string)"
echo "  2) Anthropic     (ANTHROPIC_API_KEY)"
echo "  3) OpenAI        (OPENAI_API_KEY)"
echo "  4) Ollama local  (no key, http://localhost:11434/v1)"
echo "  5) OpenCode Zen  (OPENCODE_API_KEY + opencode:<model-id>)"
echo "  6) Custom OpenAI-compatible endpoint"
read -r -p "Backend [1]: " BACKEND
BACKEND="${BACKEND:-1}"

MODEL=""
ENV_FILE="$PROJECT_DIR/.env"
touch "$ENV_FILE"

case "$BACKEND" in
  1)
    read -r -p "LYZR_API_KEY: " LYZR_API_KEY
    read -r -p "Lyzr agent id (for lyzr:<id>@base): " LYZR_AGENT_ID
    MODEL="lyzr:${LYZR_AGENT_ID}@https://agent-prod.studio.lyzr.ai/v4"
    {
      echo "LYZR_API_KEY=$LYZR_API_KEY"
      echo "GITAGENT_MODEL_BASE_URL=https://agent-prod.studio.lyzr.ai/v4"
    } >> "$ENV_FILE"
    ;;
  2)
    read -r -p "ANTHROPIC_API_KEY: " ANTHROPIC_API_KEY
    echo "ANTHROPIC_API_KEY=$ANTHROPIC_API_KEY" >> "$ENV_FILE"
    MODEL="anthropic:claude-sonnet-4-6"
    ;;
  3)
    read -r -p "OPENAI_API_KEY: " OPENAI_API_KEY
    echo "OPENAI_API_KEY=$OPENAI_API_KEY" >> "$ENV_FILE"
    MODEL="openai:gpt-4o-mini"
    ;;
  4)
    MODEL="ollama:gemma3:4b"
    info "Ollama selected — make sure 'ollama serve' is running."
    ;;
  5)
    read -r -p "OPENCODE_API_KEY (from opencode.ai/auth): " OPENCODE_API_KEY
    read -r -p "Zen model id [kimi-k2.6]: " ZEN_MODEL
    ZEN_MODEL="${ZEN_MODEL:-kimi-k2.6}"
    echo "OPENCODE_API_KEY=$OPENCODE_API_KEY" >> "$ENV_FILE"
    MODEL="opencode:$ZEN_MODEL"
    ;;
  6)
    read -r -p "Base URL (e.g. http://localhost:8090/v1): " BASE_URL
    read -r -p "Model id at that endpoint: " MODEL_ID
    read -r -p "API key env value (stored as OPENAI_API_KEY, may be dummy): " ANY_KEY
    echo "OPENAI_API_KEY=$ANY_KEY" >> "$ENV_FILE"
    echo "GITAGENT_MODEL_BASE_URL=$BASE_URL" >> "$ENV_FILE"
    MODEL="openai:$MODEL_ID"
    ;;
  *) fatal "unknown backend: $BACKEND" ;;
esac

echo
info "project : $PROJECT_DIR"
info "model   : $MODEL"
info "env file: $ENV_FILE (keys appended — keep it secret)"
echo
info "scaffolding your agent..."
# shellcheck disable=SC1090
set -a; source "$ENV_FILE"; set +a
"$BIN" --dir "$PROJECT_DIR" --model "$MODEL" --prompt "Reply with one line confirming you are awake." || warn "first run needs a valid key/endpoint — re-run once configured."

echo
info "done. Next steps:"
echo "  export \$(grep -v '^#' \"$ENV_FILE\" | xargs)   # load keys (or restart your shell)"
echo "  gitagent --dir \"$PROJECT_DIR\"                 # interactive chat"
echo "  gitagent --dir \"$PROJECT_DIR\" \"do something\"  # one-shot"
