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
        self.validate_container_name(name)
    }

    pub fn validate_volume_name(&self, name: &str) -> Result<()> {
        self.validate_container_name(name)
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

/// Escape control and ANSI non-printable bytes for safe terminal output (issue #05-3).
pub fn escape_name(s: &str) -> String {
    s.chars().flat_map(|c| c.escape_default()).collect()
}

/// True when `source` is a path on disk rather than an image reference.
///
/// The reference's rule (`install.py:_is_local_path`): a source is local only
/// when it carries explicit path syntax - a leading `/`, `./`, `../`, or `~`. A
/// bare token like `ubuntu` stays a registry reference even when a file of that
/// name exists in the working directory.
pub fn is_local_source(source: &str) -> bool {
    source.starts_with('/')
        || source.starts_with("./")
        || source.starts_with("../")
        || source.starts_with('~')
}

/// True when `source` is an `http://` or `https://` URL (the reference's `_is_url`).
pub fn is_url_source(source: &str) -> bool {
    source.starts_with("http://") || source.starts_with("https://")
}

/// Archive extensions stripped when deriving a name from a filename.
///
/// The reference's `_ARCHIVE_EXTS`, in its order - the first suffix that matches
/// wins, so a compound extension listed earlier is stripped whole.
const ARCHIVE_EXTS: &[&str] = &[
    ".tar.gz",
    ".tgz",
    ".tar.bz2",
    ".tbz2",
    ".tar.xz",
    ".txz",
    ".oci.tar.xz",
    ".oci.tar.gz",
    ".oci.tar.zst",
    ".oci.tar",
    ".tar.lzma",
    ".tlzma",
    ".tar.zst",
    ".tzst",
    ".tar",
];

/// Derive a container name from an archive filename or URL path.
///
/// The reference's `_derive_local_name`: take the basename, strip one archive
/// extension, lowercase, replace every character outside `[a-z0-9_.-]` with `-`,
/// drop leading non-alphanumerics, collapse `--` runs and trim `-`. Returns the
/// empty string when nothing usable remains, so the caller can refuse by name.
pub fn derive_local_name(path: &str) -> String {
    let base = path.rsplit_once('/').map(|(_, b)| b).unwrap_or(path);
    let lower = base.to_ascii_lowercase();
    let stem = ARCHIVE_EXTS
        .iter()
        .find_map(|ext| {
            lower
                .strip_suffix(ext)
                .map(|_| &base[..base.len() - ext.len()])
        })
        .unwrap_or(base);

    let mapped: String = stem
        .chars()
        .map(|c| {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();

    let mut collapsed = String::with_capacity(mapped.len());
    let mut prev_dash = false;
    for c in mapped
        .trim_start_matches(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit()))
        .chars()
    {
        if c == '-' {
            if prev_dash {
                continue;
            }
            prev_dash = true;
        } else {
            prev_dash = false;
        }
        collapsed.push(c);
    }
    collapsed.trim_matches('-').to_string()
}

/// Derive a short local alias from an image reference.
///
/// The reference's `derive_alias` (via `parse_image_ref`): drop a digest pin,
/// drop the registry host when the first component is one (it carries a `.`, a
/// `:`, or is `localhost`), then take the last path component and drop its tag.
/// So `ghcr.io:5000/acme/web:1.2` is `web`, not `ghcr.io`, and a port is never
/// mistaken for the tag.
pub fn derive_alias(reference: &str) -> String {
    let without_digest = reference.split('@').next().unwrap_or(reference);
    let path = match without_digest.split_once('/') {
        Some((first, rest))
            if first.contains('.') || first.contains(':') || first == "localhost" =>
        {
            rest
        }
        _ => without_digest,
    };
    let last = path.rsplit('/').next().unwrap_or(path);
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

    #[test]
    fn rejects_path_traversal_in_volume_and_network_names() {
        let v = super::NameValidator::new();
        for bad in [
            "x/../../../../PWNED",
            "../escape",
            "/abs/path",
            "a/b",
            "win\\slash",
            "..",
            ".",
        ] {
            assert!(
                v.validate_volume_name(bad).is_err(),
                "validate_volume_name({bad:?}) must fail"
            );
            assert!(
                v.validate_network_name(bad).is_err(),
                "validate_network_name({bad:?}) must fail"
            );
        }
    }

    #[test]
    fn rejects_control_bytes_and_spaces() {
        let v = super::NameValidator::new();
        let long_name = "a".repeat(65);
        for bad in [
            "a\x01b",
            "\x1b[31mred",
            "hello world",
            " ",
            "\t",
            "\n",
            &long_name,
        ] {
            assert!(
                v.validate_volume_name(bad).is_err(),
                "validate_volume_name({bad:?}) must fail"
            );
            assert!(
                v.validate_network_name(bad).is_err(),
                "validate_network_name({bad:?}) must fail"
            );
        }
    }

    #[test]
    fn escape_name_sanitizes_nonprintable_and_ansi() {
        assert_eq!(super::escape_name("normal-name_1"), "normal-name_1");
        assert_eq!(super::escape_name("a\x01b"), "a\\u{1}b");
        assert_eq!(super::escape_name("\x1b[31mred"), "\\u{1b}[31mred");
        assert_eq!(super::escape_name("line\nbreak"), "line\\nbreak");
    }
}

#[cfg(test)]
mod source_name_tests {
    //! The reference's name-derivation rules, asserted against its own doc
    //! examples: `helpers/docker/refs.py:derive_alias` and
    //! `commands/install.py:_derive_local_name`.

    use super::{derive_alias, derive_local_name, is_local_source, is_url_source};

    #[test]
    fn a_bare_token_is_an_image_but_explicit_path_syntax_is_local() {
        for local in [
            "./rootfs.tar.gz",
            "/abs/rootfs.tar",
            "../up.tar.xz",
            "~/x.tar",
        ] {
            assert!(
                is_local_source(local),
                "{local:?} must be read as a local path"
            );
        }
        for image in ["ubuntu", "alpine:3.20", "ghcr.io/a/b:c", "rootfs.tar.gz"] {
            assert!(
                !is_local_source(image),
                "{image:?} must stay a registry reference"
            );
        }
        assert!(is_url_source("https://host/rootfs.tar.xz"));
        assert!(is_url_source("http://127.0.0.1:1/x"));
        assert!(!is_url_source("ftp://host/x"));
        assert!(!is_url_source("./https://x"));
    }

    #[test]
    fn derive_alias_matches_the_reference() {
        // The exact doc examples from refs.py:derive_alias.
        assert_eq!(derive_alias("ubuntu:24.04"), "ubuntu");
        assert_eq!(derive_alias("myuser/img:tag"), "img");
        assert_eq!(derive_alias("ghcr.io/foo/bar:tag"), "bar");
        // A port in the registry host is not the tag.
        assert_eq!(derive_alias("localhost:5000/foo:tag"), "foo");
        assert_eq!(derive_alias("ghcr.io:5000/acme/web:1.2"), "web");
        // A digest pin is dropped, not mistaken for a tag.
        assert_eq!(derive_alias("debian@sha256:0123456789abcdef"), "debian");
        assert_eq!(derive_alias("docker.io/library/ubuntu:24.04"), "ubuntu");
        assert_eq!(derive_alias("alpine"), "alpine");
    }

    #[test]
    fn derive_local_name_matches_the_reference() {
        assert_eq!(derive_local_name("./rootfs.tar.gz"), "rootfs");
        assert_eq!(
            derive_local_name("/a/b/alpine-rootfs.tar.xz"),
            "alpine-rootfs"
        );
        assert_eq!(derive_local_name("debian.oci.tar"), "debian");
        assert_eq!(derive_local_name("alpine-latest.tgz"), "alpine-latest");
        // Uppercase is lowered; every other byte becomes a single `-`.
        assert_eq!(derive_local_name("./My Rootfs (1).tar.gz"), "my-rootfs-1");
        // A URL path answers like a filename: query and fragment are the caller's
        // to strip, and the basename is what remains.
        assert_eq!(
            derive_local_name("/dl/Ubuntu_24.04.tar.zst"),
            "ubuntu_24.04"
        );
        // Nothing usable left -> the empty string, so the caller refuses by name.
        assert_eq!(derive_local_name("./.tar.gz"), "");
        assert_eq!(derive_local_name("./***"), "");
    }
}
