#!/usr/bin/env bash
# ============================================================================
# gitagent remote installer — install a prebuilt binary with curl.
# ----------------------------------------------------------------------------
# USAGE (no git clone, no Rust toolchain needed):
#   curl -fsSL https://raw.githubusercontent.com/Jangidyogesh12/GitAgent/main/installer/install-remote.sh | bash
#
# Knobs (all optional env vars / flags):
#   GITAGENT_REPO=owner/name   which fork's releases to use (default below)
#   GITAGENT_VERSION=v0.2.0    pin a version (default: latest release)
#   GITAGENT_BINDIR=~/.local/bin  where to put the binary (default: ~/.cargo/bin)
#   bash install-remote.sh --from-source   skip binaries: cargo install --git
#
# WHAT IT DOES, step by step (good first script to read to learn bash):
#   1. detect_os_arch() — uname -s/-m → Rust target triple (case statement).
#   2. Pick the matching release asset, or fail with a helpful message.
#   3. curl the tarball + SHA256SUMS.txt into a temp dir (trap cleans up).
#   4. Verify the checksum (shasum on macOS, sha256sum on Linux).
#   5. Extract, install to BINDIR (created if missing), verify --version.
# ============================================================================
set -euo pipefail

REPO="${GITAGENT_REPO:-Jangidyogesh12/GitAgent}"
VERSION="${GITAGENT_VERSION:-latest}"   # "latest" or a tag like "v0.2.0"
BINDIR="${GITAGENT_BINDIR:-$HOME/.cargo/bin}"

info()  { printf '\033[1;34m[gitagent]\033[0m %s\n' "$*"; }
fatal() { printf '\033[1;31m[gitagent]\033[0m %s\n' "$*"; exit 1; }

# --- 0. source-build escape hatch -------------------------------------------
if [ "${1:-}" = "--from-source" ]; then
  command -v cargo >/dev/null 2>&1 || fatal "cargo not found — install Rust from https://rustup.rs first"
  REF="$VERSION"; [ "$REF" = "latest" ] && REF="main"
  info "installing from source ($REPO @ $REF)…"
  cargo install --locked --git "https://github.com/$REPO.git" --rev "$REF" --root "${PREFIX:-$HOME/.cargo}" --bin gitagent
  info "done. Run: gitagent --help"
  exit 0
fi

# --- 1. detect platform → Rust target triple ---------------------------------
detect_os_arch() {
  local os arch
  os="$(uname -s)"; arch="$(uname -m)"
  case "$os" in
    Linux) case "$arch" in
      x86_64)  echo "x86_64-unknown-linux-gnu" ;;
      aarch64) echo "aarch64-unknown-linux-gnu" ;; # no prebuilt asset (see below)
      *) fatal "unsupported Linux arch: $arch (try --from-source)" ;;
    esac ;;
    Darwin) case "$arch" in
      arm64)  echo "aarch64-apple-darwin" ;;
      x86_64) echo "x86_64-apple-darwin" ;; # no prebuilt asset (see below)
      *) fatal "unsupported macOS arch: $arch" ;;
    esac ;;
    MINGW*|MSYS*|CYGWIN*) fatal "Windows: download gitagent-*-x86_64-pc-windows-msvc.zip from https://github.com/$REPO/releases and unzip it into PATH" ;;
    *) fatal "unsupported OS: $os" ;;
  esac
}

TARGET="$(detect_os_arch)"
case "$TARGET" in
  x86_64-unknown-linux-gnu|aarch64-apple-darwin) ;; # built by release.yml
  *) fatal "no prebuilt binary for $TARGET yet — rerun with --from-source, or ask the maintainer to add it to .github/workflows/release.yml" ;;
esac

# --- 2. resolve download URLs -------------------------------------------------
if [ "$VERSION" = "latest" ]; then
  BASE="https://github.com/$REPO/releases/latest/download"
  VER_LABEL="latest"
else
  BASE="https://github.com/$REPO/releases/download/$VERSION"
  VER_LABEL="$VERSION"
fi
info "installing gitagent $VER_LABEL for $TARGET from ${REPO}…"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT # always clean the temp dir, even on failure
cd "$WORK"

# --- 3. download tarball + checksums ------------------------------------------
# NOTE: release.yml names assets gitagent-<ver>-<target>.tar.gz, but the
# "latest" URL needs the exact filename — fetch it from the checksums file,
# which always exists next to the assets.
curl -fsSL -o SHA256SUMS.txt "$BASE/SHA256SUMS.txt" \
  || fatal "release $VER_LABEL not found at $BASE (has vX.Y.Z been tagged? see release.yml)"
# Normalise line endings: a CRLF checksum file breaks the grep below.
tr -d '\r' < SHA256SUMS.txt > SHA256SUMS.tmp && mv SHA256SUMS.tmp SHA256SUMS.txt

# Pick the one asset line matching our target triple.
# NOTE: `|| true` is load-bearing — without it, `set -o pipefail` + `set -e`
# aborts the script right here on no-match, before the friendly fatal below.
ASSET="$(grep -E "gitagent-.*-${TARGET}\\.tar\\.gz$" SHA256SUMS.txt | awk '{print $2}' | head -n 1 || true)"
[ -n "$ASSET" ] || fatal "no $TARGET asset in $VER_LABEL (available: $(awk '{print $2}' SHA256SUMS.txt | tr '\n' ' '))"

curl -fsSL -o "$ASSET" "$BASE/$ASSET" || fatal "download failed: $BASE/$ASSET"

# --- 4. verify checksum (shasum on macOS, sha256sum on Linux) ------------------
if command -v sha256sum >/dev/null 2>&1; then
  grep -E "${ASSET}\$" SHA256SUMS.txt | sha256sum -c - \
    || fatal "checksum mismatch for $ASSET — delete it and retry (possible MITM/corruption)"
elif command -v shasum >/dev/null 2>&1; then
  grep -E "${ASSET}\$" SHA256SUMS.txt | shasum -a 256 -c - \
    || fatal "checksum mismatch for $ASSET — delete it and retry (possible MITM/corruption)"
else
  fatal "need sha256sum or shasum to verify the download"
fi
info "checksum OK."

# --- 5. extract + install ------------------------------------------------------
tar xzf "$ASSET"
BIN="$(find . -name gitagent -type f | head -n 1)"
[ -n "$BIN" ] || fatal "tarball $ASSET contains no gitagent binary"
mkdir -p "$BINDIR"
install -m 755 "$BIN" "$BINDIR/gitagent" 2>/dev/null || cp "$BIN" "$BINDIR/gitagent"
info "installed: $BINDIR/gitagent"
"$BINDIR/gitagent" --version || fatal "installed binary won't run"

case ":$PATH:" in
  *":$BINDIR:"*) ;;
  *) info "NOTE: $BINDIR is not on your PATH — add: export PATH=\"$BINDIR:\$PATH\"" ;;
esac
info "done. Next: gitagent --dir ~/my-project \"Explain this project\""
