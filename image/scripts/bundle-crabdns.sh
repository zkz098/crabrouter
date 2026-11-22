#!/usr/bin/env bash
set -euo pipefail

CRABDNS_DIR="${CRABDNS_DIR:-$HOME/projects/crabdns}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
EXTRA_BIN="$REPO_ROOT/image/mkosi.extra/usr/local/bin"

if [[ ! -d "$CRABDNS_DIR" ]]; then
  echo "error: crabdns repository not found at $CRABDNS_DIR" >&2
  exit 1
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

mkdir -p "$EXTRA_BIN"
install -m 0755 "$CRABDNS_BIN" "$EXTRA_BIN/crabdns"
echo "==> Installed crabdns binary into $EXTRA_BIN/crabdns"

if [[ "${1:-}" == "--deb" ]] && command -v dpkg-deb >/dev/null 2>&1; then
  PKG_STAGE="$(mktemp -d)"
  trap 'rm -rf "$PKG_STAGE"' EXIT
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
