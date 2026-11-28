#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGE_DIR="$REPO_ROOT/image"
EXTRA_BIN="$IMAGE_DIR/mkosi.extra/usr/local/bin"
REPART_DST="$IMAGE_DIR/mkosi.extra/usr/share/crabrouter/repart.d"
OUT_DIR="$IMAGE_DIR/mkosi.output"
VERSION="${CRABROUTER_VERSION:-0.1.0}"
KERNEL_VARIANT="${CRABROUTER_KERNEL:-lts}"
DEBIAN_MIRROR="${CRABROUTER_DEBIAN_MIRROR:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --hwe)
      KERNEL_VARIANT="hwe"
      shift
      ;;
    --lts)
      KERNEL_VARIANT="lts"
      shift
      ;;
    --kernel=*)
      KERNEL_VARIANT="${1#*=}"
      shift
      ;;
    --kernel)
      KERNEL_VARIANT="${2:-}"
      shift 2
      ;;
    --mirror=*)
      DEBIAN_MIRROR="${1#*=}"
      shift
      ;;
    --mirror)
      DEBIAN_MIRROR="${2:-}"
      shift 2
      ;;
    *)
      echo "error: unknown argument '$1' (expected --lts, --hwe, --kernel <lts|hwe>, or --mirror <url>)" >&2
      exit 1
      ;;
  esac
done

case "$KERNEL_VARIANT" in
  lts)
    MKOSI_PROFILE_ARGS=()
    ;;
  hwe)
    MKOSI_PROFILE_ARGS=(--profile=hwe)
    ;;
  *)
    echo "error: unsupported CRABROUTER_KERNEL='$KERNEL_VARIANT' (expected 'lts' or 'hwe')" >&2
    exit 1
    ;;
esac

MKOSI_MIRROR_ARGS=()
BACKPORTS_SOURCES="$IMAGE_DIR/mkosi.profiles/hwe/mkosi.sandbox/etc/apt/sources.list.d/trixie-backports.sources"
BACKPORTS_BACKUP=""
ISO_STAGE=""

cleanup() {
  if [[ -n "$BACKPORTS_BACKUP" ]] && [[ -f "$BACKPORTS_BACKUP" ]]; then
    mv -f "$BACKPORTS_BACKUP" "$BACKPORTS_SOURCES"
  fi
  if [[ -n "$ISO_STAGE" ]] && [[ -d "$ISO_STAGE" ]]; then
    rm -rf "$ISO_STAGE"
  fi
}
trap cleanup EXIT

if [[ -n "$DEBIAN_MIRROR" ]]; then
  DEBIAN_MIRROR="${DEBIAN_MIRROR%/}"
  DEBIAN_MIRROR="${DEBIAN_MIRROR%/debian}"
  MKOSI_MIRROR_ARGS=(--mirror="$DEBIAN_MIRROR")
  if [[ "$KERNEL_VARIANT" == "hwe" ]] && [[ -f "$BACKPORTS_SOURCES" ]]; then
    BACKPORTS_BACKUP="$(mktemp)"
    cp "$BACKPORTS_SOURCES" "$BACKPORTS_BACKUP"
    sed -i "s|^URIs: .*|URIs: ${DEBIAN_MIRROR}/debian|" "$BACKPORTS_SOURCES"
  fi
fi

echo "==> 1. Building WebUI & release Rust binaries"
(cd "$REPO_ROOT/webui" && pnpm install --frozen-lockfile && pnpm build)
(cd "$REPO_ROOT" && cargo build --release -p crabrouter-daemon -p crabrouter-cli)

mkdir -p "$EXTRA_BIN" "$REPART_DST"
install -m 0755 "$REPO_ROOT/target/release/crabrouter-daemon" "$EXTRA_BIN/crabrouterd"
install -m 0755 "$REPO_ROOT/target/release/crabrouter-cli" "$EXTRA_BIN/crabctl"
cp "$IMAGE_DIR"/mkosi.repart/*.conf "$REPART_DST/"
sed -i -E '/^(CopyFiles|ExcludeFiles)=|^Format=squashfs$|^Compression(Level)?=/d' "$REPART_DST/"*.conf

if [[ "${SKIP_CRABDNS:-0}" != "1" ]]; then
  bash "$IMAGE_DIR/scripts/bundle-crabdns.sh"
else
  echo "info: SKIP_CRABDNS=1; skipping crabdns binary staging"
fi

if [[ "${SKIP_UDPXY:-0}" != "1" ]]; then
  bash "$IMAGE_DIR/scripts/bundle-udpxy.sh"
else
  echo "info: SKIP_UDPXY=1; skipping udpxy binary staging"
fi

if ! command -v mkosi >/dev/null 2>&1; then
  echo "error: mkosi is not installed. Install mkosi, systemd-container, systemd-ukify, squashfs-tools, and dosfstools." >&2
  exit 1
fi

echo "==> 2. Building Debian 13 (Trixie) UEFI appliance image (kernel=$KERNEL_VARIANT, mkosi $(mkosi --version))"
(cd "$IMAGE_DIR" && mkosi "${MKOSI_PROFILE_ARGS[@]}" "${MKOSI_MIRROR_ARGS[@]}" --image-version="$VERSION" --force)

RAW_IMG="$(find "$OUT_DIR" -maxdepth 1 -name 'crabrouter*.raw' ! -name '*.root-*' | head -n 1 || true)"
if [[ -n "$RAW_IMG" ]] && [[ -f "$RAW_IMG" ]] && command -v qemu-img >/dev/null 2>&1; then
  echo "==> 3. Converting raw disk image ($RAW_IMG) to Hyper-V VHDX"
  qemu-img convert -f raw -O vhdx -o subformat=dynamic \
    "$RAW_IMG" "$OUT_DIR/crabrouter.vhdx"
else
  echo "warn: qemu-img not found or raw image missing in $OUT_DIR; skipping Hyper-V .vhdx conversion"
fi

if [[ "${BUILD_ISO:-1}" == "1" ]] && command -v xorriso >/dev/null 2>&1 && command -v mcopy >/dev/null 2>&1 && command -v mkfs.vfat >/dev/null 2>&1; then
  echo "==> 4. Building UEFI Live/Installer ISO for Hyper-V / bare-metal USB (kernel=$KERNEL_VARIANT)"
  (cd "$IMAGE_DIR" && mkosi "${MKOSI_PROFILE_ARGS[@]}" "${MKOSI_MIRROR_ARGS[@]}" --format=uki --split-artifacts=no --output=crabrouter-live --image-version="$VERSION" --force)
  LIVE_EFI="$(find "$OUT_DIR" -maxdepth 1 -name 'crabrouter-live*.efi' | head -n 1 || true)"
  if [[ -n "$LIVE_EFI" ]]; then
    ISO_STAGE="$(mktemp -d)"
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
    for uki_efi in "$OUT_DIR"/crabrouter*.efi; do
      [[ -f "$uki_efi" ]] || continue
      [[ "$(basename "$uki_efi")" == crabrouter-live* ]] && continue
      cp "$uki_efi" "$ISO_STAGE/iso/crabrouter/"
    done
    printf '%s\n' "$VERSION" > "$ISO_STAGE/iso/crabrouter/VERSION"
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
