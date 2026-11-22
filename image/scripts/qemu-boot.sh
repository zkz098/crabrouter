#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGE_PATH="${1:-$REPO_ROOT/image/mkosi.output/crabrouter.raw}"
OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
if [[ ! -f "$OVMF_CODE" ]]; then
  OVMF_CODE="/usr/share/OVMF/OVMF_CODE.fd"
fi

if [[ ! -f "$IMAGE_PATH" ]]; then
  echo "error: disk image not found at $IMAGE_PATH (run image/scripts/build-image.sh first)" >&2
  exit 1
fi
if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
  echo "error: qemu-system-x86_64 is not installed" >&2
  exit 1
fi

KVM_ARGS=()
if [[ -w /dev/kvm ]]; then
  KVM_ARGS=(-enable-kvm -cpu host)
fi

exec qemu-system-x86_64 \
  "${KVM_ARGS[@]}" \
  -m 2048 \
  -smp 2 \
  -nographic \
  -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE" \
  -drive "file=$IMAGE_PATH,format=raw,if=virtio" \
  -netdev "user,id=wan0,hostfwd=tcp:127.0.0.1:8080-:8080,hostfwd=tcp:127.0.0.1:8443-:8443,hostfwd=tcp:127.0.0.1:2222-:22" \
  -device virtio-net-pci,netdev=wan0 \
  -netdev user,id=lan0 \
  -device virtio-net-pci,netdev=lan0
