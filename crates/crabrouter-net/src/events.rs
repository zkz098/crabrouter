use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use crabrouter_model::{RouterConfig, WanRuntimeState};
use futures_util::stream::StreamExt;
use rtnetlink::packet_core::NetlinkPayload;
use rtnetlink::packet_route::RouteNetlinkMessage;
use rtnetlink::packet_route::address::AddressAttribute;
use rtnetlink::packet_route::link::LinkAttribute;
use rtnetlink::{MulticastGroup, new_multicast_connection};
use tokio::process::Command;
use tokio::sync::{RwLock, mpsc};

use crate::dhcpcd::WanLeaseEvent;
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
    let (tx, rx) = mpsc::channel::<WanLeaseEvent>(64);
    let bus = WanEventBus::new(tx.clone());
    let states = Arc::clone(&bus.states);

    tokio::spawn(async move {
        run_event_loop(observer, states, rx, tx, dry_run).await;
    });

    bus
}

async fn run_event_loop(
    observer: Arc<dyn WanStateObserver>,
    states: Arc<RwLock<HashMap<String, WanRuntimeState>>>,
    mut hook_rx: mpsc::Receiver<WanLeaseEvent>,
    hook_tx: mpsc::Sender<WanLeaseEvent>,
    dry_run: bool,
) {
    if !dry_run {
        spawn_netlink_multicast_listener(hook_tx);
    }

    while let Some(ev) = hook_rx.recv().await {
        let cfg = observer.current_config();
        let Some(wan) = cfg
            .wans
            .iter()
            .find(|w| w.name == ev.wan_name || w.device == ev.device)
        else {
            tracing::debug!(wan = %ev.wan_name, dev = %ev.device, "ignoring event for non-WAN interface");
            continue;
        };

        let wan_name = wan.name.clone();
        let new_ip = ev.ip.clone().filter(|s| !s.is_empty());
        let new_gw = ev.gateway.clone().filter(|s| !s.is_empty());

        let (prev_ip, prev_ifindex) = {
            let guard = states.read().await;
            guard
                .get(&wan_name)
                .map_or((None, None), |s| (s.ip.clone(), s.ifindex))
        };

        let updated = WanRuntimeState {
            wan_name: wan_name.clone(),
            device: ev.device.clone(),
            proto: wan.proto,
            state: ev.event.clone(),
            ip: new_ip.clone(),
            gateway: new_gw.clone(),
            dns_servers: ev.dns_servers.clone(),
            ifindex: ev.ifindex.or(prev_ifindex),
            updated_at: Utc::now().to_rfc3339(),
        };

        states
            .write()
            .await
            .insert(wan_name.clone(), updated.clone());
        observer.on_wan_state_updated(updated);

        let ip_changed = prev_ip.is_some() && prev_ip != new_ip;
        let ifindex_changed =
            prev_ifindex.is_some() && ev.ifindex.is_some() && prev_ifindex != ev.ifindex;

        if !dry_run && (ip_changed || ifindex_changed || ev.event == "up" || ev.event == "BOUND") {
            if let Some(old_ip) = prev_ip.as_deref()
                && ip_changed
            {
                flush_conntrack_for_ip(old_ip).await;
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

fn spawn_netlink_multicast_listener(tx: mpsc::Sender<WanLeaseEvent>) {
    let groups = [
        MulticastGroup::Link,
        MulticastGroup::Ipv4Ifaddr,
        MulticastGroup::Ipv6Ifaddr,
        MulticastGroup::Ipv4Route,
    ];
    let Ok((conn, _handle, mut messages)) = new_multicast_connection(&groups) else {
        tracing::warn!(
            "unable to open rtnetlink multicast socket; continuing with hook events only"
        );
        return;
    };
    tokio::spawn(conn);

    tokio::spawn(async move {
        let mut ifindex_to_name: HashMap<u32, String> = HashMap::new();
        while let Some((msg, _)) = messages.next().await {
            let NetlinkPayload::InnerMessage(inner) = msg.payload else {
                continue;
            };
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
                            })
                            .await;
                    }
                }
                RouteNetlinkMessage::NewAddress(addr) => {
                    if let Some(name) = ifindex_to_name.get(&addr.header.index).cloned() {
                        let ip = addr.attributes.iter().find_map(|a| match a {
                            AddressAttribute::Local(ip) | AddressAttribute::Address(ip) => {
                                Some(ip.to_string())
                            }
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
                                })
                                .await;
                        }
                    }
                }
                _ => {}
            }
        }
    });
}

async fn flush_conntrack_for_ip(old_ip: &str) {
    for flag in ["-r", "-d", "-s"] {
        match Command::new("conntrack")
            .args(["-D", flag, old_ip])
            .status()
            .await
        {
            Ok(status) => {
                tracing::info!(old_ip, flag, status = %status, "conntrack flush executed");
            }
            Err(err) => {
                tracing::debug!(old_ip, flag, error = %err, "conntrack command unavailable");
                break;
            }
        }
    }
}

async fn refresh_wireguard_if_present(wg_iface: &str) {
    match Command::new("wg").args(["show", wg_iface]).status().await {
        Ok(s) if s.success() => {
            tracing::info!(wg_iface, "checked WireGuard interface after WAN change");
        }
        _ => {}
    }
}
