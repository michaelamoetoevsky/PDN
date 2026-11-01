//! Name validation and management

use anyhow::Result;
use std::collections::HashSet;

/// Name validator for containers and images
pub struct NameValidator {
    reserved_names: HashSet<String>,
}

impl NameValidator {
    pub fn new() -> Self {
        let mut reserved = HashSet::new();
        reserved.insert("default".to_string());
        reserved.insert("host".to_string());
        reserved.insert("none".to_string());
        reserved.insert("bridge".to_string());
        reserved.insert("all".to_string());
        reserved.insert("pn-next".to_string());
        reserved.insert("proot".to_string());
        reserved.insert("distro".to_string());

        Self {
            reserved_names: reserved,
        }
    }

    pub fn validate_container_name(&self, name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(anyhow::anyhow!("Container name cannot be empty"));
        }

        if name.len() > 64 {
            return Err(anyhow::anyhow!("Container name too long (max 64 chars)"));
        }

        if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(anyhow::anyhow!(
                "Container name can only contain alphanumeric, hyphen, and underscore"
            ));
        }

        if name.starts_with('-') || name.ends_with('-') {
            return Err(anyhow::anyhow!(
                "Container name cannot start or end with hyphen"
            ));
        }

        if self.reserved_names.contains(name) {
            return Err(anyhow::anyhow!("Name '{}' is reserved", name));
        }

        Ok(())
    }

    pub fn validate_image_name(&self, name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(anyhow::anyhow!("Image name cannot be empty"));
        }

        // Docker-style image name validation
        let parts: Vec<&str> = name.split('/').collect();
        if parts.len() > 3 {
            return Err(anyhow::anyhow!("Invalid image name format"));
        }

        for part in &parts {
            if part.is_empty() {
                return Err(anyhow::anyhow!("Image name cannot have empty parts"));
            }
            if !part
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
            {
                return Err(anyhow::anyhow!(
                    "Image name parts can only contain alphanumeric, hyphen, underscore, and dot"
                ));
            }
        }

        Ok(())
    }

    pub fn validate_network_name(&self, name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(anyhow::anyhow!("Network name cannot be empty"));
        }

        if name.len() > 32 {
            return Err(anyhow::anyhow!("Network name too long (max 32 chars)"));
        }

        if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(anyhow::anyhow!(
                "Network name can only contain alphanumeric, hyphen, and underscore"
            ));
        }

        Ok(())
    }

    pub fn validate_volume_name(&self, name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(anyhow::anyhow!("Volume name cannot be empty"));
        }

        if name.len() > 64 {
            return Err(anyhow::anyhow!("Volume name too long (max 64 chars)"));
        }

        if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            return Err(anyhow::anyhow!(
                "Volume name can only contain alphanumeric, hyphen, underscore, and dot"
            ));
        }

        Ok(())
    }
}

impl Default for NameValidator {
    fn default() -> Self {
        Self::new()
    }
}
