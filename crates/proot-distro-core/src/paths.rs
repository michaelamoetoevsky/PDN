//! Path management for PRoot Distro

use anyhow::Result;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Resolve a writable temp base directory from explicit environment values.
///
/// Split out from [`temp_base`] so the resolution order is testable without
/// mutating the process environment.
fn resolve_temp_base(tmpdir: Option<OsString>, prefix: Option<OsString>) -> PathBuf {
    tmpdir
        .map(PathBuf::from)
        .or_else(|| prefix.map(|p| PathBuf::from(p).join("tmp")))
        .unwrap_or_else(std::env::temp_dir)
}

/// Resolve a writable temp base directory.
///
/// `/tmp` is not writable by an app uid on Android, and `std::env::temp_dir()`
/// falls back to it when `TMPDIR` is unset, so it must never be used alone.
/// Order: `$TMPDIR` -> `$PREFIX/tmp` -> `std::env::temp_dir()`.
pub fn temp_base() -> PathBuf {
    resolve_temp_base(std::env::var_os("TMPDIR"), std::env::var_os("PREFIX"))
}

pub fn resolve_home() -> Result<PathBuf> {
    resolve_home_from(
        std::env::var("PN_TEST_ROOT").ok(),
        std::env::var("HOME").ok(),
        std::env::var("USERPROFILE").ok(),
        dirs::home_dir(),
    )
}

fn resolve_home_from(
    test_root: Option<String>,
    home: Option<String>,
    userprofile: Option<String>,
    dirs_home: Option<PathBuf>,
) -> Result<PathBuf> {
    test_root
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| home.filter(|s| !s.trim().is_empty()).map(PathBuf::from))
        .or_else(|| {
            userprofile
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from)
        })
        .or_else(|| dirs_home.filter(|p| p != Path::new("/")))
        .ok_or_else(|| anyhow::anyhow!("HOME is not set / cannot resolve store"))
}

/// Container paths
pub struct ContainerPaths {
    root_dir: PathBuf,
    pub containers_dir: PathBuf,
    pub images_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub log_dir: PathBuf,
    pub config_dir: PathBuf,
}

impl ContainerPaths {
    pub fn new() -> Result<Self> {
        let root = resolve_home()?;

        // Ensure we can create the directory
        std::fs::create_dir_all(&root)?;

        // Use .podnext directory for consistency with native CLI
        let podnext_root = root.join(".podnext");
        std::fs::create_dir_all(&podnext_root)?;

        Ok(Self {
            root_dir: root.clone(),
            containers_dir: podnext_root.join("containers"),
            images_dir: podnext_root.join("images"),
            cache_dir: podnext_root.join("cache"),
            log_dir: podnext_root.join("logs"),
            config_dir: podnext_root.join("config"),
        })
    }

    pub fn container_rootfs(&self, name: &str) -> PathBuf {
        self.containers_dir.join(name).join("rootfs")
    }

    pub fn container_config(&self, name: &str) -> PathBuf {
        self.containers_dir.join(name).join("config.json")
    }

    pub fn container_state(&self, name: &str) -> PathBuf {
        self.containers_dir.join(name).join("state.json")
    }

    pub fn container_logs(&self, name: &str) -> PathBuf {
        self.log_dir.join(name)
    }

    pub fn image_path(&self, name: &str) -> PathBuf {
        self.images_dir.join(name)
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in &[
            &self.containers_dir,
            &self.images_dir,
            &self.cache_dir,
            &self.log_dir,
            &self.config_dir,
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

/// Path resolver for guest/host path translation
pub struct PathResolver {
    guest_root: PathBuf,
    host_root: PathBuf,
}

impl PathResolver {
    pub fn new(guest_root: PathBuf, host_root: PathBuf) -> Self {
        Self {
            guest_root,
            host_root,
        }
    }

    pub fn guest_to_host(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            let relative = path.strip_prefix(&self.guest_root).unwrap_or(path);
            self.host_root.join(relative)
        } else {
            self.host_root.join(path)
        }
    }

    pub fn host_to_guest(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            let relative = path.strip_prefix(&self.host_root).unwrap_or(path);
            self.guest_root.join(relative)
        } else {
            self.guest_root.join(path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmpdir_takes_priority() {
        let got = resolve_temp_base(
            Some(OsString::from("/custom/tmp")),
            Some(OsString::from("/data/usr")),
        );
        assert_eq!(got, PathBuf::from("/custom/tmp"));
    }

    #[test]
    fn prefix_tmp_used_when_tmpdir_unset_and_is_not_tmp() {
        let got = resolve_temp_base(None, Some(OsString::from("/data/usr")));
        assert_eq!(got, PathBuf::from("/data/usr/tmp"));
        assert_ne!(got, PathBuf::from("/tmp"));
    }

    #[test]
    fn resolve_home_fails_when_all_unset_or_root() {
        let err = resolve_home_from(None, None, None, None).unwrap_err();
        assert_eq!(err.to_string(), "HOME is not set / cannot resolve store");

        let err2 = resolve_home_from(None, None, None, Some(PathBuf::from("/"))).unwrap_err();
        assert_eq!(err2.to_string(), "HOME is not set / cannot resolve store");

        let err3 = resolve_home_from(Some("  ".into()), Some("".into()), None, None).unwrap_err();
        assert_eq!(err3.to_string(), "HOME is not set / cannot resolve store");
    }

    #[test]
    fn resolve_home_finds_valid_home() {
        let home = resolve_home_from(None, Some("/data/home".into()), None, None).unwrap();
        assert_eq!(home, PathBuf::from("/data/home"));

        let test_root = resolve_home_from(
            Some("/tmp/test".into()),
            Some("/data/home".into()),
            None,
            None,
        )
        .unwrap();
        assert_eq!(test_root, PathBuf::from("/tmp/test"));
    }
}

/// Termux and XDG aware host paths.
///
/// The Termux layout (`/data/data/<package>/files/{home,usr}`) is a default, not a law:
/// the app's own environment names the real locations, and `termux-exec`/`proot-distro`
/// export them. Roadmap §9.2 asks for these to be honoured instead of hard-coded, so the
/// binary keeps working when the home or the prefix moves (a second profile, a container,
/// a relocated install). Every resolver follows the documented order and falls back to the
/// default only when nothing is set.
pub mod host {
    use std::path::{Path, PathBuf};

    /// `/data/data/com.termux/files/usr`, the documented default.
    pub const DEFAULT_PREFIX: &str = "/data/data/com.termux/files/usr";
    /// `com.termux`, the documented default package name.
    pub const DEFAULT_PACKAGE: &str = "com.termux";

    fn first_non_empty(values: &[Option<String>]) -> Option<PathBuf> {
        values
            .iter()
            .flatten()
            .map(|v| v.trim())
            .find(|v| !v.is_empty())
            .map(PathBuf::from)
    }

    /// Resolve the prefix from explicit values; split out so the order is testable
    /// without mutating this process's environment.
    pub fn resolve_prefix(termux_prefix: Option<String>, prefix: Option<String>) -> PathBuf {
        first_non_empty(&[termux_prefix, prefix]).unwrap_or_else(|| PathBuf::from(DEFAULT_PREFIX))
    }

    /// The Termux prefix: `TERMUX__PREFIX`, then `PREFIX`, then the default.
    pub fn prefix() -> PathBuf {
        resolve_prefix(
            std::env::var("TERMUX__PREFIX").ok(),
            std::env::var("PREFIX").ok(),
        )
    }

    /// The Termux home: `TERMUX__HOME`, then `HOME`. `None` when neither is usable, which
    /// callers must treat as an error rather than guessing a location.
    pub fn home() -> Option<PathBuf> {
        resolve_home(
            std::env::var("TERMUX__HOME").ok(),
            std::env::var("HOME").ok(),
        )
    }

    pub fn resolve_home(termux_home: Option<String>, home: Option<String>) -> Option<PathBuf> {
        first_non_empty(&[termux_home, home])
    }

    /// `XDG_RUNTIME_DIR`, else `$HOME/.local/run`, else the prefix's tmp directory. The
    /// last resort exists so a socket path can always be formed on a host with neither.
    pub fn runtime_dir() -> PathBuf {
        resolve_runtime_dir(
            std::env::var("XDG_RUNTIME_DIR").ok(),
            home(),
            Some(prefix()),
        )
    }

    pub fn resolve_runtime_dir(
        xdg_runtime: Option<String>,
        home: Option<PathBuf>,
        prefix: Option<PathBuf>,
    ) -> PathBuf {
        first_non_empty(&[xdg_runtime])
            .or_else(|| home.map(|h| h.join(".local/run")))
            .or_else(|| prefix.map(|p| p.join("tmp")))
            .unwrap_or_else(|| PathBuf::from("/data/local/tmp"))
    }

    /// `XDG_CONFIG_HOME`, else `$HOME/.config`.
    pub fn config_dir() -> Option<PathBuf> {
        resolve_config_dir(std::env::var("XDG_CONFIG_HOME").ok(), home())
    }

    pub fn resolve_config_dir(
        xdg_config: Option<String>,
        home: Option<PathBuf>,
    ) -> Option<PathBuf> {
        first_non_empty(&[xdg_config]).or_else(|| home.map(|h| h.join(".config")))
    }

    /// `XDG_STATE_HOME`, else `$HOME/.local/state`. Used for logs, which are state rather
    /// than cache or configuration.
    pub fn state_dir() -> Option<PathBuf> {
        resolve_state_dir(std::env::var("XDG_STATE_HOME").ok(), home())
    }

    pub fn resolve_state_dir(xdg_state: Option<String>, home: Option<PathBuf>) -> Option<PathBuf> {
        first_non_empty(&[xdg_state]).or_else(|| home.map(|h| h.join(".local/state")))
    }

    /// The Android package name of the host app: `TERMUX_APP__PACKAGE_NAME`, else
    /// `TERMUX__PACKAGE_NAME`, else the documented default.
    pub fn package_name() -> String {
        std::env::var("TERMUX_APP__PACKAGE_NAME")
            .ok()
            .or_else(|| std::env::var("TERMUX__PACKAGE_NAME").ok())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_PACKAGE.to_string())
    }

    /// The prefix for the host app named by [`package_name`], used when the prefix is not
    /// exported at all.
    pub fn prefix_for_package(package: &str) -> PathBuf {
        Path::new("/data/data").join(package).join("files/usr")
    }
}

#[cfg(test)]
mod host_path_tests {
    use super::host::*;
    use std::path::PathBuf;

    #[test]
    fn the_termux_variables_win_over_the_generic_ones() {
        assert_eq!(
            resolve_prefix(
                Some("/data/data/app.x/files/usr".into()),
                Some("/usr".into())
            ),
            PathBuf::from("/data/data/app.x/files/usr")
        );
        assert_eq!(
            resolve_prefix(None, Some("/opt/termux/usr".into())),
            PathBuf::from("/opt/termux/usr")
        );
        assert_eq!(resolve_prefix(None, None), PathBuf::from(DEFAULT_PREFIX));
        // An empty variable is not a value.
        assert_eq!(
            resolve_prefix(Some("   ".into()), Some("/opt/termux/usr".into())),
            PathBuf::from("/opt/termux/usr")
        );
    }

    #[test]
    fn the_home_prefers_the_termux_variable_and_refuses_to_guess() {
        assert_eq!(
            resolve_home(
                Some("/data/data/app.x/files/home".into()),
                Some("/root".into())
            ),
            Some(PathBuf::from("/data/data/app.x/files/home"))
        );
        assert_eq!(
            resolve_home(None, None),
            None,
            "no home must stay unresolved"
        );
    }

    #[test]
    fn the_runtime_directory_follows_xdg_then_home_then_the_prefix() {
        assert_eq!(
            resolve_runtime_dir(Some("/run/user/0".into()), Some(PathBuf::from("/h")), None),
            PathBuf::from("/run/user/0")
        );
        assert_eq!(
            resolve_runtime_dir(None, Some(PathBuf::from("/h")), None),
            PathBuf::from("/h/.local/run")
        );
        assert_eq!(
            resolve_runtime_dir(None, None, Some(PathBuf::from("/usr"))),
            PathBuf::from("/usr/tmp")
        );
    }

    #[test]
    fn configuration_and_state_follow_xdg() {
        assert_eq!(
            resolve_config_dir(Some("/etc/xdgapp".into()), Some(PathBuf::from("/h"))),
            Some(PathBuf::from("/etc/xdgapp"))
        );
        assert_eq!(
            resolve_config_dir(None, Some(PathBuf::from("/h"))),
            Some(PathBuf::from("/h/.config"))
        );
        assert_eq!(
            resolve_state_dir(None, Some(PathBuf::from("/h"))),
            Some(PathBuf::from("/h/.local/state"))
        );
        assert_eq!(resolve_state_dir(None, None), None);
    }

    #[test]
    fn a_package_specific_prefix_is_derived_from_the_package_name() {
        assert_eq!(
            prefix_for_package("com.termux"),
            PathBuf::from("/data/data/com.termux/files/usr")
        );
    }
}
