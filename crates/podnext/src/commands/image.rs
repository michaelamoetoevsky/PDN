//! Image and registry commands: `podnext image …` and `podnext search`.
//!
//! Split out of `main.rs` for roadmap section 9.2 ("no CLI file over ~800 lines"). The dispatch in
//! `main.rs` pulls these back in with `use commands::image::*`, so the command surface — and
//! therefore `--help` — is unchanged, which `scripts/dump_cli_help.py` proves byte for byte.

use anyhow::{bail, Result};
use serde::Serialize;
use std::io::{self, Write};
use std::path::PathBuf;
use tabled::Tabled;

use crate::native::search_docker_hub;
use crate::utils::format_size;
use crate::{output_json, output_table, output_yaml, ImageCommands, OutputFormat};

#[derive(Serialize, Clone)]
struct SerializableImage {
    repo_name: String,
    short_description: String,
    star_count: u32,
    pull_count: u64,
    is_official: bool,
    is_automated: bool,
}

pub async fn cmd_search(query: String, limit: usize, format: OutputFormat) -> Result<()> {
    if query.trim().is_empty() {
        #[derive(Tabled, Serialize)]
        struct CuratedDistro {
            #[tabled(rename = "DISTRIBUTION")]
            name: &'static str,
            #[tabled(rename = "RECOMMENDED TAG")]
            tag: &'static str,
            #[tabled(rename = "DESCRIPTION")]
            description: &'static str,
            #[tabled(rename = "INSTALL COMMAND")]
            install_cmd: &'static str,
        }

        let curated = vec![
            CuratedDistro {
                name: "debian",
                tag: "13 (trixie) / 12 (bookworm)",
                description: "Stable, lightweight, vast apt package ecosystem",
                install_cmd: "podnext install debian:13",
            },
            CuratedDistro {
                name: "ubuntu",
                tag: "24.04 (noble) / 22.04 (jammy)",
                description: "Standard LTS Linux development environment",
                install_cmd: "podnext install ubuntu:22.04",
            },
            CuratedDistro {
                name: "alpine",
                tag: "latest / 3.20",
                description: "Ultra-small ~5MB musl/busybox base",
                install_cmd: "podnext install alpine:latest",
            },
            CuratedDistro {
                name: "archlinux",
                tag: "latest",
                description: "Rolling-release Arch Linux with pacman",
                install_cmd: "podnext install archlinux:latest",
            },
            CuratedDistro {
                name: "fedora",
                tag: "40",
                description: "Modern RPM-based Fedora workstation",
                install_cmd: "podnext install fedora:40",
            },
            CuratedDistro {
                name: "rockylinux",
                tag: "9",
                description: "Enterprise RHEL binary-compatible distro",
                install_cmd: "podnext install rockylinux:9",
            },
            CuratedDistro {
                name: "almalinux",
                tag: "9",
                description: "Enterprise Linux 1:1 RHEL compatible",
                install_cmd: "podnext install almalinux:9",
            },
            CuratedDistro {
                name: "voidlinux",
                tag: "latest",
                description: "Independent distro with xbps package manager",
                install_cmd: "podnext install voidlinux/voidlinux:latest",
            },
            CuratedDistro {
                name: "kali-rolling",
                tag: "latest",
                description: "Penetration testing & forensics tools platform",
                install_cmd: "podnext install kalilinux/kali-rolling:latest",
            },
        ];

        match format {
            OutputFormat::Table => {
                println!("Curated Official Distributions for PDN:\n");
                output_table(&curated);
                println!("\nHint: Run 'podnext install <distro:tag>' or search Docker Hub with 'podnext search <term>'.");
            }
            OutputFormat::Json => output_json(&curated)?,
            OutputFormat::Yaml => output_yaml(&curated)?,
            OutputFormat::Text => {
                for d in curated {
                    println!("{} ({}) - {}", d.name, d.tag, d.install_cmd);
                }
            }
        }
        return Ok(());
    }

    let images = search_docker_hub(&query, limit).await?;

    #[derive(Tabled, Serialize)]
    struct ImageRow {
        #[tabled(rename = "NAME")]
        name: String,
        #[tabled(rename = "DESCRIPTION")]
        description: String,
        #[tabled(rename = "STARS")]
        stars: u32,
        #[tabled(rename = "PULLS")]
        pulls: u64,
        #[tabled(rename = "OFFICIAL")]
        official: String,
    }

    let rows: Vec<ImageRow> = images
        .iter()
        .map(|img| ImageRow {
            name: img.repo_name.clone(),
            description: if img.short_description.len() > 48 {
                format!("{}...", &img.short_description[..48])
            } else {
                img.short_description.clone()
            },
            stars: img.star_count,
            pulls: img.pull_count,
            official: if img.is_official {
                "[OK]".to_string()
            } else {
                "".to_string()
            },
        })
        .collect();

    let serializable: Vec<SerializableImage> = images
        .iter()
        .map(|img| SerializableImage {
            repo_name: img.repo_name.clone(),
            short_description: img.short_description.clone(),
            star_count: img.star_count,
            pull_count: img.pull_count,
            is_official: img.is_official,
            is_automated: img.is_automated,
        })
        .collect();

    match format {
        OutputFormat::Table => {
            output_table(&rows);
            println!("\nHint: To install an image, run: podnext install <name>[:tag]");
        }
        OutputFormat::Json => output_json(&serializable)?,
        OutputFormat::Yaml => output_yaml(&serializable)?,
        OutputFormat::Text => {
            for img in images {
                println!(
                    "{} - {} (stars: {})",
                    img.repo_name, img.short_description, img.star_count
                );
            }
        }
    }
    Ok(())
}

pub async fn execute_image_command(cmd: ImageCommands) -> Result<()> {
    match cmd {
        ImageCommands::Pull {
            image,
            platform,
            all_tags,
        } => cmd_image_pull(image, platform, all_tags).await,
        ImageCommands::Push { image } => cmd_image_push(image).await,
        ImageCommands::Rm { images, force, yes } => cmd_image_rm(images, force || yes).await,
        ImageCommands::Tag { source, target } => cmd_image_tag(source, target).await,
        ImageCommands::History {
            image,
            no_trunc,
            format,
        } => cmd_image_history(image, no_trunc, format).await,
        ImageCommands::Inspect { images, format } => cmd_image_inspect(images, format).await,
        ImageCommands::Ls { format, all } => cmd_image_ls(format, all).await,
    }
}

pub async fn cmd_image_pull(image: String, platform: Option<String>, all_tags: bool) -> Result<()> {
    // SAFETY: image pull is not implemented; the previous stub claimed success but wrote nothing.
    // Fail honestly rather than silently no-op (issue #41, finding 01-1).
    // Real implementation requires: OCI registry client, manifest fetch, blob download,
    // layer extraction to ~/.podnext/images/<name>, and metadata persistence.
    // Until then: use `podnext install <image>` instead, which does pull + extract to a runnable container.
    anyhow::bail!(
        "Error: image pull is unavailable in this version.\n\
         \n\
         This command was a stub that reported success without downloading anything.\n\
         \n\
         Workaround: use 'podnext install {}' to pull and create a runnable container.\n\
         \n\
         Tracking: https://github.com/michaelamoetoevsky/PDN/issues/37 (image store architecture)",
        image
    );
}

pub async fn cmd_image_push(image: String) -> Result<()> {
    eprintln!("Pushing image: {}", image);
    let mut image_mgr = proot_distro_core::ImageManager::new()?;
    image_mgr.push(&image, None, None).await?;
    Ok(())
}

pub async fn cmd_image_rm(images: Vec<String>, force: bool) -> Result<()> {
    use std::io::IsTerminal;
    let container_mgr = proot_distro_core::ContainerManager::new()?;

    // Check existence first (Finding 04-4, 04-5)
    for image in &images {
        container_mgr.get(image).await?;
    }

    if !force {
        if !io::stdin().is_terminal() {
            anyhow::bail!(
                "stdin is not a tty: use -f/--force or -y/--yes to confirm removal of image(s)"
            );
        }
        for image in &images {
            eprint!("Remove image '{}'? [y/N]: ", image);
            io::stderr().flush()?;
            let mut input = String::new();
            let bytes = io::stdin().read_line(&mut input)?;
            if bytes == 0 || !input.trim().eq_ignore_ascii_case("y") {
                eprintln!("Cancelled.");
                std::process::exit(1);
            }
        }
    }

    for image in &images {
        container_mgr.remove(image).await?;
        println!("Removed image: {}", image);
    }
    Ok(())
}

pub async fn cmd_image_tag(source: String, target: String) -> Result<()> {
    eprintln!("Tagging {} -> {}", source, target);
    let mut image_mgr = proot_distro_core::ImageManager::new()?;
    image_mgr.tag(&source, &target)?;
    eprintln!("Tagged {} as {}", source, target);
    Ok(())
}

pub async fn cmd_image_history(image: String, no_trunc: bool, format: OutputFormat) -> Result<()> {
    eprintln!("Showing history for image: {}", image);
    let image_mgr = proot_distro_core::ImageManager::new()?;
    let history = image_mgr.history(&image)?;

    #[derive(Tabled, Serialize)]
    struct HistoryRow {
        #[tabled(rename = "IMAGE")]
        image: String,
        #[tabled(rename = "CREATED")]
        created: String,
        #[tabled(rename = "CREATED BY")]
        created_by: String,
        #[tabled(rename = "SIZE")]
        size: String,
        #[tabled(rename = "COMMENT")]
        comment: String,
    }

    let rows: Vec<HistoryRow> = history
        .iter()
        .map(|h| HistoryRow {
            image: if no_trunc {
                h.id.clone()
            } else {
                format!("{}...", &h.id[..12.min(h.id.len())])
            },
            created: h.created.format("%Y-%m-%d %H:%M:%S").to_string(),
            created_by: h.created_by.clone(),
            size: format_size(h.size),
            comment: if no_trunc {
                h.comment.clone()
            } else {
                if h.comment.len() > 50 {
                    format!("{}...", &h.comment[..50])
                } else {
                    h.comment.clone()
                }
            },
        })
        .collect();

    match format {
        OutputFormat::Table => output_table(&rows),
        OutputFormat::Json => output_json(&history)?,
        OutputFormat::Yaml => output_yaml(&history)?,
        OutputFormat::Text => {
            for h in history {
                println!(
                    "{} {} {} {}",
                    if no_trunc {
                        h.id.clone()
                    } else {
                        format!("{}...", &h.id[..12.min(h.id.len())])
                    },
                    h.created.format("%Y-%m-%d %H:%M:%S"),
                    h.created_by,
                    format_size(h.size)
                );
            }
        }
    }
    Ok(())
}

/// Read the `metadata.json` of a locally built image from the container store.
///
/// A built image *is* a container directory (#29), so `image inspect` has to look
/// there as well as in `~/.podnext/images`; otherwise an image that `build local`
/// just created is invisible to the command that is supposed to describe it. The
/// store key is the derived, colon-free name (#35), and the original reference is
/// recorded inside the metadata.
pub(crate) fn read_store_metadata(
    containers_dir: &std::path::Path,
    reference: &str,
) -> Option<(std::path::PathBuf, serde_json::Value)> {
    // An image reference is a name, not a path: `..`, a separator or anything
    // else path-shaped is refused outright rather than reinterpreted as a store
    // entry (`demo/../other` must not silently resolve to `other`).
    if reference.contains('/') || reference.contains('\\') || reference.contains("..") {
        return None;
    }
    // Accept the reference (`app:latest`) or the store key (`app`), so the command
    // works whichever name the user has in hand.
    let candidates = [
        proot_distro_core::names::store_key(reference),
        reference.to_string(),
    ];
    for candidate in candidates {
        // The same validator the rest of the CLI uses, so `..`, a path or any
        // other oddity is not silently reinterpreted as a store entry.
        if proot_distro_core::NameValidator::new()
            .validate_container_name(&candidate)
            .is_err()
        {
            continue;
        }
        let dir = containers_dir.join(&candidate);
        let Ok(text) = std::fs::read_to_string(dir.join("metadata.json")) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        return Some((dir, json));
    }
    None
}

pub async fn cmd_image_inspect(images: Vec<String>, format: OutputFormat) -> Result<()> {
    let image_mgr = proot_distro_core::ImageManager::new()?;
    let containers_dir = proot_distro_core::paths::resolve_home()?
        .join(".podnext")
        .join("containers");

    // Two sources, reported separately because they describe different things:
    // `~/.podnext/images` holds pulled images, and the container store holds what
    // `build local` produced (#29). Neither knowing the reference used to print
    // nothing and exit 0 - a command whose purpose is to report on an image
    // reported success while reporting nothing.
    let mut pulled = Vec::new();
    let mut built: Vec<(PathBuf, serde_json::Value)> = Vec::new();
    let mut missing: Vec<String> = Vec::new();

    for image_ref in images {
        if let Ok(info) = image_mgr.inspect(&image_ref) {
            pulled.push(info);
            continue;
        }
        match read_store_metadata(&containers_dir, &image_ref) {
            Some(entry) => built.push(entry),
            None => missing.push(image_ref),
        }
    }

    if !missing.is_empty() {
        bail!(
            "no such image: {}. Looked in ~/.podnext/images and in the container store \
             (~/.podnext/containers/<name>), which is where `podnext build local` puts an image",
            missing.join(", ")
        );
    }

    if !built.is_empty() {
        match format {
            OutputFormat::Json => {
                let values: Vec<&serde_json::Value> = built.iter().map(|(_, m)| m).collect();
                output_json(&values)?;
            }
            OutputFormat::Yaml => {
                for (_, meta) in &built {
                    output_yaml(meta)?;
                }
            }
            OutputFormat::Table => {
                #[derive(Tabled, Serialize)]
                struct BuiltRow {
                    #[tabled(rename = "IMAGE")]
                    image: String,
                    #[tabled(rename = "STORED AS")]
                    stored_as: String,
                    #[tabled(rename = "BASE")]
                    base: String,
                    #[tabled(rename = "CREATED")]
                    created: String,
                }
                let rows: Vec<BuiltRow> = built
                    .iter()
                    .map(|(dir, meta)| BuiltRow {
                        image: meta
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        stored_as: dir
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                        base: meta
                            .get("image")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        created: meta
                            .get("created_at")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                    })
                    .collect();
                output_table(&rows);
            }
            OutputFormat::Text => {
                for (dir, meta) in &built {
                    println!(
                        "Image: {} (built locally, stored as '{}')",
                        meta.get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default(),
                        dir.file_name().unwrap_or_default().to_string_lossy()
                    );
                    println!("  Stored at: {}", dir.display());
                    for (label, key) in [
                        ("Base image", "image"),
                        ("Created", "created_at"),
                        ("Working dir", "working_dir"),
                        ("User (recorded, not enforced)", "user"),
                    ] {
                        if let Some(value) = meta.get(key).and_then(|v| v.as_str()) {
                            if !value.is_empty() {
                                println!("  {label}: {value}");
                            }
                        }
                    }
                    if let Some(ports) = meta.get("exposed_ports").and_then(|v| v.as_array()) {
                        if !ports.is_empty() {
                            let list = ports
                                .iter()
                                .filter_map(|p| p.as_u64())
                                .map(|p| p.to_string())
                                .collect::<Vec<_>>()
                                .join(", ");
                            println!("  Exposed (recorded, no host forwarding): {list}");
                        }
                    }
                    if let Some(cmd) = meta.get("cmd").and_then(|v| v.as_array()) {
                        if !cmd.is_empty() {
                            let list = cmd
                                .iter()
                                .filter_map(|c| c.as_str())
                                .collect::<Vec<_>>()
                                .join(" ");
                            println!("  Cmd: {list}");
                        }
                    }
                    println!();
                }
            }
        }
    }

    // Only when a pulled image was actually found: otherwise the store output
    // above is the whole answer, and an empty `[]` (or an empty table) after it
    // would read as a second, contradictory result.
    if !pulled.is_empty() {
        match format {
            OutputFormat::Table => {
                // For table, show summary
                #[derive(Tabled, Serialize)]
                struct InspectRow {
                    #[tabled(rename = "IMAGE")]
                    image: String,
                    #[tabled(rename = "ID")]
                    id: String,
                    #[tabled(rename = "CREATED")]
                    created: String,
                    #[tabled(rename = "SIZE")]
                    size: String,
                    #[tabled(rename = "ARCH")]
                    arch: String,
                    #[tabled(rename = "OS")]
                    os: String,
                }

                let rows: Vec<InspectRow> = pulled
                    .iter()
                    .map(|i| InspectRow {
                        image: i.repo_tags.first().cloned().unwrap_or_default(),
                        id: format!("{}...", &i.id[..12.min(i.id.len())]),
                        created: i.created.format("%Y-%m-%d %H:%M:%S").to_string(),
                        size: format_size(i.size),
                        arch: i.architecture.clone(),
                        os: i.os.clone(),
                    })
                    .collect();
                if !rows.is_empty() {
                    output_table(&rows);
                }
            }
            OutputFormat::Json => output_json(&pulled)?,
            OutputFormat::Yaml => output_yaml(&pulled)?,
            OutputFormat::Text => {
                for i in pulled {
                    println!(
                        "Image: {}",
                        i.repo_tags.first().cloned().unwrap_or_default()
                    );
                    println!("  ID: {}", i.id);
                    println!("  Created: {}", i.created.format("%Y-%m-%d %H:%M:%S"));
                    println!("  Size: {}", format_size(i.size));
                    println!("  Architecture: {}", i.architecture);
                    println!("  OS: {}", i.os);
                    println!();
                }
            }
        }
    }
    Ok(())
}

/// Bytes used by `path`: `Some((bytes, is_lower_bound))`, or `None` when the size
/// cannot be measured at all.
///
/// `du -sb` is the measurement of record. It fails on a container rootfs that carries
/// Android system directories an app uid cannot read, and the previous fallback printed
/// the directory's own inode size as the size (3.4 KB for a 402 MB rootfs) with nothing
/// to mark it as not-a-measurement. When `du` fails we sum the readable regular files and
/// report the result as a lower bound, because the unreadable subtrees are missing from it.
pub(crate) fn du_size_bytes(path: &std::path::Path) -> Option<(u64, bool)> {
    match std::process::Command::new("du")
        .args(["-sb", &path.to_string_lossy()])
        .output()
    {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            match stdout
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u64>().ok())
            {
                Some(bytes) => Some((bytes, false)),
                // `du` succeeded but printed something unparseable: do not invent a number.
                None => walk_size_bytes(path).map(|b| (b, true)),
            }
        }
        _ => walk_size_bytes(path).map(|b| (b, true)),
    }
}

/// Sum the sizes of the readable regular files under `path`, skipping everything that
/// cannot be read or followed (unreadable directories, dangling symlinks, devices).
pub(crate) fn walk_size_bytes(path: &std::path::Path) -> Option<u64> {
    let mut total = 0u64;
    let mut saw_file = false;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            match entry.file_type() {
                // A symlink is not followed: its target is counted where it lives.
                Ok(ft) if ft.is_dir() => stack.push(entry.path()),
                Ok(ft) if ft.is_file() => {
                    if let Ok(meta) = entry.metadata() {
                        saw_file = true;
                        total += meta.len();
                    }
                }
                _ => {}
            }
        }
    }
    if saw_file {
        Some(total)
    } else {
        None
    }
}

pub async fn cmd_image_ls(format: OutputFormat, _all: bool) -> Result<()> {
    let container_mgr = proot_distro_core::ContainerManager::new()?;
    let containers = container_mgr.list().await?;

    #[derive(Tabled, Serialize)]
    struct ImageRow {
        #[tabled(rename = "REPOSITORY")]
        repository: String,
        #[tabled(rename = "TAG")]
        tag: String,
        // This column held the container name under a header that claimed it was an image
        // ID. No image ID exists: the metadata carries the image name and tag and no digest.
        #[tabled(rename = "CONTAINER")]
        image_id: String,
        #[tabled(rename = "CREATED")]
        created: String,
        #[tabled(rename = "SIZE")]
        size: String,
    }

    let rows: Vec<ImageRow> = containers
        .iter()
        .map(|c| {
            let meta_path = c.rootfs_path.parent().map(|p| p.join("metadata.json"));
            let (repo, tag) = if let Some(mp) = meta_path {
                if let Ok(meta_str) = std::fs::read_to_string(mp) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&meta_str) {
                        let r = val
                            .get("image")
                            .and_then(|v| v.as_str())
                            .map(str::to_string);
                        let t = val.get("tag").and_then(|v| v.as_str()).map(str::to_string);
                        (r, t)
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            };
            let repository = repo.unwrap_or_else(|| c.distro.clone());
            let tag = tag.unwrap_or_else(|| c.version.clone());
            let size = match du_size_bytes(&c.rootfs_path) {
                Some((bytes, false)) => format_size(bytes),
                // `>=` marks a figure the unreadable subtrees are missing from: honest
                // about being a bound instead of a number that merely looks precise.
                Some((bytes, true)) => format!(">={}", format_size(bytes)),
                None => "unknown".to_string(),
            };
            ImageRow {
                repository,
                tag,
                image_id: c.name.clone(),
                created: c.created_at.format("%Y-%m-%d %H:%M").to_string(),
                size,
            }
        })
        .collect();

    match format {
        OutputFormat::Table => output_table(&rows),
        OutputFormat::Json => output_json(&containers)?,
        OutputFormat::Yaml => output_yaml(&containers)?,
        OutputFormat::Text => {
            for r in &rows {
                println!("{} {} {} {}", r.repository, r.tag, r.image_id, r.created);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod size_measurement_tests {
    use super::{du_size_bytes, walk_size_bytes};

    fn scratch(label: &str) -> std::path::PathBuf {
        let base = std::env::var_os("TMPDIR")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("PREFIX").map(|p| std::path::PathBuf::from(p).join("tmp")))
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join(format!("pdn-size-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn a_du_measurement_is_reported_as_measured_not_as_a_bound() {
        let dir = scratch("measured");
        std::fs::write(dir.join("blob"), vec![7u8; 4096]).expect("write");
        let (bytes, is_bound) = du_size_bytes(&dir).expect("du must measure this directory");
        assert!(
            !is_bound,
            "du succeeded, so the figure is not a lower bound"
        );
        assert!(bytes >= 4096, "measured {bytes} for a 4096-byte file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unmeasurable_path_produces_no_number() {
        // The old code returned the inode size here and printed it as the size.
        assert_eq!(
            du_size_bytes(std::path::Path::new("/definitely/not/here")),
            None
        );
    }

    #[test]
    fn the_fallback_walk_sums_the_readable_files_and_nothing_else() {
        let dir = scratch("walk");
        std::fs::write(dir.join("a"), vec![0u8; 1000]).expect("write a");
        std::fs::create_dir_all(dir.join("nested")).expect("nested");
        std::fs::write(dir.join("nested/b"), vec![0u8; 24]).expect("write b");
        assert_eq!(walk_size_bytes(&dir), Some(1024));
        // An empty tree yields no figure at all rather than a confident zero.
        let empty = scratch("walk-empty");
        let nested_empty = empty.join("only/dirs");
        std::fs::create_dir_all(&nested_empty).expect("dirs");
        assert_eq!(walk_size_bytes(&empty), None);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&empty);
    }
}
