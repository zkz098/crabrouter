use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{ModelError, Result};

/// Status of a single `systemd-sysext` extension image under `/var/lib/extensions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SysextImageStatus {
    pub name: String,
    pub filename: String,
    pub path: String,
    #[ts(type = "number")]
    pub size_bytes: u64,
    pub sha256: String,
    pub active: bool,
    pub scopes: Vec<String>,
}

/// Overall runtime status of `systemd-sysext` on the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SysextRuntimeStatus {
    pub supported: bool,
    pub extensions_dir: String,
    pub merged_paths: Vec<String>,
    pub extensions: Vec<SysextImageStatus>,
    pub last_error: Option<String>,
}

/// Result of a `systemd-sysext` operation (`refresh`, `unmerge`, `upload`, `delete`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SysextActionResult {
    pub ok: bool,
    pub action: String,
    pub message: String,
}

/// Validates and sanitizes an uploaded or referenced `systemd-sysext` `.raw` image filename.
pub fn sanitize_sysext_filename(raw_name: &str) -> Result<String> {
    let trimmed = raw_name.trim();
    if trimmed.is_empty() || trimmed.len() > 128 {
        return Err(ModelError::Validation(
            "sysext filename must be 1..=128 characters".to_string(),
        ));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return Err(ModelError::Validation(format!(
            "sysext filename '{trimmed}' contains invalid path characters"
        )));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return Err(ModelError::Validation(format!(
            "sysext filename '{trimmed}' must contain only [a-zA-Z0-9._-]"
        )));
    }
    if !std::path::Path::new(trimmed)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("raw"))
    {
        return Err(ModelError::Validation(format!(
            "sysext image '{trimmed}' must use the .raw extension"
        )));
    }
    Ok(trimmed.to_string())
}
