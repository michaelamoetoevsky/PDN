//! Direct-execution mode: `podnext -r <rootfs> -- <cmd>`.
//!
//! Two entry points reach the same config. `scan_direct_execution` runs before
//! clap, because a `--`-separated command must not be re-parsed as flags, and
//! refuses the flags the engine cannot honour; `execute_direct` is the
//! clap-parsed path. Both build the same `proot_core::ProotConfig`.

use super::*;

use anyhow::bail;

pub(crate) async fn execute_direct(cli: &Cli, trailing_args: &[String]) -> Result<()> {
    if trailing_args.is_empty() {
        bail!("Direct execution mode requires a command. Use 'podnext -r /path/to/rootfs -- <command>'");
    }

    pn_trace!("[DEBUG direct] Rootfs: {:?}", cli.rootfs);
    pn_trace!("[DEBUG direct] Binds: {:?}", cli.bind);
    pn_trace!("[DEBUG direct] Working dir: {:?}", cli.pwd);
    pn_trace!("[DEBUG direct] QEMU: {:?}", cli.qemu);
    pn_trace!("[DEBUG direct] Link2symlink: {}", cli.link2symlink);
    pn_trace!("[DEBUG direct] No sysvipc: {}", cli.no_sysvipc);
    pn_trace!("[DEBUG direct] Sysvipc: {}", cli.sysvipc);
    pn_trace!("[DEBUG direct] Kernel: {:?}", cli.kernel);
    pn_trace!("[DEBUG direct] Hostname: {:?}", cli.hostname);
    pn_trace!("[DEBUG direct] UID: {:?}", cli.uid);
    pn_trace!("[DEBUG direct] GID: {:?}", cli.gid);
    pn_trace!("[DEBUG direct] Seccomp: {}", cli.seccomp);
    pn_trace!("[DEBUG direct] No seccomp: {}", cli.no_seccomp);
    pn_trace!("[DEBUG direct] Trace: {}", cli.trace);
    pn_trace!("[DEBUG direct] Kill on exit: {}", cli.kill_on_exit);
    pn_trace!("[DEBUG direct] No kill on exit: {}", cli.no_kill_on_exit);
    pn_trace!("[DEBUG direct] Env: {:?}", cli.env);
    pn_trace!("[DEBUG direct] Command: {:?}", trailing_args);

    // Create a temporary container config for direct execution
    let rootfs_path = cli.rootfs.as_ref().unwrap();

    // Use proot-core directly for direct execution
    let mut config = proot_core::ProotConfig::default();
    config.verbose = cli.verbose;
    let mut engine = proot_core::ProotEngine::new(config)?;

    engine.config.rootfs = Some(rootfs_path.into());

    // Process bind mounts
    for bind in &cli.bind {
        engine.config.bind.push(bind.clone());
    }

    // Set working directory
    if let Some(pwd) = &cli.pwd {
        engine.config.cwd = Some(pwd.into());
    }

    // Set environment variables
    for env in &cli.env {
        engine.config.env.push(env.clone());
    }

    // Set kernel release
    if let Some(kernel) = &cli.kernel {
        engine.config.kernel_release = Some(kernel.clone());
    }

    // Set hostname
    if let Some(hostname) = &cli.hostname {
        engine.config.hostname = Some(hostname.clone());
    }

    // Set UID/GID
    if let Some(uid) = cli.uid {
        engine.config.uid = Some(uid);
    }
    if let Some(gid) = cli.gid {
        engine.config.gid = Some(gid);
    }

    // Asking to switch seccomp filtering OFF stays a no-op: it requests nothing. Asking to switch
    // it ON is refused before dispatch, where the global flag is read (see `refuse_unenforceable`).
    if cli.no_seccomp {
        engine.config.seccomp = false;
    }

    // Set trace
    if cli.trace {
        engine.config.trace = true;
    }

    // Kill-on-exit is the default and `proot-core` passes `--kill-on-exit` to proot
    // unconditionally; `--no-kill-on-exit` refuses before dispatch instead (issue #63).
    if cli.kill_on_exit {
        engine.config.kill_on_exit = true;
    }

    // Set sysvipc
    if cli.no_sysvipc {
        engine.config.sysvipc = false;
    }
    if cli.sysvipc {
        engine.config.sysvipc = true;
    }

    // Set link2symlink
    if cli.link2symlink {
        engine.config.link2symlink = true;
    }

    // Set command
    engine.config.command = trailing_args.to_vec();

    // Execute and wait for completion
    let exit_code = engine.execute().await? as i32;
    if exit_code != 0 {
        return Err(anyhow::Error::new(proot_core::ChildExit(exit_code)));
    }

    Ok(())
}

pub(crate) async fn execute_direct_manual(config: proot_core::ProotConfig) -> Result<()> {
    if config.command.is_empty() {
        bail!("Direct execution mode requires a command. Use 'podnext -r /path/to/rootfs -- <command>'");
    }

    pn_trace!("[DEBUG direct] Rootfs: {:?}", config.rootfs);
    pn_trace!("[DEBUG direct] Command: {:?}", config.command);
    pn_trace!("[DEBUG direct] Verbose: {}", config.verbose);

    // Use proot-core directly for direct execution. The config already carries every flag this
    // entry point can honour; the ones it cannot refused in the scanner before reaching here.
    let mut engine = proot_core::ProotEngine::new(config)?;

    // Execute and wait for completion
    let exit_code = engine.execute().await? as i32;
    if exit_code != 0 {
        return Err(anyhow::Error::new(proot_core::ChildExit(exit_code)));
    }

    Ok(())
}

/// The manual `podnext -r <rootfs> -- <cmd>` path, taken before clap.
pub(crate) async fn scan_direct_execution(raw_args: &[String], pn_debug: bool) -> Result<()> {
    // Parse all flags manually for direct execution
    // Format: podnext [flags...] -r <rootfs> [flags...] -- <cmd...>

    let mut rootfs: Option<String> = None;
    let mut binds = Vec::new();
    let mut pwd: Option<String> = None;
    let mut qemu: Option<String> = None;
    let mut link2symlink = false;
    let mut no_sysvipc = false;
    let mut sysvipc = false;
    let mut kernel: Option<String> = None;
    let mut hostname: Option<String> = None;
    let mut uid: Option<u32> = None;
    let mut gid: Option<u32> = None;
    let mut seccomp = false;
    let mut no_seccomp = false;
    let mut trace = false;
    let mut kill_on_exit = false;
    let mut no_kill_on_exit = false;
    let mut env = Vec::new();
    let mut verbose = false;
    let mut cmd_args: Vec<String> = Vec::new();

    let mut i = 1; // Skip program name

    // Parse all flags until --
    while i < raw_args.len() {
        let arg = &raw_args[i];
        if arg == "--" {
            // Remaining args are the command
            cmd_args = raw_args[i + 1..].to_vec();
            break;
        }

        match arg.as_str() {
            "-r" | "--rootfs" => {
                i += 1;
                if i < raw_args.len() {
                    rootfs = Some(raw_args[i].clone());
                }
            }
            "-b" | "--bind" => {
                i += 1;
                if i < raw_args.len() {
                    binds.push(raw_args[i].clone());
                }
            }
            "-w" | "--pwd" => {
                i += 1;
                if i < raw_args.len() {
                    pwd = Some(raw_args[i].clone());
                }
            }
            "-q" | "--qemu" => {
                i += 1;
                if i < raw_args.len() {
                    qemu = Some(raw_args[i].clone());
                }
            }
            "-0" | "--link2symlink" => {
                link2symlink = true;
            }
            "--no-sysvipc" => {
                no_sysvipc = true;
            }
            "--sysvipc" => {
                sysvipc = true;
            }
            "--kernel" => {
                i += 1;
                if i < raw_args.len() {
                    kernel = Some(raw_args[i].clone());
                }
            }
            "--hostname" => {
                i += 1;
                if i < raw_args.len() {
                    hostname = Some(raw_args[i].clone());
                }
            }
            "--uid" => {
                i += 1;
                if i < raw_args.len() {
                    uid = raw_args[i].parse().ok();
                }
            }
            "--gid" => {
                i += 1;
                if i < raw_args.len() {
                    gid = raw_args[i].parse().ok();
                }
            }
            "--seccomp" => {
                seccomp = true;
            }
            "--no-seccomp" => {
                no_seccomp = true;
            }
            "--trace" => {
                trace = true;
            }
            "--kill-on-exit" => {
                kill_on_exit = true;
            }
            "--no-kill-on-exit" => {
                no_kill_on_exit = true;
            }
            "-e" | "--env" => {
                i += 1;
                if i < raw_args.len() {
                    env.push(raw_args[i].clone());
                }
            }
            "-v" | "--verbose" => {
                verbose = true;
            }
            _ => {
                // Unknown flag, skip
            }
        }
        i += 1;
    }

    // Measured on this device: with `PN_DEBUG=1 podnext -r <rootfs> <flag> -- /bin/true`,
    // tokenising the `[DEBUG] Executing proot command` line, a bare direct run is 37 tokens,
    // and each flag below leaves that multiset unchanged -- because proot-core's `execute()`
    // never reads the field the flag would set (it hard-codes the token, or has no field at
    // all). An option the caller can pass that changes nothing is the class this project
    // refuses to ship (#62, #63, #67), so each refuses here, before any work, rather than
    // being parsed and dropped. The string `--seccomp is not implemented` is the one the
    // issue ledger greps for (issue #62).
    if seccomp {
        anyhow::bail!(
            "--seccomp is not implemented: this PRoot build applies no seccomp filter, so the \
             flag would be accepted and never enforced (the generated proot argv is \
             byte-identical with and without it). Drop the flag, or use a runtime that can \
             enforce it."
        );
    }
    if no_kill_on_exit {
        anyhow::bail!(
            "--no-kill-on-exit is not implemented: proot-core passes `--kill-on-exit` to proot \
             unconditionally, so the flag would be accepted and never enforced (the generated \
             proot argv is byte-identical with and without it). Drop the flag (issue #63)."
        );
    }
    if kill_on_exit {
        anyhow::bail!(
            "--kill-on-exit is not implemented on the direct-execution path: kill-on-exit is \
             the default and proot-core passes `--kill-on-exit` to proot unconditionally, so \
             the flag changes nothing (measured: the generated proot argv is token-for-token \
             identical with and without it, 37 tokens). Drop the flag (issue #67)."
        );
    }
    if link2symlink {
        anyhow::bail!(
            "--link2symlink is not implemented on the direct-execution path: link2symlink is \
             the default and proot-core passes `--link2symlink` to proot unconditionally, so \
             the flag changes nothing (measured: the generated proot argv is token-for-token \
             identical with and without it, 37 tokens). Drop the flag (issue #67)."
        );
    }
    if sysvipc {
        anyhow::bail!(
            "--sysvipc is not implemented on the direct-execution path: proot-core passes \
             `--sysvipc` to proot unconditionally, so the flag changes nothing (measured: the \
             generated proot argv is token-for-token identical with and without it, 37 \
             tokens). Drop the flag (issue #67)."
        );
    }
    if no_sysvipc {
        anyhow::bail!(
            "--no-sysvipc is not implemented on the direct-execution path: proot-core passes \
             `--sysvipc` to proot unconditionally, so the flag cannot turn it off (measured: \
             the generated proot argv is token-for-token identical with and without it, 37 \
             tokens). Drop the flag (issue #67)."
        );
    }
    if trace {
        anyhow::bail!(
            "--trace is not implemented on the direct-execution path: proot-core never reads \
             config.trace, so the flag changes nothing (measured: the generated proot argv is \
             token-for-token identical with and without it, 37 tokens). Honouring it needs \
             engine plumbing in proot-core. Drop the flag (issue #67)."
        );
    }
    if qemu.is_some() {
        anyhow::bail!(
            "--qemu is not implemented on the direct-execution path: proot-core's ProotConfig \
             has no qemu field, so the value has nowhere to reach (measured: the generated \
             proot argv is token-for-token identical with and without it, 37 tokens). \
             Honouring it needs engine plumbing in proot-core. Drop the flag (issue #67)."
        );
    }
    if uid.is_some() {
        anyhow::bail!(
            "--uid is not implemented on the direct-execution path: proot-core hard-codes \
             `--change-id=0:0:0` and never reads config.uid, so the flag changes nothing \
             (measured: the generated proot argv is token-for-token identical with and without \
             it, 37 tokens). Honouring it needs engine plumbing in proot-core. Drop the flag \
             (issue #67)."
        );
    }
    if gid.is_some() {
        anyhow::bail!(
            "--gid is not implemented on the direct-execution path: proot-core hard-codes \
             `--change-id=0:0:0` and never reads config.gid, so the flag changes nothing \
             (measured: the generated proot argv is token-for-token identical with and without \
             it, 37 tokens). Honouring it needs engine plumbing in proot-core. Drop the flag \
             (issue #67)."
        );
    }

    if rootfs.is_none() {
        eprintln!("Error: -r/--rootfs is required for direct execution mode");
        std::process::exit(1);
    }

    if cmd_args.is_empty() {
        eprintln!("Error: Direct execution mode requires a command after --");
        std::process::exit(1);
    }

    if pn_debug {
        pn_trace!("[DEBUG main] Calling execute_direct_manual with all flags");
    }

    // The flags the engine can honour are threaded through here; the rest refuse above. This
    // config is the same one the clap path builds in `execute_direct`, so both direct
    // entry points agree on what a flag does (issue #67).
    let mut config = proot_core::ProotConfig::default();
    config.verbose = if verbose { 1 } else { 0 };
    config.rootfs = Some(rootfs.unwrap().into());
    config.command = cmd_args.clone();
    config.bind = binds.clone();
    config.cwd = pwd.map(Into::into);
    config.env = env.clone();
    config.kernel_release = kernel;
    config.hostname = hostname;
    // Asking to switch seccomp filtering OFF requests nothing; asking to switch it ON is
    // refused above (#62).
    if no_seccomp {
        config.seccomp = false;
    }
    return execute_direct_manual(config).await;
}
