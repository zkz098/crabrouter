use std::net::{IpAddr, SocketAddr};

use axum::Json;
use axum::extract::{ConnectInfo, Extension};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use crabrouter_core::CoreError;
use serde::Serialize;

use crate::AppState;

#[derive(Serialize)]
pub struct ErrorBody {
    pub error: String,
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

pub(crate) fn extract_token(headers: &HeaderMap) -> Option<String> {
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
    require_strict_auth(state, headers)
}

pub(crate) fn require_strict_auth(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
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

fn is_loopback_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => {
            v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
    }
}

/// Allows unauthenticated local daemon hooks over loopback (`127.0.0.0/8`, `::1`, `::ffff:127.0.0.1`),
/// requires a valid authenticated session for any non-loopback peer, and falls back to standard
/// auth checks when invoked in-process without `ConnectInfo` (e.g. unit tests).
pub(crate) fn require_loopback_or_auth(
    state: &AppState,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    if let Some(Extension(ConnectInfo(addr))) = peer {
        if is_loopback_ip(addr.ip()) {
            return Ok(());
        }
        return require_strict_auth(state, headers);
    }
    require_auth_if_initialized(state, headers)
}

pub(crate) fn session_cookie_header(token: &str) -> String {
    format!("crabrouter_session={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=604800")
}
