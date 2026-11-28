default: gate

# Install and build SolidJS WebUI assets
build-ui:
    cd webui && pnpm install && pnpm build

# Run static checks + fmt + clippy orchestrated by cargo-warden
check:
    cargo warden check

# Run full verification gate orchestrated by cargo-warden (check + nextest + doc + webui-check)
gate:
    cargo warden gate

# Run deep verification gate including machete, deny, and audit
gate-full:
    cargo warden gate --full

# Auto-fix formatting and clippy/WebUI lints via cargo-warden
fix:
    cargo warden gate --fix

# Diagnose cargo-warden environment, toolchain, and Git hooks
doctor:
    cargo warden doctor

# Build release binaries with embedded WebUI
build: build-ui
    cargo build --release --workspace

# Run crabrouterd in dry-run mode locally
dev-daemon:
    RUST_LOG=info cargo run -p crabrouter-daemon -- --dry-run

# Run WebUI Vite dev server
dev-ui:
    cd webui && pnpm dev

# Build Debian 13 (Trixie) immutable appliance image via mkosi (kernel: lts | hwe)
bundle-crabdns:
    bash image/scripts/bundle-crabdns.sh

build-image kernel="lts": build bundle-crabdns
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{kernel}}" in
        lts) (cd image && mkosi --force) ;;
        hwe) (cd image && mkosi --profile=hwe --force) ;;
        *) echo "error: unsupported kernel '{{kernel}}' (expected 'lts' or 'hwe')" >&2; exit 1 ;;
    esac

# Build full appliance artifacts (raw + Hyper-V vhdx + UEFI live iso; kernel: lts | hwe)
build-hyperv kernel="lts":
    CRABROUTER_KERNEL="{{kernel}}" bash image/scripts/build-image.sh
