//! Native implementations for podnext
//!
//! This module provides native implementations of all podnext commands
//! using the Rust crates directly instead of wrapping proot-distro.

use anyhow::{Result, Context, bail};
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::io::{self, Write};
use reqwest::Client;
use urlencoding::encode;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use tar::Archive;
use flate2::read::GzDecoder;
use oci_spec::image::{ImageManifest, ImageConfiguration};
use sha2::{Sha256, Digest};
use hex;

// ===== Docker Hub Search =====

#[derive(serde::Deserialize, Debug)]
pub struct DockerHubSearchResponse {
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

pub async fn search_docker_hub(query: &str, limit: usize) -> Result<Vec<DockerHubImage>> {
    let client = Client::new();
    let url = format!(
        "https://registry.hub.docker.com/v2/search/repositories/?query={}&page_size={}",
        encode(query),
        limit.min(100)
    );

    let response = client
        .get(&url)
        .header("User-Agent", "podnext/0.1.0")
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
    pub media_type: String,
    pub platform: Option<Platform>,
}

#[derive(serde::Deserialize, Debug)]
pub struct Platform {
    pub architecture: String,
    pub os: String,
}

struct ImageLayer {
    digest: String,
    size: u64,
    media_type: String,
}

pub async fn install_image_native(
    image_ref: &str,
    container_name: &str,
    force: bool,
    rootfs_path: Option<PathBuf>,
) -> Result<PathBuf, anyhow::Error> {
    // Parse image reference (e.g., "ubuntu:22.04")
    let (name, tag) = parse_image_ref(image_ref)?;
    
    let container_name = container_name.to_string();

    println!("Installing '{}' as '{}'...", image_ref, container_name);

    // Determine rootfs path
    let rootfs = rootfs_path.unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/home/user"))
            .join(".podnext")
            .join("containers")
            .join(&container_name)
            .join("rootfs")
    });

    // Create rootfs directory
    fs::create_dir_all(&rootfs)?;

    // Check if container already exists
    if rootfs.exists() && !force {
        bail!("Container '{}' already exists at {}. Use --force to overwrite.", 
              container_name, rootfs.display());
    }

    // Step 1: Get auth token from Docker Hub
    println!("Fetching auth token...");
    let token = get_docker_hub_token(&name).await?;

    // Step 2: Get manifest list to find correct platform
    println!("Fetching manifest...");
    let manifest = get_manifest(&name, &tag, &token).await?;

    // Step 3: Download and extract layers
    println!("Downloading layers...");
    download_and_extract_layers(&name, &manifest, &token, &rootfs).await?;

    // Step 4: Apply proot-compat patches
    println!("Applying compatibility patches...");
    proot_compat::patch_rootfs(&rootfs)?;

    // Step 5: Save container metadata
    save_container_metadata(&container_name, &name, &tag, &rootfs)?;

    println!("\nContainer '{}' installed successfully!", container_name);
    println!("Rootfs: {}", rootfs.display());
    println!("\nTo start the container:");
    println!("  podnext login {}", container_name);

    Ok(rootfs)
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
    let client = Client::new();
    let url = format!("https://auth.docker.io/token?service=registry.docker.io&scope=repository:{}:pull", repo);
    
    let response = client.get(&url).send().await?;
    let token_resp: DockerHubTokenResponse = response.json().await?;
    Ok(token_resp.token)
}

async fn get_manifest(name: &str, tag: &str, token: &str) -> Result<ImageManifest> {
    let client = Client::new();
    let url = format!("https://registry.hub.docker.com/v2/{}/manifests/{}", name, tag);
    
    let response = client
        .get(&url)
        .bearer_auth(token)
        .header("Accept", "application/vnd.docker.distribution.manifest.v2+json")
        .header("Accept", "application/vnd.docker.distribution.manifest.list.v2+json")
        .header("Accept", "application/vnd.oci.image.manifest.v1+json")
        .send()
        .await?;

    if !response.status().is_success() {
        bail!("Failed to fetch manifest: {}", response.status());
    }

    let content_type = {
        let headers = response.headers();
        headers.get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    };
    
    let body = response.bytes().await?;

    if content_type.contains("manifest.list") || content_type.contains("manifest.list.v2") {
        // Multi-platform manifest - find linux/arm64
        let manifest_list: ManifestList = serde_json::from_slice(&body)?;
        let arm64_manifest = manifest_list.manifests.iter()
            .find(|m| m.platform.as_ref().map(|p| p.architecture == "arm64" && p.os == "linux").unwrap_or(false))
            .ok_or_else(|| anyhow::anyhow!("No linux/arm64 manifest found"))?;
        
        // Fetch the specific manifest
        return get_manifest_by_digest(name, &arm64_manifest.digest, token).await;
    }

    // Single platform manifest
    let manifest: ImageManifest = serde_json::from_slice(&body)?;
    Ok(manifest)
}

async fn get_manifest_by_digest(name: &str, digest: &str, token: &str) -> Result<ImageManifest> {
    let client = Client::new();
    let url = format!("https://registry.hub.docker.com/v2/{}/manifests/{}", name, digest);
    
    let response = client
        .get(&url)
        .bearer_auth(token)
        .header("Accept", "application/vnd.docker.distribution.manifest.v2+json")
        .header("Accept", "application/vnd.oci.image.manifest.v1+json")
        .send()
        .await?;

    if !response.status().is_success() {
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
    let client = Client::new();
    let layers = manifest.layers();
    let pb = ProgressBar::new(layers.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} layers ({msg})")
        .unwrap()
        .progress_chars("#>-"));

    for layer in layers {
            let digest_str = layer.digest().to_string();
            pb.set_message(format!("Downloading {}", &digest_str[..20]));
        
            let layer_data = download_layer(name, &digest_str, token).await?;
        
            // Verify digest
            let mut hasher = Sha256::new();
            hasher.update(&layer_data);
            let computed = hex::encode(hasher.finalize());
            let expected = digest_str.strip_prefix("sha256:").unwrap_or(&digest_str);
        
            if computed != expected {
                bail!("Layer digest mismatch: expected {}, got {}", expected, computed);
            }

            pb.set_message(format!("Extracting {}", &digest_str[..20]));
            extract_layer(&layer_data, &layer.media_type().to_string(), rootfs)?;
        
            pb.inc(1);
        }

    pb.finish_with_message("All layers extracted");
    Ok(())
}

async fn download_layer(name: &str, digest: &str, token: &str) -> Result<Vec<u8>> {
    let client = Client::new();
    let url = format!("https://registry.hub.docker.com/v2/{}/blobs/{}", name, digest);
    
    let response = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await?;

    if !response.status().is_success() {
        bail!("Failed to download layer {}: {}", digest, response.status());
    }

    let bytes = response.bytes().await?;
    Ok(bytes.to_vec())
}

fn extract_layer(data: &[u8], _media_type: &str, rootfs: &Path) -> Result<()> {
    let mut archive = Archive::new(GzDecoder::new(data));
    archive.unpack(rootfs)?;
    Ok(())
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
    
    fs::write(metadata_path, serde_json::to_string_pretty(&metadata)?)?;
    Ok(())
}

// ===== Native Container Operations =====

pub async fn list_containers_native(all: bool) -> Result<Vec<proot_distro_core::ContainerInfo>> {
    let container_mgr = proot_distro_core::ContainerManager::new()?;
    let containers = container_mgr.list().await?;
    
    if !all {
        // Filter to only running
        let containers: Vec<_> = containers.into_iter()
            .filter(|c| c.status == proot_distro_core::ContainerStatus::Running)
            .collect();
        return Ok(containers);
    }
    
    Ok(containers)
}

pub async fn login_container_native(
    container_name: &str,
    user: Option<String>,
    command: Vec<String>,
) -> Result<()> {
    let mut distro_core = proot_distro_core::ProotDistroCore::new()?;
    
    let mut options = proot_distro_core::LoginOptions::default();
    options.container_name = container_name.to_string();
    options.command = if command.is_empty() {
        vec!["/bin/bash".to_string()]
    } else {
        command
    };
    
    if let Some(u) = user {
        options.user = Some(u);
    }
    
    distro_core.login(options).await?;
    Ok(())
}

pub async fn run_container_native(
    container_name: &str,
    command: Vec<String>,
) -> Result<()> {
    let mut distro_core = proot_distro_core::ProotDistroCore::new()?;
    
    let mut options = proot_distro_core::LoginOptions::default();
    options.container_name = container_name.to_string();
    options.command = command;
    
    distro_core.login(options).await?;
    Ok(())
}

pub async fn list_sessions_native() -> Result<Vec<proot_distro_core::SessionInfo>> {
    let session_mgr = proot_distro_core::SessionManager::new()?;
    Ok(session_mgr.list_all())
}

pub async fn kill_container_native(container_name: &str) -> Result<()> {
    let container_mgr = proot_distro_core::ContainerManager::new()?;
    container_mgr.kill(container_name).await
}

pub async fn remove_container_native(name: String, force: bool) -> Result<()> {
    let container_mgr = proot_distro_core::ContainerManager::new()?;
    
    if !force {
        print!("Remove container '{}'? [y/N]: ", name);
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }
    
    container_mgr.remove(&name).await?;
    println!("Container '{}' removed.", name);
    Ok(())
}