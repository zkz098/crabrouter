use std::path::Path;
use std::time::Duration;

use axum::Json;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path as AxumPath, State, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use crabrouter_core::{CommitStatus, ConfigSnapshotSummary, CoreError};
use crabrouter_model::{
    ConntrackEntry, ConntrackKillRequest, DhcpLeaseEntry, EbpfRuntimeStatus,
    HardwareHealthSnapshot, HostInterfaceSnapshot, LanConfig, MultiWanRuntimeStatus, RouterConfig,
    WanConfig, WanRuntimeState,
};
use crabrouter_net::{
    WanLeaseEvent, collect_hardware_health, kill_conntrack_flow, read_conntrack_table,
    read_host_interfaces, render_dhcpcd_conf, render_nftables_ruleset, render_pppd_peer,
    render_systemd_link_files, render_tc_qos_script, render_udev_net_rules,
};
use crabrouter_services::{
    SystemdUnitStatus, generate_wireguard_keypair as svc_gen_wg_keypair, read_online_clients,
    render_crabdns_toml, render_dnsmasq_conf, render_wg_quick_conf,
};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use ts_rs::TS;

use crate::AppState;
use crate::logs::LogEvent;

#[derive(Serialize, TS)]
#[ts(export)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct RenderedFilePreview {
    pub name: String,
    pub content: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct RenderedConfigsPreview {
    pub nftables: String,
    pub tc_qos: String,
    pub dnsmasq: String,
    pub crabdns: String,
    pub dhcpcd: String,
    pub udev_rules: String,
    pub wg_quick: String,
    pub systemd_links: Vec<RenderedFilePreview>,
    pub pppd_peers: Vec<RenderedFilePreview>,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct SetupStatusResponse {
    pub initialized: bool,
    pub authenticated: bool,
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct SetupInitRequest {
    pub admin_username: String,
    pub admin_password: String,
    pub hostname: String,
    pub timezone: String,
    pub wan: WanConfig,
    pub lan: LanConfig,
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct AuthTokenResponse {
    pub token: String,
    pub username: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct WireGuardKeypairResponse {
    pub private_key: String,
    pub public_key: String,
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct PingRequest {
    pub host: String,
    pub count: Option<u8>,
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct TracerouteRequest {
    pub host: String,
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct Iperf3Request {
    pub server: String,
    pub port: Option<u16>,
    pub duration_secs: Option<u8>,
    pub reverse: Option<bool>,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct DiagCommandResult {
    pub ok: bool,
    pub command: String,
    pub exit_code: Option<i32>,
    pub output: String,
}

#[derive(Serialize)]
pub struct ErrorBody {
    error: String,
}

pub struct ApiError(pub CoreError);

impl From<CoreError> for ApiError {
    fn from(err: CoreError) -> Self {
        Self(err)
    }
}

impl From<crabrouter_services::ServiceError> for ApiError {
    fn from(err: crabrouter_services::ServiceError) -> Self {
        Self(CoreError::Service(err))
    }
}

impl From<crabrouter_net::NetError> for ApiError {
    fn from(err: crabrouter_net::NetError) -> Self {
        Self(CoreError::Net(err))
    }
}

impl From<crabrouter_model::ModelError> for ApiError {
    fn from(err: crabrouter_model::ModelError) -> Self {
        Self(CoreError::Model(err))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let code = match &self.0 {
            CoreError::Model(_) | CoreError::Transaction(_) => StatusCode::BAD_REQUEST,
            CoreError::Auth(_) => StatusCode::UNAUTHORIZED,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            code,
            Json(ErrorBody {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}

fn extract_token(headers: &HeaderMap) -> Option<String> {
    if let Some(auth) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        && let Some(token) = auth.strip_prefix("Bearer ")
    {
        return Some(token.trim().to_string());
    }
    if let Some(cookie_hdr) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        for part in cookie_hdr.split(';') {
            let trimmed = part.trim();
            if let Some(val) = trimmed.strip_prefix("crabrouter_session=") {
                return Some(val.to_string());
            }
        }
    }
    None
}

pub fn require_auth_if_initialized(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let initialized = state.engine.db().is_initialized()?;
    if !initialized {
        return Ok(());
    }
    let Some(token) = extract_token(headers) else {
        return Err(ApiError(CoreError::Auth(
            "authentication required".to_string(),
        )));
    };
    if state.engine.db().verify_session_token(&token)? {
        Ok(())
    } else {
        Err(ApiError(CoreError::Auth(
            "invalid or expired session".to_string(),
        )))
    }
}

fn session_cookie_header(token: &str) -> String {
    format!("crabrouter_session={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=604800")
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

pub async fn setup_status_with_headers(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SetupStatusResponse>, ApiError> {
    let initialized = state.engine.db().is_initialized()?;
    let authenticated = if initialized {
        match extract_token(&headers) {
            Some(tok) => state.engine.db().verify_session_token(&tok)?,
            None => false,
        }
    } else {
        true
    };
    Ok(Json(SetupStatusResponse {
        initialized,
        authenticated,
    }))
}

pub async fn setup_init(
    State(state): State<AppState>,
    Json(req): Json<SetupInitRequest>,
) -> Result<Response, ApiError> {
    if state.engine.db().is_initialized()? {
        return Err(ApiError(CoreError::Transaction(
            "router is already initialized".to_string(),
        )));
    }
    let mut cfg = (*state.engine.active_config()).clone();
    cfg.system.hostname = req.hostname;
    cfg.system.timezone = req.timezone;
    cfg.wans = vec![req.wan];
    cfg.lans = vec![req.lan];
    cfg.validate()?;

    state
        .engine
        .db()
        .set_admin_credentials(&req.admin_username, &req.admin_password)?;

    state.engine.begin_commit(cfg).await?;
    state.engine.confirm_commit().await?;

    let token = state.engine.db().issue_session_token(&req.admin_username)?;
    let body = AuthTokenResponse {
        token: token.clone(),
        username: req.admin_username,
    };
    let mut resp = Json(body).into_response();
    if let Ok(cookie_val) = session_cookie_header(&token).parse() {
        resp.headers_mut().insert(header::SET_COOKIE, cookie_val);
    }
    Ok(resp)
}

pub async fn auth_login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    let token = state
        .engine
        .db()
        .verify_login_and_issue_token(&req.username, &req.password)?;
    let body = AuthTokenResponse {
        token: token.clone(),
        username: req.username,
    };
    let mut resp = Json(body).into_response();
    if let Ok(cookie_val) = session_cookie_header(&token).parse() {
        resp.headers_mut().insert(header::SET_COOKIE, cookie_val);
    }
    Ok(resp)
}

pub async fn auth_logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(tok) = extract_token(&headers) {
        let _ = state.engine.db().revoke_session_token(&tok);
    }
    let mut resp = StatusCode::NO_CONTENT.into_response();
    if let Ok(cookie_val) = "crabrouter_session=; Path=/; HttpOnly; Max-Age=0".parse() {
        resp.headers_mut().insert(header::SET_COOKIE, cookie_val);
    }
    Ok(resp)
}

pub async fn status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CommitStatus>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    Ok(Json(state.engine.status().await))
}

pub async fn interfaces() -> Result<Json<Vec<HostInterfaceSnapshot>>, (StatusCode, Json<ErrorBody>)>
{
    tokio::task::spawn_blocking(read_host_interfaces)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorBody {
                    error: e.to_string(),
                }),
            )
        })?
        .map(Json)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorBody {
                    error: e.to_string(),
                }),
            )
        })
}

pub async fn wan_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<WanRuntimeState>>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let mut live = state.wan_bus.snapshot().await;
    if live.is_empty() {
        live = state.engine.db().list_wan_states()?;
    }
    Ok(Json(live))
}

pub async fn multi_wan_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MultiWanRuntimeStatus>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    Ok(Json(state.multi_wan.snapshot().await))
}

pub async fn multi_wan_probe_now(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MultiWanRuntimeStatus>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    Ok(Json(state.multi_wan.run_probe_cycle().await))
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct EbpfDnsEventRequest {
    pub domain: String,
    pub ips: Vec<String>,
    #[serde(default = "default_dns_ttl")]
    #[ts(type = "number")]
    pub ttl_secs: u64,
}

const fn default_dns_ttl() -> u64 {
    300
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct EbpfDnsIngestResponse {
    pub inserted: u32,
}

pub async fn ebpf_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<EbpfRuntimeStatus>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = state.engine.active_config();
    Ok(Json(state.ebpf.snapshot(&cfg).await))
}

pub async fn ingest_ebpf_dns_event(
    State(state): State<AppState>,
    Json(req): Json<EbpfDnsEventRequest>,
) -> Result<Json<EbpfDnsIngestResponse>, ApiError> {
    let cfg = state.engine.active_config();
    let inserted = state
        .ebpf
        .ingest_dns_resolution(&cfg, &req.domain, &req.ips, req.ttl_secs)
        .await?;
    Ok(Json(EbpfDnsIngestResponse { inserted }))
}

pub async fn ingest_wan_event(
    State(state): State<AppState>,
    Json(event): Json<WanLeaseEvent>,
) -> StatusCode {
    state.wan_bus.emit(event).await;
    StatusCode::ACCEPTED
}

pub async fn get_config(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<RouterConfig>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    Ok(Json((*state.engine.active_config()).clone()))
}

pub async fn preview_rendered(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<RenderedConfigsPreview>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = state.engine.active_config();
    let systemd_links = render_systemd_link_files(&cfg.interface_pins)
        .into_iter()
        .map(|(name, content)| RenderedFilePreview { name, content })
        .collect();
    let pppd_peers = cfg
        .wans
        .iter()
        .filter(|w| w.proto == crabrouter_model::WanProto::Pppoe)
        .map(|w| RenderedFilePreview {
            name: format!("crabrouter-{}", w.name),
            content: render_pppd_peer(w),
        })
        .collect();

    let wg_quick = render_wg_quick_conf(
        &cfg,
        cfg.wireguard
            .private_key
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("<SERVER_PRIVATE_KEY>"),
    )?;

    Ok(Json(RenderedConfigsPreview {
        nftables: render_nftables_ruleset(&cfg),
        tc_qos: render_tc_qos_script(&cfg),
        dnsmasq: render_dnsmasq_conf(&cfg)?,
        crabdns: render_crabdns_toml(&cfg)?,
        dhcpcd: render_dhcpcd_conf(&cfg),
        udev_rules: render_udev_net_rules(&cfg.interface_pins),
        wg_quick,
        systemd_links,
        pppd_peers,
    }))
}

pub async fn begin_commit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(next): Json<RouterConfig>,
) -> Result<Json<CommitStatus>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let status = state.engine.begin_commit(next).await?;
    let cfg = state.engine.active_config();
    if let Err(err) = state.ebpf.sync_config(&cfg).await {
        tracing::warn!(error = %err, "eBPF sync after commit failed");
    }
    Ok(Json(status))
}

pub async fn confirm_commit(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CommitStatus>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let status = state.engine.confirm_commit().await?;
    Ok(Json(status))
}

pub async fn rollback_commit(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CommitStatus>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let status = state.engine.rollback_pending().await?;
    let cfg = state.engine.active_config();
    if let Err(err) = state.ebpf.sync_config(&cfg).await {
        tracing::warn!(error = %err, "eBPF sync after rollback failed");
    }
    Ok(Json(status))
}

pub async fn config_history(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ConfigSnapshotSummary>>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    Ok(Json(state.engine.list_history()?))
}

pub async fn restore_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(generation): AxumPath<u64>,
) -> Result<Json<CommitStatus>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    Ok(Json(state.engine.restore_snapshot(generation).await?))
}

pub async fn online_clients(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<DhcpLeaseEntry>>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = state.engine.active_config();
    let lease_path = state.lease_path.clone();
    let clients = tokio::task::spawn_blocking(move || {
        read_online_clients(&lease_path, Path::new("/proc/net/arp"), &cfg)
    })
    .await
    .map_err(|e| CoreError::Transaction(e.to_string()))?;
    Ok(Json(clients))
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct ConntrackKillResponse {
    pub deleted: u32,
}

pub async fn list_conntrack(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ConntrackEntry>>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let entries = read_conntrack_table(state.dry_run).await?;
    Ok(Json(entries))
}

pub async fn kill_conntrack(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ConntrackKillRequest>,
) -> Result<Json<ConntrackKillResponse>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let deleted = kill_conntrack_flow(&req, state.dry_run).await?;
    Ok(Json(ConntrackKillResponse { deleted }))
}

pub async fn hardware_health(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<HardwareHealthSnapshot>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let snap = collect_hardware_health(state.dry_run).await?;
    Ok(Json(snap))
}

pub async fn generate_wireguard_keypair(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<WireGuardKeypairResponse>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let pair = svc_gen_wg_keypair();
    Ok(Json(WireGuardKeypairResponse {
        private_key: pair.private_key,
        public_key: pair.public_key,
    }))
}

pub async fn service_statuses(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SystemdUnitStatus>>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    Ok(Json(state.engine.service_statuses().await?))
}

pub async fn diag_ping(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PingRequest>,
) -> Result<Json<DiagCommandResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let host = validate_diag_host(&req.host)?;
    let count = req.count.unwrap_or(4).clamp(1, 10);
    let count_str = count.to_string();
    let cmd_str = format!("ping -c {count_str} -W 2 {host}");
    let res = run_diag_command("ping", &["-c", &count_str, "-W", "2", &host], &cmd_str).await;
    Ok(Json(res))
}

pub async fn diag_traceroute(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<TracerouteRequest>,
) -> Result<Json<DiagCommandResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let host = validate_diag_host(&req.host)?;
    let cmd_str = format!("traceroute -n -w 2 -m 15 {host}");
    let first = run_diag_command(
        "traceroute",
        &["-n", "-w", "2", "-m", "15", &host],
        &cmd_str,
    )
    .await;
    if first.exit_code.is_none() && first.output.contains("No such file") {
        let fallback_cmd = format!("tracepath -n {host}");
        return Ok(Json(
            run_diag_command("tracepath", &["-n", &host], &fallback_cmd).await,
        ));
    }
    Ok(Json(first))
}

pub async fn diag_iperf3(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<Iperf3Request>,
) -> Result<Json<DiagCommandResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let server = validate_diag_host(&req.server)?;
    let port = req.port.unwrap_or(5201);
    let duration = req.duration_secs.unwrap_or(5).clamp(1, 30);
    let port_s = port.to_string();
    let dur_s = duration.to_string();
    let mut args = vec![
        "-c",
        server.as_str(),
        "-p",
        port_s.as_str(),
        "-t",
        dur_s.as_str(),
    ];
    if req.reverse.unwrap_or(false) {
        args.push("-R");
    }
    let cmd_str = format!("iperf3 {}", args.join(" "));
    let res = run_diag_command("iperf3", &args, &cmd_str).await;
    Ok(Json(res))
}

pub async fn diag_logs_snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<LogEvent>>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    Ok(Json(state.logs.snapshot()))
}

pub async fn diag_logs_ws(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    Ok(ws.on_upgrade(move |socket| handle_log_ws(socket, state)))
}

async fn handle_log_ws(mut socket: WebSocket, state: AppState) {
    for entry in state.logs.snapshot() {
        if let Ok(json) = serde_json::to_string(&entry)
            && socket.send(Message::Text(json.into())).await.is_err()
        {
            return;
        }
    }

    let mut rx = state.logs.subscribe();
    loop {
        tokio::select! {
            recv = rx.recv() => {
                if !forward_log_broadcast(&mut socket, recv).await {
                    break;
                }
            }
            msg = socket.recv() => {
                if msg.is_none() || matches!(msg, Some(Ok(Message::Close(_)) | Err(_))) {
                    break;
                }
            }
        }
    }
}

async fn forward_log_broadcast(
    socket: &mut WebSocket,
    recv: Result<LogEvent, tokio::sync::broadcast::error::RecvError>,
) -> bool {
    match recv {
        Ok(entry) => {
            if let Ok(json) = serde_json::to_string(&entry)
                && socket.send(Message::Text(json.into())).await.is_err()
            {
                return false;
            }
            true
        }
        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => true,
        Err(tokio::sync::broadcast::error::RecvError::Closed) => false,
    }
}

fn validate_diag_host(raw: &str) -> Result<String, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.len() > 253
        || trimmed.starts_with('-')
        || !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
    {
        return Err(ApiError(CoreError::Transaction(format!(
            "invalid diagnostic target host '{trimmed}'"
        ))));
    }
    Ok(trimmed.to_string())
}

async fn run_diag_command(bin: &str, args: &[&str], display_cmd: &str) -> DiagCommandResult {
    let fut = Command::new(bin).args(args).output();
    match tokio::time::timeout(Duration::from_secs(35), fut).await {
        Ok(Ok(out)) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
                (false, false) => format!("{stdout}\n{stderr}"),
                (false, true) => stdout.into_owned(),
                (true, false) => stderr.into_owned(),
                (true, true) => "(no output)".to_string(),
            };
            DiagCommandResult {
                ok: out.status.success(),
                command: display_cmd.to_string(),
                exit_code: out.status.code(),
                output: combined,
            }
        }
        Ok(Err(err)) => DiagCommandResult {
            ok: false,
            command: display_cmd.to_string(),
            exit_code: None,
            output: format!("failed to execute {bin}: {err}"),
        },
        Err(_) => DiagCommandResult {
            ok: false,
            command: display_cmd.to_string(),
            exit_code: None,
            output: format!("command timed out after 35s: {display_cmd}"),
        },
    }
}
