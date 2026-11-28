#!/usr/bin/env bash
set -euo pipefail

CRABDNS_DIR="${CRABDNS_DIR:-$HOME/projects/crabdns}"
CRABDNS_REPO_URL="${CRABDNS_REPO_URL:-https://github.com/zkz098/crabdns.git}"
CRABDNS_GIT_REF="${CRABDNS_GIT_REF:-main}"
CRABDNS_BIN_PATH="${CRABDNS_BIN_PATH:-}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
EXTRA_BIN="$REPO_ROOT/image/mkosi.extra/usr/local/bin"

mkdir -p "$EXTRA_BIN"

if [[ -n "$CRABDNS_BIN_PATH" ]]; then
  if [[ ! -x "$CRABDNS_BIN_PATH" ]]; then
    echo "error: CRABDNS_BIN_PATH='$CRABDNS_BIN_PATH' is not executable" >&2
    exit 1
  fi
  install -m 0755 "$CRABDNS_BIN_PATH" "$EXTRA_BIN/crabdns"
  echo "==> Installed prebuilt crabdns binary from $CRABDNS_BIN_PATH"
  exit 0
fi

TMP_CLONE=""
cleanup() {
  if [[ -n "$TMP_CLONE" ]] && [[ -d "$TMP_CLONE" ]]; then
    rm -rf "$TMP_CLONE"
  fi
}
trap cleanup EXIT

if [[ ! -d "$CRABDNS_DIR" ]]; then
  if ! command -v git >/dev/null 2>&1; then
    echo "error: crabdns directory '$CRABDNS_DIR' not found and git is unavailable" >&2
    exit 1
  fi
  TMP_CLONE="$(mktemp -d /tmp/crabdns-src.XXXXXX)"
  echo "==> Local crabdns dir not found; cloning $CRABDNS_REPO_URL ($CRABDNS_GIT_REF)..."
  git clone --depth 1 --branch "$CRABDNS_GIT_REF" "$CRABDNS_REPO_URL" "$TMP_CLONE"
  CRABDNS_DIR="$TMP_CLONE"
fi

echo "==> Building crabdns WebUI & release server from $CRABDNS_DIR"
if [[ -d "$CRABDNS_DIR/webui" ]] && command -v pnpm >/dev/null 2>&1; then
  (cd "$CRABDNS_DIR/webui" && pnpm install --frozen-lockfile && pnpm build)
fi

(cd "$CRABDNS_DIR" && cargo build --release -p crabdns-server)

CRABDNS_BIN="$CRABDNS_DIR/target/release/crabdns-server"
if [[ ! -x "$CRABDNS_BIN" ]]; then
  CRABDNS_BIN="$CRABDNS_DIR/target/release/crabdns"
fi

install -m 0755 "$CRABDNS_BIN" "$EXTRA_BIN/crabdns"
echo "==> Installed crabdns binary into $EXTRA_BIN/crabdns"

if [[ "${1:-}" == "--deb" ]] && command -v dpkg-deb >/dev/null 2>&1; then
  PKG_STAGE="$(mktemp -d)"
  trap 'rm -rf "$PKG_STAGE"; cleanup' EXIT
  mkdir -p "$PKG_STAGE/DEBIAN" "$PKG_STAGE/usr/local/bin" "$PKG_STAGE/etc/systemd/system"
  install -m 0755 "$CRABDNS_BIN" "$PKG_STAGE/usr/local/bin/crabdns"
  install -m 0644 "$REPO_ROOT/image/mkosi.extra/etc/systemd/system/crabdns.service" \
    "$PKG_STAGE/etc/systemd/system/crabdns.service"
  cat >"$PKG_STAGE/DEBIAN/control" <<EOF
Package: crabdns
Version: 0.1.0
Section: net
Priority: optional
Architecture: amd64
Maintainer: CrabRouter Maintainers <zkz098@users.noreply.github.com>
Description: High-performance Rust DNS server & ad-blocker for CrabRouter
EOF
  mkdir -p "$REPO_ROOT/image/mkosi.packages"
  dpkg-deb --build "$PKG_STAGE" "$REPO_ROOT/image/mkosi.packages/crabdns_0.1.0_amd64.deb"
  echo "==> Built .deb package in $REPO_ROOT/image/mkosi.packages/crabdns_0.1.0_amd64.deb"
fi
