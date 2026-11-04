//! Bindings management for PRoot Distro

use serde::{Deserialize, Serialize};

/// Bindings manager
pub struct BindingManager {
    binds: Vec<BindMount>,
}

impl BindingManager {
    pub fn new() -> Self {
        Self { binds: Vec::new() }
    }

    pub fn add_bind(&mut self, bind: BindMount) {
        self.binds.push(bind);
    }

    pub fn remove_bind(&mut self, target: &str) -> bool {
        self.binds.retain(|b| b.target != target);
        true
    }

    pub fn get_binds(&self) -> &[BindMount] {
        &self.binds
    }

    pub fn clear(&mut self) {
        self.binds.clear();
    }
}

/// Bind mount configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindMount {
    pub source: String,
    pub target: String,
    pub readonly: bool,
    pub recursive: bool,
    pub private: bool,
}

impl Default for BindMount {
    fn default() -> Self {
        Self {
            source: String::new(),
            target: String::new(),
            readonly: false,
            recursive: true,
            private: false,
        }
    }
}

/// Bind configuration for container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindConfig {
    pub binds: Vec<BindMount>,
    pub tmpfs: Vec<TmpfsMount>,
    pub devices: Vec<DeviceMount>,
}

impl Default for BindConfig {
    fn default() -> Self {
        Self {
            binds: Vec::new(),
            tmpfs: Vec::new(),
            devices: Vec::new(),
        }
    }
}

/// Tmpfs mount configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmpfsMount {
    pub target: String,
    pub size: Option<String>,
    pub mode: Option<u32>,
    pub options: Vec<String>,
}

/// Device mount configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceMount {
    pub source: String,
    pub target: String,
    pub permissions: String,
}

// Why a bind source can fail to be expressible
//
// PRoot parses a bind argument by splitting it on `:` (`<source>:<target>` or
// `<source>:<target>:<ro|rw>`), so a source path that contains a colon cannot be
// expressed. PRoot does not reject such an argument: it warns
// (`can't sanitize binding`) and drops the binding. That is how issue #35
// presented - a container built with a colon in its store key lost `/dev/shm`
// and the `/proc` stubs with nothing but warnings, so workloads needing shared
// memory (Chromium, PostgreSQL) failed later with errors pointing nowhere near
// the cause.
//
// The trigger is removed at its source (store keys are colon-free, see
// [`crate::names::store_key`]). These helpers are the second layer: a bind that
// cannot be expressed is a hard error naming the path instead of a silent loss
// of the mount. They matter for user-configured bind mounts, which the
// store-key fix cannot cover.

/// Refuse a bind source that PRoot cannot express. See the note above.
pub fn ensure_bindable(source: &str) -> anyhow::Result<()> {
    if source.contains(':') {
        anyhow::bail!(
            "bind source '{source}' contains ':', which PRoot cannot express: it splits bind \
             arguments on ':' and silently drops the binding. Move the path so it has no ':' \
             or drop the offending bind mount"
        );
    }
    Ok(())
}

/// Build a `<source>:<target>:<ro|rw>` bind argument, refusing anything PRoot
/// would silently drop. The single implementation behind
/// [`BindMount::to_argument`] and `container::BindMount::to_argument`, so the two
/// mount types cannot drift apart.
pub fn argument(source: &str, target: &str, readonly: bool) -> anyhow::Result<String> {
    ensure_bindable(source)?;
    if target.contains(':') {
        anyhow::bail!("bind target '{target}' contains ':', which PRoot cannot express");
    }
    Ok(format!(
        "{}:{}:{}",
        source,
        target,
        if readonly { "ro" } else { "rw" }
    ))
}

impl BindMount {
    /// Render this mount as the `--bind` argument PRoot expects.
    ///
    /// Returns an error rather than an argument PRoot would silently drop when
    /// the source or target contains a colon.
    pub fn to_argument(&self) -> anyhow::Result<String> {
        argument(&self.source, &self.target, self.readonly)
    }
}
