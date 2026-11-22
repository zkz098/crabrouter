#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGE_PATH="${1:-$(find "$REPO_ROOT/image/mkosi.output" -maxdepth 1 -name 'crabrouter*.raw' ! -name '*.root-*' ! -name '*.esp.*' ! -name '*.var.*' | head -n 1 || true)}"
IMAGE_PATH="${IMAGE_PATH:-$REPO_ROOT/image/mkosi.output/crabrouter.raw}"

OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
OVMF_VARS="${OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}"
if [[ ! -f "$OVMF_CODE" ]]; then
  OVMF_CODE="/usr/share/OVMF/OVMF_CODE.fd"
fi
if [[ ! -f "$OVMF_VARS" ]]; then
  OVMF_VARS="/usr/share/OVMF/OVMF_VARS.fd"
fi

HTTP_PORT="${CRABROUTER_QEMU_HTTP_PORT:-18080}"
HTTPS_PORT="${CRABROUTER_QEMU_HTTPS_PORT:-18443}"
SSH_PORT="${CRABROUTER_QEMU_SSH_PORT:-12222}"
BOOT_TIMEOUT_SECS="${CRABROUTER_QEMU_TIMEOUT:-120}"

for bin in qemu-system-x86_64 qemu-img curl; do
  if ! command -v "$bin" >/dev/null 2>&1; then
    echo "error: required tool '$bin' is not installed" >&2
    exit 1
  fi
done

if [[ ! -f "$IMAGE_PATH" ]]; then
  echo "error: disk image not found at $IMAGE_PATH (run image/scripts/build-image.sh first)" >&2
  exit 1
fi
if [[ ! -f "$OVMF_CODE" ]]; then
  echo "error: OVMF_CODE not found at $OVMF_CODE" >&2
  exit 1
fi

WORK_DIR="$(mktemp -d /tmp/crabrouter-qemu-e2e.XXXXXX)"
QEMU_PID=""

cleanup() {
  local rc=$?
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    if [[ -S "$WORK_DIR/monitor.sock" ]] && command -v socat >/dev/null 2>&1; then
      echo "quit" | socat - UNIX-CONNECT:"$WORK_DIR/monitor.sock" >/dev/null 2>&1 || true
      sleep 1
    fi
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
  if [[ $rc -ne 0 ]] && [[ -f "$WORK_DIR/serial.log" ]]; then
    echo "=== QEMU serial log (last 80 lines) ===" >&2
    tail -n 80 "$WORK_DIR/serial.log" >&2 || true
  fi
  rm -rf "$WORK_DIR"
  exit "$rc"
}
trap cleanup EXIT

echo "==> Preparing CoW snapshot from $IMAGE_PATH"
qemu-img create -q -f qcow2 -F raw -b "$IMAGE_PATH" "$WORK_DIR/disk.qcow2"
if [[ -f "$OVMF_VARS" ]]; then
  cp "$OVMF_VARS" "$WORK_DIR/ovmf_vars.fd"
  OVMF_VARS_DRIVE=(-drive "if=pflash,format=raw,file=$WORK_DIR/ovmf_vars.fd")
else
  OVMF_VARS_DRIVE=()
fi

KVM_ARGS=()
if [[ -w /dev/kvm ]]; then
  KVM_ARGS=(-enable-kvm -cpu host)
  echo "==> KVM acceleration enabled"
else
  echo "==> /dev/kvm not writable; using TCG emulation"
fi

echo "==> Booting CrabRouter UEFI VM (HTTP=127.0.0.1:${HTTP_PORT})"
qemu-system-x86_64 \
  "${KVM_ARGS[@]}" \
  -m 2048 \
  -smp 2 \
  -nographic \
  -serial "file:$WORK_DIR/serial.log" \
  -monitor "unix:$WORK_DIR/monitor.sock,server,nowait" \
  -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE" \
  "${OVMF_VARS_DRIVE[@]}" \
  -drive "file=$WORK_DIR/disk.qcow2,format=qcow2,if=virtio" \
  -netdev user,id=wan0 \
  -device virtio-net-pci,netdev=wan0 \
  -netdev "user,id=lan0,net=192.168.10.0/24,hostfwd=tcp:127.0.0.1:${HTTP_PORT}-192.168.10.1:8080,hostfwd=tcp:127.0.0.1:${HTTPS_PORT}-192.168.10.1:8443,hostfwd=tcp:127.0.0.1:${SSH_PORT}-192.168.10.1:22" \
  -device virtio-net-pci,netdev=lan0 \
  </dev/null >/dev/null 2>&1 &
QEMU_PID=$!

echo "==> Waiting for http://127.0.0.1:${HTTP_PORT}/api/health (timeout ${BOOT_TIMEOUT_SECS}s)..."
DEADLINE=$((SECONDS + BOOT_TIMEOUT_SECS))
HEALTH_JSON=""
while (( SECONDS < DEADLINE )); do
  if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    echo "error: QEMU exited prematurely" >&2
    exit 1
  fi
  if HEALTH_JSON="$(curl -fsS --max-time 2 "http://127.0.0.1:${HTTP_PORT}/api/health" 2>/dev/null)"; then
    if [[ "$HEALTH_JSON" == *'"status":"ok"'* ]]; then
      break
    fi
  fi
  sleep 2
done

if [[ "$HEALTH_JSON" != *'"status":"ok"'* ]]; then
  echo "error: timed out waiting for crabrouterd /api/health" >&2
  exit 1
fi
echo "==> /api/health OK: $HEALTH_JSON"

echo "==> Checking WebUI SPA, /api/setup/status, /api/ota/status, /api/interfaces"
SPA_CODE="$(curl -sS -o /dev/null -w '%{http_code}' "http://127.0.0.1:${HTTP_PORT}/")"
if [[ "$SPA_CODE" != "200" ]]; then
  echo "error: expected 200 for /, got $SPA_CODE" >&2
  exit 1
fi

SETUP_JSON="$(curl -fsS "http://127.0.0.1:${HTTP_PORT}/api/setup/status")"
OTA_JSON="$(curl -fsS "http://127.0.0.1:${HTTP_PORT}/api/ota/status")"
IFACES_JSON="$(curl -fsS "http://127.0.0.1:${HTTP_PORT}/api/interfaces")"

echo "==> /api/setup/status: $SETUP_JSON"
echo "==> /api/ota/status:   $OTA_JSON"
echo "==> /api/interfaces:   $(echo "$IFACES_JSON" | head -c 180)..."
echo "==> QEMU/KVM E2E boot & API verification PASSED!"
