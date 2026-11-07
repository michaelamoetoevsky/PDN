//! The OCI / Docker Hub install path: registry search, manifest and layer
//! fetch, and [`install_image_native`].
//!
//! Split out of `native` in wave 7; behaviour unchanged.

use super::{digest, retry};
use anyhow::{bail, Context, Result};
use hex;
use indicatif::{ProgressBar, ProgressStyle};
use oci_spec::image::ImageManifest;
use proot_distro_core::refcount::REPLACED_PREFIX;
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use urlencoding::encode;

// ===== Docker Hub Search =====

#[derive(serde::Deserialize, Debug)]
pub struct DockerHubSearchResponse {
    #[allow(dead_code)]
    pub count: usize,
    pub results: Vec<DockerHubImage>,
}

#[derive(serde::Deserialize, Debug)]
pub struct DockerHubImage {
    pub repo_name: String,
    pub short_description: String,
    pub star_count: u32,
    pub pull_count: u64,
    pub is_official: bool,
    pub is_automated: bool,
}

fn http_client() -> Client {
    Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        // B2: credentials are bound to their origin. reqwest itself drops the sensitive headers
        // when the host or port changes; what it would otherwise still do - follow a same-host
        // HTTPS -> HTTP redirect, i.e. hand the bearer token to a cleartext connection - is
        // refused here, so the transfer fails instead (retry::plan_redirect owns the decision).
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let from = attempt.previous().last().unwrap_or(attempt.url());
            match retry::plan_redirect(from, attempt.url()) {
                retry::Redirect::Refuse(reason) => attempt.error(reason),
                retry::Redirect::Follow | retry::Redirect::FollowWithoutCredentials => {
                    attempt.follow()
                }
            }
        }))
        .build()
        .unwrap_or_else(|_| Client::new())
}

pub async fn search_docker_hub(query: &str, limit: usize) -> Result<Vec<DockerHubImage>> {
    let client = http_client();
    let url = format!(
        "https://registry.hub.docker.com/v2/search/repositories/?query={}&page_size={}",
        encode(query),
        limit.min(100)
    );

    let response = client
        .get(&url)
        .header(
            "User-Agent",
            format!("podnext/{}", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .context("Failed to send search request")?;

    if !response.status().is_success() {
        bail!("Docker Hub search failed: {}", response.status());
    }

    let search_response: DockerHubSearchResponse = response
        .json()
        .await
        .context("Failed to parse search response")?;

    Ok(search_response.results.into_iter().take(limit).collect())
}

// ===== OCI Image Download & Install =====

#[derive(serde::Deserialize, Debug)]
pub struct DockerHubTokenResponse {
    pub token: String,
}

#[derive(serde::Deserialize, Debug)]
pub struct ManifestList {
    pub manifests: Vec<ManifestDescriptor>,
}

#[derive(serde::Deserialize, Debug)]
pub struct ManifestDescriptor {
    pub digest: String,
    #[serde(rename = "mediaType")]
    #[allow(dead_code)]
    pub media_type: String,
    pub platform: Option<Platform>,
}

#[derive(serde::Deserialize, Debug)]
pub struct Platform {
    pub architecture: String,
    pub os: String,
}

#[allow(dead_code)]
struct ImageLayer {
    digest: String,
    size: u64,
    media_type: String,
}

pub(crate) async fn smoke_test_rootfs(rootfs: &Path) -> Result<()> {
    let has_sh = rootfs.join("bin/sh").symlink_metadata().is_ok()
        || rootfs.join("usr/bin/sh").symlink_metadata().is_ok();
    if !has_sh {
        return Ok(());
    }

    let proot_bin = std::env::var("PROOT_BIN").unwrap_or_else(|_| "proot".to_string());
    let mut cmd = tokio::process::Command::new(proot_bin);
    cmd.arg("--kill-on-exit")
        .arg("--link2symlink")
        .arg("--sysvipc")
        .arg("-L")
        .arg("--change-id=0:0")
        .arg("--rootfs=.")
        .current_dir(rootfs)
        .arg("--bind=/dev")
        .arg("--bind=/proc")
        .arg("--bind=/sys")
        .env_clear()
        .env(
            "PATH",
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        )
        .env("HOME", "/root")
        .arg("/bin/sh")
        .arg("-c")
        .arg("exit 0");

    match cmd.output().await {
        Ok(out) => {
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if stderr.contains("signal 4") || stderr.contains("SIGILL") {
                    eprintln!(
                        "Warning: guest shell terminated with signal 4 (SIGILL).\n\
                         The guest binaries appear incompatible with host CPU/PRoot.\n\
                         Consider using QEMU emulation (-q) or a different distribution."
                    );
                } else {
                    eprintln!(
                        "Warning: guest shell smoke test failed (status: {:?}): {}",
                        out.status.code(),
                        stderr.trim()
                    );
                }
            }
        }
        Err(e) => {
            eprintln!("Warning: could not execute post-extract smoke test: {}", e);
        }
    }
    Ok(())
}

/// Sidecar lock path for a staging directory: `<staging dir>.lock`.
///
/// The lock is created and taken *before* the staging tree exists, so a staging
/// directory whose lock file is missing is residue from an older build (or from
/// a kill before the lock was written) and can be reclaimed. A live install holds
/// the lock for its whole run, so [`reap_stale_staging`] never touches it.
fn staging_lock_path(stage_dir: &Path) -> PathBuf {
    proot_distro_core::refcount::sidecar_lock_path(stage_dir)
}

/// Create and take an exclusive `flock` on the staging directory's sidecar file.
///
/// Holding the returned handle is what marks the staging tree as live: the lock
/// is released when the handle is dropped or the process dies, which is exactly
/// the signal [`reap_stale_staging`] reads.
fn hold_staging_lock(stage_dir: &Path) -> Result<fs::File> {
    let path = staging_lock_path(stage_dir);
    let file =
        fs::File::create(&path).with_context(|| format!("could not create {}", path.display()))?;
    let ret = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if ret != 0 {
        bail!("could not lock staging directory {}", stage_dir.display());
    }
    Ok(file)
}

/// Reclaim staging residue: every `.tmp-install-*` / `.tmp-replaced-*` directory
/// that no live install owns (A3b).
///
/// An install that is `SIGKILL`ed used to leave its half-filled staging
/// directory behind with nothing to reclaim it, so every interruption added one
/// more permanent directory to the store. The sidecar lock is the liveness
/// signal: if this process can take a staging tree's lock, its owner is gone and
/// the tree is residue; if the lock file is absent the tree was never claimed and
/// is residue too. Called at the start of every install and from `remove`.
///
/// The decision now comes from `proot_distro_core::refcount`: every staging unit
/// carries the count of live references that hold it, and eviction is "delete iff
/// the count is zero" (WORKLIST C6). A count of one is reported, not evicted, so a
/// refusal says what held the tree instead of looking like a no-op.
pub fn reap_stale_staging(containers_dir: &Path) {
    for eviction in proot_distro_core::refcount::sweep(&proot_distro_core::refcount::staging_units(
        containers_dir,
    )) {
        if let Some(line) = eviction.message() {
            eprintln!("{line}");
        }
    }
}

/// Move a fully built staging tree into place as `target`.
///
/// The old code did `if target.exists() { remove_dir_all(target)? }` and then
/// `rename(stage, target)`; under two concurrent same-name installs that let the
/// loser delete the winner's freshly committed container (A4b). Here the `rename`
/// itself is the arbiter — it refuses with `ENOTEMPTY` when `target` already
/// holds a container — and the only container ever destroyed is the one that was
/// already present when *this* install began and that the caller explicitly asked
/// to replace with `--force`. A container that appeared while this install was
/// running therefore cannot be destroyed by it.
fn commit_staging(
    stage: &Path,
    target: &Path,
    container_name: &str,
    force: bool,
    target_preexisting: bool,
) -> Result<()> {
    let drop_staging = || {
        let _ = fs::remove_dir_all(stage);
        let _ = fs::remove_file(staging_lock_path(stage));
    };

    match fs::rename(stage, target) {
        Ok(()) => {
            let _ = fs::remove_file(staging_lock_path(stage));
            Ok(())
        }
        Err(e) if is_not_empty(&e) => {
            // `target` holds a container: either the one that was here when this
            // install started (a --force replace), or one a concurrent install
            // committed while we were building.
            if !(force && target_preexisting) {
                drop_staging();
                bail!(
                    "Container '{}' was installed while this install was still running; \
                     refusing to replace it (a concurrent install won)",
                    container_name
                );
            }
            // Replace the container that was already here: move it aside
            // atomically — never remove a committed container in place — swap the
            // new tree in, then drop the superseded one.
            let trash = target.with_file_name(format!(
                "{}{}-{}",
                REPLACED_PREFIX,
                container_name,
                uuid::Uuid::new_v4()
            ));
            fs::rename(target, &trash)?;
            fs::rename(stage, target)?;
            let _ = fs::remove_file(staging_lock_path(stage));
            let _ = fs::remove_dir_all(&trash);
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

/// `rename(2)` refuses to replace a non-empty directory with `ENOTEMPTY` (and
/// `EEXIST` on some kernels), which is how [`commit_staging`] learns the target
/// is a live container without a race-prone existence check.
fn is_not_empty(e: &std::io::Error) -> bool {
    matches!(e.raw_os_error(), Some(libc::ENOTEMPTY) | Some(libc::EEXIST))
}

/// Where an install takes its rootfs from.
enum Source {
    /// An OCI/Docker registry reference, pulled over the network.
    Registry(String),
    /// A local archive, or a URL already downloaded to `path`.
    Archive { label: String, path: PathBuf },
}

pub async fn install_image_native(
    image_ref: &str,
    container_name: &str,
    force: bool,
    rootfs_path: Option<PathBuf>,
) -> Result<PathBuf, anyhow::Error> {
    install_source(
        Source::Registry(image_ref.to_string()),
        container_name,
        force,
        rootfs_path,
    )
    .await
}

/// Install from a local archive (`./rootfs.tar.gz`, `./image.oci.tar`) or an
/// http(s) URL to one, verified against a sha256 checksum when one is given.
///
/// The checksum is bare lowercase hex, already normalised by the caller. When it
/// is absent the archive is used unverified and one named line says so; the
/// reference verifies nothing for an install (only a declared body length), so a
/// checksum is where podnext does better. A mismatch removes the download and
/// refuses, naming both digests.
pub async fn install_archive_native(
    source: &str,
    container_name: &str,
    force: bool,
    checksum: Option<&str>,
) -> Result<PathBuf, anyhow::Error> {
    let label = source.to_string();

    // A URL is downloaded first, so the checksum covers what was actually
    // received; a local path is used where it lies. The download lives in a
    // temporary that is dropped - and so removed - when this function returns.
    let downloaded = if proot_distro_core::names::is_url_source(source) {
        Some(download_to_temp(source).await?)
    } else {
        None
    };
    let path = match &downloaded {
        Some(tmp) => tmp.path().to_path_buf(),
        None => expand_home_source(source),
    };

    match checksum {
        Some(expected) => {
            let got = proot_distro_core::archive::sha256_file(&path)?;
            if got != expected {
                bail!(
                    "checksum mismatch for '{source}': expected sha256:{expected}, \
                     got sha256:{got}"
                );
            }
            eprintln!("Checksum verified: sha256:{got}");
        }
        None => eprintln!(
            "Warning: no --checksum given for '{source}'; the archive is used \
             unverified. Pass --checksum sha256:<hex> to verify it."
        ),
    }

    install_source(Source::Archive { label, path }, container_name, force, None).await
}

/// Download `url` into a temporary file, checking the declared body length.
///
/// The body is streamed to disk rather than buffered: a rootfs archive is large,
/// and only the length the server declared is checked here (the reference's
/// `require_complete_body`), the optional `--checksum` covering the rest.
async fn download_to_temp(url: &str) -> Result<tempfile::NamedTempFile> {
    let client = http_client();
    let mut resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("requesting {url}"))?;
    if !resp.status().is_success() {
        bail!(
            "failed to download archive: {url} returned {}",
            resp.status()
        );
    }

    let declared = resp.content_length();
    let mut tmp = tempfile::Builder::new()
        .prefix("pdn-download-")
        .tempfile()
        .context("creating a temporary file for the download")?;
    let mut received = 0u64;
    while let Some(chunk) = resp.chunk().await? {
        tmp.write_all(&chunk)?;
        received += chunk.len() as u64;
    }
    retry::ensure_complete_body(declared, received)
        .with_context(|| format!("downloading {url}"))?;
    tmp.flush()?;
    Ok(tmp)
}

/// Expand a leading `~`/`~/` against `$HOME` (the reference's `os.path.expanduser`).
fn expand_home_source(source: &str) -> PathBuf {
    if let Some(rest) = source.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    } else if source == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home);
        }
    }
    PathBuf::from(source)
}

/// The one install path: acquire the name lock, staging tree, fill the rootfs
/// from `source`, configure it, and commit it atomically.
async fn install_source(
    source: Source,
    container_name: &str,
    force: bool,
    rootfs_path: Option<PathBuf>,
) -> Result<PathBuf, anyhow::Error> {
    // Parse a registry reference up front - before the lock or any staging tree,
    // as the registry path always did - so an unparsable reference refuses with
    // no side effect. An archive carries no reference.
    let registry = match &source {
        Source::Registry(image_ref) => Some(parse_image_ref(image_ref)?),
        Source::Archive { .. } => None,
    };
    let label = match &source {
        Source::Registry(image_ref) => image_ref.clone(),
        Source::Archive { label, .. } => label.clone(),
    };

    let container_name = container_name.to_string();

    // Serialise installs of the same container name (WORKLIST A4). The lock primitive and its refusal
    // - which names the holder's PID - already existed in `proot_distro_core::locking`, but nothing on
    // THIS path ever took it, so two concurrent installs of one name both reported success and
    // `~/.podnext/locks/` stayed empty through every race. Held for the whole install: `ContainerLock`
    // releases on drop, so every error path below releases it too. The commit-time `ENOTEMPTY` arbiter
    // in `commit_staging` stays as the second line of defence (it is what makes a loss impossible even
    // if a lock is somehow not held), but a contender should now be refused BEFORE doing any work.
    let mut name_lock = proot_distro_core::locking::ContainerLock::new(&container_name, true);
    name_lock.acquire()?;

    println!("Installing '{}' as '{}'...", label, container_name);

    // `_staging_lock` holds an exclusive flock on the staging tree for as long as
    // this install runs. It must stay bound until this function returns:
    // `reap_stale_staging` reclaims a tree the moment that lock is free, and
    // dropping it early would let another install reap a live tree.
    let (target_container_dir, staging_dir, staging_rootfs, target_preexisting, _staging_lock) =
        if let Some(ref r) = rootfs_path {
            if r.exists() && !force {
                bail!(
                    "Container '{}' already exists at {}. Use --force to overwrite.",
                    container_name,
                    r.display()
                );
            }
            fs::create_dir_all(r)?;
            (None, None, r.clone(), false, None)
        } else {
            let containers_dir = proot_distro_core::paths::resolve_home()?
                .join(".podnext")
                .join("containers");
            fs::create_dir_all(&containers_dir)?;
            // Reclaim residue left by a previous install that was killed, before
            // adding more of it (A3b).
            reap_stale_staging(&containers_dir);

            let target_dir = containers_dir.join(&container_name);
            if target_dir.exists() && !force {
                bail!(
                    "Container '{}' already exists at {}. Use --force to overwrite.",
                    container_name,
                    target_dir.display()
                );
            }
            // Whether the container was already here when this install began is
            // what separates "replace what the caller asked to replace" from
            // "clobber a container a concurrent install just committed" (A4b).
            let target_preexisting = target_dir.exists();

            let staging_name = format!(".tmp-install-{}-{}", container_name, uuid::Uuid::new_v4());
            let stage_dir = containers_dir.join(staging_name);
            // Take the lock *before* creating the tree: a staging directory whose
            // sidecar lock is missing is treated as residue, so the lock has to
            // exist first.
            let lock = hold_staging_lock(&stage_dir)?;
            let stage_rootfs = stage_dir.join("rootfs");
            fs::create_dir_all(&stage_rootfs)?;
            (
                Some(target_dir),
                Some(stage_dir),
                stage_rootfs,
                target_preexisting,
                Some(lock),
            )
        };

    let install_work = async {
        // Fill the staging rootfs, then configure it the same way whatever the
        // source was: a registry pull and a local archive end in one place.
        let (image_name, image_tag) = match &source {
            Source::Registry(_) => {
                let (name, tag) = registry.clone().expect("parsed before the lock was taken");

                // Step 1: Get auth token from Docker Hub
                eprintln!("[1/4] Authenticating with Docker Hub registry...");
                let token = get_docker_hub_token(&name).await?;

                // Step 2: Get manifest list to find correct platform
                eprintln!(
                    "[2/4] Fetching image manifest (linux/{})...",
                    std::env::consts::ARCH
                );
                let manifest = get_manifest(&name, &tag, &token).await?;

                // Step 3: Download and extract layers
                eprintln!("[3/4] Downloading and extracting OCI layers (Android-safe)...");
                download_and_extract_layers(&name, &manifest, &token, &staging_rootfs).await?;

                (name, tag)
            }
            Source::Archive { path, .. } => {
                // A local archive or a downloaded URL: a plain rootfs tarball or
                // an OCI image layout, unpacked and (for the layout) verified
                // against the digests its own manifest declares.
                eprintln!("Extracting rootfs from '{}'...", path.display());
                proot_distro_core::archive::install_from_archive(path, &staging_rootfs)?;
                // A plain tarball records no image reference; an OCI archive's
                // reference lives in its manifest, which the container's own
                // metadata does not carry either. Both list from the guest's
                // os-release, so an empty reference here is honest, not a gap.
                (String::new(), String::new())
            }
        };

        // Step 4: Apply proot-compat patches
        eprintln!("[4/4] Configuring guest environment, DNS, and /dev/shm...");
        proot_compat::patch_rootfs(&staging_rootfs)?;

        configure_guest_network(&staging_rootfs, &container_name)?;

        if let Some(container_dir) = staging_rootfs.parent() {
            proot_distro_core::sysdata::ensure(container_dir)?;
            proot_distro_core::guestdirs::ensure(container_dir)?;
        }

        // Post-extract smoke test
        smoke_test_rootfs(&staging_rootfs).await?;

        // Step 5: Save container metadata
        save_container_metadata(&container_name, &image_name, &image_tag, &staging_rootfs)?;

        Ok::<(), anyhow::Error>(())
    }
    .await;

    if let Err(e) = install_work {
        if let Some(ref stage) = staging_dir {
            let _ = fs::remove_dir_all(stage);
            let _ = fs::remove_file(staging_lock_path(stage));
        } else if let Some(ref r) = rootfs_path {
            let _ = fs::remove_dir_all(r);
        }
        return Err(e);
    }

    let final_rootfs = if let (Some(target), Some(stage)) = (target_container_dir, staging_dir) {
        commit_staging(&stage, &target, &container_name, force, target_preexisting)?;
        target.join("rootfs")
    } else {
        staging_rootfs
    };

    println!("\nContainer '{}' installed successfully!", container_name);
    println!("Rootfs: {}", final_rootfs.display());
    println!("\nTo start the container:");
    println!("  podnext login {}", container_name);

    Ok(final_rootfs)
}

/// Check that an image reference can actually be fetched, before anything destructive.
///
/// `reset` deletes the container directory and only then installs the base image, so
/// an unfetchable reference — including one a `backup restore` wrote as a *container*
/// name rather than a registry reference — destroyed the container and then failed to
/// replace it. Resolving the manifest here costs one round trip and lets the caller
/// fail first, while the container is still on disk.
pub async fn verify_image_available(image_ref: &str) -> Result<()> {
    let (name, tag) = parse_image_ref(image_ref)?;
    let token = get_docker_hub_token(&name).await?;
    get_manifest(&name, &tag, &token).await?;
    Ok(())
}

/// Write the guest's network configuration files.
///
/// Docker/OCI rootfs images ship no `/etc/resolv.conf`, and Termux has none to
/// inherit, so a freshly installed PDN container had zero DNS: `apk update`
/// died with "temporary error" and `getent`/`nslookup` failed. proot-distro
/// solves this by writing resolv.conf at install time; we do the same.
///
/// Nameservers come from the host when it has any (Android's own resolver is
/// not visible from Termux, so this rarely hits) and otherwise fall back to the
/// public resolvers. `/etc/hosts` gets a minimal loopback entry plus the
/// container's own hostname so `hostname`-based lookups resolve locally.
fn configure_guest_network(rootfs: &Path, container_name: &str) -> Result<()> {
    let etc = rootfs.join("etc");
    fs::create_dir_all(&etc)?;

    let resolv = etc.join("resolv.conf");
    if !resolv.exists() {
        let contents = "nameserver 8.8.8.8\nnameserver 8.8.4.4\nnameserver 1.1.1.1\n";
        fs::write(&resolv, contents)?;
    }

    let hosts = etc.join("hosts");
    if !hosts.exists() {
        let contents = format!(
            "127.0.0.1\tlocalhost localhost.localdomain\n\
             ::1\t\tlocalhost localhost.localdomain\n\
             127.0.0.1\t{}\n",
            container_name
        );
        fs::write(&hosts, contents)?;
    }

    let hostname_file = etc.join("hostname");
    if !hostname_file.exists() {
        fs::write(&hostname_file, format!("{}\n", container_name))?;
    }

    Ok(())
}

fn parse_image_ref(image_ref: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = image_ref.split(':').collect();
    match parts.len() {
        1 => Ok((parts[0].to_string(), "latest".to_string())),
        2 => Ok((parts[0].to_string(), parts[1].to_string())),
        _ => bail!("Invalid image reference: {}", image_ref),
    }
}

async fn get_docker_hub_token(repo: &str) -> Result<String> {
    let client = http_client();
    // Use library/ prefix for official images in token scope
    let scope_repo = if repo.contains('/') {
        repo.to_string()
    } else {
        format!("library/{}", repo)
    };
    let url = format!(
        "https://auth.docker.io/token?service=registry.docker.io&scope=repository:{}:pull",
        scope_repo
    );

    let response = client.get(&url).send().await?;
    if !response.status().is_success() {
        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::NOT_FOUND
        {
            bail!(
                "Image '{}' not found (or private). Check the name with 'podnext search'.",
                repo
            );
        }
        bail!(
            "Failed to authenticate with registry: {}",
            response.status()
        );
    }
    let token_resp: DockerHubTokenResponse = response.json().await?;
    Ok(token_resp.token)
}

async fn get_manifest(name: &str, tag: &str, token: &str) -> Result<ImageManifest> {
    let client = http_client();
    // Add library/ prefix for official images (no slash in name)
    let manifest_name = if name.contains('/') {
        name.to_string()
    } else {
        format!("library/{}", name)
    };
    let url = format!(
        "https://registry.hub.docker.com/v2/{}/manifests/{}",
        manifest_name, tag
    );

    let response = client
        .get(&url)
        .bearer_auth(token)
        .header(
            "Accept",
            "application/vnd.docker.distribution.manifest.v2+json",
        )
        .header(
            "Accept",
            "application/vnd.docker.distribution.manifest.list.v2+json",
        )
        .header("Accept", "application/vnd.oci.image.manifest.v1+json")
        .send()
        .await?;

    if !response.status().is_success() {
        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::NOT_FOUND
        {
            bail!(
                "Image '{}:{}' not found (or private). Check the name with 'podnext search'.",
                name,
                tag
            );
        }
        bail!("Failed to fetch manifest: {}", response.status());
    }

    let content_type = {
        let headers = response.headers();
        headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    };

    let body = response.bytes().await?;

    // Handle both Docker manifest list and OCI image index
    if content_type.contains("manifest.list")
        || content_type.contains("manifest.list.v2")
        || content_type.contains("oci.image.index")
    {
        // Multi-platform manifest - find linux/arm64
        let manifest_list: ManifestList = serde_json::from_slice(&body)?;
        let arm64_manifest = manifest_list
            .manifests
            .iter()
            .find(|m| {
                m.platform
                    .as_ref()
                    .map(|p| p.architecture == "arm64" && p.os == "linux")
                    .unwrap_or(false)
            })
            .ok_or_else(|| anyhow::anyhow!("No linux/arm64 manifest found"))?;

        // Fetch the specific manifest
        return get_manifest_by_digest(name, &arm64_manifest.digest, token).await;
    }

    // Single platform manifest
    let manifest: ImageManifest = serde_json::from_slice(&body)?;
    Ok(manifest)
}

async fn get_manifest_by_digest(name: &str, digest: &str, token: &str) -> Result<ImageManifest> {
    let client = http_client();
    // Add library/ prefix for official images (no slash in name)
    let manifest_name = if name.contains('/') {
        name.to_string()
    } else {
        format!("library/{}", name)
    };
    let url = format!(
        "https://registry.hub.docker.com/v2/{}/manifests/{}",
        manifest_name, digest
    );

    let response = client
        .get(&url)
        .bearer_auth(token)
        .header(
            "Accept",
            "application/vnd.docker.distribution.manifest.v2+json",
        )
        .header("Accept", "application/vnd.oci.image.manifest.v1+json")
        .send()
        .await?;

    if !response.status().is_success() {
        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::NOT_FOUND
        {
            bail!(
                "Image manifest for '{}' (digest {}) not found (or private). Check the name with 'podnext search'.",
                name,
                digest
            );
        }
        bail!("Failed to fetch manifest by digest: {}", response.status());
    }

    let body = response.bytes().await?;
    let manifest: ImageManifest = serde_json::from_slice(&body)?;
    Ok(manifest)
}

async fn download_and_extract_layers(
    name: &str,
    manifest: &ImageManifest,
    token: &str,
    rootfs: &Path,
) -> Result<()> {
    let _client = http_client();
    let layers = manifest.layers();
    // The content-addressed store retains every layer blob under the digest that
    // names it, so a later image sharing this layer adds no bytes and a container
    // can be re-materialised from the store without the registry or an archive.
    let store = proot_distro_core::blobstore::BlobStore::new()?;
    let pb = ProgressBar::new(layers.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} layers ({msg})")
        .unwrap()
        .progress_chars("#>-"));

    for layer in layers {
        let digest_str = layer.digest().to_string();
        // B1: validate before this value reaches any path, URL or slice.
        digest::validate_digest(&digest_str)?;
        // A validated digest is `sha256:` + 64 hex, so this cannot panic; it is taken through
        // `get` so a refusal upstream can never be turned into a slice-bounds panic here.
        let short = digest_str.get(..20).unwrap_or(&digest_str);
        pb.set_message(format!("Downloading {}", short));

        let layer_data = download_layer(name, &digest_str, token).await?;

        // Verify digest
        let mut hasher = Sha256::new();
        hasher.update(&layer_data);
        let computed = hex::encode(hasher.finalize());
        let expected = digest_str.strip_prefix("sha256:").unwrap_or(&digest_str);

        if computed != expected {
            bail!(
                "Layer digest mismatch: expected {}, got {}",
                expected,
                computed
            );
        }

        pb.set_message(format!("Extracting {short}"));
        // Retain the verified compressed blob, then apply it from the store: the
        // same two steps the archive path takes, so both install sources feed one
        // deduplicating store.
        let media_type = layer.media_type().to_string();
        store.put_bytes(&digest_str, &layer_data)?;
        proot_distro_core::archive::apply_layer_from_store(
            &store,
            &digest_str,
            &media_type,
            rootfs,
        )?;

        pb.inc(1);
    }

    pb.finish_with_message("All layers extracted");
    Ok(())
}

/// One transfer attempt, for the retry loop in [`download_layer`].
enum AttemptOutcome {
    /// The body arrived whole.
    Body(Vec<u8>),
    /// The attempt failed; `status` is `None` when no response arrived at all (B3).
    Failed {
        status: Option<u16>,
        retry_after: Option<String>,
        note: String,
    },
}

/// One attempt: send, read, and check the body against its declared length.
///
/// A hard error is returned as one (a truncated body is a broken transfer, not a shorter layer);
/// a failed status is returned as data, because whether it may be repeated is the policy's
/// decision, not this function's.
async fn fetch_layer(client: &Client, url: &str, token: &str) -> Result<AttemptOutcome> {
    let response = match client.get(url).bearer_auth(token).send().await {
        Ok(response) => response,
        Err(e) => {
            return Ok(AttemptOutcome::Failed {
                status: None,
                retry_after: None,
                note: e.to_string(),
            })
        }
    };

    let status = response.status();
    if !status.is_success() {
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        return Ok(AttemptOutcome::Failed {
            status: Some(status.as_u16()),
            retry_after,
            note: status.to_string(),
        });
    }

    let declared = response.content_length();
    let body = response.bytes().await?;
    retry::ensure_complete_body(declared, body.len() as u64)?;
    Ok(AttemptOutcome::Body(body.to_vec()))
}

async fn download_layer(name: &str, digest: &str, token: &str) -> Result<Vec<u8>> {
    // B1: validate before any path - here the request path of the blob URL - is composed from the
    // digest. This is the choke point the comparator names (`docker/cache.py:validate_digest`).
    digest::validate_digest(digest)?;

    let client = http_client();
    // Add library/ prefix for official images (no slash in name)
    let manifest_name = if name.contains('/') {
        name.to_string()
    } else {
        format!("library/{}", name)
    };
    let url = format!(
        "https://registry.hub.docker.com/v2/{}/blobs/{}",
        manifest_name, digest
    );

    // B3: a real retry policy. Transient failures are repeated up to the budget; a deterministic
    // status (a 404, a 401) is not repeated at all, because a repeat cannot succeed.
    let mut attempt = 1;
    loop {
        match fetch_layer(&client, &url, token).await? {
            AttemptOutcome::Body(body) => return Ok(body),
            AttemptOutcome::Failed {
                status,
                retry_after,
                note,
            } => {
                let now = std::time::SystemTime::now();
                match retry::next_attempt(attempt, status, retry_after.as_deref(), now) {
                    retry::Decision::Retry { delay } => {
                        eprintln!(
                            "layer {digest}: {note}; attempt {attempt} failed, retrying in {delay:?}"
                        );
                        tokio::time::sleep(delay).await;
                        attempt += 1;
                    }
                    retry::Decision::GiveUp { reason } => {
                        bail!("Failed to download layer {digest}: {note} ({reason})")
                    }
                }
            }
        }
    }
}

fn save_container_metadata(
    container_name: &str,
    image_name: &str,
    image_tag: &str,
    rootfs: &Path,
) -> Result<()> {
    let metadata_dir = rootfs.parent().unwrap();
    let metadata_path = metadata_dir.join("metadata.json");

    let metadata = serde_json::json!({
        "name": container_name,
        "image": image_name,
        "tag": image_tag,
        "rootfs": rootfs.to_string_lossy(),
        "created_at": chrono::Utc::now().to_rfc3339(),
        "status": "Stopped"
    });

    // Write to a temporary sibling and rename, so an interruption cannot leave a
    // half-written metadata.json behind (B12).
    let tmp = metadata_dir.join(format!("metadata.json.tmp-{}", uuid::Uuid::new_v4()));
    fs::write(&tmp, serde_json::to_string_pretty(&metadata)?)?;
    fs::rename(&tmp, &metadata_path)?;
    Ok(())
}
