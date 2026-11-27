use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::container::{ContainerCreateRequest, ContainerMountSpec, ContainerPortBinding};
use crate::error::{ModelError, Result};

/// Category for curated containerized router applications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AppCategory {
    Networking,
    Storage,
    Security,
    SmartHome,
    Utilities,
}

/// Form field input kind for an App Store entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AppFieldType {
    Text,
    Password,
    Number,
    Port,
    Boolean,
    Path,
}

/// Configurable parameter exposed in the App Store install form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AppFieldSpec {
    pub key: String,
    pub label: String,
    pub field_type: AppFieldType,
    pub default_value: String,
    pub placeholder: String,
    pub required: bool,
    pub description: String,
}

/// Curated containerized application definition.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AppCatalogEntry {
    pub id: String,
    pub name: String,
    pub category: AppCategory,
    pub summary: String,
    pub image: String,
    pub network_mode: String,
    pub ports: Vec<ContainerPortBinding>,
    pub volumes: Vec<ContainerMountSpec>,
    pub env_template: Vec<String>,
    pub cmd: Vec<String>,
    pub privileged: bool,
    pub fields: Vec<AppFieldSpec>,
    pub web_port: Option<u16>,
}

/// Request to install and optionally start a catalog application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AppInstallRequest {
    pub app_id: String,
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub start_immediately: bool,
}

/// Runtime view of an installed App Store application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct InstalledApp {
    pub app_id: String,
    pub name: String,
    pub container_id: String,
    pub container_name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub web_port: Option<u16>,
}

/// Prefix used for containers managed by the `CrabRouter` App Store.
pub const APP_CONTAINER_PREFIX: &str = "crabrouter-app-";

/// Returns the deterministic container name for a catalog `app_id`.
pub fn app_container_name(app_id: &str) -> String {
    format!("{APP_CONTAINER_PREFIX}{app_id}")
}

/// Returns the curated built-in catalog of containerized soft-router applications.
pub fn builtin_app_catalog() -> Vec<AppCatalogEntry> {
    vec![
        app_samba(),
        app_tailscale(),
        app_zerotier(),
        app_adguardhome(),
        app_alist(),
        app_homeassistant(),
    ]
}

fn app_samba() -> AppCatalogEntry {
    AppCatalogEntry {
        id: "samba".to_string(),
        name: "Samba 文件共享 (SMB)".to_string(),
        category: AppCategory::Storage,
        summary: "为局域网设备提供 SMB/CIFS 共享目录与 Time Machine 支持".to_string(),
        image: "docker.io/dperson/samba:latest".to_string(),
        network_mode: "bridge".to_string(),
        ports: vec![port_bind(139, 139, "tcp"), port_bind(445, 445, "tcp")],
        volumes: vec![mount_spec(
            "/var/lib/crabrouter/volumes/apps/samba/share",
            "/mount",
            false,
        )],
        env_template: vec![
            "USER={{username}};{{password}}".to_string(),
            "SHARE=share;/mount;yes;no;no;{{username}}".to_string(),
        ],
        cmd: Vec::new(),
        privileged: false,
        fields: vec![
            field(
                "username",
                "用户名",
                AppFieldType::Text,
                "crab",
                "crab",
                true,
                "SMB 登录用户名",
            ),
            field(
                "password",
                "密码",
                AppFieldType::Password,
                "crabrouter",
                "密码",
                true,
                "SMB 登录密码",
            ),
            field(
                "host_path",
                "宿主机共享目录",
                AppFieldType::Path,
                "/var/lib/crabrouter/volumes/apps/samba/share",
                "/var/lib/crabrouter/volumes/apps/samba/share",
                true,
                "映射到容器 /mount 的宿主机路径",
            ),
        ],
        web_port: None,
    }
}

fn app_tailscale() -> AppCatalogEntry {
    AppCatalogEntry {
        id: "tailscale".to_string(),
        name: "Tailscale 异地组网".to_string(),
        category: AppCategory::Networking,
        summary: "零配置 WireGuard Mesh VPN，支持子网路由与出口节点".to_string(),
        image: "docker.io/tailscale/tailscale:stable".to_string(),
        network_mode: "host".to_string(),
        ports: Vec::new(),
        volumes: vec![mount_spec(
            "/var/lib/crabrouter/volumes/apps/tailscale/state",
            "/var/lib/tailscale",
            false,
        )],
        env_template: vec![
            "TS_AUTHKEY={{auth_key}}".to_string(),
            "TS_ROUTES={{advertise_routes}}".to_string(),
            "TS_STATE_DIR=/var/lib/tailscale".to_string(),
            "TS_USERSPACE=false".to_string(),
        ],
        cmd: Vec::new(),
        privileged: true,
        fields: vec![
            field(
                "auth_key",
                "Auth Key",
                AppFieldType::Password,
                "",
                "tskey-auth-...",
                true,
                "Tailscale 控制台生成的 Pre-auth Key",
            ),
            field(
                "advertise_routes",
                "通告子网 (Routes)",
                AppFieldType::Text,
                "192.168.10.0/24",
                "192.168.10.0/24",
                false,
                "向 Tailnet 广播的 LAN 子网 CIDR",
            ),
        ],
        web_port: None,
    }
}

fn app_zerotier() -> AppCatalogEntry {
    AppCatalogEntry {
        id: "zerotier".to_string(),
        name: "ZeroTier 虚拟局域网".to_string(),
        category: AppCategory::Networking,
        summary: "P2P 虚拟二层组网客户端，快速接入 ZeroTier Central 网络".to_string(),
        image: "docker.io/zyclonite/zerotier:latest".to_string(),
        network_mode: "host".to_string(),
        ports: Vec::new(),
        volumes: vec![mount_spec(
            "/var/lib/crabrouter/volumes/apps/zerotier/lib",
            "/var/lib/zerotier-one",
            false,
        )],
        env_template: vec!["ZEROTIER_ONE_NETWORK_IDS={{network_id}}".to_string()],
        cmd: Vec::new(),
        privileged: true,
        fields: vec![field(
            "network_id",
            "Network ID",
            AppFieldType::Text,
            "",
            "16位十六进制网络 ID",
            true,
            "在 ZeroTier Central 创建的 Network ID",
        )],
        web_port: None,
    }
}

fn app_adguardhome() -> AppCatalogEntry {
    AppCatalogEntry {
        id: "adguardhome".to_string(),
        name: "AdGuard Home".to_string(),
        category: AppCategory::Security,
        summary: "全网广告过滤、DNS 重写与家长控制面板".to_string(),
        image: "docker.io/adguard/adguardhome:latest".to_string(),
        network_mode: "bridge".to_string(),
        ports: vec![
            port_bind(3000, 3000, "tcp"),
            port_bind(5355, 53, "udp"),
            port_bind(5355, 53, "tcp"),
        ],
        volumes: vec![
            mount_spec(
                "/var/lib/crabrouter/volumes/apps/adguardhome/work",
                "/opt/adguardhome/work",
                false,
            ),
            mount_spec(
                "/var/lib/crabrouter/volumes/apps/adguardhome/conf",
                "/opt/adguardhome/conf",
                false,
            ),
        ],
        env_template: Vec::new(),
        cmd: Vec::new(),
        privileged: false,
        fields: vec![field(
            "web_port",
            "Web 管理端口",
            AppFieldType::Port,
            "3000",
            "3000",
            true,
            "AdGuard Home 初始化及管理界面端口",
        )],
        web_port: Some(3000),
    }
}

fn app_alist() -> AppCatalogEntry {
    AppCatalogEntry {
        id: "alist".to_string(),
        name: "Alist 网盘与 WebDAV".to_string(),
        category: AppCategory::Storage,
        summary: "支持多网盘聚合、WebDAV 服务与局域网流媒体直链".to_string(),
        image: "docker.io/xhofe/alist:latest".to_string(),
        network_mode: "bridge".to_string(),
        ports: vec![port_bind(5244, 5244, "tcp")],
        volumes: vec![mount_spec(
            "/var/lib/crabrouter/volumes/apps/alist/data",
            "/opt/alist/data",
            false,
        )],
        env_template: vec!["PUID=0".to_string(), "PGID=0".to_string()],
        cmd: Vec::new(),
        privileged: false,
        fields: vec![field(
            "web_port",
            "Web 端口",
            AppFieldType::Port,
            "5244",
            "5244",
            true,
            "Alist Web 管理与 WebDAV 端口",
        )],
        web_port: Some(5244),
    }
}

fn app_homeassistant() -> AppCatalogEntry {
    AppCatalogEntry {
        id: "homeassistant".to_string(),
        name: "Home Assistant".to_string(),
        category: AppCategory::SmartHome,
        summary: "开源本地智能家居中枢，支持 Matter / HomeKit / 米家集成".to_string(),
        image: "ghcr.io/home-assistant/home-assistant:stable".to_string(),
        network_mode: "host".to_string(),
        ports: Vec::new(),
        volumes: vec![mount_spec(
            "/var/lib/crabrouter/volumes/apps/homeassistant/config",
            "/config",
            false,
        )],
        env_template: vec!["TZ=Asia/Shanghai".to_string()],
        cmd: Vec::new(),
        privileged: false,
        fields: Vec::new(),
        web_port: Some(8123),
    }
}

/// Resolves a catalog entry and user settings into a validated `ContainerCreateRequest`.
pub fn build_app_container_request(
    entry: &AppCatalogEntry,
    req: &AppInstallRequest,
) -> Result<ContainerCreateRequest> {
    let mut resolved = BTreeMap::new();
    for field in &entry.fields {
        let val = req
            .settings
            .get(&field.key)
            .map(String::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(field.default_value.as_str());
        if field.required && val.is_empty() {
            return Err(ModelError::Validation(format!(
                "app '{}' missing required setting '{}'",
                entry.id, field.label
            )));
        }
        resolved.insert(field.key.as_str(), val.to_string());
    }

    let env = entry
        .env_template
        .iter()
        .map(|tpl| substitute_placeholders(tpl, &resolved))
        .collect();

    let mut ports = entry.ports.clone();
    if let Some(port_str) = resolved.get("web_port")
        && let Ok(custom_port) = port_str.parse::<u16>()
        && let Some(first_port) = ports.first_mut()
    {
        first_port.host_port = custom_port;
    }

    let mut mounts = entry.volumes.clone();
    if let Some(host_path) = resolved.get("host_path")
        && let Some(first_mount) = mounts.first_mut()
    {
        first_mount.source.clone_from(host_path);
    }

    Ok(ContainerCreateRequest {
        name: app_container_name(&entry.id),
        image: entry.image.clone(),
        cmd: entry.cmd.clone(),
        env,
        ports,
        mounts,
        network: Some(entry.network_mode.clone()),
        restart_policy: Some("unless-stopped".to_string()),
        privileged: entry.privileged,
        start_immediately: req.start_immediately,
    })
}

fn substitute_placeholders(template: &str, values: &BTreeMap<&str, String>) -> String {
    let mut out = template.to_string();
    for (k, v) in values {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    out
}

fn port_bind(host_port: u16, container_port: u16, protocol: &str) -> ContainerPortBinding {
    ContainerPortBinding {
        host_ip: None,
        host_port,
        container_port,
        protocol: protocol.to_string(),
    }
}

fn mount_spec(source: &str, target: &str, read_only: bool) -> ContainerMountSpec {
    ContainerMountSpec {
        source: source.to_string(),
        target: target.to_string(),
        read_only,
    }
}

fn field(
    key: &str,
    label: &str,
    field_type: AppFieldType,
    default_value: &str,
    placeholder: &str,
    required: bool,
    description: &str,
) -> AppFieldSpec {
    AppFieldSpec {
        key: key.to_string(),
        label: label.to_string(),
        field_type,
        default_value: default_value.to_string(),
        placeholder: placeholder.to_string(),
        required,
        description: description.to_string(),
    }
}

const fn default_true() -> bool {
    true
}
