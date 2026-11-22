# AGENTS.md — CrabRouter

## 1. 项目概览与架构边界

CrabRouter 是基于 **Debian 13 (Trixie, Linux 6.12 LTS)** 不可变系统（`mkosi` + `systemd-repart` + `systemd-sysupdate` A/B 原子 OTA）的 x86-64 软路由系统，控制面由 **Rust** 编写，前端管理界面基于 **SolidJS + UnoCSS + ts-rs**。

### 工作区分层架构（由 `warden.toml` `[dag.workspace]` 强制约束）

Workspace 内部严格遵循单向分层依赖（禁止跨层反向依赖，禁止任何库依赖二进制 crate）：

```
app   : crabrouter-daemon (crabrouterd 守护进程), crabrouter-cli (crabctl CLI 与刷盘安装器)
  ↓
web   : crabrouter-web (axum REST/WebSocket/PTY API + crabdns 反代 + rust-embed 前端)
  ↓
core  : crabrouter-core (配置持久化、redb 状态与鉴权存储、Commit-Confirm 自动回滚事务引擎)
  ↓
infra : crabrouter-net (rtnetlink 增量 Diff、nftables、tc CAKE/fq_codel QoS、conntrack、PPPoE/dhcpcd/IPv6-PD、硬件健康)
        crabrouter-services (zbus systemd 编排、crabdns :53 与 dnsmasq :5354 配置渲染、WireGuard 密钥与配置)
  ↓
model : crabrouter-model (声明式 RouterConfig 模型、校验器、ts-rs TypeScript 类型导出)
```

- **同 crate 模块无环约束**：每个 crate `src/` 下的顶层模块必须构成无环有向图（DAG），禁止顶层模块互相 import 形成循环依赖。

---

## 2. 质量保障与门禁工具（`cargo-warden`）— 强制规范

本仓库使用 [`cargo-warden`](https://github.com/zkz098/cargo-warden) 统一编排静态架构检查、代码质量扫描、Rust 工具链（`fmt` / `clippy` / `nextest` / `doc`）与前端 WebUI 检查。配置文件位于 [`warden.toml`](warden.toml)。

### 核心命令（亦可通过 `just` 调用）

| 场景 | 命令 | 说明 |
| :--- | :--- | :--- |
| **快速检查（Pre-commit）** | `cargo warden check`（或 `just check`） | 并行运行全部静态检查（`dag`、`workspace-dag`、`complexity`、`file-length`、`hygiene`、`manifest`、`ci-parity`）+ `cargo fmt --check` + `cargo clippy` |
| **完整门禁（Pre-push / CI）** | `cargo warden gate`（或 `just gate`） | 在 `check` 基础上继续编排 `cargo nextest run`、`cargo doc` 与 `webui-check`（`pnpm fmt:check && pnpm typecheck && pnpm lint`） |
| **深度审计门禁** | `cargo warden gate --full`（或 `just gate-full`） | 额外运行 `cargo-machete`（未用依赖）、`cargo-deny` 与 `cargo-audit` |
| **一键自动修复** | `cargo warden gate --fix`（或 `just fix`） | 自动按序执行 `cargo clippy --fix`、`cargo fmt --all` 与前端 `pnpm fmt` |
| **环境与钩子诊断** | `cargo warden doctor`（或 `just doctor`） | 检查工具链、`cargo-nextest`/`machete`/`deny`/`audit` 及 Git Hooks 状态 |

> **铁律**：在交付或提交任何代码前，必须确保 `cargo warden gate` 全绿通过（0 error, 0 warning）。Git `pre-commit`（运行 `check`）与 `pre-push`（运行 `gate`）钩子已由 `cargo-warden` 托管，**严禁使用 `--no-verify` 跳过钩子**。

---

## 3. 代码红线与静态检查阈值（`warden.toml`）

后续工作者在编写或修改 Rust 代码时，必须遵守以下由 `cargo-warden` 强制执行的红线：

1. **函数复杂度 (`[complexity]`)**：
   - 单函数认知复杂度（Cognitive Complexity）**≤ 25**
   - 单函数物理行数 **≤ 120 行**
   - 最大控制流嵌套深度 **≤ 5 层**（`tokio::select!` 或多层 `match`/`if let` 建议将分支逻辑拆分为独立辅助函数）
   - 函数参数个数 **≤ 7 个**（超出时请封装上下文结构体）
2. **文件长度 (`[file_length]`)**：
   - 单个 `.rs` 源文件物理行数 **≤ 800 行**（超出时应按职责拆分为子模块，如 `netlink/mod.rs` + `netlink/apply.rs`）。
3. **代码卫生与安全 (`[hygiene]`)**：
   - **禁止 `unsafe`**（Workspace Cargo.toml 与 warden 同时开启 `forbid_unsafe`）。
   - **生产代码禁止 `.unwrap()` / `.unwrap_err()`**：必须使用 `?`、`.unwrap_or(...)`、`.unwrap_or_default()` 或显式错误映射（测试代码 `#[cfg(test)]` 除外）。
   - **生产代码禁止 `dbg!()`、`todo!()`、`unimplemented!()`**。
   - **生产代码禁止 `println!()` / `eprintln!()`**：统一使用 `tracing::{info, warn, error, debug}` 记录结构化日志（仅 `crates/crabrouter-cli/**` 命令行工具允许终端打印）。
4. **Cargo.toml 清单规范 (`[manifest]`)**：
   - 新增或修改子 crate 时，`version`、`edition`、`license`、`repository` 与 `[lints]` 必须使用 `.workspace = true` 继承。
   - 所有跨 crate 或第三方依赖必须先在根目录 `Cargo.toml` 的 `[workspace.dependencies]` 中声明，子 crate 中使用 `<dep>.workspace = true`。
   - 禁止 `git = "..."` 依赖与工作区外部的 `path = "..."` 依赖。

---

## 4. 前后端联调与镜像构建注意事项

1. **`ts-rs` 前后端类型同步**：
   - `.cargo/config.toml` 已配置 `TS_RS_EXPORT_DIR = "webui/src/types"`。
   - 修改 `crabrouter-model`、`crabrouter-core`、`crabrouter-services` 或 `crabrouter-web` 中带 `#[derive(TS)]` / `#[ts(export)]` 的结构体后，运行 `cargo test --workspace`（或 `cargo nextest run`）会自动刷新 `webui/src/types/*.ts`。
2. **嵌入式前端静态资源 (`rust-embed`)**：
   - `crabrouter-web` 通过 `rust-embed` 将 `webui/dist` 编译进二进制。若修改了 `webui/` 前端代码，请运行 `just build-ui`（或 `cd webui && pnpm build`）重新生成 `webui/dist`。
3. **干跑（Dry-Run）与宿主机安全**：
   - 开发与单元测试在非软路由宿主机上运行时，涉及内核 `rtnetlink`、`nft`、`tc`、`conntrack`、`systemd` D-Bus、`smartctl` 等操作必须通过 `NetworkBackend` / `ServiceSupervisor` 的 `DryRun` 实现或 `dry_run: bool` 分支隔离，切勿在单元测试中直接改动宿主机真实网络配置。
4. **开发进度追踪**：
   - 完成或推进功能阶段后，请同步更新 [`TODOS.md`](TODOS.md) 中的对应任务状态。
