use std::time::Duration;

use chrono::Utc;
use crabrouter_model::{DdnsConfig, DdnsIpVersion, DdnsProvider};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::process::Command;

use crate::{Result, ServiceError};

/// Splits a FQDN into `(sub_domain, root_domain)` using optional `zone_hint` as root domain.
pub fn split_domain_parts(fqdn: &str, zone_hint: Option<&str>) -> Result<(String, String)> {
    let clean = fqdn.trim().trim_end_matches('.');
    if let Some(zone) = zone_hint.map(str::trim).filter(|s| !s.is_empty()) {
        let root = zone.trim_end_matches('.');
        if clean == root {
            return Ok(("@".to_string(), root.to_string()));
        }
        if let Some(prefix) = clean.strip_suffix(&format!(".{root}")) {
            return Ok((prefix.to_string(), root.to_string()));
        }
    }
    let parts: Vec<&str> = clean.split('.').collect();
    if parts.len() < 2 {
        return Err(ServiceError::Render(format!(
            "DDNS domain '{fqdn}' must include at least host and root domain"
        )));
    }
    if parts.len() == 2 {
        return Ok(("@".to_string(), clean.to_string()));
    }
    let root = format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1]);
    let sub = parts[..parts.len() - 2].join(".");
    Ok((sub, root))
}

/// Computes `HMAC-SHA256(key, data)` using pure `sha2::Sha256` (RFC 2104).
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    const BLOCK_SIZE: usize = 64;
    let mut k_block = [0u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        let hashed = Sha256::digest(key);
        k_block[..32].copy_from_slice(&hashed);
    } else {
        k_block[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; BLOCK_SIZE];
    let mut opad = [0x5cu8; BLOCK_SIZE];
    for i in 0..BLOCK_SIZE {
        ipad[i] ^= k_block[i];
        opad[i] ^= k_block[i];
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(data);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    let out: [u8; 32] = outer.finalize().into();
    out
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

pub fn record_type_for(ip_version: DdnsIpVersion) -> &'static str {
    match ip_version {
        DdnsIpVersion::Ipv4 => "A",
        DdnsIpVersion::Ipv6 => "AAAA",
    }
}

/// Dispatches a domestic DDNS update (`Aliyun`, `Dnspod`, `HuaweiCloud`).
pub async fn sync_domestic_ddns(ddns: &DdnsConfig, ip: &str) -> Result<()> {
    let ak = ddns
        .username
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ServiceError::Render("missing AccessKey/SecretId (username)".to_string()))?;
    let sk = ddns
        .token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ServiceError::Render("missing SecretKey (token)".to_string()))?;

    match ddns.provider {
        DdnsProvider::Aliyun => sync_aliyun(ddns, ak, sk, ip).await,
        DdnsProvider::Dnspod => sync_dnspod(ddns, ak, sk, ip).await,
        DdnsProvider::HuaweiCloud => sync_huaweicloud(ddns, ak, sk, ip).await,
        _ => Err(ServiceError::Render(
            "sync_domestic_ddns called for non-domestic provider".to_string(),
        )),
    }
}

/// Builds an Alibaba Cloud `ACS3-HMAC-SHA256` signed request `(url, headers)`.
pub fn build_aliyun_acs3_headers(
    ak: &str,
    sk: &str,
    action: &str,
    query: &str,
    iso_date: &str,
    nonce: &str,
) -> (String, Vec<(String, String)>) {
    let host = "alidns.aliyuncs.com";
    let body_hash = sha256_hex(b"");
    let canonical_headers = format!(
        "host:{host}\nx-acs-action:{action}\nx-acs-content-sha256:{body_hash}\nx-acs-date:{iso_date}\nx-acs-signature-nonce:{nonce}\nx-acs-version:2015-01-09\n"
    );
    let signed_headers =
        "host;x-acs-action;x-acs-content-sha256;x-acs-date;x-acs-signature-nonce;x-acs-version";
    let canonical_request =
        format!("POST\n/\n{query}\n{canonical_headers}\n{signed_headers}\n{body_hash}");
    let string_to_sign = format!(
        "ACS3-HMAC-SHA256\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let signature = hex::encode(hmac_sha256(sk.as_bytes(), string_to_sign.as_bytes()));
    let auth = format!(
        "ACS3-HMAC-SHA256 Credential={ak},SignedHeaders={signed_headers},Signature={signature}"
    );
    let url = if query.is_empty() {
        format!("https://{host}/")
    } else {
        format!("https://{host}/?{query}")
    };
    let headers = vec![
        ("Host".to_string(), host.to_string()),
        ("x-acs-action".to_string(), action.to_string()),
        ("x-acs-version".to_string(), "2015-01-09".to_string()),
        ("x-acs-date".to_string(), iso_date.to_string()),
        ("x-acs-signature-nonce".to_string(), nonce.to_string()),
        ("x-acs-content-sha256".to_string(), body_hash),
        ("Authorization".to_string(), auth),
    ];
    (url, headers)
}

/// Builds Tencent Cloud `TC3-HMAC-SHA256` headers `(url, headers)` for `DNSPod` API.
pub fn build_dnspod_tc3_headers(
    secret_id: &str,
    secret_key: &str,
    action: &str,
    payload: &str,
    timestamp: i64,
    date_utc: &str,
) -> (String, Vec<(String, String)>) {
    let host = "dnspod.tencentcloudapi.com";
    let service = "dnspod";
    let canonical_headers = format!("content-type:application/json; charset=utf-8\nhost:{host}\n");
    let signed_headers = "content-type;host";
    let hashed_payload = sha256_hex(payload.as_bytes());
    let canonical_request =
        format!("POST\n/\n\n{canonical_headers}\n{signed_headers}\n{hashed_payload}");
    let credential_scope = format!("{date_utc}/{service}/tc3_request");
    let string_to_sign = format!(
        "TC3-HMAC-SHA256\n{timestamp}\n{credential_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let secret_date = hmac_sha256(format!("TC3{secret_key}").as_bytes(), date_utc.as_bytes());
    let secret_service = hmac_sha256(&secret_date, service.as_bytes());
    let secret_signing = hmac_sha256(&secret_service, b"tc3_request");
    let signature = hex::encode(hmac_sha256(&secret_signing, string_to_sign.as_bytes()));
    let auth = format!(
        "TC3-HMAC-SHA256 Credential={secret_id}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}"
    );
    let headers = vec![
        (
            "Content-Type".to_string(),
            "application/json; charset=utf-8".to_string(),
        ),
        ("Host".to_string(), host.to_string()),
        ("X-TC-Action".to_string(), action.to_string()),
        ("X-TC-Version".to_string(), "2021-03-23".to_string()),
        ("X-TC-Timestamp".to_string(), timestamp.to_string()),
        ("Authorization".to_string(), auth),
    ];
    (format!("https://{host}/"), headers)
}

/// Builds Huawei Cloud `SDK-HMAC-SHA256` headers `(url, headers)` for DNS v2 API.
pub fn build_huaweicloud_sdk_headers(
    ak: &str,
    sk: &str,
    method: &str,
    path: &str,
    query: &str,
    body: &str,
    x_sdk_date: &str,
) -> (String, Vec<(String, String)>) {
    let host = "dns.myhuaweicloud.com";
    let body_hash = sha256_hex(body.as_bytes());
    let canonical_headers =
        format!("content-type:application/json\nhost:{host}\nx-sdk-date:{x_sdk_date}\n");
    let signed_headers = "content-type;host;x-sdk-date";
    let canonical_uri = if path.ends_with('/') {
        path.to_string()
    } else {
        format!("{path}/")
    };
    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{query}\n{canonical_headers}\n{signed_headers}\n{body_hash}"
    );
    let string_to_sign = format!(
        "SDK-HMAC-SHA256\n{x_sdk_date}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let signature = hex::encode(hmac_sha256(sk.as_bytes(), string_to_sign.as_bytes()));
    let auth = format!(
        "SDK-HMAC-SHA256 Access={ak}, SignedHeaders={signed_headers}, Signature={signature}"
    );
    let url = if query.is_empty() {
        format!("https://{host}{path}")
    } else {
        format!("https://{host}{path}?{query}")
    };
    let headers = vec![
        ("Content-Type".to_string(), "application/json".to_string()),
        ("Host".to_string(), host.to_string()),
        ("X-Sdk-Date".to_string(), x_sdk_date.to_string()),
        ("Authorization".to_string(), auth),
    ];
    (url, headers)
}

async fn sync_aliyun(ddns: &DdnsConfig, ak: &str, sk: &str, ip: &str) -> Result<()> {
    let rtype = record_type_for(ddns.ip_version);
    let (rr, root) = split_domain_parts(&ddns.domain, ddns.zone_id.as_deref())?;
    let record_id = if let Some(id) = ddns
        .record_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(id.to_string())
    } else {
        let query = format!("SubDomain={}&Type={rtype}", url_encode(ddns.domain.trim()));
        let resp = call_aliyun(ak, sk, "DescribeSubDomainRecords", &query).await?;
        resp["DomainRecords"]["Record"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|r| r["RecordId"].as_str())
            .map(str::to_string)
    };

    if let Some(id) = record_id {
        let query = format!(
            "RR={}&RecordId={}&Type={rtype}&Value={}",
            url_encode(&rr),
            url_encode(&id),
            url_encode(ip)
        );
        call_aliyun(ak, sk, "UpdateDomainRecord", &query).await?;
    } else {
        let query = format!(
            "DomainName={}&RR={}&Type={rtype}&Value={}",
            url_encode(&root),
            url_encode(&rr),
            url_encode(ip)
        );
        call_aliyun(ak, sk, "AddDomainRecord", &query).await?;
    }
    Ok(())
}

async fn call_aliyun(ak: &str, sk: &str, action: &str, query: &str) -> Result<Value> {
    let now = Utc::now();
    let iso_date = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let nonce = now.timestamp_nanos_opt().unwrap_or_default().to_string();
    let (url, headers) = build_aliyun_acs3_headers(ak, sk, action, query, &iso_date, &nonce);
    exec_curl_json("POST", &url, &headers, None, "aliyun ddns").await
}

async fn sync_dnspod(ddns: &DdnsConfig, secret_id: &str, secret_key: &str, ip: &str) -> Result<()> {
    let rtype = record_type_for(ddns.ip_version);
    let (sub, root) = split_domain_parts(&ddns.domain, ddns.zone_id.as_deref())?;
    let record_id = if let Some(id) = ddns
        .record_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        id.parse::<u64>().ok()
    } else {
        let payload = serde_json::json!({
            "Domain": root,
            "Subdomain": sub,
            "RecordType": rtype,
        })
        .to_string();
        let resp = call_dnspod(secret_id, secret_key, "DescribeRecordList", &payload).await?;
        resp["Response"]["RecordList"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|r| r["RecordId"].as_u64())
    };

    if let Some(id) = record_id {
        let payload = serde_json::json!({
            "Domain": root,
            "RecordType": rtype,
            "RecordLine": "默认",
            "Value": ip,
            "RecordId": id,
            "SubDomain": sub,
        })
        .to_string();
        call_dnspod(secret_id, secret_key, "ModifyRecord", &payload).await?;
    } else {
        let payload = serde_json::json!({
            "Domain": root,
            "RecordType": rtype,
            "RecordLine": "默认",
            "Value": ip,
            "SubDomain": sub,
        })
        .to_string();
        call_dnspod(secret_id, secret_key, "CreateRecord", &payload).await?;
    }
    Ok(())
}

async fn call_dnspod(
    secret_id: &str,
    secret_key: &str,
    action: &str,
    payload: &str,
) -> Result<Value> {
    let now = Utc::now();
    let ts = now.timestamp();
    let date_utc = now.format("%Y-%m-%d").to_string();
    let (url, headers) =
        build_dnspod_tc3_headers(secret_id, secret_key, action, payload, ts, &date_utc);
    let resp = exec_curl_json("POST", &url, &headers, Some(payload), "dnspod ddns").await?;
    if let Some(err) = resp["Response"]["Error"]["Message"].as_str() {
        return Err(ServiceError::Orchestration(format!("DNSPod error: {err}")));
    }
    Ok(resp)
}

async fn sync_huaweicloud(ddns: &DdnsConfig, ak: &str, sk: &str, ip: &str) -> Result<()> {
    let rtype = record_type_for(ddns.ip_version);
    let zone_id = ddns
        .zone_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ServiceError::Render("HuaweiCloud DDNS requires zone_id".to_string()))?;
    let fqdn = format!("{}.", ddns.domain.trim().trim_end_matches('.'));
    let recordset_id = if let Some(id) = ddns
        .record_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(id.to_string())
    } else {
        let query = format!("name={}&type={rtype}", url_encode(&fqdn));
        let resp = call_huaweicloud(
            ak,
            sk,
            "GET",
            &format!("/v2/zones/{zone_id}/recordsets"),
            &query,
            "",
        )
        .await?;
        resp["recordsets"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|r| r["id"].as_str())
            .map(str::to_string)
    };

    let body = serde_json::json!({
        "name": fqdn,
        "type": rtype,
        "ttl": 300,
        "records": [ip],
    })
    .to_string();
    if let Some(id) = recordset_id {
        call_huaweicloud(
            ak,
            sk,
            "PUT",
            &format!("/v2/zones/{zone_id}/recordsets/{id}"),
            "",
            &body,
        )
        .await?;
    } else {
        call_huaweicloud(
            ak,
            sk,
            "POST",
            &format!("/v2/zones/{zone_id}/recordsets"),
            "",
            &body,
        )
        .await?;
    }
    Ok(())
}

async fn call_huaweicloud(
    ak: &str,
    sk: &str,
    method: &str,
    path: &str,
    query: &str,
    body: &str,
) -> Result<Value> {
    let x_sdk_date = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let (url, headers) =
        build_huaweicloud_sdk_headers(ak, sk, method, path, query, body, &x_sdk_date);
    let body_opt = (!body.is_empty()).then_some(body);
    exec_curl_json(method, &url, &headers, body_opt, "huaweicloud ddns").await
}

async fn exec_curl_json(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&str>,
    label: &str,
) -> Result<Value> {
    let mut cmd = Command::new("curl");
    cmd.args(["-fsS", "--max-time", "10", "-X", method]);
    for (k, v) in headers {
        cmd.args(["-H", &format!("{k}: {v}")]);
    }
    if let Some(payload) = body {
        cmd.args(["-d", payload]);
    }
    cmd.arg(url);

    let out = crate::cmd::run_output_with_timeout(&mut cmd, Duration::from_secs(15), label).await?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(ServiceError::Orchestration(format!(
            "{label} failed: {stderr}"
        )));
    }
    serde_json::from_slice(&out.stdout)
        .map_err(|e| ServiceError::Orchestration(format!("{label} invalid JSON response: {e}")))
}

fn url_encode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for b in raw.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            use std::fmt::Write;
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_sha256_matches_rfc4231_case_2() {
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex::encode(mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn splits_fqdn_into_sub_and_root() {
        let (sub, root) = split_domain_parts("gw.home.example.com", None).expect("split");
        assert_eq!(sub, "gw.home");
        assert_eq!(root, "example.com");

        let (sub2, root2) = split_domain_parts("example.com", None).expect("apex");
        assert_eq!(sub2, "@");
        assert_eq!(root2, "example.com");

        let (sub3, root3) =
            split_domain_parts("gw.example.co.uk", Some("example.co.uk")).expect("zone hint");
        assert_eq!(sub3, "gw");
        assert_eq!(root3, "example.co.uk");
    }

    #[test]
    fn builds_domestic_signed_headers() {
        let (ali_url, ali_hdrs) = build_aliyun_acs3_headers(
            "ak",
            "sk",
            "UpdateDomainRecord",
            "RR=gw&RecordId=1&Type=AAAA&Value=2001:db8::1",
            "2026-11-28T00:00:00Z",
            "nonce1",
        );
        assert!(ali_url.starts_with("https://alidns.aliyuncs.com/?"));
        assert!(
            ali_hdrs
                .iter()
                .any(|(k, v)| k == "Authorization" && v.starts_with("ACS3-HMAC-SHA256"))
        );

        let (_, tc_hdrs) = build_dnspod_tc3_headers(
            "sid",
            "skey",
            "ModifyRecord",
            "{}",
            1_700_000_000,
            "2026-11-28",
        );
        assert!(
            tc_hdrs
                .iter()
                .any(|(k, v)| k == "Authorization" && v.starts_with("TC3-HMAC-SHA256"))
        );

        let (hw_url, hw_hdrs) = build_huaweicloud_sdk_headers(
            "ak",
            "sk",
            "PUT",
            "/v2/zones/z1/recordsets/r1",
            "",
            "{}",
            "20261128T000000Z",
        );
        assert_eq!(
            hw_url,
            "https://dns.myhuaweicloud.com/v2/zones/z1/recordsets/r1"
        );
        assert!(
            hw_hdrs
                .iter()
                .any(|(k, v)| k == "Authorization" && v.starts_with("SDK-HMAC-SHA256"))
        );
    }
}
