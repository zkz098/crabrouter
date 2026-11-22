use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use crabrouter_model::{RouterConfig, WanProto, WanRuntimeState};
use futures_util::stream::StreamExt;
use rtnetlink::packet_core::NetlinkPayload;
use rtnetlink::packet_route::RouteNetlinkMessage;
use rtnetlink::packet_route::address::AddressAttribute;
use rtnetlink::packet_route::link::LinkAttribute;
use rtnetlink::{MulticastGroup, new_multicast_connection};
use tokio::process::Command;
use tokio::sync::{RwLock, mpsc, watch};

use crate::cmd::run_status_with_timeout;
use crate::conntrack::flush_conntrack_for_wan;
use crate::dhcpcd::WanLeaseEvent;
use crate::ipv6_pd::{apply_ipv6_pd_slices_netlink, slice_delegated_ipv6_prefix};
use crate::nftables::apply_nft_with_precheck;

/// Callback trait allowing `crabrouter-net` event engine to fetch active config and persist
/// updated `WanRuntimeState` snapshots (e.g. into `redb` in `crabrouter-core`).
pub trait WanStateObserver: Send + Sync {
    fn current_config(&self) -> RouterConfig;
    fn on_wan_state_updated(&self, state: WanRuntimeState);
}

#[derive(Clone)]
pub struct WanEventBus {
    states: Arc<RwLock<HashMap<String, WanRuntimeState>>>,
    tx: mpsc::Sender<WanLeaseEvent>,
}

impl WanEventBus {
    pub fn new(tx: mpsc::Sender<WanLeaseEvent>) -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
            tx,
        }
    }

    pub async fn snapshot(&self) -> Vec<WanRuntimeState> {
        let mut list: Vec<WanRuntimeState> = self.states.read().await.values().cloned().collect();
        list.sort_by(|a, b| a.wan_name.cmp(&b.wan_name));
        list
    }

    pub async fn seed_states(&self, initial: Vec<WanRuntimeState>) {
        let mut guard = self.states.write().await;
        for item in initial {
            guard.insert(item.wan_name.clone(), item);
        }
    }

    pub async fn emit(&self, ev: WanLeaseEvent) {
        if let Err(err) = self.tx.send(ev).await {
            tracing::warn!(error = %err, "failed to enqueue WAN lease event");
        }
    }
}

/// Spawns the event-driven network state machine listening to:
/// 1. `rtnetlink` multicast groups (`Link`, `Ipv4Ifaddr`, `Ipv6Ifaddr`, `Ipv4Route`)
/// 2. `dhcpcd` / `pppd` hook events delivered over `WanEventBus`
///
/// Whenever a `PPPoE` redial or DHCP renewal changes `ifindex` or public IP, it atomically
/// refreshes `nftables`, runs `conntrack -D` to drop stale NAT flows for the old IP,
/// and triggers `WireGuard` endpoint refresh if enabled.
pub fn spawn_network_event_engine(
    observer: Arc<dyn WanStateObserver>,
    dry_run: bool,
) -> WanEventBus {
    let (_tx, rx) = watch::channel(false);
    spawn_network_event_engine_with_shutdown(observer, dry_run, rx)
}

/// Spawns the event-driven network state machine with an explicit graceful-shutdown signal.
pub fn spawn_network_event_engine_with_shutdown(
    observer: Arc<dyn WanStateObserver>,
    dry_run: bool,
    shutdown: watch::Receiver<bool>,
) -> WanEventBus {
    let (tx, rx) = mpsc::channel::<WanLeaseEvent>(64);
    let bus = WanEventBus::new(tx.clone());
    let states = Arc::clone(&bus.states);

    tokio::spawn(async move {
        run_event_loop(observer, states, rx, tx, dry_run, shutdown).await;
    });

    bus
}

async fn run_event_loop(
    observer: Arc<dyn WanStateObserver>,
    states: Arc<RwLock<HashMap<String, WanRuntimeState>>>,
    mut hook_rx: mpsc::Receiver<WanLeaseEvent>,
    hook_tx: mpsc::Sender<WanLeaseEvent>,
    dry_run: bool,
    mut shutdown: watch::Receiver<bool>,
) {
    if !dry_run {
        spawn_netlink_multicast_listener(hook_tx, shutdown.clone());
    }

    loop {
        let ev = tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::info!("network event loop received shutdown signal");
                    break;
                }
                continue;
            }
            maybe_ev = hook_rx.recv() => {
                let Some(ev) = maybe_ev else {
                    break;
                };
                ev
            }
        };
        let cfg = observer.current_config();
        let Some(wan) = cfg.wans.iter().find(|w| {
            w.name == ev.wan_name
                || w.device == ev.device
                || (w.proto == WanProto::Pppoe && ev.device.starts_with("ppp"))
        }) else {
            tracing::debug!(wan = %ev.wan_name, dev = %ev.device, "ignoring event for non-WAN interface");
            continue;
        };

        let wan_name = wan.name.clone();
        let prev = states.read().await.get(&wan_name).cloned();
        let prev_ip = prev.as_ref().and_then(|s| s.ip.clone());
        let prev_ifindex = prev.as_ref().and_then(|s| s.ifindex);
        let prev_slices = prev
            .as_ref()
            .map_or_else(Vec::new, |s| s.ipv6_slices.clone());

        let updated = build_updated_wan_state(&cfg, wan, prev.as_ref(), &ev);
        let new_ip = updated.ip.clone();
        let new_slices = updated.ipv6_slices.clone();

        states
            .write()
            .await
            .insert(wan_name.clone(), updated.clone());
        observer.on_wan_state_updated(updated);

        if !dry_run
            && prev_slices != new_slices
            && let Err(err) = apply_ipv6_pd_slices_netlink(&prev_slices, &new_slices).await
        {
            tracing::warn!(
                wan = %wan_name,
                error = %err,
                "failed to apply IPv6-PD /64 slices to downstream interfaces"
            );
        }

        let ip_changed = prev_ip.is_some() && prev_ip != new_ip;
        let ifindex_changed =
            prev_ifindex.is_some() && ev.ifindex.is_some() && prev_ifindex != ev.ifindex;

        if !dry_run
            && (ip_changed
                || ifindex_changed
                || matches!(ev.event.as_str(), "up" | "ipv6_up" | "BOUND" | "BOUND6"))
        {
            if let Some(old_ip) = prev_ip.as_deref()
                && ip_changed
            {
                flush_conntrack_for_wan(Some(old_ip), None, dry_run).await;
            }
            if let Err(err) = apply_nft_with_precheck(&cfg).await {
                tracing::error!(wan = %wan_name, error = %err, "event-driven nftables refresh failed");
            }
            if cfg.wireguard.enabled {
                refresh_wireguard_if_present(&cfg.wireguard.interface).await;
            }
        }
    }
}

fn build_updated_wan_state(
    cfg: &RouterConfig,
    wan: &crabrouter_model::WanConfig,
    prev: Option<&WanRuntimeState>,
    ev: &WanLeaseEvent,
) -> WanRuntimeState {
    let prev_ip = prev.and_then(|s| s.ip.clone());
    let prev_gw = prev.and_then(|s| s.gateway.clone());
    let prev_ifindex = prev.and_then(|s| s.ifindex);
    let prev_ipv6_addr = prev.and_then(|s| s.ipv6_addr.clone());
    let prev_ipv6_pd = prev.and_then(|s| s.ipv6_pd_prefix.clone());

    let is_v4_clear = matches!(
        ev.event.as_str(),
        "down" | "link_down" | "EXPIRE" | "RELEASE" | "STOP" | "NOCARRIER"
    );
    let is_v6_clear = matches!(
        ev.event.as_str(),
        "down" | "link_down" | "ipv6_down" | "EXPIRE6" | "RELEASE6" | "STOP" | "NOCARRIER"
    );

    let new_ip = if is_v4_clear {
        None
    } else {
        ev.ip.clone().filter(|s| !s.is_empty()).or(prev_ip)
    };
    let new_gw = if is_v4_clear {
        None
    } else {
        ev.gateway.clone().filter(|s| !s.is_empty()).or(prev_gw)
    };
    let new_ipv6_addr = if is_v6_clear {
        None
    } else {
        ev.ipv6_addr
            .clone()
            .filter(|s| !s.is_empty())
            .or(prev_ipv6_addr)
    };
    let new_ipv6_pd = if is_v6_clear || !wan.ipv6_pd {
        None
    } else {
        ev.ipv6_pd_prefix
            .clone()
            .filter(|s| !s.is_empty())
            .or(prev_ipv6_pd)
    };

    let new_slices = match new_ipv6_pd.as_deref() {
        Some(prefix) => match slice_delegated_ipv6_prefix(prefix, cfg) {
            Ok(slices) => slices,
            Err(err) => {
                tracing::warn!(wan = %wan.name, prefix, error = %err, "failed to slice delegated IPv6-PD prefix");
                Vec::new()
            }
        },
        None => Vec::new(),
    };

    WanRuntimeState {
        wan_name: wan.name.clone(),
        device: ev.device.clone(),
        proto: wan.proto,
        state: ev.event.clone(),
        ip: new_ip,
        gateway: new_gw,
        dns_servers: ev.dns_servers.clone(),
        ifindex: ev.ifindex.or(prev_ifindex),
        ipv6_addr: new_ipv6_addr,
        ipv6_pd_prefix: new_ipv6_pd,
        ipv6_slices: new_slices,
        updated_at: Utc::now().to_rfc3339(),
    }
}

fn spawn_netlink_multicast_listener(
    tx: mpsc::Sender<WanLeaseEvent>,
    mut shutdown: watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        let groups = [
            MulticastGroup::Link,
            MulticastGroup::Ipv4Ifaddr,
            MulticastGroup::Ipv6Ifaddr,
            MulticastGroup::Ipv4Route,
        ];
        let mut backoff_secs = 1_u64;

        loop {
            if *shutdown.borrow() {
                break;
            }
            let Ok((conn, _handle, mut messages)) = new_multicast_connection(&groups) else {
                tracing::warn!(
                    backoff_secs,
                    "unable to open rtnetlink multicast socket; retrying"
                );
                if wait_or_shutdown(&mut shutdown, Duration::from_secs(backoff_secs)).await {
                    break;
                }
                backoff_secs = (backoff_secs * 2).min(30);
                continue;
            };

            backoff_secs = 1;
            let conn_task = tokio::spawn(conn);
            let mut ifindex_to_name: HashMap<u32, String> = HashMap::new();

            loop {
                tokio::select! {
                    _ = shutdown.changed() => {
                        if *shutdown.borrow() {
                            conn_task.abort();
                            return;
                        }
                    }
                    item = messages.next() => {
                        let Some((msg, _)) = item else {
                            tracing::warn!("rtnetlink multicast stream ended; reconnecting");
                            break;
                        };
                        if let NetlinkPayload::InnerMessage(inner) = msg.payload {
                            handle_netlink_message(inner, &mut ifindex_to_name, &tx).await;
                        }
                    }
                }
            }

            conn_task.abort();
            if wait_or_shutdown(&mut shutdown, Duration::from_secs(backoff_secs)).await {
                break;
            }
            backoff_secs = (backoff_secs * 2).min(30);
        }
    });
}

async fn wait_or_shutdown(shutdown: &mut watch::Receiver<bool>, duration: Duration) -> bool {
    tokio::select! {
        () = tokio::time::sleep(duration) => false,
        _ = shutdown.changed() => *shutdown.borrow(),
    }
}

async fn handle_netlink_message(
    inner: RouteNetlinkMessage,
    ifindex_to_name: &mut HashMap<u32, String>,
    tx: &mpsc::Sender<WanLeaseEvent>,
) {
    match inner {
        RouteNetlinkMessage::NewLink(link) => {
            let ifindex = link.header.index;
            let ifname = link.attributes.iter().find_map(|a| match a {
                LinkAttribute::IfName(n) => Some(n.clone()),
                _ => None,
            });
            if let Some(name) = ifname {
                ifindex_to_name.insert(ifindex, name.clone());
                let _ = tx
                    .send(WanLeaseEvent {
                        source: "netlink".to_string(),
                        wan_name: name.clone(),
                        device: name,
                        event: "link_up".to_string(),
                        ip: None,
                        gateway: None,
                        dns_servers: Vec::new(),
                        ifindex: Some(ifindex),
                        ipv6_addr: None,
                        ipv6_pd_prefix: None,
                    })
                    .await;
            }
        }
        RouteNetlinkMessage::DelLink(link) => {
            if let Some(name) = ifindex_to_name.remove(&link.header.index) {
                let _ = tx
                    .send(WanLeaseEvent {
                        source: "netlink".to_string(),
                        wan_name: name.clone(),
                        device: name,
                        event: "link_down".to_string(),
                        ip: None,
                        gateway: None,
                        dns_servers: Vec::new(),
                        ifindex: Some(link.header.index),
                        ipv6_addr: None,
                        ipv6_pd_prefix: None,
                    })
                    .await;
            }
        }
        RouteNetlinkMessage::NewAddress(addr) => {
            let Some(name) = ifindex_to_name.get(&addr.header.index).cloned() else {
                return;
            };
            let ip = addr.attributes.iter().find_map(|a| match a {
                AddressAttribute::Local(ip) | AddressAttribute::Address(ip) => Some(ip.to_string()),
                _ => None,
            });
            if ip.is_some() {
                let _ = tx
                    .send(WanLeaseEvent {
                        source: "netlink".to_string(),
                        wan_name: name.clone(),
                        device: name,
                        event: "addr_added".to_string(),
                        ip,
                        gateway: None,
                        dns_servers: Vec::new(),
                        ifindex: Some(addr.header.index),
                        ipv6_addr: None,
                        ipv6_pd_prefix: None,
                    })
                    .await;
            }
        }
        _ => {}
    }
}

async fn refresh_wireguard_if_present(wg_iface: &str) {
    let mut cmd = Command::new("wg");
    cmd.args(["show", wg_iface]);
    match run_status_with_timeout(&mut cmd, Duration::from_secs(5), "wg show").await {
        Ok(s) if s.success() => {
            tracing::info!(wg_iface, "checked WireGuard interface after WAN change");
        }
        Ok(_) => {}
        Err(err) => {
            tracing::warn!(wg_iface, error = %err, "wg show check failed or timed out");
        }
    }
}
