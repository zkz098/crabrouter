use std::fs;
use std::path::{Path, PathBuf};

use crabrouter_model::RouterConfig;

use crate::{CoreError, Result};

#[derive(Debug, Clone)]
pub struct ConfigStore {
    config_path: PathBuf,
}

impl ConfigStore {
    pub fn new(config_path: impl Into<PathBuf>) -> Self {
        Self {
            config_path: config_path.into(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.config_path
    }

    pub fn load_or_init(&self) -> Result<RouterConfig> {
        if !self.config_path.exists() {
            let cfg = RouterConfig::default();
            self.save_atomic(&cfg)?;
            return Ok(cfg);
        }
        let raw = fs::read_to_string(&self.config_path)?;
        let cfg: RouterConfig = toml::from_str(&raw).map_err(|e| CoreError::Toml(e.to_string()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn save_atomic(&self, cfg: &RouterConfig) -> Result<()> {
        cfg.validate()?;
        if let Some(parent) = self.config_path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        let encoded = toml::to_string_pretty(cfg).map_err(|e| CoreError::Toml(e.to_string()))?;
        let tmp_path = self.config_path.with_extension("toml.tmp");
        fs::write(&tmp_path, encoded)?;
        fs::rename(&tmp_path, &self.config_path)?;
        Ok(())
    }
}
