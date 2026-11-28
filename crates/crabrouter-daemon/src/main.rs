use std::fmt::Write as _;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum_server::Handle;
use axum_server::tls_rustls::RustlsConfig;
use clap::Parser;
use crabrouter_core::{ConfigStore, RouterEngine};
use crabrouter_model::RouterConfig;
use crabrouter_net::{
    DryRunNetworkBackend, EbpfEngine, LinuxNetworkBackend, NetworkBackend, WanStateObserver,
    spawn_multi_wan_engine_with_shutdown, spawn_network_event_engine_with_shutdown,
};
use crabrouter_services::{
    ContainerBackend, CrowdSecManager, DryRunContainerBackend, DryRunSupervisor, ServiceSupervisor,
    SocketContainerBackend, SystemdSupervisor, TlsCertManager, auto_bless_if_tentative,
    spawn_ddns_engine_with_shutdown,
};
use crabrouter_web::{AppState, LogHub, build_router, spawn_crabdns_ebpf_bridge_with_shutdown};
use ipnet::IpNet;
use mimalloc::MiMalloc;
use tokio::net::TcpListener;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[derive(Debug, Parser)]
#[command(name = "crabrouterd", about = "CrabRouter control plane daemon")]
struct Cli {
    #[arg(short, long, default_value = "./data/router.toml")]
    config: PathBuf,

    #[arg(long, default_value = "/var/lib/misc/dnsmasq.leases")]
    leases: PathBuf,

    /// Run without mutating host nftables/network or calling systemd D-Bus.
    #[arg(long)]
    dry_run: bool,

    #[arg(long)]
    web_listen: Option<String>,

    /// Enable HTTPS listener using configured or auto-generated self-signed TLS certs.
    #[arg(long)]
    https: bool,
}

struct LogHubLayer {
    hub: LogHub,
}

#[derive(Default)]
struct MessageVisitor {
    message: String,
    fields: String,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{value:?}");
        } else {
            if !self.fields.is_empty() {
                self.fields.push(' ');
            }
            let _ = write!(self.fields, "{}={value:?}", field.name());
        }
    }
}

impl<S: Subscriber> Layer<S> for LogHubLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        let text = match (visitor.message.is_empty(), visitor.fields.is_empty()) {
            (false, false) => format!("{} ({})", visitor.message, visitor.fields),
            (false, true) => visitor.message,
            (true, false) => visitor.fields,
            (true, true) => String::new(),
        };
        let meta = event.metadata();
        self.hub.push(meta.level().as_str(), meta.target(), text);
    }
}

fn init_logging() -> LogHub {
    let logs = LogHub::new();
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .with(LogHubLayer { hub: logs.clone() })
        .init();
    logs
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let logs = init_logging();
    let cli = Cli::parse();
    let store = ConfigStore::new(&cli.config);

    let net: Arc<dyn NetworkBackend> = if cli.dry_run {
        Arc::new(DryRunNetworkBackend)
    } else {
        Arc::new(LinuxNetworkBackend::default())
    };
    let services: Arc<dyn ServiceSupervisor> = if cli.dry_run {
        Arc::new(DryRunSupervisor)
    } else {
        Arc::new(SystemdSupervisor::default())
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let engine = RouterEngine::bootstrap(store, net, services).await?;
    let observer: Arc<dyn WanStateObserver> = Arc::clone(&engine) as Arc<dyn WanStateObserver>;
    let wan_bus = spawn_network_event_engine_with_shutdown(
        Arc::clone(&observer),
        cli.dry_run,
        shutdown_rx.clone(),
    );
    if let Ok(saved_wans) = engine.db().list_wan_states() {
        wan_bus.seed_states(saved_wans).await;
    }
    let multi_wan = spawn_multi_wan_engine_with_shutdown(
        observer,
        wan_bus.clone(),
        cli.dry_run,
        shutdown_rx.clone(),
    );
    let cfg = engine.active_config();
    let ebpf = init_ebpf_and_bridge(&engine, &cfg, cli.dry_run, &shutdown_rx).await;

    let ddns = {
        let eng_cfg = Arc::clone(&engine);
        let eng_wans = Arc::clone(&engine);
        spawn_ddns_engine_with_shutdown(
            move || (*eng_cfg.active_config()).clone(),
            move || eng_wans.db().list_wan_states().unwrap_or_default(),
            &cfg,
            cli.dry_run,
            shutdown_rx.clone(),
        )
    };
    let tls = TlsCertManager::new(&cfg, cli.dry_run);
    let crowdsec = spawn_crowdsec_sync_loop(
        Arc::clone(&engine),
        ebpf.clone(),
        cli.dry_run,
        shutdown_rx.clone(),
    );
    let containers: Arc<dyn ContainerBackend> = if cli.dry_run {
        Arc::new(DryRunContainerBackend::new())
    } else {
        Arc::new(SocketContainerBackend)
    };

    let ota_dir = cli
        .config
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("ota"), |p| p.join("ota"));
    let app = build_router(AppState {
        engine: Arc::clone(&engine),
        wan_bus,
        multi_wan,
        ebpf,
        ddns,
        tls,
        crowdsec,
        containers,
        logs,
        lease_path: cli.leases.clone(),
        ota_dir,
        sysupdate_dir: PathBuf::from("/etc/sysupdate.d"),
        sysext_dir: PathBuf::from("/var/lib/extensions"),
        dry_run: cli.dry_run,
    });

    let shutdown_handle = Handle::new();
    let server_tasks = spawn_web_listeners(&cfg, &cli, &app, &shutdown_handle).await?;
    sd_notify("READY=1");
    spawn_traffic_recorder(Arc::clone(&engine), shutdown_rx.clone());
    spawn_watchdog_and_autobless(cli.dry_run, shutdown_rx);

    tracing::info!(
        lan_only = cfg.system.web_lan_only,
        https = cli.https || cfg.system.web_https_enabled,
        config = %cli.config.display(),
        "crabrouterd started"
    );

    wait_for_shutdown_signal().await;
    sd_notify("STOPPING=1");
    let _ = shutdown_tx.send(true);
    shutdown_handle.graceful_shutdown(Some(std::time::Duration::from_secs(5)));
    for task in server_tasks {
        let _ = task.await;
    }

    tracing::info!("crabrouterd stopped");
    Ok(())
}

async fn init_ebpf_and_bridge(
    engine: &Arc<RouterEngine>,
    cfg: &RouterConfig,
    dry_run: bool,
    shutdown_rx: &tokio::sync::watch::Receiver<bool>,
) -> EbpfEngine {
    let ebpf = EbpfEngine::new(dry_run);
    engine.attach_ebpf(ebpf.clone());
    if let Err(err) = ebpf.sync_config(cfg).await {
        tracing::warn!(error = %err, "initial eBPF tc/tcx sync failed");
    }
    spawn_crabdns_ebpf_bridge_with_shutdown(
        Arc::clone(engine),
        ebpf.clone(),
        dry_run,
        shutdown_rx.clone(),
    );
    ebpf
}

fn spawn_crowdsec_sync_loop(
    engine: Arc<RouterEngine>,
    ebpf: EbpfEngine,
    dry_run: bool,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> CrowdSecManager {
    let crowdsec = CrowdSecManager::new(dry_run);
    let manager = crowdsec.clone();
    tokio::spawn(async move {
        loop {
            let cfg = engine.active_config();
            let status = manager.refresh_decisions(&cfg).await;
            if cfg.crowdsec.enabled
                && cfg.crowdsec.sync_to_xdp
                && cfg.ebpf.enabled
                && cfg.ebpf.xdp_guard_enabled
            {
                let extra: Vec<String> = status
                    .decisions
                    .iter()
                    .filter(|d| d.decision_type == "ban")
                    .map(|d| d.value.clone())
                    .collect();
                if let Err(err) = ebpf
                    .sync_extra_xdp_blocklist(&cfg.ebpf.xdp_blocklist, &extra)
                    .await
                {
                    tracing::debug!(error = %err, "crowdsec XDP mirror skipped");
                }
            }
            let wait_secs = u64::from(cfg.crowdsec.update_frequency_secs.max(5));
            tokio::select! {
                () = tokio::time::sleep(std::time::Duration::from_secs(wait_secs)) => {}
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        break;
                    }
                }
            }
        }
    });
    crowdsec
}

fn spawn_traffic_recorder(
    engine: Arc<RouterEngine>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        let mut prev: std::collections::HashMap<String, (u64, u64)> =
            std::collections::HashMap::new();
        loop {
            if let Ok(ifaces) = crabrouter_net::read_host_interfaces() {
                let mut deltas = Vec::new();
                for iface in ifaces {
                    if iface.name == "lo" || iface.name.starts_with("ifb-") {
                        continue;
                    }
                    if let Some(&(last_rx, last_tx)) = prev.get(&iface.name) {
                        let rx_d = iface.rx_bytes.saturating_sub(last_rx);
                        let tx_d = iface.tx_bytes.saturating_sub(last_tx);
                        if rx_d > 0 || tx_d > 0 {
                            deltas.push((iface.name.clone(), rx_d, tx_d));
                        }
                    }
                    prev.insert(iface.name, (iface.rx_bytes, iface.tx_bytes));
                }
                if !deltas.is_empty()
                    && let Err(err) = engine.db().record_today_traffic_deltas(&deltas)
                {
                    tracing::debug!(error = %err, "failed to record daily traffic deltas");
                }
            }
            tokio::select! {
                () = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
            }
        }
    });
}

fn spawn_watchdog_and_autobless(dry_run: bool, mut shutdown: tokio::sync::watch::Receiver<bool>) {
    tokio::spawn(async move {
        let mut ticks: u32 = 0;
        let mut blessed = false;
        loop {
            tokio::select! {
                () = tokio::time::sleep(std::time::Duration::from_secs(10)) => {
                    sd_notify("WATCHDOG=1");
                    ticks = ticks.saturating_add(1);
                    if !blessed && !dry_run && ticks >= 3 {
                        blessed = true;
                        if let Err(err) = auto_bless_if_tentative(false).await {
                            tracing::warn!(error = %err, "auto-bless check failed");
                        }
                    }
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
            }
        }
    });
}

/// Sends a `systemd` `sd_notify` datagram over `NOTIFY_SOCKET` using safe Rust.
fn sd_notify(state: &str) {
    let Ok(socket_path) = std::env::var("NOTIFY_SOCKET") else {
        return;
    };
    let Ok(sock) = std::os::unix::net::UnixDatagram::unbound() else {
        return;
    };
    #[cfg(target_os = "linux")]
    if let Some(abstract_name) = socket_path.strip_prefix('@') {
        use std::os::linux::net::SocketAddrExt;
        if let Ok(addr) = std::os::unix::net::SocketAddr::from_abstract_name(abstract_name) {
            let _ = sock.send_to_addr(state.as_bytes(), &addr);
        }
        return;
    }
    let _ = sock.send_to(state.as_bytes(), socket_path);
}

async fn spawn_web_listeners(
    cfg: &RouterConfig,
    cli: &Cli,
    app: &axum::Router,
    shutdown_handle: &Handle<SocketAddr>,
) -> anyhow::Result<Vec<tokio::task::JoinHandle<()>>> {
    let http_raw = cli
        .web_listen
        .as_deref()
        .unwrap_or(cfg.system.web_listen.as_str());
    let http_addrs = resolve_listen_addrs(cfg, http_raw, cli.web_listen.is_some());
    let mut server_tasks = Vec::new();

    for addr in &http_addrs {
        match TcpListener::bind(addr).await {
            Ok(listener) => {
                tracing::info!(addr = %addr, dry_run = cli.dry_run, "HTTP WebUI listener bound");
                let router = app.clone();
                server_tasks.push(tokio::spawn(async move {
                    if let Err(err) = axum::serve(
                        listener,
                        router.into_make_service_with_connect_info::<SocketAddr>(),
                    )
                    .with_graceful_shutdown(wait_for_shutdown_signal())
                    .await
                    {
                        tracing::error!(error = %err, "HTTP server error");
                    }
                }));
            }
            Err(err) if cfg.system.web_lan_only && !addr.ip().is_loopback() => {
                tracing::warn!(
                    addr = %addr,
                    error = %err,
                    "LAN-only bind address unavailable on this host; loopback remains active"
                );
            }
            Err(err) => return Err(err.into()),
        }
    }

    if cli.https || cfg.system.web_https_enabled {
        let tls_cfg = ensure_tls_config(cfg, &cli.config).await?;
        let tls_addrs = resolve_listen_addrs(cfg, &cfg.system.web_https_listen, false);
        for addr in tls_addrs {
            if std::net::TcpListener::bind(addr).is_err()
                && cfg.system.web_lan_only
                && !addr.ip().is_loopback()
            {
                tracing::warn!(addr = %addr, "HTTPS LAN-only bind address unavailable; skipping");
                continue;
            }
            tracing::info!(addr = %addr, "HTTPS WebUI listener active");
            let router = app.clone();
            let tls = tls_cfg.clone();
            let handle = shutdown_handle.clone();
            server_tasks.push(tokio::spawn(async move {
                if let Err(err) = axum_server::tls_rustls::bind_rustls(addr, tls)
                    .handle(handle)
                    .serve(router.into_make_service_with_connect_info::<SocketAddr>())
                    .await
                {
                    tracing::error!(error = %err, "HTTPS server error");
                }
            }));
        }
    }

    if server_tasks.is_empty() {
        anyhow::bail!("no WebUI listeners could be bound");
    }
    Ok(server_tasks)
}

fn resolve_listen_addrs(
    cfg: &RouterConfig,
    raw_addr: &str,
    explicit_override: bool,
) -> Vec<SocketAddr> {
    let parsed: SocketAddr = raw_addr
        .parse()
        .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], 8080)));
    if explicit_override || !cfg.system.web_lan_only {
        return vec![parsed];
    }

    let port = parsed.port();
    let mut addrs = vec![SocketAddr::from(([127, 0, 0, 1], port))];
    for lan in &cfg.lans {
        if let Ok(net) = lan.cidr.parse::<IpNet>() {
            let sock = SocketAddr::new(net.addr(), port);
            if !addrs.contains(&sock) {
                addrs.push(sock);
            }
        }
    }
    addrs
}

async fn ensure_tls_config(cfg: &RouterConfig, config_path: &Path) -> anyhow::Result<RustlsConfig> {
    let data_dir = config_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let cert_path = cfg
        .system
        .web_tls_cert
        .as_deref()
        .map_or_else(|| data_dir.join("tls-cert.pem"), PathBuf::from);
    let key_path = cfg
        .system
        .web_tls_key
        .as_deref()
        .map_or_else(|| data_dir.join("tls-key.pem"), PathBuf::from);

    if !cert_path.exists() || !key_path.exists() {
        let mut sans = vec![
            cfg.system.hostname.clone(),
            format!("{}.lan", cfg.system.hostname),
            "localhost".to_string(),
            "127.0.0.1".to_string(),
        ];
        for lan in &cfg.lans {
            if let Ok(net) = lan.cidr.parse::<IpNet>() {
                let ip: IpAddr = net.addr();
                sans.push(ip.to_string());
            }
        }
        let certified = rcgen::generate_simple_self_signed(sans)?;
        if let Some(parent) = cert_path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&cert_path, certified.cert.pem())?;
        std::fs::write(&key_path, certified.signing_key.serialize_pem())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
        }
        tracing::info!(cert = %cert_path.display(), "generated self-signed TLS certificate for HTTPS WebUI");
    }

    Ok(RustlsConfig::from_pem_file(cert_path, key_path).await?)
}

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let Ok(mut sigterm) = signal(SignalKind::terminate()) else {
            let _ = tokio::signal::ctrl_c().await;
            return;
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
