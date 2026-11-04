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

/// Directory component that stores an image or container reference.
///
/// A reference carries a registry host, a port, a namespace and a tag
/// (`ghcr.io:5000/acme/web:1.2`), none of which may reach the filesystem: the
/// store directory is a single path component under `~/.podnext/containers/`,
/// and `/` would nest it while `:` would make PRoot unable to bind anything
/// under it - the bind syntax is `source:target[:mode]` (issue #35).
///
/// The rule is the one `podnext install` has always used: keep the last path
/// component and drop everything from the first colon.
///
/// ```
/// # use proot_distro_core::names::store_key;
/// assert_eq!(store_key("debian:13"), "debian");
/// assert_eq!(store_key("ghcr.io:5000/acme/web:1.2"), "web");
/// assert_eq!(store_key("custom:latest"), "custom");
/// assert_eq!(store_key("alpine"), "alpine");
/// ```
pub fn store_key(reference: &str) -> String {
    let last = reference.rsplit('/').next().unwrap_or(reference);
    last.split(':').next().unwrap_or(last).to_string()
}

#[cfg(test)]
mod store_key_tests {
    use super::store_key;

    #[test]
    fn keeps_only_the_last_component_and_drops_the_tag() {
        assert_eq!(store_key("debian:13"), "debian");
        assert_eq!(store_key("ghcr.io:5000/acme/web:1.2"), "web");
        assert_eq!(store_key("custom:latest"), "custom");
        assert_eq!(store_key("alpine"), "alpine");
    }

    #[test]
    fn the_result_never_contains_a_colon_or_a_slash() {
        for reference in [
            "c2env:latest",
            "ghcr.io:5000/acme/web:1.2",
            "/abs/path/img:tag",
            "localhost:5000/ns/name:tag",
            "custom:latest",
        ] {
            let key = store_key(reference);
            assert!(
                !key.contains(':') && !key.contains('/'),
                "store_key({reference:?}) = {key:?} must be a single colon-free component"
            );
            assert!(
                !key.is_empty(),
                "store_key({reference:?}) must not be empty"
            );
        }
    }
}
