//! Storage/Volume module - Stub for future implementation

use anyhow::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Volume configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VolumeConfig {
    pub name: String,
    pub driver: String,
    pub labels: std::collections::HashMap<String, String>,
    pub options: std::collections::HashMap<String, String>,
}

/// Volume information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeInfo {
    pub name: String,
    pub driver: String,
    pub mountpoint: PathBuf,
    pub labels: std::collections::HashMap<String, String>,
    pub options: std::collections::HashMap<String, String>,
    pub size: u64,
}

/// Root directory holding volume data.
///
/// The mountpoint used to be hardcoded to `/var/lib/podnext/volumes/<name>`,
/// which does not exist on Termux/Android (no writable `/var/lib`, no root), so
/// every volume advertised an unusable path. Keep the data beside the
/// containers instead, where the user can actually write.
fn volumes_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".podnext")
        .join("volumes")
}

/// Storage manager
pub struct StorageManager {
    volumes: HashMap<String, VolumeConfig>,
    config_path: PathBuf,
    storage_root: PathBuf,
}

impl StorageManager {
    pub fn new() -> Result<Self, Error> {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| crate::paths::temp_base())
            .join("podnext");
        std::fs::create_dir_all(&config_dir)?;
        let config_path = config_dir.join("volumes.toml");

        let mut manager = Self {
            volumes: HashMap::new(),
            config_path,
            storage_root: volumes_root(),
        };
        manager.load()?;
        Ok(manager)
    }

    #[cfg(test)]
    pub fn tmp_manager(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("pdn-vol-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self {
            volumes: HashMap::new(),
            config_path: dir.join("volumes.toml"),
            storage_root: dir.join("volumes"),
        }
    }

    pub fn mountpoint(&self, name: &str) -> PathBuf {
        self.storage_root.join(name)
    }

    fn load(&mut self) -> Result<()> {
        if self.config_path.exists() {
            let content = std::fs::read_to_string(&self.config_path)?;
            let volumes: HashMap<String, VolumeConfig> = toml::from_str(&content)?;
            self.volumes = volumes;
        }
        Ok(())
    }

    fn save(&self) -> Result<()> {
        let content = toml::to_string_pretty(&self.volumes)?;
        std::fs::write(&self.config_path, content)?;
        Ok(())
    }

    pub fn create_volume(&mut self, config: &VolumeConfig) -> Result<()> {
        crate::names::NameValidator::new().validate_volume_name(&config.name)?;
        if self.volumes.contains_key(&config.name) {
            return Err(Error::msg(format!(
                "Volume '{}' already exists",
                crate::names::escape_name(&config.name)
            )));
        }
        // Materialise the mountpoint: a volume the guest can bind into must
        // have a real directory to bind from.
        let mountpoint = self.mountpoint(&config.name);
        std::fs::create_dir_all(&mountpoint)?;

        self.volumes.insert(config.name.clone(), config.clone());
        self.save()?;
        Ok(())
    }

    pub fn remove_volume(&mut self, name: &str) -> Result<()> {
        crate::names::NameValidator::new().validate_volume_name(name)?;
        if self.volumes.remove(name).is_some() {
            let mountpoint = self.mountpoint(name);
            if mountpoint.is_dir() {
                std::fs::remove_dir_all(&mountpoint)?;
            } else if mountpoint.exists() || mountpoint.is_symlink() {
                std::fs::remove_file(&mountpoint)?;
            }
            self.save()?;
            Ok(())
        } else {
            Err(Error::msg(format!(
                "Volume '{}' not found",
                crate::names::escape_name(name)
            )))
        }
    }

    pub fn list_volumes(&self) -> Result<Vec<VolumeInfo>> {
        let infos: Vec<VolumeInfo> = self
            .volumes
            .iter()
            .map(|(_, config)| VolumeInfo {
                name: config.name.clone(),
                driver: config.driver.clone(),
                mountpoint: self.mountpoint(&config.name),
                labels: config.labels.clone(),
                options: config.options.clone(),
                size: 0,
            })
            .collect();
        Ok(infos)
    }

    pub fn inspect_volume(&self, name: &str) -> Result<VolumeInfo> {
        crate::names::NameValidator::new().validate_volume_name(name)?;
        if let Some(config) = self.volumes.get(name) {
            Ok(VolumeInfo {
                name: config.name.clone(),
                driver: config.driver.clone(),
                mountpoint: self.mountpoint(&config.name),
                labels: config.labels.clone(),
                options: config.options.clone(),
                size: 0,
            })
        } else {
            Err(Error::msg(format!(
                "Volume '{}' not found",
                crate::names::escape_name(name)
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_volume_rejects_path_traversal() {
        let mut sm = StorageManager::tmp_manager("traversal");
        let bad_names = [
            "x/../../../../PWNED",
            "../escape",
            "/abs/path",
            "a/b",
            "win\\slash",
            "..",
            ".",
        ];
        for bad in bad_names {
            let cfg = VolumeConfig {
                name: bad.to_string(),
                ..Default::default()
            };
            assert!(
                sm.create_volume(&cfg).is_err(),
                "create_volume({bad:?}) must fail"
            );
        }
        assert!(!sm.storage_root.parent().unwrap().join("PWNED").exists());
        let _ = std::fs::remove_dir_all(sm.storage_root.parent().unwrap());
    }

    #[test]
    fn remove_volume_deletes_directory_and_metadata() {
        let mut sm = StorageManager::tmp_manager("rm");
        let cfg = VolumeConfig {
            name: "testvol".to_string(),
            ..Default::default()
        };
        sm.create_volume(&cfg).unwrap();
        let mp = sm.mountpoint("testvol");
        assert!(mp.exists());
        assert!(mp.is_dir());

        // Create a file inside to ensure remove_dir_all handles non-empty dirs
        std::fs::write(mp.join("data.txt"), "hello").unwrap();

        sm.remove_volume("testvol").unwrap();
        assert!(!mp.exists());
        assert!(sm.inspect_volume("testvol").is_err());
        assert!(sm.list_volumes().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(sm.storage_root.parent().unwrap());
    }

    #[test]
    fn remove_missing_volume_returns_error() {
        let mut sm = StorageManager::tmp_manager("missing");
        let res = sm.remove_volume("ghostvol");
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("not found"));
        let _ = std::fs::remove_dir_all(sm.storage_root.parent().unwrap());
    }

    #[test]
    fn inspect_and_rm_reject_empty_or_invalid_names() {
        let mut sm = StorageManager::tmp_manager("empty");
        assert!(sm.inspect_volume("").is_err());
        assert!(sm.remove_volume("").is_err());
        assert!(sm.inspect_volume("..").is_err());
        assert!(sm.remove_volume("..").is_err());
        let _ = std::fs::remove_dir_all(sm.storage_root.parent().unwrap());
    }
}
