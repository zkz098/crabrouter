#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
EXTRA_BIN="$REPO_ROOT/image/mkosi.extra/usr/local/bin"
UDPXY_REPO_URL="${UDPXY_REPO_URL:-https://github.com/pcherenkov/udpxy.git}"
UDPXY_GIT_REF="${UDPXY_GIT_REF:-master}"
UDPXY_BIN_PATH="${UDPXY_BIN_PATH:-}"

mkdir -p "$EXTRA_BIN"

if [[ -n "$UDPXY_BIN_PATH" ]]; then
  if [[ ! -x "$UDPXY_BIN_PATH" ]]; then
    echo "error: UDPXY_BIN_PATH='$UDPXY_BIN_PATH' is not executable" >&2
    exit 1
  fi
  install -m 0755 "$UDPXY_BIN_PATH" "$EXTRA_BIN/udpxy"
  echo "==> Installed prebuilt udpxy binary from $UDPXY_BIN_PATH"
  exit 0
fi

if [[ -x "$EXTRA_BIN/udpxy" ]] && [[ "${UDPXY_FORCE_REBUILD:-0}" != "1" ]]; then
  echo "==> Using existing staged udpxy binary at $EXTRA_BIN/udpxy"
  exit 0
fi

for tool in git make gcc; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "warn: '$tool' not found; skipping udpxy source build" >&2
    exit 0
  fi
done

TMP_DIR="$(mktemp -d /tmp/udpxy-build.XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "==> Cloning udpxy from $UDPXY_REPO_URL ($UDPXY_GIT_REF)"
if ! git clone --depth 1 --branch "$UDPXY_GIT_REF" "$UDPXY_REPO_URL" "$TMP_DIR/udpxy"; then
  echo "warn: failed to clone udpxy; skipping udpxy bundling" >&2
  exit 0
fi

echo "==> Building udpxy release binary"
if ! make -C "$TMP_DIR/udpxy/chipmunk" -j"$(nproc)" udpxy; then
  echo "warn: udpxy build failed; skipping udpxy bundling" >&2
  exit 0
fi

install -m 0755 "$TMP_DIR/udpxy/chipmunk/udpxy" "$EXTRA_BIN/udpxy"
echo "==> Installed udpxy binary into $EXTRA_BIN/udpxy"
