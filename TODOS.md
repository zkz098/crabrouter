# CrabRouter 剩余工作清单 (TODOs)

当前仓库已完成 **控制面工作区骨架、配置校验与 Commit-Confirm 事务框架、`nftables` / `crabdns` / `dnsmasq` 渲染器、SolidJS 基础预览界面以及 Debian 13 `mkosi` 镜像骨架**。以下为后续分阶段开发与落地任务。

---

## Phase 1：MVP 可用有线软路由（核心闭环）

### 1. `crabrouter-net` — Linux 内核网络与状态机
- [x] **接入 `rtnetlink` 原生接口管理**（[netlink.rs](crates/crabrouter-net/src/netlink.rs)）
  - [x] 创建/更新 Bridge（如 `br-lan`）并将物理网卡绑定到对应 Bridge
  - [x] 支持 802.1Q VLAN 子接口创建与清理（对应 `RouterConfig.vlans`）
  - [x] 支持静态 IP 地址、默认路由、路由表与策略路由（PBR）增量 Diff 下发与回收
  - [x] 增加网卡 MAC 固化命名规则生成（写入 `systemd.link` 或 udev 规则），防止插拔网卡后接口名漂移（[pins.rs](crates/crabrouter-net/src/pins.rs)）
- [x] **WAN 接入协议联调**
  - [x] `dhcpcd-base` hook / IPC 集成：处理 WAN DHCPv4 获取、租约更新与网关/DNS 变更通知（[dhcpcd.rs](crates/crabrouter-net/src/dhcpcd.rs)）
  - [x] PPPoE 拨号托管（[pppoe.rs](crates/crabrouter-net/src/pppoe.rs)）：生成 `/etc/ppp/peers/*` 与 `/etc/ppp/chap-secrets`，拉起并监控 `pppd` 进程
  - [x] 接口事件驱动联动：PPPoE 重拨导致 `ifindex` 或公网 IP 变更时，自动刷新 `nftables`、`conntrack -D` 清理残留状态、更新默认路由与 WireGuard 端点（[events.rs](crates/crabrouter-net/src/events.rs)）
- [x] **防火墙与 NAT 完善**（[nftables.rs](crates/crabrouter-net/src/nftables.rs)）
  - [x] 校验 `nft -c -f -` 预检查（在真正应用前做语法与接口校验，失败直接拒绝提交）
  - [x] 增加自定义访问控制规则（Zone 间放行/阻断、IP/端口过滤）与 SYN Flood 限速规则生成
  - [x] 处理动态接口（如 `ppp0` 未上线时 `flowtable` 设备的安全加载或回退逻辑）

### 2. `crabrouter-services` — 服务编排与 `crabdns` / `dnsmasq` 联动
- [x] **用 `zbus` 替换 `systemctl` 子进程调用**（[supervisor.rs](crates/crabrouter-services/src/supervisor.rs)）
  - [x] 通过 `org.freedesktop.systemd1` D-Bus 接口启动、重载、查询 `crabdns.service`、`dnsmasq.service`、`chrony.service`、`wg-quick@*`
- [x] **完善 `crabdns` 集成**（[crabdns.rs](crates/crabrouter-services/src/crabdns.rs)）
  - [x] 将 `~/projects/crabdns` 打包为 `.deb` 或直接构建进 `mkosi.extra`，补齐 `crabdns.service` systemd 单元（[bundle-crabdns.sh](image/scripts/bundle-crabdns.sh)）
  - [x] 配置 `crabdns` 与 `dnsmasq` 租约主机名同步（开启 `dnsmasq` `expand-hosts`/`dhcp-leasefile` 并让 `crabdns` `split_dns` 与 `private_ptr_upstreams` 稳定反解 `127.0.0.1:5354`）
  - [x] 在 `crabrouter-web` 中增加 `crabdns` 核心状态聚合（`/api/dns/status`）与反向代理入口（`/crabdns/*`，无需暴露两个管理端口）
- [x] **WireGuard 服务端配置下发**（[wireguard.rs](crates/crabrouter-services/src/wireguard.rs)）
  - [x] 实现纯 Rust `x25519-dalek` 密钥对生成/导入、`/etc/wireguard/<iface>.conf` 生成与 `wg-quick@.service` 生命周期管理

### 3. `crabrouter-core` & `crabrouter-daemon` — 可靠性与鉴权
- [x] **启动恢复与异常兜底**（[transaction.rs](crates/crabrouter-core/src/transaction.rs)、[db.rs](crates/crabrouter-core/src/db.rs)）
  - [x] 记录未确认的 pending 状态标记（纯 Rust `redb`），若在 Commit-Confirm 期间设备意外断电重启，开机优先从已确认的 `last-good` 配置启动
  - [x] 支持配置历史版本快照与一键恢复（保存最近 20 份已确认的 `router.toml`）
- [x] **Web 鉴权与安全加固**（[crabrouter-web](crates/crabrouter-web/src/lib.rs)、[main.rs](crates/crabrouter-daemon/src/main.rs)）
  - [x] 实现用户登录认证（`Argon2` 密码哈希存储于 `redb`、HttpOnly Cookie / Bearer Token 会话）
  - [x] 仅监听 LAN 接口（`web_lan_only`）与 HTTPS 支持（`axum-server` + `rustls` + `rcgen` 自签/自定义证书）
  - [x] 增加基于 `WebSocket` 的实时指标推送（`/api/metrics/ws`：每秒网口 RX/TX 速率、CPU/内存/温度、Commit-Confirm 状态）

### 4. `webui` — 核心配置页面补全
- [x] 接入 `crabrouter-model` 导出的 `ts-rs` 类型，完善表单编辑与 `begin_commit` -> 倒计时确认弹窗交互
- [x] **配置页面开发**：
  - [x] 首次初始化向导（选定 WAN 口、LAN 口组合、管理员密码、PPPoE/DHCP 配置）
  - [x] 接口与 WAN/LAN/VLAN 管理页（支持拖拽/勾选物理口加入 `br-lan`）
  - [x] 防火墙、Zone 策略与端口转发管理页
  - [x] DHCP 静态地址绑定、在线终端列表与 WireGuard Peer（含二维码生成）管理页
  - [x] 诊断工具箱（Ping / Traceroute / `iperf3` / 实时日志流）

---

## Phase 2：高阶功能与不可变系统 OTA

### 5. Debian 13 镜像与 OTA 闭环（[image/](image/mkosi.conf)）
- [x] 搭建 CI / 本地 `mkosi` 构建流水线（[.github/workflows/image.yml](.github/workflows/image.yml)、[build-image.sh](image/scripts/build-image.sh)、[qemu-boot.sh](image/scripts/qemu-boot.sh)）（实际产物构建与 UEFI/QEMU 启动需在安装了 `mkosi`/`qemu-system-x86_64` 的环境或 CI runner 执行）
- [x] 编写首次安装刷盘工具（[installer.rs](crates/crabrouter-cli/src/installer.rs)：`crabctl disks` / `crabctl install`，写入目标盘 `systemd-repart` 分区布局并安装 `systemd-boot`）
- [x] 补全 `systemd-boot` 启动项计数（Boot Assessment）与 `systemd-sysupdate` A/B 升级配置（[20-uki.transfer](image/mkosi.sysupdate/20-uki.transfer)、[crabrouter-boot-bless.service](image/mkosi.extra/etc/systemd/system/crabrouter-boot-bless.service)）— 真机/QEMU A/B 升级失败自动回滚实测归入 Phase 3 QEMU 测试
- [x] 接入 WebUI 固件升级页面（[OtaPage.tsx](webui/src/components/OtaPage.tsx)、[ota_api.rs](crates/crabrouter-web/src/ota_api.rs)：上传/拉取 OTA 镜像、触发 `systemd-sysupdate`、`logind` D-Bus 重启切换槽位）

### 6. 高阶网络特性
- [ ] **多 WAN 负载均衡与故障切换**
  - [ ] 基于策略路由 + 多路由表 + ICMP/HTTP 健康探测实现主备切换与按权重分流
  - [ ] WAN 切换时精确清理受影响的 `conntrack` 条目
- [x] **完整 IPv6-PD 支持**
  - [x] 从上游 `dhcpcd` / `pppd` 获取动态 `/56` 或 `/60` 前缀并自动向多个 LAN/VLAN 下发 `/64` RA（`dnsmasq` 或 `radvd`）（[ipv6_pd.rs](crates/crabrouter-net/src/ipv6_pd.rs)、[dhcpcd.rs](crates/crabrouter-net/src/dhcpcd.rs)、[pppoe.rs](crates/crabrouter-net/src/pppoe.rs)、[dnsmasq.rs](crates/crabrouter-services/src/dnsmasq.rs)）
- [ ] **连接追踪与 QoS**
  - [ ] 通过 Netlink 读取内核 `conntrack` 实时连接表并在 WebUI 展示与搜索（接入 `@tanstack/solid-table`）
  - [ ] 集成 `tc` + `CAKE` / `fq_codel` 队列管理，解决高负载 Bufferbloat
- [ ] **Web 终端 & 硬件监控**
  - [ ] WebUI 集成 `xterm.js` + 后端 PTY WebSocket
  - [ ] 采集 `smartctl` / `nvme` SSD 健康度与 `lm-sensors` 温度并增加异常告警
- [ ] **eBPF / XDP 进阶加速（可选）**
  - [ ] 评估基于 `aya` 将 `crabdns` 域名分流结果写入内核 eBPF Map，实现 TC/XDP 级策略分流与精细化流量统计

---

## Phase 3：测试矩阵与生态扩展

### 7. 端到端自动化仿真测试
- [ ] 构建基于 **Linux Network Namespace (`netns`) + `veth`** 的集成测试套件（在隔离命名空间中真实运行 `nft`、`dnsmasq`、`pppd` 服务端模拟多线拨号与 NAT 转发验证）
- [ ] 构建 **QEMU/KVM 整机镜像启动测试**（验证 `mkosi` 产物启动、`crabrouterd.service` 拉起、WebUI 健康检查及 A/B OTA 升级回滚）

### 8. 可选扩展服务
- [ ] 动态路由协议托管与 UI：`frr` 或 `bird2`/`bird3`（BGP / OSPF）
- [ ] 无线 AP 管理：托管 `hostapd` + `iw`（面向带 Wi-Fi 网卡的 x86 软路由）
- [ ] 企业 VPN 扩展：`strongswan` IPsec 站点互联
