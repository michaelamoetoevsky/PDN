//! `podnext build` — materialise a Dockerfile into the local image store.
//!
//! Extracted from `commands/mod.rs` by the roadmap 9.2 CLI split. The helpers
//! below (Dockerfile preprocessing, glob expansion, COPY) are used only here.

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;

pub async fn execute_build_command(cmd: crate::BuildCommands) -> Result<()> {
    match cmd {
        crate::BuildCommands::Local { tag, file, path } => cmd_build_local(tag, file, path).await,
        crate::BuildCommands::Submit { spec, format } => cmd_build_submit(spec, format).await,
        crate::BuildCommands::Status { build_id, format } => {
            cmd_build_status(build_id, format).await
        }
        crate::BuildCommands::Cancel { build_id } => cmd_build_cancel(build_id).await,
        crate::BuildCommands::List { status, format } => cmd_build_list(status, format).await,
        crate::BuildCommands::Artifacts { build_id } => cmd_build_artifacts(build_id).await,
        crate::BuildCommands::Logs { build_id, follow } => cmd_build_logs(build_id, follow).await,
    }
}

pub async fn cmd_build_local(
    tag: Option<String>,
    file: Option<PathBuf>,
    path: PathBuf,
) -> Result<()> {
    let target_tag = tag.unwrap_or_else(|| "custom:latest".to_string());
    // `build_local_image` performs the build and only returns once the image is
    // on disk. The success line below is unreachable on failure, so a printed
    // "Successfully built" is now always backed by a real image (#29).
    let image_dir = build_local_image(&target_tag, file, &path, None).await?;

    // The runnable name is the store key, not the tag: `NameValidator` rejects a
    // colon, so the advice for `custom:latest` used to be a command that could
    // never work.
    let key = proot_distro_core::names::store_key(&target_tag);
    println!("\nSuccessfully built image '{}'", target_tag);
    println!("  Image stored at {}", image_dir.display());
    if key != target_tag {
        println!("  Stored as container '{key}' (a tag is not a path component)");
    }
    // `args_os`, not `args`: std caches argv once, and `args()` panics re-reading it when any
    // argument was not valid UTF-8 - the defect of issue #41. Re-applied here because the
    // integration took this file from a branch cut before the fix; the ledger predicate
    // "the panicking reader is gone from the tree" is what noticed.
    let bin_name = std::env::args_os()
        .next()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "podnext".to_string());
    let rootfs = image_dir.join("rootfs");
    let has_sh = rootfs.join("bin/sh").symlink_metadata().is_ok()
        || rootfs.join("usr/bin/sh").symlink_metadata().is_ok();
    if has_sh {
        println!("  Run it with: {bin_name} run {key}");
    } else {
        // The store's single metadata reader: a FIFO/socket at the path is
        // refused by name and never opened, so this cannot block (S3).
        let meta_cmd: Vec<String> = proot_distro_core::container::read_metadata_text(&image_dir)
            .ok()
            .flatten()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v.get("cmd").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        if !meta_cmd.is_empty() {
            println!(
                "  Run it with: {bin_name} run {key} -- {}",
                meta_cmd.join(" ")
            );
        } else {
            println!("  Run it with: {bin_name} run {key} -- <command>");
        }
    }
    Ok(())
}

/// Materialise a Dockerfile build into the runtime's image store.
///
/// A "built image" in this runtime *is* a container directory under
/// `~/.podnext/containers/<store-key>/rootfs` — what `ContainerManager::load_containers`
/// scans, and what `podnext run <tag>` / `podnext image ls` read. This function
/// produces exactly that and returns its directory, so a returned `Ok` means the
/// image is really runnable. `containers_dir` overrides the store root for tests.
///
/// The previous implementation diffed the raw build context against an empty
/// base, printed the blob digest, deleted the blob and then claimed success while
/// writing nothing — a silent false success. It also printed that success line
/// even when the context path did not exist, i.e. after doing no work at all.
pub async fn build_local_image(
    tag: &str,
    file: Option<PathBuf>,
    path: &std::path::Path,
    containers_dir: Option<PathBuf>,
) -> Result<PathBuf> {
    let dockerfile_path = file.unwrap_or_else(|| path.join("Dockerfile"));
    // The old code guarded the only work it did with `if path.exists()` and then
    // printed success regardless; a missing context must be a hard error. Checked
    // first so it is reported ahead of the derived Dockerfile path.
    if !path.exists() {
        bail!(
            "build context '{}' does not exist; pass a directory containing the Dockerfile",
            path.display()
        );
    }
    if !dockerfile_path.exists() {
        bail!("Dockerfile not found at '{}'", dockerfile_path.display());
    }

    println!("Parsing Dockerfile: {}", dockerfile_path.display());
    let content = std::fs::read_to_string(&dockerfile_path)?;
    // The Dockerfile parser (owned by proot-distro-core) has no ARG variant, so
    // consume ARG instructions here: declare them and substitute their values
    // into the instructions that follow.
    let (content, build_args) = preprocess_build_args(&content);
    for (name, value) in &build_args {
        if value.is_empty() {
            println!("Build arg: {} (no default)", name);
        } else {
            println!("Build arg: {}={}", name, value);
        }
    }
    // Before parsing: the parser drops `--` flags and uses only the last FROM, so
    // a multi-stage Dockerfile would build something subtly different without
    // saying so. Refuse those shapes by name.
    refuse_unsupported_build_features(&content)?;
    let instructions = proot_distro_core::dockerfile::parse_dockerfile(&content)?;
    println!("Parsed {} instruction(s):", instructions.len());
    for (idx, inst) in instructions.iter().enumerate() {
        println!("  [{}/{}] {:?}", idx + 1, instructions.len(), inst);
    }

    // Resolve the base image from the last FROM. Without one there is nothing to
    // build on, so refuse rather than fabricate a result.
    let base_image = instructions
        .iter()
        .rev()
        .find_map(|inst| match inst {
            proot_distro_core::dockerfile::DockerfileInstruction::From { image, tag, .. } => {
                Some(match tag {
                    Some(t) if !t.is_empty() => format!("{}:{}", image, t),
                    _ => image.clone(),
                })
            }
            _ => None,
        })
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Dockerfile has no FROM instruction; cannot build"))?;
    println!("\nTarget image tag: {}", tag);
    println!("Base image: {}", base_image);

    let containers_dir = containers_dir.unwrap_or_else(|| {
        // A19: one resolver for the store root, so a redirected root
        // (`PN_TEST_ROOT`) is followed instead of the raw `dirs::home_dir()`.
        proot_distro_core::paths::resolve_home()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".podnext")
            .join("containers")
    });
    // The store key is derived, never the raw tag. `c2env:latest` is a legal
    // directory name, but it puts a colon in the host path and PRoot splits bind
    // arguments on ':' - the container then silently loses /dev/shm and the /proc
    // stubs (#35). `store_key` is the rule `podnext install` already used, and
    // `podnext run <store_key>` resolves the result.
    let key = proot_distro_core::names::store_key(tag);
    // An empty or otherwise unusable key would mean a container the CLI can never
    // address. Fail here rather than create it.
    proot_distro_core::NameValidator::new()
        .validate_container_name(&key)
        .with_context(|| {
            format!("tag '{tag}' does not yield a usable container name (derived '{key}')")
        })?;
    let container_dir = containers_dir.join(&key);
    if container_dir.exists() {
        // Rebuilding the same tag replaces its image, which is what repeat builds
        // expect. A *different* tag that derives to the same key is not the same
        // thing: clearing it would destroy an image the user still refers to by
        // another name (`app:v1` then `app:v2`). The recorded tag is what tells the
        // two cases apart, so refuse when they disagree instead of deleting.
        let recorded = proot_distro_core::container::read_metadata_text(&container_dir)
            .ok()
            .flatten()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|meta| {
                meta.get("name")
                    .and_then(|n| n.as_str())
                    .map(str::to_string)
            });
        if let Some(recorded) = recorded {
            if recorded != tag {
                bail!(
                    "container '{key}' already holds image '{recorded}' and tag '{tag}' \
                     derives to the same name; rebuilding would destroy that image. \
                     Remove it first (podnext remove {key}) or choose another tag"
                );
            }
        }
        std::fs::remove_dir_all(&container_dir).with_context(|| {
            format!(
                "failed to clear existing image '{}'",
                container_dir.display()
            )
        })?;
    }
    let rootfs = container_dir.join("rootfs");

    if base_image == "scratch" {
        // `FROM scratch` is Docker's empty base: the Dockerfile supplies the
        // whole filesystem. No registry pull (and, as in Docker, no shell for RUN).
        std::fs::create_dir_all(&rootfs)?;
    } else {
        println!("Pulling base image '{}'...", base_image);
        // `key`, not `tag`: this installs the base INTO the container directory
        // that `podnext run <key>` will resolve, and a colon in that name would
        // put a colon in every bind path derived from it (#35).
        crate::native::install_image_native(&base_image, &key, true, Some(rootfs.clone())).await?;
    }

    // Apply the Dockerfile on top of the base rootfs, in order. `env` and
    // `workdir` accumulate the way Docker scopes them for later RUNs.
    let mut env: Vec<(String, String)> = Vec::new();
    let mut workdir: Option<String> = None;
    let mut cmd: Vec<String> = Vec::new();
    let mut labels: Vec<(String, String)> = Vec::new();
    // EXPOSE/USER are image metadata, not filesystem changes. `EXPOSE` in Docker
    // only *documents* a port (it never binds one) and this runtime has no host
    // port-forwarding daemon, so recording it is the faithful behaviour. `USER`
    // is recorded but not enforced: proot's rootless model maps every guest id
    // to the single host uid and `ContainerConfig` carries no user/uid field
    // (adding one means changing the shared crate, outside this branch's scope).
    let mut exposed_ports: Vec<u16> = Vec::new();
    let mut user: Option<String> = None;
    // Constructed lazily: only RUN needs the proot engine, which needs a
    // multi-thread runtime. A metadata-only (scratch) build must not require it.
    let mut mgr: Option<proot_distro_core::ContainerManager> = None;

    for inst in &instructions {
        use proot_distro_core::dockerfile::DockerfileInstruction as Instr;
        match inst {
            Instr::From { .. } => {}
            Instr::Copy { src, dest } | Instr::Add { src, dest } => {
                println!("Copy {} -> {}", src, dest);
                copy_into_rootfs(path, src, dest, &rootfs)?;
            }
            Instr::Env { key, value } => {
                env.retain(|(k, _)| k != key);
                env.push((key.clone(), value.clone()));
            }
            Instr::Workdir(dir) => {
                let dir = dir.trim_matches('"').to_string();
                let abs = if dir.starts_with('/') {
                    dir
                } else {
                    format!("/{}", dir)
                };
                std::fs::create_dir_all(rootfs.join(abs.trim_start_matches('/')))?;
                workdir = Some(abs);
            }
            Instr::Run(run_cmd) => {
                println!("Running: {}", run_cmd);
                if mgr.is_none() {
                    mgr = Some(proot_distro_core::ContainerManager::new()?);
                }
                let mgr = mgr.as_ref().unwrap();
                let mut c = mgr.get(&key).await?;
                c.config.env_vars = env.clone();
                if let Some(wd) = &workdir {
                    c.config.working_dir = Some(wd.clone());
                }
                let code = mgr
                    .execute_container(
                        &c,
                        vec!["/bin/sh".to_string(), "-c".to_string(), run_cmd.clone()],
                    )
                    .await?;
                if code != 0 {
                    bail!("RUN instruction failed (exit {}): '{}'", code, run_cmd);
                }
            }
            Instr::Cmd(args) | Instr::Entrypoint(args) => {
                if !args.is_empty() {
                    cmd = args.clone();
                }
            }
            Instr::Label { key, value } => {
                labels.retain(|(k, _)| k != key);
                labels.push((key.clone(), value.clone()));
            }
            Instr::Expose(port) => {
                if !exposed_ports.contains(port) {
                    println!("Expose: {} (recorded in the image metadata)", port);
                    if exposed_ports.is_empty() {
                        // Docker's EXPOSE only documents a port; this runtime has
                        // no port-forwarding daemon, so say so rather than let a
                        // reader assume traffic will arrive.
                        println!(
                            "NOTE: EXPOSE is metadata here. This runtime does not forward host \
                             ports, so publishing them is a no-op."
                        );
                    }
                    exposed_ports.push(*port);
                }
            }
            Instr::User(u) => {
                let u = u.trim().to_string();
                if !u.is_empty() {
                    println!("User: {} (recorded in the image metadata)", u);
                    println!(
                        "NOTE: USER is not enforced here. PRoot maps every guest id to the single \
                         host uid, so RUN and `podnext run` both execute as the host user. Do not \
                         rely on this image's USER for privilege separation."
                    );
                    user = Some(u);
                }
            }
        }
    }

    // Record image metadata so `podnext image ls` / `inspect` can see the result.
    let (target_repo, target_tag) = if let Some((r, t)) = tag.split_once(':') {
        (r, t)
    } else {
        (tag, "latest")
    };
    let metadata = serde_json::json!({
        "name": tag,
        "image": target_repo,
        "tag": target_tag,
        "base_image": base_image,
        "rootfs": rootfs.to_string_lossy(),
        "created_at": chrono::Utc::now().to_rfc3339(),
        "status": "Stopped",
        "cmd": cmd,
        "env": env.iter().cloned().collect::<HashMap<_, _>>(),
        "working_dir": workdir,
        "labels": labels.iter().cloned().collect::<HashMap<_, _>>(),
        "exposed_ports": exposed_ports,
        "user": user,
    });
    std::fs::write(
        container_dir.join("metadata.json"),
        serde_json::to_string_pretty(&metadata)?,
    )?;

    // Success is claimed only once the image is actually on disk.
    if !rootfs.is_dir() {
        bail!(
            "build finished but image rootfs '{}' is missing",
            rootfs.display()
        );
    }
    Ok(container_dir)
}

/// Copy a build-context path into the image rootfs (Dockerfile COPY/ADD).
///
/// Handles a single source path (file or directory) and, since a wildcard in the
/// source is a common Dockerfile idiom, `*`/`?` globs expanded against the build
/// context. `--from=` stage copies are still not supported.
pub fn copy_into_rootfs(
    ctx: &std::path::Path,
    src: &str,
    dest: &str,
    rootfs: &std::path::Path,
) -> Result<()> {
    // A `..` component climbs out of whatever the path is joined onto: the
    // destination is joined onto the image rootfs, so `COPY x ../../../escaped`
    // wrote outside the image and into the store; the source is joined onto the
    // build context, so `../secret` read outside it. Docker refuses both. Checked
    // here, before any filesystem write, so nothing lands outside the root
    // (WORKLIST A11).
    //
    // ponytail: a literal `..` component only - a symlink already inside the
    // rootfs that points outward is not resolved. Add realpath containment when a
    // Dockerfile can place such a symlink before its COPY.
    if dest.split('/').any(|c| c == "..") {
        bail!(
            "COPY/ADD destination '{dest}' escapes the image root: '..' is not allowed in a COPY/ADD path"
        );
    }
    if src.split('/').any(|c| c == "..") {
        bail!(
            "COPY/ADD source '{src}' escapes the build context: '..' is not allowed in a COPY/ADD path"
        );
    }
    if src.contains('*') || src.contains('?') {
        // ponytail: `*`/`?` within a path component only — no `**`, `[...]`
        // classes or brace expansion. Add when a real Dockerfile needs them.
        let mut matched = expand_glob(ctx, src)?;
        if matched.is_empty() {
            bail!(
                "COPY/ADD source '{}' matched no files in the build context",
                src
            );
        }
        let dest_is_dir = dest.ends_with('/') || dest == "." || dest == "/";
        if matched.len() > 1 && !dest_is_dir {
            bail!(
                "COPY/ADD with multiple sources needs a directory destination ending in '/', got '{}'",
                dest
            );
        }
        matched.sort();
        for path in matched {
            copy_one(&path, dest, rootfs)?;
        }
        return Ok(());
    }

    let src_path = ctx.join(src);
    if !src_path.exists() {
        bail!("COPY/ADD source '{}' not found in build context", src);
    }
    copy_one(&src_path, dest, rootfs)
}

/// Copy one already-resolved source path into the rootfs.
pub fn copy_one(src_path: &std::path::Path, dest: &str, rootfs: &std::path::Path) -> Result<()> {
    let mut dest_path = if let Some(stripped) = dest.strip_prefix('/') {
        rootfs.join(stripped)
    } else {
        rootfs.join(dest)
    };
    // A directory (or `.`/`/`) destination means "keep the source's name".
    if dest.ends_with('/') || dest == "." || dest == "/" {
        let base = src_path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("invalid COPY/ADD source '{}'", src_path.display()))?;
        dest_path = dest_path.join(base);
    }
    if src_path.is_dir() {
        copy_dir_all(src_path, &dest_path)
    } else {
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src_path, &dest_path)
            .with_context(|| format!("failed to copy '{}' into image", src_path.display()))?;
        Ok(())
    }
}

/// Refuse Dockerfile shapes this builder cannot honour.
///
/// Both cases below would otherwise build something other than what the file
/// asks for: the parser drops every `--` flag (so `COPY --from=` would copy from
/// the context instead of a stage) and only the last `FROM` supplies the base
/// (so earlier stages vanish). A loud refusal is the only honest option until
/// per-stage rootfs support exists.
pub fn refuse_unsupported_build_features(content: &str) -> Result<()> {
    let mut stages = 0usize;
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let upper = trimmed.to_ascii_uppercase();
        if upper.starts_with("FROM ") || upper == "FROM" {
            stages += 1;
        }
        if (upper.starts_with("COPY ") || upper.starts_with("ADD ")) && trimmed.contains("--from=")
        {
            bail!(
                "line {}: 'COPY/ADD --from=' (multi-stage copy) is not supported: only the last \
                 FROM is used as the base and there is no per-stage rootfs. Build each stage \
                 separately, or copy the files from the build context",
                idx + 1
            );
        }
    }
    if stages > 1 {
        bail!(
            "this Dockerfile declares {stages} FROM stages, which this builder cannot honour: \
             only the last FROM becomes the base and the earlier stages are discarded. Build \
             each stage separately or remove the unused stages"
        );
    }
    Ok(())
}

/// Expand a COPY/ADD source pattern against the build context.
///
/// Components without a wildcard must exist literally; wildcard components are
/// matched against the directory listing. Returns the accumulated matches (empty
/// when nothing matches).
pub fn expand_glob(ctx: &std::path::Path, pattern: &str) -> Result<Vec<std::path::PathBuf>> {
    let mut current = vec![ctx.to_path_buf()];
    for comp in pattern.split('/').filter(|c| !c.is_empty() && *c != ".") {
        if comp == "**" {
            bail!(
                "'**' is not supported in a COPY/ADD source ('{pattern}'): this builder matches \
                 one path component at a time, so '**' would silently match nothing. List the \
                 files explicitly or use '*' once per component"
            );
        }
        let mut next = Vec::new();
        if comp.contains('*') || comp.contains('?') || comp.contains('[') {
            for dir in &current {
                let Ok(entries) = std::fs::read_dir(dir) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if glob_match(comp, &name) {
                        next.push(entry.path());
                    }
                }
            }
        } else {
            for dir in &current {
                let path = dir.join(comp);
                if path.exists() {
                    next.push(path);
                }
            }
        }
        next.sort();
        current = next;
    }
    Ok(current)
}

/// `*` matches any run of characters, `?` exactly one, `[...]` one character from
/// a set or range (`[abc]`, `[a-z]`, `[!0-9]`). `**` is handled (refused) by
/// [`expand_glob`] before reaching here, because it is a path-level feature.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = name.chars().collect();
    matches_from(&pat, &txt)
}

pub fn matches_from(pat: &[char], txt: &[char]) -> bool {
    let Some(&head) = pat.first() else {
        return txt.is_empty();
    };
    match head {
        '*' => (0..=txt.len()).any(|n| matches_from(&pat[1..], &txt[n..])),
        '?' => !txt.is_empty() && matches_from(&pat[1..], &txt[1..]),
        '[' => {
            if txt.is_empty() {
                return false;
            }
            match parse_class(pat) {
                // A malformed class is a literal '[' - the shell behaviour, and
                // the least surprising thing for a COPY pattern.
                Some((set, negated, rest)) => {
                    (set.contains(&txt[0]) != negated) && matches_from(rest, &txt[1..])
                }
                None => txt[0] == '[' && matches_from(&pat[1..], &txt[1..]),
            }
        }
        c => !txt.is_empty() && txt[0] == c && matches_from(&pat[1..], &txt[1..]),
    }
}

/// Parse a `[...]` class at the start of `pat`, returning its characters, whether
/// it is negated (`[!...]` or `[^...]`) and the pattern after the class.
pub fn parse_class(pat: &[char]) -> Option<(Vec<char>, bool, &[char])> {
    let mut i = 1;
    let negated = matches!(pat.get(i), Some('!') | Some('^'));
    if negated {
        i += 1;
    }
    let mut set = Vec::new();
    let mut at_start = true;
    loop {
        let c = *pat.get(i)?;
        if c == ']' && !at_start {
            return Some((set, negated, &pat[i + 1..]));
        }
        // `a-z` is a range; a '-' first or last inside the class is literal.
        if pat.get(i + 1) == Some(&'-') && !matches!(pat.get(i + 2), None | Some(&']')) {
            let hi = *pat.get(i + 2)?;
            if c > hi {
                return None;
            }
            set.extend(c..=hi);
            i += 3;
        } else {
            set.push(c);
            i += 1;
        }
        at_start = false;
    }
}

pub fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// Consume `ARG` instructions from Dockerfile content.
///
/// Returns the content with `ARG` lines removed and the declared args in order.
/// Values are substituted into the remaining instructions so later lines can
/// use `$NAME` / `${NAME}`. `proot-distro-core`'s parser has no `ARG` variant,
/// so this keeps `build local` working without changing that crate.
pub fn preprocess_build_args(content: &str) -> (String, Vec<(String, String)>) {
    let mut args: Vec<(String, String)> = Vec::new();
    let mut kept = String::with_capacity(content.len());
    for line in content.lines() {
        let trimmed = line.trim();
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let verb = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("").trim();
        if verb.eq_ignore_ascii_case("ARG") {
            if let Some((name, default)) = rest.split_once('=') {
                args.push((name.trim().to_string(), strip_arg_quotes(default.trim())));
            } else if !rest.is_empty() {
                args.push((rest.to_string(), String::new()));
            }
            continue;
        }
        kept.push_str(line);
        kept.push('\n');
    }
    let substituted = substitute_build_args(&kept, &args);
    (substituted, args)
}

pub fn strip_arg_quotes(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Replace `$NAME` / `${NAME}` with the value of a declared build arg.
pub fn substitute_build_args(content: &str, args: &[(String, String)]) -> String {
    if args.is_empty() {
        return content.to_string();
    }
    let lookup = |name: &str| {
        args.iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    };
    let bytes = content.as_bytes();
    let mut out = String::with_capacity(content.len());
    let mut i = 0;
    while i < content.len() {
        if bytes[i] == b'$' {
            if i + 1 < content.len() && bytes[i + 1] == b'{' {
                if let Some(close) = content[i + 2..].find('}') {
                    let name = &content[i + 2..i + 2 + close];
                    if let Some(v) = lookup(name) {
                        out.push_str(v);
                        i += 2 + close + 1;
                        continue;
                    }
                }
            } else {
                let start = i + 1;
                let mut end = start;
                while end < content.len()
                    && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
                {
                    end += 1;
                }
                if end > start {
                    if let Some(v) = lookup(&content[start..end]) {
                        out.push_str(v);
                        i = end;
                        continue;
                    }
                }
            }
        }
        let ch = content[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Every `build` subcommand except `build local` needs a build service that does not exist.
/// These used to print a not-implemented note and return `Ok(())`, which exits 0: a script
/// reading the status saw success for something that did nothing.
pub fn no_build_backend(what: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "'{what}' is not implemented: PDN has no build service, so there is nothing to submit, \
         track, cancel or log. Build an image locally with \
         `podnext build local <tag> --file <Dockerfile>`, then `podnext install` from it."
    )
}

pub async fn cmd_build_submit(_spec: PathBuf, _format: crate::OutputFormat) -> Result<()> {
    Err(no_build_backend("build submit"))
}

pub async fn cmd_build_status(_build_id: String, _format: crate::OutputFormat) -> Result<()> {
    Err(no_build_backend("build status"))
}

pub async fn cmd_build_cancel(_build_id: String) -> Result<()> {
    Err(no_build_backend("build cancel"))
}

pub async fn cmd_build_list(_status: Option<String>, _format: crate::OutputFormat) -> Result<()> {
    Err(no_build_backend("build list"))
}

pub async fn cmd_build_artifacts(_build_id: String) -> Result<()> {
    Err(no_build_backend("build artifacts"))
}

pub async fn cmd_build_logs(_build_id: String, _follow: bool) -> Result<()> {
    Err(no_build_backend("build logs"))
}
