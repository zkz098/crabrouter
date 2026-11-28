use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use crabrouter_model::{AcmeChallengeType, RouterConfig, TlsRuntimeStatus};
use tokio::process::Command;
use tokio::sync::RwLock;

use crate::{Result, ServiceError};

#[derive(Clone)]
pub struct TlsCertManager {
    status: Arc<RwLock<TlsRuntimeStatus>>,
    dry_run: bool,
}

impl TlsCertManager {
    pub fn new(cfg: &RouterConfig, dry_run: bool) -> Self {
        Self {
            status: Arc::new(RwLock::new(TlsRuntimeStatus {
                https_enabled: cfg.system.web_https_enabled,
                acme_enabled: cfg.acme.enabled,
                domain: cfg.acme.domain.clone(),
                cert_path: cfg.system.web_tls_cert.clone(),
                expires_at: None,
                last_renewed_at: None,
                last_error: None,
            })),
            dry_run,
        }
    }

    pub async fn snapshot(&self, cfg: &RouterConfig) -> TlsRuntimeStatus {
        let mut st = self.status.read().await.clone();
        st.https_enabled = cfg.system.web_https_enabled;
        st.acme_enabled = cfg.acme.enabled;
        st.domain = cfg.acme.domain.clone();
        if let Some(path) = cfg.system.web_tls_cert.as_deref() {
            st.cert_path = Some(path.to_string());
            if !self.dry_run {
                st.expires_at = read_cert_expiry(Path::new(path)).await;
            }
        }
        st
    }

    /// Triggers ACME certificate issuance/renewal (or simulates it in `dry_run` mode).
    pub async fn renew_now(&self, cfg: &RouterConfig) -> TlsRuntimeStatus {
        if !cfg.acme.enabled {
            let disabled = TlsRuntimeStatus {
                https_enabled: cfg.system.web_https_enabled,
                acme_enabled: false,
                domain: cfg.acme.domain.clone(),
                cert_path: cfg.system.web_tls_cert.clone(),
                expires_at: None,
                last_renewed_at: None,
                last_error: Some("ACME dynamic TLS is not enabled".to_string()),
            };
            *self.status.write().await = disabled.clone();
            return disabled;
        }

        if self.dry_run {
            let sim =
                TlsRuntimeStatus {
                    https_enabled: cfg.system.web_https_enabled,
                    acme_enabled: true,
                    domain: cfg.acme.domain.clone(),
                    cert_path: cfg.system.web_tls_cert.clone().or_else(|| {
                        Some(format!("/var/lib/crabrouter/acme/{}.crt", cfg.acme.domain))
                    }),
                    expires_at: Some("90d (simulated)".to_string()),
                    last_renewed_at: Some(Utc::now().to_rfc3339()),
                    last_error: None,
                };
            *self.status.write().await = sim.clone();
            return sim;
        }

        let res = run_acme_renew(cfg).await;
        let next = match res {
            Ok(cert_path) => {
                let expires_at = read_cert_expiry(Path::new(&cert_path)).await;
                TlsRuntimeStatus {
                    https_enabled: cfg.system.web_https_enabled,
                    acme_enabled: true,
                    domain: cfg.acme.domain.clone(),
                    cert_path: Some(cert_path),
                    expires_at,
                    last_renewed_at: Some(Utc::now().to_rfc3339()),
                    last_error: None,
                }
            }
            Err(err) => TlsRuntimeStatus {
                https_enabled: cfg.system.web_https_enabled,
                acme_enabled: true,
                domain: cfg.acme.domain.clone(),
                cert_path: cfg.system.web_tls_cert.clone(),
                expires_at: None,
                last_renewed_at: None,
                last_error: Some(err.to_string()),
            },
        };
        *self.status.write().await = next.clone();
        next
    }
}

/// Builds the `certbot` CLI arguments for ACME issuance/renewal.
pub fn build_certbot_args(cfg: &RouterConfig) -> Vec<String> {
    let mut args = vec![
        "certonly".to_string(),
        "--non-interactive".to_string(),
        "--agree-tos".to_string(),
        "-m".to_string(),
        cfg.acme.email.clone(),
        "-d".to_string(),
        cfg.acme.domain.clone(),
        "--server".to_string(),
        cfg.acme.ca_directory.clone(),
    ];
    match cfg.acme.challenge_type {
        AcmeChallengeType::Http01 => {
            args.push("--standalone".to_string());
        }
        AcmeChallengeType::DnsCloudflare => {
            args.push("--dns-cloudflare".to_string());
        }
        AcmeChallengeType::DnsAliyun => {
            args.push("--authenticator".to_string());
            args.push("dns-aliyun".to_string());
        }
        AcmeChallengeType::DnsDnspod => {
            args.push("--authenticator".to_string());
            args.push("dns-dnspod".to_string());
        }
        AcmeChallengeType::DnsHuaweiCloud => {
            args.push("--authenticator".to_string());
            args.push("dns-huaweicloud".to_string());
        }
    }
    args
}

async fn run_acme_renew(cfg: &RouterConfig) -> Result<String> {
    let args = build_certbot_args(cfg);
    let mut cmd = Command::new("certbot");
    cmd.args(&args);
    let out = crate::cmd::run_output_with_timeout(
        &mut cmd,
        std::time::Duration::from_secs(120),
        "certbot",
    )
    .await?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(ServiceError::Orchestration(format!(
            "certbot renew failed: {stderr}"
        )));
    }
    Ok(format!(
        "/etc/letsencrypt/live/{}/fullchain.pem",
        cfg.acme.domain
    ))
}

async fn read_cert_expiry(path: &Path) -> Option<String> {
    if !path.exists() {
        return None;
    }
    let mut cmd = Command::new("openssl");
    cmd.args(["x509", "-enddate", "-noout", "-in"]).arg(path);
    let out = crate::cmd::run_output_with_timeout(
        &mut cmd,
        std::time::Duration::from_secs(5),
        "openssl x509",
    )
    .await
    .ok()?;
    if out.status.success() {
        let text = String::from_utf8_lossy(&out.stdout);
        return text
            .trim()
            .strip_prefix("notAfter=")
            .map(str::to_string)
            .or_else(|| Some(text.trim().to_string()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_certbot_command_for_http_and_dns() {
        let mut cfg = RouterConfig::default();
        cfg.acme.enabled = true;
        cfg.acme.domain = "gw.example.com".to_string();
        cfg.acme.email = "ops@example.com".to_string();
        let args = build_certbot_args(&cfg);
        assert!(args.contains(&"--standalone".to_string()));
        assert!(args.contains(&"gw.example.com".to_string()));

        cfg.acme.challenge_type = AcmeChallengeType::DnsCloudflare;
        let args_dns = build_certbot_args(&cfg);
        assert!(args_dns.contains(&"--dns-cloudflare".to_string()));
    }

    #[tokio::test]
    async fn dry_run_acme_renews_cert() {
        let mut cfg = RouterConfig::default();
        cfg.acme.enabled = true;
        cfg.acme.domain = "gw.example.com".to_string();
        let mgr = TlsCertManager::new(&cfg, true);
        let st = mgr.renew_now(&cfg).await;
        assert!(st.last_error.is_none());
        assert!(st.last_renewed_at.is_some());
    }
}
