use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use crabrouter_core::{ConfigStore, RouterEngine};
use crabrouter_net::{
    DryRunNetworkBackend, LinuxNetworkBackend, NetworkBackend, WanStateObserver,
    spawn_network_event_engine,
};
use crabrouter_services::{DryRunSupervisor, ServiceSupervisor, SystemdSupervisor};
use crabrouter_web::{LogHub, build_router};
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

    /// Run without mutating host nftables/network or calling systemctl.
    #[arg(long)]
    dry_run: bool,

    #[arg(long)]
    web_listen: Option<String>,
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let logs = LogHub::new();
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .with(LogHubLayer { hub: logs.clone() })
        .init();

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

    let engine = RouterEngine::bootstrap(store, net, services).await?;
    let observer: Arc<dyn WanStateObserver> = Arc::clone(&engine) as Arc<dyn WanStateObserver>;
    let wan_bus = spawn_network_event_engine(observer, cli.dry_run);
    if let Ok(saved_wans) = engine.db().list_wan_states() {
        wan_bus.seed_states(saved_wans).await;
    }

    let listen_addr = cli
        .web_listen
        .unwrap_or_else(|| engine.active_config().system.web_listen.clone());

    let app = build_router(Arc::clone(&engine), wan_bus, logs, cli.leases.clone());
    let listener = TcpListener::bind(&listen_addr).await?;
    tracing::info!(
        addr = %listen_addr,
        dry_run = cli.dry_run,
        config = %cli.config.display(),
        "crabrouterd started"
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(wait_for_shutdown_signal())
        .await?;

    tracing::info!("crabrouterd stopped");
    Ok(())
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
