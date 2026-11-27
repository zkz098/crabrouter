use crabrouter_model::RouterConfig;
use serde::Serialize;

use crate::{Result, ServiceError};

#[derive(Serialize)]
struct SplitDnsEntry<'a> {
    domain: &'a str,
    upstreams: Vec<String>,
}

#[derive(Serialize)]
#[allow(clippy::struct_excessive_bools)]
struct CrabDnsToml<'a> {
    listen_dns: String,
    listen_web: String,
    upstreams: &'a [String],
    doh_enabled: bool,
    private_ptr_enabled: bool,
    private_ptr_upstreams: Vec<String>,
    rdns_enabled: bool,
    dns_rebinding_protection: bool,
    split_dns: Vec<SplitDnsEntry<'a>>,
}

/// Renders a `crabdns.toml` matching `crabdns-core::Config` for the frontline `:53` role:
/// - Handles LAN client queries directly (preserving real client IP for ACL/groups/logs)
/// - Routes local LAN domains (`*.lan`, etc.) and private PTR lookups to `dnsmasq` at `127.0.0.1:<dnsmasq_dns_port>`
pub fn render_crabdns_toml(cfg: &RouterConfig) -> Result<String> {
    let dnsmasq_upstream = format!("udp://127.0.0.1:{}", cfg.dns.dnsmasq_dns_port);
    let mut split_dns = Vec::new();
    let mut seen_domains = std::collections::HashSet::new();
    for lan in &cfg.lans {
        let dom = lan.domain.trim();
        if !dom.is_empty() && seen_domains.insert(dom.to_string()) {
            split_dns.push(SplitDnsEntry {
                domain: dom,
                upstreams: vec![dnsmasq_upstream.clone()],
            });
        }
    }
    for host in &cfg.dns.custom_hosts {
        let h = host.hostname.trim();
        let already_covered = seen_domains
            .iter()
            .any(|d| h == d || h.ends_with(&format!(".{d}")));
        if !h.is_empty() && !already_covered && seen_domains.insert(h.to_string()) {
            split_dns.push(SplitDnsEntry {
                domain: h,
                upstreams: vec![dnsmasq_upstream.clone()],
            });
        }
    }

    let doc = CrabDnsToml {
        listen_dns: format!("0.0.0.0:{}", cfg.dns.crabdns_port),
        listen_web: format!("127.0.0.1:{}", cfg.dns.crabdns_web_port),
        upstreams: &cfg.dns.public_upstreams,
        doh_enabled: cfg.dns.encrypted_dns_enabled,
        private_ptr_enabled: true,
        private_ptr_upstreams: vec![dnsmasq_upstream],
        rdns_enabled: true,
        dns_rebinding_protection: true,
        split_dns,
    };

    toml::to_string_pretty(&doc).map_err(|e| ServiceError::Render(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_crabdns_config_pointing_local_ptr_to_dnsmasq() {
        let cfg = RouterConfig::default();
        let rendered = render_crabdns_toml(&cfg).unwrap();
        assert!(rendered.contains("listen_dns = \"0.0.0.0:53\""));
        assert!(rendered.contains("private_ptr_enabled = true"));
        assert!(rendered.contains("udp://127.0.0.1:5354"));
        assert!(rendered.contains("domain = \"lan\""));
    }
}
