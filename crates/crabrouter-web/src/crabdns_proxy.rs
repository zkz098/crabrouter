use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use crabrouter_core::RouterEngine;
use crabrouter_model::RouterConfig;
use crabrouter_net::EbpfEngine;
use http_body_util::BodyExt;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use serde::Serialize;
use ts_rs::TS;

use crate::AppState;
use crate::api::{ApiError, require_auth_if_initialized};

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct CrabDnsAggregateStatus {
    pub reachable: bool,
    pub web_port: u16,
    #[ts(type = "Record<string, unknown> | null")]
    pub health: Option<serde_json::Value>,
    #[ts(type = "Record<string, unknown> | null")]
    pub stats: Option<serde_json::Value>,
    pub error: Option<String>,
}

fn http_client() -> Client<HttpConnector, Body> {
    Client::builder(TokioExecutor::new()).build_http()
}

pub async fn dns_aggregate_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CrabDnsAggregateStatus>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let port = state.engine.active_config().dns.crabdns_web_port;
    let client = http_client();

    let health_fut = fetch_local_json(&client, port, "/api/health");
    let stats_fut = fetch_local_json(&client, port, "/api/stats");
    let (health_res, stats_res) = tokio::join!(health_fut, stats_fut);

    match (health_res, stats_res) {
        (Ok(h), Ok(s)) => Ok(Json(CrabDnsAggregateStatus {
            reachable: true,
            web_port: port,
            health: Some(h),
            stats: Some(s),
            error: None,
        })),
        (Ok(h), Err(err)) => Ok(Json(CrabDnsAggregateStatus {
            reachable: true,
            web_port: port,
            health: Some(h),
            stats: None,
            error: Some(err),
        })),
        (Err(err), _) => Ok(Json(CrabDnsAggregateStatus {
            reachable: false,
            web_port: port,
            health: None,
            stats: None,
            error: Some(err),
        })),
    }
}

async fn fetch_local_json(
    client: &Client<HttpConnector, Body>,
    port: u16,
    path: &str,
) -> Result<serde_json::Value, String> {
    let uri: Uri = format!("http://127.0.0.1:{port}{path}")
        .parse()
        .map_err(|e| format!("invalid crabdns URI: {e}"))?;
    let resp = tokio::time::timeout(Duration::from_secs(2), client.get(uri))
        .await
        .map_err(|_| "crabdns status request timed out".to_string())?
        .map_err(|e| format!("crabdns unreachable: {e}"))?;

    let status = resp.status();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .map_err(|e| format!("read crabdns body failed: {e}"))?
        .to_bytes();

    if !status.is_success() {
        return Err(format!("crabdns returned HTTP {status}"));
    }

    serde_json::from_slice(&bytes).map_err(|e| format!("invalid crabdns JSON: {e}"))
}

/// Reverse-proxies `/crabdns` and `/crabdns/*` to `http://127.0.0.1:<crabdns_web_port>` so users
/// only need to access `CrabRouter`'s management port.
pub async fn proxy_crabdns(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut req: Request,
) -> Result<Response, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let port = state.engine.active_config().dns.crabdns_web_port;

    let orig_path_and_query = req
        .uri()
        .path_and_query()
        .map_or("/", hyper::http::uri::PathAndQuery::as_str);
    let stripped = orig_path_and_query
        .strip_prefix("/crabdns")
        .filter(|s| !s.is_empty())
        .unwrap_or("/");
    let target = format!("http://127.0.0.1:{port}{stripped}");
    let target_uri: Uri = target.parse().map_err(|_| {
        ApiError(crabrouter_core::CoreError::Transaction(
            "invalid proxy target URI".to_string(),
        ))
    })?;

    *req.uri_mut() = target_uri;
    req.headers_mut().remove(hyper::header::HOST);

    let client = http_client();
    match tokio::time::timeout(Duration::from_secs(15), client.request(req)).await {
        Ok(Ok(upstream)) => {
            let (parts, body) = upstream.into_parts();
            Ok(Response::from_parts(parts, Body::new(body)))
        }
        Ok(Err(err)) => Ok((
            StatusCode::BAD_GATEWAY,
            format!("crabdns proxy error (127.0.0.1:{port}): {err}"),
        )
            .into_response()),
        Err(_) => Ok((StatusCode::GATEWAY_TIMEOUT, "crabdns proxy timed out").into_response()),
    }
}

/// Spawns a background bridge that syncs `crabdns` resolved A/AAAA query logs and exact
/// configured domain rules into `EbpfEngine`'s `DOMAIN_POLICY_LPM` map.
pub fn spawn_crabdns_ebpf_bridge(engine: Arc<RouterEngine>, ebpf: EbpfEngine, dry_run: bool) {
    tokio::spawn(async move {
        let client = http_client();
        let mut seen: HashSet<(String, Vec<String>)> = HashSet::new();
        let mut tick: u64 = 0;

        loop {
            let cfg = engine.active_config();
            if cfg.ebpf.enabled && !cfg.ebpf.domain_rules.is_empty() && !dry_run {
                if let Ok(json_val) =
                    fetch_local_json(&client, cfg.dns.crabdns_web_port, "/api/logs?limit=200").await
                {
                    let _ = ingest_crabdns_logs_json(&ebpf, &cfg, &json_val, &mut seen).await;
                }
                if tick.is_multiple_of(20) {
                    prewarm_exact_domain_rules(&ebpf, &cfg).await;
                }
            }
            if seen.len() > 4096 {
                seen.clear();
            }
            tick = tick.saturating_add(1);
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });
}

/// Parses `crabdns` `/api/logs` JSON payload and feeds matching `(domain, answers)` into eBPF LPM.
pub async fn ingest_crabdns_logs_json<S: std::hash::BuildHasher>(
    ebpf: &EbpfEngine,
    cfg: &RouterConfig,
    payload: &serde_json::Value,
    seen: &mut HashSet<(String, Vec<String>), S>,
) -> u32 {
    let Some(items) = payload
        .as_array()
        .or_else(|| payload.get("logs").and_then(|v| v.as_array()))
    else {
        return 0;
    };

    let mut total_inserted = 0u32;
    for item in items {
        let Some(domain) = item.get("domain").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(ans_arr) = item.get("answers").and_then(|v| v.as_array()) else {
            continue;
        };
        let ips: Vec<String> = ans_arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        if ips.is_empty() || !seen.insert((domain.to_string(), ips.clone())) {
            continue;
        }
        if let Ok(n) = ebpf.ingest_dns_resolution(cfg, domain, &ips, 300).await {
            total_inserted = total_inserted.saturating_add(n);
        }
    }
    total_inserted
}

async fn prewarm_exact_domain_rules(ebpf: &EbpfEngine, cfg: &RouterConfig) {
    for rule in &cfg.ebpf.domain_rules {
        let dom = rule.domain.trim().trim_end_matches('.');
        if dom.is_empty() || dom.starts_with("*.") {
            continue;
        }
        if let Ok(addrs) = tokio::net::lookup_host((dom, 80)).await {
            let ips: Vec<String> = addrs.map(|a| a.ip().to_string()).collect();
            if !ips.is_empty() {
                let _ = ebpf.ingest_dns_resolution(cfg, dom, &ips, 300).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabrouter_model::DomainPolicyRule;

    #[tokio::test]
    async fn ingests_crabdns_log_payload_into_ebpf() {
        let mut cfg = RouterConfig::default();
        cfg.ebpf.enabled = true;
        cfg.ebpf.domain_rules.push(DomainPolicyRule {
            domain: "*.openai.com".to_string(),
            wan_name: "wan".to_string(),
            tag_id: 4,
        });

        let ebpf = EbpfEngine::new(true);
        ebpf.sync_config(&cfg).await.expect("sync");
        let payload = serde_json::json!([
            { "domain": "api.openai.com", "answers": ["104.18.7.192"] },
            { "domain": "unrelated.org", "answers": ["93.184.216.34"] }
        ]);

        let mut seen = HashSet::new();
        let inserted = ingest_crabdns_logs_json(&ebpf, &cfg, &payload, &mut seen).await;
        assert_eq!(inserted, 1);

        // Duplicate batch should be deduplicated
        let inserted2 = ingest_crabdns_logs_json(&ebpf, &cfg, &payload, &mut seen).await;
        assert_eq!(inserted2, 0);
    }
}
