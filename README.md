# CrabRouter

x86-64 soft-router operating system built on a **Debian 13 (Trixie)** immutable base with a **Rust** control plane and **SolidJS** WebUI.

## Architecture

- **Base OS**: Debian 13 (Trixie, Linux 6.12 LTS) built via `mkosi` + `systemd-repart` + `systemd-sysupdate` (A/B atomic OTA)
- **Control Plane (`crabrouterd`)**:
  - `crabrouter-model`: Declarative `RouterConfig` schema, validation, and `ts-rs` TypeScript bindings
  - `crabrouter-net`: Linux interface/telemetry reader, `nftables` atomic ruleset compiler, Netlink/PPPoE abstractions
  - `crabrouter-services`: Service config generators & lifecycle orchestration (`crabdns` on `:53`, `dnsmasq` on `:5354` for DHCP/RA & `.lan` PTR, `pppd`, `wireguard`)
  - `crabrouter-core`: Config persistence, transaction apply, and commit-confirm auto-rollback engine
  - `crabrouter-web`: `axum` REST/WS API + `rust-embed` SolidJS frontend
- **DNS / DHCP Split**:
  - **`crabdns`** listens on `:53` (client filtering, DoT/DoH, caching, query logs) and forwards local domains / private PTRs to `127.0.0.1:5354`
  - **`dnsmasq`** listens on `127.0.0.1:5354` with `cache-size=0` and handles DHCPv4/v6 leases, IPv6 RA, and `.lan` local host resolution
- **Web UI**: `SolidJS` + `UnoCSS` + `ts-rs`

## Quick Start

```bash
# 1. Build WebUI assets (required before rust-embed compilation)
cd webui && pnpm install && pnpm build && cd ..

# 2. Run tests & export TypeScript types
cargo test --workspace

# 3. Start daemon in local dry-run mode (WebUI on http://127.0.0.1:8080)
cargo run -p crabrouter-daemon -- --dry-run
```
