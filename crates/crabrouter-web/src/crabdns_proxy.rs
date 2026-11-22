use std::time::Duration;

use axum::Json;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
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
