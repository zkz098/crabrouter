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
CONTAINERS_JSON="$(curl -fsS "http://127.0.0.1:${HTTP_PORT}/api/containers/info")"

echo "==> /api/setup/status:    $SETUP_JSON"
echo "==> /api/ota/status:      $OTA_JSON"
echo "==> /api/interfaces:      $(echo "$IFACES_JSON" | head -c 180)..."
echo "==> /api/containers/info: $CONTAINERS_JSON"
if [[ "$CONTAINERS_JSON" != *'"runtime":'* ]]; then
  echo "error: unexpected /api/containers/info payload: $CONTAINERS_JSON" >&2
  exit 1
fi

if command -v python3 >/dev/null 2>&1; then
  echo "==> Enabling containers in RouterConfig (commit + confirm) and testing live Podman socket..."
  CFG_CUR="$(curl -fsS "http://127.0.0.1:${HTTP_PORT}/api/config")"
  CFG_NEXT="$(python3 -c 'import json,sys; c=json.loads(sys.argv[1]); c["containers"]["enabled"]=True; print(json.dumps(c))' "$CFG_CUR")"
  COMMIT_OUT="$WORK_DIR/commit.json"
  COMMIT_CODE="$(curl -sS -o "$COMMIT_OUT" -w '%{http_code}' -X POST -H 'Content-Type: application/json' -d "$CFG_NEXT" "http://127.0.0.1:${HTTP_PORT}/api/config/commit")"
  if [[ "$COMMIT_CODE" != "200" ]]; then
    echo "error: /api/config/commit failed (HTTP $COMMIT_CODE): $(cat "$COMMIT_OUT")" >&2
    exit 1
  fi
  CONFIRM_CODE="$(curl -sS -o /dev/null -w '%{http_code}' -X POST "http://127.0.0.1:${HTTP_PORT}/api/config/confirm")"
  if [[ "$CONFIRM_CODE" != "200" ]]; then
    echo "error: /api/config/confirm failed (HTTP $CONFIRM_CODE)" >&2
    exit 1
  fi

  PODMAN_READY=0
  PODMAN_INFO=""
  for _ in $(seq 1 15); do
    PODMAN_INFO="$(curl -fsS --max-time 20 "http://127.0.0.1:${HTTP_PORT}/api/containers/info" || true)"
    if [[ "$PODMAN_INFO" == *'"available":true'* ]]; then
      PODMAN_READY=1
      break
    fi
    sleep 2
  done
  if [[ "$PODMAN_READY" != "1" ]]; then
    echo "error: Podman socket did not become available within 30s: $PODMAN_INFO" >&2
    exit 1
  fi
  echo "==> Live Podman runtime info: $PODMAN_INFO"

  for ep in "/api/containers" "/api/containers/images" "/api/containers/networks" "/api/containers/volumes"; do
    EP_CODE="$(curl -sS --max-time 20 -o "$WORK_DIR/ep.json" -w '%{http_code}' "http://127.0.0.1:${HTTP_PORT}${ep}")"
    if [[ "$EP_CODE" != "200" ]]; then
      echo "error: GET $ep returned HTTP $EP_CODE: $(cat "$WORK_DIR/ep.json")" >&2
      exit 1
    fi
  done

  NET_CREATE_CODE="$(curl -sS --max-time 20 -o "$WORK_DIR/net_create.json" -w '%{http_code}' -X POST -H 'Content-Type: application/json' \
    -d '{"name":"e2e-net","driver":"bridge","subnet":"10.89.0.0/24","gateway":"10.89.0.1","internal":false}' \
    "http://127.0.0.1:${HTTP_PORT}/api/containers/networks")"
  if [[ "$NET_CREATE_CODE" != "200" ]]; then
    echo "error: POST /api/containers/networks failed (HTTP $NET_CREATE_CODE): $(cat "$WORK_DIR/net_create.json")" >&2
    exit 1
  fi
  NETS_JSON="$(curl -fsS --max-time 20 "http://127.0.0.1:${HTTP_PORT}/api/containers/networks")"
  if [[ "$NETS_JSON" != *'"e2e-net"'* ]]; then
    echo "error: created network e2e-net not listed: $NETS_JSON" >&2
    exit 1
  fi
  NET_DEL_CODE="$(curl -sS --max-time 20 -o "$WORK_DIR/net_del.json" -w '%{http_code}' -X DELETE "http://127.0.0.1:${HTTP_PORT}/api/containers/networks/e2e-net")"
  if [[ "$NET_DEL_CODE" != "200" ]]; then
    echo "error: DELETE /api/containers/networks/e2e-net failed (HTTP $NET_DEL_CODE): $(cat "$WORK_DIR/net_del.json")" >&2
    exit 1
  fi

  BAD_MOUNT_CODE="$(curl -sS --max-time 20 -o "$WORK_DIR/bad_mount.json" -w '%{http_code}' -X POST -H 'Content-Type: application/json' \
    -d '{"name":"evil","image":"alpine:latest","mounts":[{"source":"/etc/shadow","target":"/mnt/shadow","read_only":true}]}' \
    "http://127.0.0.1:${HTTP_PORT}/api/containers")"
  if [[ "$BAD_MOUNT_CODE" != "400" ]]; then
    echo "error: expected HTTP 400 for forbidden bind mount, got $BAD_MOUNT_CODE: $(cat "$WORK_DIR/bad_mount.json")" >&2
    exit 1
  fi
  echo "==> Live container network (netavark+nftables) & mount security guard verified!"
else
  echo "warn: python3 not found; skipping live container runtime E2E"
fi

if [[ "${CRABROUTER_TEST_OTA_ROLLBACK:-0}" == "1" ]] || [[ "${1:-}" == "--test-ota-rollback" ]]; then
  if command -v sfdisk >/dev/null 2>&1 && command -v mcopy >/dev/null 2>&1 && command -v mdir >/dev/null 2>&1; then
    echo "==> Testing systemd-boot Boot Assessment A/B OTA rollback..."
    if [[ -S "$WORK_DIR/monitor.sock" ]] && command -v socat >/dev/null 2>&1; then
      echo "quit" | socat - UNIX-CONNECT:"$WORK_DIR/monitor.sock" >/dev/null 2>&1 || true
      sleep 1
    fi
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
    QEMU_PID=""

    qemu-img convert -f qcow2 -O raw "$WORK_DIR/disk.qcow2" "$WORK_DIR/rollback.raw"
    ESP_START_SECTOR="$(sfdisk -d "$WORK_DIR/rollback.raw" | awk '/type=C12A7328-F81F-11D2-BA4B-00A0C93EC93B/ {for(i=1;i<=NF;i++) if($i ~ /^start=/) {gsub(/[^0-9]/,"",$i); print $i; exit}}')"
    if [[ -n "$ESP_START_SECTOR" ]]; then
      ESP_OFFSET=$((ESP_START_SECTOR * 512))
      printf "INVALID_UKI_IMAGE" > "$WORK_DIR/bad.efi"
      mcopy -o -i "$WORK_DIR/rollback.raw@@${ESP_OFFSET}" "$WORK_DIR/bad.efi" "::/EFI/Linux/crabrouter_9.9.9+1-0.efi"
      qemu-img create -q -f qcow2 -F raw -b "$WORK_DIR/rollback.raw" "$WORK_DIR/rollback.qcow2"

      qemu-system-x86_64 \
        "${KVM_ARGS[@]}" -m 2048 -smp 2 -nographic \
        -serial "file:$WORK_DIR/serial-rollback.log" \
        -monitor "unix:$WORK_DIR/monitor.sock,server,nowait" \
        -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE" \
        "${OVMF_VARS_DRIVE[@]}" \
        -drive "file=$WORK_DIR/rollback.qcow2,format=qcow2,if=virtio" \
        -netdev user,id=wan0 -device virtio-net-pci,netdev=wan0 \
        -netdev "user,id=lan0,net=192.168.10.0/24,hostfwd=tcp:127.0.0.1:${HTTP_PORT}-192.168.10.1:8080" \
        -device virtio-net-pci,netdev=lan0 \
        </dev/null >/dev/null 2>&1 &
      QEMU_PID=$!

      RB_DEADLINE=$((SECONDS + BOOT_TIMEOUT_SECS))
      RB_HEALTH=""
      while (( SECONDS < RB_DEADLINE )); do
        if RB_HEALTH="$(curl -fsS --max-time 2 "http://127.0.0.1:${HTTP_PORT}/api/health" 2>/dev/null)"; then
          if [[ "$RB_HEALTH" == *'"version":"0.1.0"'* ]]; then
            break
          fi
        fi
        sleep 2
      done
      if [[ "$RB_HEALTH" != *'"version":"0.1.0"'* ]]; then
        echo "error: A/B OTA rollback failed to recover version 0.1.0" >&2
        exit 1
      fi
      echo "==> A/B OTA rollback verified! Recovered healthy slot: $RB_HEALTH"
    fi
  else
    echo "warn: sfdisk/mtools unavailable; skipping OTA rollback injection"
  fi
fi

if [[ "${CRABROUTER_TEST_POWER_LOSS_RECOVERY:-0}" == "1" ]] || [[ "${1:-}" == "--test-power-loss" ]]; then
  if [[ -S "$WORK_DIR/monitor.sock" ]] && command -v socat >/dev/null 2>&1; then
    echo "==> Testing unconfirmed commit power-loss crash recovery..."
    CFG_BEFORE="$(curl -fsS "http://127.0.0.1:${HTTP_PORT}/api/config" || true)"
    if [[ "$CFG_BEFORE" == *'"hostname"'* ]]; then
      MOD_CFG="$(echo "$CFG_BEFORE" | sed 's/"hostname":"[^"]*"/"hostname":"unconfirmed-crash"/')"
      curl -fsS -X POST -H 'Content-Type: application/json' -d "$MOD_CFG" \
        "http://127.0.0.1:${HTTP_PORT}/api/config/commit" >/dev/null || true
      echo "system_reset" | socat - UNIX-CONNECT:"$WORK_DIR/monitor.sock" >/dev/null 2>&1 || true

      PL_DEADLINE=$((SECONDS + BOOT_TIMEOUT_SECS))
      PL_HEALTH=""
      while (( SECONDS < PL_DEADLINE )); do
        if PL_HEALTH="$(curl -fsS --max-time 2 "http://127.0.0.1:${HTTP_PORT}/api/health" 2>/dev/null)"; then
          if [[ "$PL_HEALTH" == *'"status":"ok"'* ]]; then
            break
          fi
        fi
        sleep 2
      done
      CFG_AFTER="$(curl -fsS "http://127.0.0.1:${HTTP_PORT}/api/config" || true)"
      if [[ "$CFG_AFTER" == *'"unconfirmed-crash"'* ]]; then
        echo "error: power-loss recovery failed to revert unconfirmed commit" >&2
        exit 1
      fi
      echo "==> Power-loss crash recovery verified!"
    else
      echo "warn: /api/config requires auth; skipping unauthenticated power-loss test"
    fi
  else
    echo "warn: monitor.sock/socat unavailable; skipping power-loss injection"
  fi
fi

echo "==> QEMU/KVM E2E boot & API verification PASSED!"
