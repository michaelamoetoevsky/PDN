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
//!
//! Durations are written as strings (`"30s"`, `"500ms"`, `"2m"`) by way of
//! [`crate::duration_fmt`], which also keeps the native `{ secs, nanos }` table
//! and bare integer seconds loadable. [`GatewayConfig::to_toml_str`] writes the
//! same form back out, so a loaded configuration round-trips.

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
    #[error("cannot serialise gateway configuration: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// The slice of a PDN config file that the gateway reads.
///
/// This wrapper is intentionally NOT `deny_unknown_fields`: sibling top-level
/// sections belong to other subsystems and must not block gateway loading.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
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

    /// Environment variable that names the gateway configuration file.
    ///
    /// Explicit by design: there is no implicit search path, so a deployment
    /// either names its file or gets "no configuration" - rather than a
    /// surprising file being picked up from the working directory.
    pub const CONFIG_ENV: &'static str = "PN_GATEWAY_CONFIG";

    /// Render this configuration back into the documented TOML shape.
    ///
    /// Symmetric with [`GatewayConfig::from_toml_str`]: the result nests the
    /// settings under `[gateway]` exactly as the loader expects, and durations
    /// are written in the documented string form (`"60s"`). Round-tripping is
    /// asserted by `tests/config_loader.rs`, so `include`-style tools and
    /// generated config can be trusted to reload.
    pub fn to_toml_str(&self) -> Result<String, ConfigError> {
        Ok(toml::to_string(&GatewaySection {
            gateway: self.clone(),
        })?)
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
