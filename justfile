default: check

# Install and build SolidJS WebUI assets
build-ui:
    cd webui && pnpm install && pnpm build

# Typecheck & lint WebUI
check-ui:
    cd webui && pnpm typecheck && pnpm lint && pnpm fmt:check

# Run Rust tests (also exports ts-rs TypeScript bindings)
test:
    cargo test --workspace

# Run full quality gates
check: test
    cargo clippy --workspace --all-targets -- -D warnings
    cargo fmt --all -- --check

# Build release binaries with embedded WebUI
build: build-ui
    cargo build --release --workspace

# Run crabrouterd in dry-run mode locally
dev-daemon:
    RUST_LOG=info cargo run -p crabrouter-daemon -- --dry-run

# Run WebUI Vite dev server
dev-ui:
    cd webui && pnpm dev

# Build Debian 13 (Trixie) immutable appliance image via mkosi
build-image: build
    cd image && mkosi --force
