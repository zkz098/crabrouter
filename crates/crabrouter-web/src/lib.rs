pub mod api;
pub mod containers_api;
pub mod crabdns_proxy;
pub mod crowdsec_api;
pub mod ddns_tls_api;
pub mod extensions_api;
pub mod logs;
pub mod metrics;
pub mod nic_tuning_api;
pub mod ota_api;
pub mod rate_limit;
pub mod terminal;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::DefaultBodyLimit;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, delete, get, post};
use crabrouter_core::RouterEngine;
use crabrouter_net::{EbpfEngine, MultiWanManager, WanEventBus};
use crabrouter_services::{ContainerBackend, CrowdSecManager, DdnsManager, TlsCertManager};
use rust_embed::RustEmbed;
use tower_http::cors::CorsLayer;

pub use crabdns_proxy::{spawn_crabdns_ebpf_bridge, spawn_crabdns_ebpf_bridge_with_shutdown};
pub use logs::{LogEvent, LogHub};

#[derive(RustEmbed, Clone)]
#[folder = "../../webui/dist"]
struct WebAssets;

#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<RouterEngine>,
    pub wan_bus: WanEventBus,
    pub multi_wan: MultiWanManager,
    pub ebpf: EbpfEngine,
    pub ddns: DdnsManager,
    pub tls: TlsCertManager,
    pub crowdsec: CrowdSecManager,
    pub containers: Arc<dyn ContainerBackend>,
    pub logs: LogHub,
    pub lease_path: PathBuf,
    pub ota_dir: PathBuf,
    pub sysupdate_dir: PathBuf,
    pub sysext_dir: PathBuf,
    pub dry_run: bool,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(api::health))
        .route("/api/setup/status", get(api::setup_status_with_headers))
        .route("/api/setup/init", post(api::setup_init))
        .route("/api/auth/login", post(api::auth_login))
        .route("/api/auth/logout", post(api::auth_logout))
        .route("/api/status", get(api::status))
        .route("/api/services/status", get(api::service_statuses))
        .route("/api/dns/status", get(crabdns_proxy::dns_aggregate_status))
        .route("/api/interfaces", get(api::interfaces))
        .route(
            "/api/interfaces/{name}/capabilities",
            get(nic_tuning_api::interface_capabilities),
        )
        .route("/api/wans/status", get(api::wan_status))
        .route("/api/wans/{name}/redial", post(api::redial_wan))
        .route("/api/wans/multi-wan/status", get(api::multi_wan_status))
        .route("/api/wans/multi-wan/probe", post(api::multi_wan_probe_now))
        .route("/api/ebpf/status", get(api::ebpf_status))
        .route("/api/ebpf/dns-event", post(api::ingest_ebpf_dns_event))
        .route("/api/ddns/status", get(ddns_tls_api::ddns_status))
        .route("/api/ddns/sync", post(ddns_tls_api::ddns_sync_now))
        .route("/api/tls/status", get(ddns_tls_api::tls_status))
        .route("/api/tls/renew", post(ddns_tls_api::tls_renew_now))
        .route("/api/crowdsec/status", get(crowdsec_api::crowdsec_status))
        .route("/api/crowdsec/sync", post(crowdsec_api::crowdsec_sync_now))
        .route("/api/net/wan-event", post(api::ingest_wan_event))
        .route("/api/clients", get(api::online_clients))
        .route("/api/wol", post(api::send_wol))
        .route("/api/upnp/leases", get(api::upnp_leases))
        .route("/api/conntrack", get(api::list_conntrack))
        .route("/api/conntrack/kill", post(api::kill_conntrack))
        .route("/api/config", get(api::get_config))
        .route("/api/config/preview", get(api::preview_rendered))
        .route("/api/config/commit", post(api::begin_commit))
        .route("/api/config/confirm", post(api::confirm_commit))
        .route("/api/config/rollback", post(api::rollback_commit))
        .route("/api/config/history", get(api::config_history))
        .route(
            "/api/config/history/{generation}/restore",
            post(api::restore_history),
        )
        .route(
            "/api/wireguard/keypair",
            post(api::generate_wireguard_keypair),
        )
        .route("/api/diag/ping", post(api::diag_ping))
        .route("/api/diag/traceroute", post(api::diag_traceroute))
        .route("/api/diag/iperf3", post(api::diag_iperf3))
        .route("/api/diag/logs", get(api::diag_logs_snapshot))
        .route("/api/diag/logs/ws", get(api::diag_logs_ws))
        .route("/api/hardware/health", get(api::hardware_health))
        .route("/api/terminal/ws", get(terminal::terminal_ws))
        .route("/api/metrics/ws", get(metrics::metrics_ws))
        .route("/api/ota/status", get(ota_api::ota_status))
        .route("/api/ota/check", post(ota_api::ota_check))
        .route(
            "/api/ota/upload",
            post(ota_api::ota_upload).layer(DefaultBodyLimit::disable()),
        )
        .route("/api/ota/apply", post(ota_api::ota_apply))
        .route("/api/ota/bless", post(ota_api::ota_bless))
        .route("/api/ota/reboot", post(ota_api::ota_reboot))
        .merge(extension_routes())
        .merge(container_routes())
        .route("/crabdns", any(crabdns_proxy::proxy_crabdns))
        .route("/crabdns/{*path}", any(crabdns_proxy::proxy_crabdns))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .fallback(static_handler)
}

fn extension_routes() -> Router<AppState> {
    Router::new()
        .route("/api/apps/catalog", get(extensions_api::apps_catalog))
        .route("/api/apps/installed", get(extensions_api::apps_installed))
        .route("/api/apps/install", post(extensions_api::apps_install))
        .route("/api/apps/{id}/start", post(extensions_api::apps_start))
        .route("/api/apps/{id}/stop", post(extensions_api::apps_stop))
        .route("/api/apps/{id}", delete(extensions_api::apps_uninstall))
        .route("/api/sysext/status", get(extensions_api::sysext_status))
        .route(
            "/api/sysext/upload",
            post(extensions_api::sysext_upload).layer(DefaultBodyLimit::disable()),
        )
        .route("/api/sysext/refresh", post(extensions_api::sysext_refresh))
        .route("/api/sysext/unmerge", post(extensions_api::sysext_unmerge))
        .route("/api/sysext/{name}", delete(extensions_api::sysext_delete))
}

fn container_routes() -> Router<AppState> {
    Router::new()
        .route("/api/containers/info", get(containers_api::containers_info))
        .route(
            "/api/containers",
            get(containers_api::list_containers).post(containers_api::create_container),
        )
        .route("/api/containers/images", get(containers_api::list_images))
        .route(
            "/api/containers/images/pull",
            post(containers_api::pull_image),
        )
        .route(
            "/api/containers/images/{id}",
            delete(containers_api::remove_image),
        )
        .route(
            "/api/containers/networks",
            get(containers_api::list_networks).post(containers_api::create_network),
        )
        .route(
            "/api/containers/networks/{id}",
            delete(containers_api::remove_network),
        )
        .route("/api/containers/volumes", get(containers_api::list_volumes))
        .route(
            "/api/containers/volumes/{name}",
            delete(containers_api::remove_volume),
        )
        .route(
            "/api/containers/{id}",
            get(containers_api::inspect_container).delete(containers_api::remove_container),
        )
        .route(
            "/api/containers/{id}/start",
            post(containers_api::start_container),
        )
        .route(
            "/api/containers/{id}/stop",
            post(containers_api::stop_container),
        )
        .route(
            "/api/containers/{id}/restart",
            post(containers_api::restart_container),
        )
        .route(
            "/api/containers/{id}/logs",
            get(containers_api::container_logs),
        )
        .route(
            "/api/containers/{id}/logs/ws",
            get(containers_api::container_logs_ws),
        )
}

async fn static_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    if let Some(content) = WebAssets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return Response::builder()
            .header(header::CONTENT_TYPE, mime.as_ref())
            .body(Body::from(content.data.into_owned()))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    if let Some(index) = WebAssets::get("index.html") {
        return Response::builder()
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(index.data.into_owned()))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    StatusCode::NOT_FOUND.into_response()
}
