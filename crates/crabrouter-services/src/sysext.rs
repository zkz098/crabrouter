use std::collections::HashSet;
use std::path::Path;

use crabrouter_model::{
    SysextActionResult, SysextImageStatus, SysextRuntimeStatus, sanitize_sysext_filename,
};
use serde::Deserialize;
use tokio::process::Command;

use crate::ota::compute_file_sha256;
use crate::{Result, ServiceError};

#[derive(Debug, Deserialize)]
struct SysextStatusEntry {
    #[serde(default)]
    extension: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    hierarchy: Option<String>,
    #[serde(default)]
    scopes: Vec<String>,
}

/// Inspects `/var/lib/extensions/*.raw` and queries `systemd-sysext status` on live hosts.
pub async fn query_sysext_status(extensions_dir: &Path, dry_run: bool) -> SysextRuntimeStatus {
    let mut active_names: HashSet<String> = HashSet::new();
    let mut merged_paths: Vec<String> = Vec::new();
    let mut scopes_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut last_error = None;

    if !dry_run {
        match run_sysext_status_json().await {
            Ok(entries) => {
                for entry in entries {
                    if let Some(h) = entry.hierarchy
                        && !merged_paths.contains(&h)
                    {
                        merged_paths.push(h);
                    }
                    if let Some(name) = entry.extension.or(entry.name) {
                        active_names.insert(name.clone());
                        scopes_map.insert(name, entry.scopes);
                    }
                }
            }
            Err(err) => last_error = Some(err),
        }
    }

    let extensions = scan_extension_images(extensions_dir, &active_names, &scopes_map);
    SysextRuntimeStatus {
        supported: true,
        extensions_dir: extensions_dir.display().to_string(),
        merged_paths,
        extensions,
        last_error,
    }
}

/// Runs `systemd-sysext refresh` followed by `depmod -a` and `udevadm trigger` for new `kmods`.
pub async fn refresh_sysext(dry_run: bool) -> Result<SysextActionResult> {
    if dry_run {
        return Ok(SysextActionResult {
            ok: true,
            action: "refresh".to_string(),
            message: "dry-run: systemd-sysext refresh simulated".to_string(),
        });
    }
    let out = run_cmd("systemd-sysext", &["refresh"]).await?;
    let _ = run_cmd("depmod", &["-a"]).await;
    let _ = run_cmd("udevadm", &["trigger", "--action=add"]).await;
    Ok(SysextActionResult {
        ok: true,
        action: "refresh".to_string(),
        message: if out.is_empty() {
            "systemd-sysext refreshed".to_string()
        } else {
            out
        },
    })
}

/// Runs `systemd-sysext unmerge` to deactivate all active system extensions.
pub async fn unmerge_sysext(dry_run: bool) -> Result<SysextActionResult> {
    if dry_run {
        return Ok(SysextActionResult {
            ok: true,
            action: "unmerge".to_string(),
            message: "dry-run: systemd-sysext unmerge simulated".to_string(),
        });
    }
    let out = run_cmd("systemd-sysext", &["unmerge"]).await?;
    Ok(SysextActionResult {
        ok: true,
        action: "unmerge".to_string(),
        message: if out.is_empty() {
            "systemd-sysext unmerged".to_string()
        } else {
            out
        },
    })
}

/// Deletes a `.raw` extension image from `extensions_dir` and refreshes `systemd-sysext`.
pub async fn remove_sysext_image(
    extensions_dir: &Path,
    raw_name: &str,
    dry_run: bool,
) -> Result<SysextActionResult> {
    let filename =
        sanitize_sysext_filename(raw_name).map_err(|e| ServiceError::Render(e.to_string()))?;
    let path = extensions_dir.join(&filename);
    if tokio::fs::try_exists(&path).await.unwrap_or(false) {
        tokio::fs::remove_file(&path).await?;
    }
    if !dry_run {
        let _ = refresh_sysext(false).await;
    }
    Ok(SysextActionResult {
        ok: true,
        action: "delete".to_string(),
        message: format!("removed sysext image {filename}"),
    })
}

fn scan_extension_images(
    extensions_dir: &Path,
    active_names: &HashSet<String>,
    scopes_map: &std::collections::HashMap<String, Vec<String>>,
) -> Vec<SysextImageStatus> {
    let Ok(entries) = std::fs::read_dir(extensions_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("raw") {
            continue;
        }
        let filename = entry.file_name().to_string_lossy().to_string();
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&filename)
            .to_string();
        let size_bytes = entry.metadata().map_or(0, |m| m.len());
        let sha256 = compute_file_sha256(&path).unwrap_or_default();
        let active = active_names.contains(&name);
        let scopes = scopes_map.get(&name).cloned().unwrap_or_default();
        out.push(SysextImageStatus {
            name,
            filename,
            path: path.display().to_string(),
            size_bytes,
            sha256,
            active,
            scopes,
        });
    }
    out.sort_by(|a, b| a.filename.cmp(&b.filename));
    out
}

async fn run_sysext_status_json() -> std::result::Result<Vec<SysextStatusEntry>, String> {
    let output = Command::new("systemd-sysext")
        .args(["status", "--json=short"])
        .output()
        .await
        .map_err(|e| format!("failed to run systemd-sysext: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("systemd-sysext status failed: {stderr}"));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Ok(Vec::new());
    }
    serde_json::from_str(trimmed).map_err(|e| format!("invalid systemd-sysext JSON: {e}"))
}

async fn run_cmd(bin: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(bin)
        .args(args)
        .output()
        .await
        .map_err(|e| ServiceError::Orchestration(format!("failed to run {bin}: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        return Err(ServiceError::Orchestration(format!(
            "{bin} {} failed: {stderr}",
            args.join(" ")
        )));
    }
    Ok(stdout)
}

#[cfg(test)]
mod tests {
    use super::{query_sysext_status, refresh_sysext};

    #[tokio::test]
    async fn scans_raw_extensions_in_dry_run() {
        let dir = std::env::temp_dir().join(format!("crabrouter-sysext-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join("usb-nic-kmods.raw"), b"dummy-squashfs").expect("write raw");

        let status = query_sysext_status(&dir, true).await;
        assert!(status.supported);
        assert_eq!(status.extensions.len(), 1);
        assert_eq!(status.extensions[0].name, "usb-nic-kmods");

        let res = refresh_sysext(true).await.expect("dry-run refresh");
        assert!(res.ok);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
