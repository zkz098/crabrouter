pub mod api;
pub mod crabdns_proxy;
pub mod logs;
pub mod metrics;
pub mod ota_api;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::DefaultBodyLimit;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use crabrouter_core::RouterEngine;
use crabrouter_net::WanEventBus;
use rust_embed::RustEmbed;
use tower_http::cors::CorsLayer;

pub use logs::{LogEvent, LogHub};

#[derive(RustEmbed, Clone)]
#[folder = "../../webui/dist"]
struct WebAssets;

#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<RouterEngine>,
    pub wan_bus: WanEventBus,
    pub logs: LogHub,
    pub lease_path: PathBuf,
    pub ota_dir: PathBuf,
    pub sysupdate_dir: PathBuf,
    pub dry_run: bool,
}

pub fn build_router(
    engine: Arc<RouterEngine>,
    wan_bus: WanEventBus,
    logs: LogHub,
    lease_path: PathBuf,
    ota_dir: PathBuf,
    dry_run: bool,
) -> Router {
    let state = AppState {
        engine,
        wan_bus,
        logs,
        lease_path,
        ota_dir,
        sysupdate_dir: PathBuf::from("/etc/sysupdate.d"),
        dry_run,
    };

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
        .route("/api/wans/status", get(api::wan_status))
        .route("/api/net/wan-event", post(api::ingest_wan_event))
        .route("/api/clients", get(api::online_clients))
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
        .route("/crabdns", any(crabdns_proxy::proxy_crabdns))
        .route("/crabdns/{*path}", any(crabdns_proxy::proxy_crabdns))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .fallback(static_handler)
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
