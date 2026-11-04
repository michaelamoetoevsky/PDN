//! TOML loading for [`GatewayConfig`].
//!
//! A PDN configuration file namespaces its gateway settings under a top-level
//! `[gateway]` table (see `docs/src/configuration.md`). The same global file
//! also carries unrelated sections (`[general]`, `[grpc]`, `[api]`, ...) that
//! the gateway does not own, so the top level here is intentionally permissive
//! while the `[gateway]` table itself stays strict: every struct in
//! [`crate::config`] is `#[serde(deny_unknown_fields)]`, so an unknown key
//! anywhere under `[gateway]` is rejected with the offending key and its
//! line/column named.

use crate::config::GatewayConfig;
use std::path::{Path, PathBuf};

/// Errors returned while locating or loading gateway configuration.
///
/// `NotFound` is deliberately distinct from `Parse`: a missing file is a
/// deployment mistake and a malformed file is a configuration bug, and callers
/// need to tell them apart.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("gateway configuration file not found: {0}")]
    NotFound(PathBuf),
    #[error("failed to read gateway configuration file {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot parse gateway configuration: {0}")]
    Parse(#[from] toml::de::Error),
}

/// The slice of a PDN config file that the gateway reads.
///
/// This wrapper is intentionally NOT `deny_unknown_fields`: sibling top-level
/// sections belong to other subsystems and must not block gateway loading.
#[derive(Debug, serde::Deserialize)]
struct GatewaySection {
    gateway: GatewayConfig,
}

impl GatewayConfig {
    /// Parse the `[gateway]` table of a PDN config file from a TOML string.
    ///
    /// Unknown keys under `[gateway]` are rejected; the error names the
    /// offending key and its line/column.
    pub fn from_toml_str(s: &str) -> Result<Self, ConfigError> {
        Ok(toml::from_str::<GatewaySection>(s)?.gateway)
    }

    /// Load the `[gateway]` table of a PDN config file from an explicit path.
    ///
    /// There is no implicit search path: the caller supplies the file.
    pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                ConfigError::NotFound(path.to_path_buf())
            } else {
                ConfigError::Read {
                    path: path.to_path_buf(),
                    source,
                }
            }
        })?;
        Self::from_toml_str(&text)
    }
}
