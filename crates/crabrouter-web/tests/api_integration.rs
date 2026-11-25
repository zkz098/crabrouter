use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use crabrouter_core::{ConfigStore, RouterEngine};
use crabrouter_model::{DomainPolicyRule, RouterConfig};
use crabrouter_net::{
    DryRunNetworkBackend, EbpfEngine, WanStateObserver, spawn_multi_wan_engine,
    spawn_network_event_engine,
};
use crabrouter_services::{DdnsManager, DryRunSupervisor, TlsCertManager};
use crabrouter_web::{AppState, LogHub, build_router};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn setup_test_app() -> (Router, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "crabrouter-web-test-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("create temp test dir");
    let cfg_path = dir.join("router.toml");
    let store = ConfigStore::new(&cfg_path);
    let engine = RouterEngine::bootstrap(
        store,
        Arc::new(DryRunNetworkBackend),
        Arc::new(DryRunSupervisor),
    )
    .await
    .expect("bootstrap engine");

    let observer: Arc<dyn WanStateObserver> = Arc::clone(&engine) as Arc<dyn WanStateObserver>;
    let wan_bus = spawn_network_event_engine(Arc::clone(&observer), true);
    let multi_wan = spawn_multi_wan_engine(observer, wan_bus.clone(), true);
    let ebpf = EbpfEngine::new(true);

    let cfg_snapshot = engine.active_config();
    let ddns = DdnsManager::new(&cfg_snapshot, true);
    let tls = TlsCertManager::new(&cfg_snapshot, true);

    let app = build_router(AppState {
        engine,
        wan_bus,
        multi_wan,
        ebpf,
        ddns,
        tls,
        logs: LogHub::new(),
        lease_path: dir.join("dnsmasq.leases"),
        ota_dir: dir.join("ota"),
        sysupdate_dir: dir.join("sysupdate.d"),
        dry_run: true,
    });
    (app, dir)
}

async fn req_json(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(tok) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {tok}"));
    }
    let req = if let Some(b) = body {
        builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(b.to_string()))
            .expect("build json request")
    } else {
        builder.body(Body::empty()).expect("build empty request")
    };
    let resp = app.clone().oneshot(req).await.expect("oneshot response");
    let status = resp.status();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let json_val = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json_val)
}

#[tokio::test]
async fn setup_auth_commit_confirm_and_history_flow() {
    let (app, dir) = setup_test_app().await;

    let (st, health) = req_json(&app, "GET", "/api/health", None, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(health["status"], "ok");

    let (st, setup) = req_json(&app, "GET", "/api/setup/status", None, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(setup["initialized"], false);

    // Initialize admin account via setup/init
    let (_, cfg) = req_json(&app, "GET", "/api/config", None, None).await;
    let init_cfg: RouterConfig = serde_json::from_value(cfg).expect("parse config");
    let (st, init_resp) = req_json(
        &app,
        "POST",
        "/api/setup/init",
        None,
        Some(json!({
            "admin_username": "admin",
            "admin_password": "correct-horse-battery-staple",
            "hostname": "lab-router",
            "timezone": "Asia/Shanghai",
            "wan": init_cfg.wans[0],
            "lan": init_cfg.lans[0]
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let token = init_resp["token"].as_str().expect("token").to_string();

    // Protected endpoints require auth after init
    let (st, _) = req_json(&app, "GET", "/api/config", None, None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);

    let (st, _) = req_json(&app, "GET", "/api/config", Some(&token), None).await;
    assert_eq!(st, StatusCode::OK);

    // Login with password
    let (st, login_resp) = req_json(
        &app,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({
            "username": "admin",
            "password": "correct-horse-battery-staple"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let token2 = login_resp["token"].as_str().expect("login token");

    // Commit -> Rollback -> Commit -> Confirm
    let (_, cur_cfg_val) = req_json(&app, "GET", "/api/config", Some(token2), None).await;
    let mut next_cfg: RouterConfig = serde_json::from_value(cur_cfg_val).expect("parse cfg");
    next_cfg.system.hostname = "rolled-back-name".to_string();
    let (st, _) = req_json(
        &app,
        "POST",
        "/api/config/commit",
        Some(token2),
        Some(serde_json::to_value(&next_cfg).expect("serialize")),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    let (st, _) = req_json(&app, "POST", "/api/config/rollback", Some(token2), None).await;
    assert_eq!(st, StatusCode::OK);

    next_cfg.system.hostname = "confirmed-name".to_string();
    next_cfg.ebpf.enabled = true;
    next_cfg.ebpf.domain_rules.push(DomainPolicyRule {
        domain: "*.example.com".to_string(),
        wan_name: "wan".to_string(),
        tag_id: 1,
    });
    let (st, _) = req_json(
        &app,
        "POST",
        "/api/config/commit",
        Some(token2),
        Some(serde_json::to_value(&next_cfg).expect("serialize")),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    let (st, _) = req_json(&app, "POST", "/api/config/confirm", Some(token2), None).await;
    assert_eq!(st, StatusCode::OK);

    let (st, history) = req_json(&app, "GET", "/api/config/history", Some(token2), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_ne!(history.as_array().expect("array").len(), 0);

    let (st, _) = req_json(&app, "GET", "/api/config/preview", Some(token2), None).await;
    assert_eq!(st, StatusCode::OK);

    let (st, _) = req_json(&app, "POST", "/api/auth/logout", Some(token2), None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn multi_wan_ebpf_conntrack_and_status_endpoints() {
    let (app, dir) = setup_test_app().await;

    // Enable multi-wan and ebpf in config
    let (_, cur_cfg_val) = req_json(&app, "GET", "/api/config", None, None).await;
    let mut cfg: RouterConfig = serde_json::from_value(cur_cfg_val).expect("parse cfg");
    cfg.multi_wan.enabled = true;
    cfg.ebpf.enabled = true;
    cfg.ebpf.xdp_guard_enabled = true;
    cfg.ebpf.domain_rules.push(DomainPolicyRule {
        domain: "*.openai.com".to_string(),
        wan_name: "wan".to_string(),
        tag_id: 3,
    });
    cfg.ddns.enabled = true;
    cfg.ddns.domain = "gw.example.com".to_string();
    cfg.acme.enabled = true;
    cfg.acme.domain = "gw.example.com".to_string();
    cfg.acme.email = "ops@example.com".to_string();
    let (st, _) = req_json(
        &app,
        "POST",
        "/api/config/commit",
        None,
        Some(serde_json::to_value(&cfg).expect("serialize")),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (st, _) = req_json(&app, "POST", "/api/config/confirm", None, None).await;
    assert_eq!(st, StatusCode::OK);

    // Multi-WAN probe & status
    let (st, mw) = req_json(&app, "POST", "/api/wans/multi-wan/probe", None, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(mw["enabled"], true);

    let (st, _) = req_json(&app, "GET", "/api/wans/multi-wan/status", None, None).await;
    assert_eq!(st, StatusCode::OK);

    // eBPF DNS event ingestion & status
    let (st, ingest) = req_json(
        &app,
        "POST",
        "/api/ebpf/dns-event",
        None,
        Some(json!({
            "domain": "api.openai.com",
            "ips": ["104.18.7.192"],
            "ttl_secs": 300
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(ingest["inserted"], 1);

    let (st, ebpf_st) = req_json(&app, "GET", "/api/ebpf/status", None, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(ebpf_st["active_lpm_entries"], 1);

    // Conntrack, WireGuard keypair, Services, OTA, Logs, Static SPA
    for (method, path, body) in [
        ("GET", "/api/status", None),
        ("GET", "/api/services/status", None),
        ("GET", "/api/dns/status", None),
        ("GET", "/api/interfaces", None),
        ("GET", "/api/wans/status", None),
        ("GET", "/api/clients", None),
        ("GET", "/api/conntrack", None),
        ("GET", "/api/hardware/health", None),
        ("GET", "/api/diag/logs", None),
        ("GET", "/api/ota/status", None),
        ("GET", "/api/ddns/status", None),
        ("POST", "/api/ddns/sync", None),
        ("GET", "/api/tls/status", None),
        ("POST", "/api/tls/renew", None),
        ("POST", "/api/wireguard/keypair", None),
        (
            "POST",
            "/api/conntrack/kill",
            Some(json!({
                "family": "ipv4",
                "proto": "tcp",
                "src": "192.168.10.50",
                "dst": "1.1.1.1",
                "sport": 50000,
                "dport": 443
            })),
        ),
    ] {
        let (code, _) = req_json(&app, method, path, None, body).await;
        assert_eq!(code, StatusCode::OK, "failed endpoint {method} {path}");
    }

    let spa_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("spa");
    assert_eq!(spa_resp.status(), StatusCode::OK);

    let _ = std::fs::remove_dir_all(&dir);
}

async fn req_json_from_peer(
    app: &Router,
    method: &str,
    uri: &str,
    peer: SocketAddr,
    token: Option<&str>,
    body: Value,
) -> StatusCode {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(tok) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {tok}"));
    }
    let mut req = builder
        .body(Body::from(body.to_string()))
        .expect("build peer request");
    req.extensions_mut().insert(ConnectInfo(peer));
    app.clone()
        .oneshot(req)
        .await
        .expect("oneshot response")
        .status()
}

async fn init_test_admin(app: &Router) -> String {
    let (_, cfg) = req_json(app, "GET", "/api/config", None, None).await;
    let init_cfg: RouterConfig = serde_json::from_value(cfg).expect("parse config");
    let (st, init_resp) = req_json(
        app,
        "POST",
        "/api/setup/init",
        None,
        Some(json!({
            "admin_username": "admin",
            "admin_password": "correct-horse-battery-staple",
            "hostname": "lab-router",
            "timezone": "Asia/Shanghai",
            "wan": init_cfg.wans[0],
            "lan": init_cfg.lans[0]
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    init_resp["token"]
        .as_str()
        .expect("session token")
        .to_string()
}

#[tokio::test]
async fn internal_callbacks_require_loopback_or_auth() {
    let (app, dir) = setup_test_app().await;

    let wan_payload = json!({
        "source": "dhcpcd",
        "wan_name": "wan",
        "device": "eth0",
        "event": "BOUND",
        "ip": "203.0.113.10",
        "gateway": "203.0.113.1",
        "dns_servers": []
    });
    let dns_payload = json!({
        "domain": "api.openai.com",
        "ips": ["104.18.7.192"],
        "ttl_secs": 300
    });

    let lan_peer: SocketAddr = "192.168.10.50:54321".parse().expect("lan peer");
    let loopback_v4: SocketAddr = "127.0.0.1:54321".parse().expect("loopback v4");
    let loopback_v6: SocketAddr = "[::1]:54321".parse().expect("loopback v6");

    assert_eq!(
        req_json_from_peer(
            &app,
            "POST",
            "/api/net/wan-event",
            lan_peer,
            None,
            wan_payload.clone()
        )
        .await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        req_json_from_peer(
            &app,
            "POST",
            "/api/ebpf/dns-event",
            lan_peer,
            None,
            dns_payload.clone()
        )
        .await,
        StatusCode::UNAUTHORIZED
    );

    let token = init_test_admin(&app).await;

    assert_eq!(
        req_json_from_peer(
            &app,
            "POST",
            "/api/net/wan-event",
            loopback_v4,
            None,
            wan_payload.clone()
        )
        .await,
        StatusCode::ACCEPTED
    );
    assert_eq!(
        req_json_from_peer(
            &app,
            "POST",
            "/api/ebpf/dns-event",
            loopback_v6,
            None,
            dns_payload.clone()
        )
        .await,
        StatusCode::OK
    );

    assert_eq!(
        req_json_from_peer(
            &app,
            "POST",
            "/api/net/wan-event",
            lan_peer,
            Some(&token),
            wan_payload
        )
        .await,
        StatusCode::ACCEPTED
    );
    assert_eq!(
        req_json_from_peer(
            &app,
            "POST",
            "/api/ebpf/dns-event",
            lan_peer,
            Some(&token),
            dns_payload
        )
        .await,
        StatusCode::OK
    );

    let _ = std::fs::remove_dir_all(&dir);
}
