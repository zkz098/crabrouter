#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGE_DIR="$REPO_ROOT/image"
EXTRA_BIN="$IMAGE_DIR/mkosi.extra/usr/local/bin"
REPART_DST="$IMAGE_DIR/mkosi.extra/usr/lib/repart.d"
OUT_DIR="$IMAGE_DIR/mkosi.output"
VERSION="${CRABROUTER_VERSION:-0.1.0}"

echo "==> 1. Building WebUI & release Rust binaries"
(cd "$REPO_ROOT/webui" && pnpm install --frozen-lockfile && pnpm build)
(cd "$REPO_ROOT" && cargo build --release -p crabrouter-daemon -p crabrouter-cli)

mkdir -p "$EXTRA_BIN" "$REPART_DST"
install -m 0755 "$REPO_ROOT/target/release/crabrouter-daemon" "$EXTRA_BIN/crabrouterd"
install -m 0755 "$REPO_ROOT/target/release/crabrouter-cli" "$EXTRA_BIN/crabctl"
cp "$IMAGE_DIR"/mkosi.repart/*.conf "$REPART_DST/"

if [[ -d "${CRABDNS_DIR:-$HOME/projects/crabdns}" ]]; then
  bash "$IMAGE_DIR/scripts/bundle-crabdns.sh"
else
  echo "warn: crabdns repo not found; skipping crabdns binary staging"
fi

if ! command -v mkosi >/dev/null 2>&1; then
  echo "error: mkosi is not installed. Install mkosi, systemd-container, systemd-ukify, squashfs-tools, and dosfstools." >&2
  exit 1
fi

echo "==> 2. Building Debian 13 (Trixie) UEFI appliance image with mkosi"
(cd "$IMAGE_DIR" && mkosi --image-version="$VERSION" --force)

RAW_IMG="$OUT_DIR/crabrouter.raw"
if [[ -f "$RAW_IMG" ]] && command -v qemu-img >/dev/null 2>&1; then
  echo "==> 3. Converting raw disk image to Hyper-V VHDX"
  qemu-img convert -f raw -O vhdx -o subformat=dynamic \
    "$RAW_IMG" "$OUT_DIR/crabrouter.vhdx"
else
  echo "warn: qemu-img not found or $RAW_IMG missing; skipping Hyper-V .vhdx conversion"
fi

if [[ "${BUILD_ISO:-1}" == "1" ]] && command -v xorriso >/dev/null 2>&1 && command -v mcopy >/dev/null 2>&1 && command -v mkfs.vfat >/dev/null 2>&1; then
  echo "==> 4. Building UEFI Live/Installer ISO for Hyper-V / bare-metal USB"
  (cd "$IMAGE_DIR" && mkosi --format=uki --output=crabrouter-live --image-version="$VERSION" --force)
  LIVE_EFI="$(find "$OUT_DIR" -maxdepth 1 -name 'crabrouter-live*.efi' | head -n 1 || true)"
  if [[ -n "$LIVE_EFI" ]]; then
    ISO_STAGE="$(mktemp -d)"
    trap 'rm -rf "$ISO_STAGE"' EXIT
    mkdir -p "$ISO_STAGE/iso/crabrouter"
    ESP_IMG="$ISO_STAGE/efiboot.img"
    ESP_SIZE_MB=$(( ($(stat -c%s "$LIVE_EFI") / 1048576) + 32 ))
    truncate -s "${ESP_SIZE_MB}M" "$ESP_IMG"
    mkfs.vfat -F 32 "$ESP_IMG" >/dev/null
    mmd -i "$ESP_IMG" ::/EFI ::/EFI/BOOT
    mcopy -i "$ESP_IMG" "$LIVE_EFI" ::/EFI/BOOT/BOOTX64.EFI
    cp "$ESP_IMG" "$ISO_STAGE/iso/efiboot.img"
    for root_part in "$OUT_DIR"/crabrouter*.root-x86-64.raw; do
      [[ -f "$root_part" ]] && cp "$root_part" "$ISO_STAGE/iso/crabrouter/"
    done
    xorriso -as mkisofs \
      -R -J -V "CRABROUTER_LIVE" \
      -eltorito-alt-boot -e efiboot.img -no-emul-boot \
      -isohybrid-gpt-basdat \
      -o "$OUT_DIR/crabrouter-live.iso" \
      "$ISO_STAGE/iso"
  fi
fi

if [[ -d "$OUT_DIR" ]]; then
  (cd "$OUT_DIR" && sha256sum * > SHA256SUMS 2>/dev/null || true)
  echo "==> Image build complete in $OUT_DIR:"
  ls -lh "$OUT_DIR"
fi
