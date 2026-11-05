//! The container session path: foreground login and run, signal forwarding to
//! the guest, and the detached-session PTY supervisor.
//!
//! Split out of `native` in wave 7; behaviour unchanged.

use super::meta::apply_image_runtime_config;
use anyhow::{bail, Context, Result};
use std::io::{self, IsTerminal, Write};
use std::os::unix::process::CommandExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::signal::unix::{signal, SignalKind};

pub async fn login_container_native(
    container_name: &str,
    isolated: bool,
    command: Vec<String>,
    mounts: Vec<proot_distro_core::container::BindMount>,
) -> Result<()> {
    let container_mgr = proot_distro_core::ContainerManager::new()?;
    if let Ok(c) = container_mgr.get(container_name).await {
        let _ = proot_distro_core::network::ensure_resolv_conf(&c.rootfs_path);
    }
    pn_trace!("[DEBUG native] Creating ProotDistroCore");
    let mut distro_core = proot_distro_core::ProotDistroCore::new()?;
    pn_trace!("[DEBUG native] ProotDistroCore created");

    let mut options = proot_distro_core::LoginOptions::default();
    options.container_name = container_name.to_string();
    options.isolated = isolated;
    // `ProotDistroCore::login` folds these into the container config it loads, which is
    // what the runtime renders into `--bind` arguments.
    options.bind_mounts = mounts;
    pn_trace!("[DEBUG native] Default command: {:?}", options.command);
    options.command = if command.is_empty() {
        vec!["/bin/sh".to_string()]
    } else {
        command
    };
    pn_trace!("[DEBUG native] Final command: {:?}", options.command);

    // `--user` is refused at the command boundary (`cmd_login`): the runtime passes
    // `--change-id=0:0:0` unconditionally, mapping every guest id to the single host uid, so there
    // is no user value to set here and none is accepted (#66).

    pn_trace!("[DEBUG native] Calling login");
    distro_core.login(options).await?;
    pn_trace!("[DEBUG native] Login completed");
    Ok(())
}

/// PID of this process's only direct child during a foreground run — the proot
/// process the runtime spawned. proot-core keeps the child pid private, so the
/// run path has to discover it from `/proc` (matching `PPid`). Returns `None`
/// when no child is currently visible.
pub(crate) fn direct_child_pid() -> Option<i32> {
    let me = std::process::id() as i32;
    for entry in std::fs::read_dir("/proc").ok()?.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<i32>().ok())
        else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        // `comm` (field 2) is parenthesised and may contain spaces/parens, so
        // the remaining fields only start reliably after the last ')'.
        let Some(rest) = stat.rfind(')').map(|i| &stat[i + 1..]) else {
            continue;
        };
        let mut fields = rest.split_whitespace();
        let _state = fields.next();
        if fields.next().and_then(|s| s.parse::<i32>().ok()) == Some(me) {
            return Some(pid);
        }
    }
    None
}

/// Forward `sig` to the running guest.
///
/// Sent to the child's *process group* so the whole traced tree is signalled at
/// once. The runtime is being given `process_group(0)` on spawn (seat
/// kill_correct); until that lands the child is not a group leader, `kill(-pid)`
/// returns ESRCH, and this falls back to signalling the child pid directly —
/// proot's own `--kill-on-exit` then tears down its tracees.
pub(crate) fn forward_to_guest(sig: i32) {
    let Some(pid) = direct_child_pid() else {
        return;
    };
    unsafe {
        if libc::kill(-pid, sig) != 0 {
            libc::kill(pid, sig);
        }
    }
}

pub async fn run_container_native(
    container_name: &str,
    command: Vec<String>,
    isolated: bool,
    mounts: Vec<proot_distro_core::container::BindMount>,
) -> Result<()> {
    let container_mgr = proot_distro_core::ContainerManager::new()?;
    let mut container = container_mgr.get(container_name).await?;
    let _ = proot_distro_core::network::ensure_resolv_conf(&container.rootfs_path);
    // The runtime renders `config.bind_mounts`; a mount the caller asked for has to be
    // in there or the run happens without it (it used to be dropped in `cmd_run`).
    container.config.bind_mounts.extend(mounts);
    // The loader does not carry the image's recorded ENV/WORKDIR; reinstate them
    // so `podnext run` behaves as the image was built (#30 residual gap 1).
    apply_image_runtime_config(&mut container);
    if isolated {
        container.config.isolated = true;
    }
    let backend = container_mgr.get_backend(&container);

    // A foreground run must survive a SIGTERM/SIGINT long enough to kill the
    // guest. Without these handlers the default action terminated podnext
    // immediately and orphaned the proot process (and everything it traced).
    // `signal()`'s handler registration is dropped when this fn returns.
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    let exec = backend.execute(&container, command);
    tokio::pin!(exec);

    let mut forwarded: Option<i32> = None;
    let result = loop {
        tokio::select! {
            res = &mut exec => break res,
            _ = sigterm.recv() => {
                forwarded = Some(libc::SIGTERM);
                forward_to_guest(libc::SIGTERM);
            }
            _ = sigint.recv() => {
                forwarded = Some(libc::SIGINT);
                forward_to_guest(libc::SIGINT);
            }
        }
    };

    // The runtime registered a session record for this pid and it is now spent;
    // remove it so `podnext ps`/sessions do not accumulate a stale entry. Runs
    // on normal, signalled and error exits alike.
    let _ = std::fs::remove_file(proot_distro_core::session::session_file_path(
        std::process::id(),
    ));

    let exit_code = result?;
    // A guest killed by signal N surfaces as 128+N (the shell convention), not
    // the -1/255 the runtime reports for a signal-terminated child.
    if let Some(sig) = forwarded {
        if exit_code != 0 {
            return Err(anyhow::Error::new(proot_core::ChildExit(128 + sig)));
        }
    }
    if exit_code != 0 {
        if exit_code == 255 {
            eprintln!(
                "Hint: if the guest terminated with signal 4 (SIGILL), its binaries are \
                 incompatible with this host CPU/kernel; consider running with QEMU emulation (-q)."
            );
        }
        return Err(anyhow::Error::new(proot_core::ChildExit(exit_code)));
    }
    Ok(())
}

// ===== Detached sessions: the `--detach` / `attach` round trip (PDN#57) =====
//
// A detached session is a supervisor process (`attach <container>` re-invoked
// with `PODNEXT_SESSION_HOST=1`) that owns a PTY pair: the guest runs on the
// slave, the supervisor holds the master and serves a UNIX socket. The
// supervisor is put in its own session by the launcher, so it is not in the
// launcher's process group and survives the launcher exiting - the #68 lesson
// applied to detach. Measured on device: two separate `attach` connections were
// answered by the same guest shell pid, and a SIGKILL of the supervisor left no
// guest and no supervisor behind.

/// Marker telling `attach` to run the supervisor loop instead of reconnecting.
/// The feature reuses one existing subcommand because a new top-level command is
/// a v0.1.9 non-goal (`scripts/non_goals.json`).
const SESSION_HOST_ENV: &str = "PODNEXT_SESSION_HOST";
const SESSION_CMD_ENV: &str = "PODNEXT_SESSION_CMD";
const SESSION_ISOLATED_ENV: &str = "PODNEXT_SESSION_ISOLATED";
const SESSION_MOUNTS_ENV: &str = "PODNEXT_SESSION_MOUNTS";
const SESSION_WINSIZE_ENV: &str = "PODNEXT_SESSION_WINSIZE";

/// Ctrl-P Ctrl-Q, the detach chord the attaching client watches for.
const DETACH_CHORD: [u8; 2] = [0x10, 0x11];

/// True when this process is the detached supervisor (`attach` re-invoked by
/// `run --detach`) rather than an attaching client.
pub fn is_session_host() -> bool {
    std::env::var_os(SESSION_HOST_ENV).is_some()
}

fn parse_winsize(spec: Option<String>) -> (u16, u16) {
    let spec = spec.unwrap_or_default();
    let mut parts = spec.split(['x', 'X']);
    let rows = parts
        .next()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(24u16);
    let cols = parts
        .next()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(80u16);
    (rows.max(1), cols.max(1))
}

/// Size of the terminal this process is on, or `None` when stdin is not a tty
/// (a detached start from a script inherits no size).
fn terminal_size() -> Option<(u16, u16)> {
    if !io::stdin().is_terminal() {
        return None;
    }
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::ioctl(0, libc::TIOCGWINSZ as _, &mut ws) };
    if rc == 0 && ws.ws_row > 0 && ws.ws_col > 0 {
        Some((ws.ws_row, ws.ws_col))
    } else {
        None
    }
}

fn set_winsize(fd: libc::c_int, rows: u16, cols: u16) {
    let ws = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    unsafe {
        libc::ioctl(fd, libc::TIOCSWINSZ as _, &ws);
    }
}

fn set_nonblocking(fd: libc::c_int) -> Result<()> {
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            bail!(
                "fcntl(O_NONBLOCK) on the pty master failed: {}",
                io::Error::last_os_error()
            );
        }
    }
    Ok(())
}

/// Feed bytes to the guest through the pty master. The master is non-blocking,
/// so a full pty buffer retries rather than blocking the supervisor loop.
async fn write_master(fd: libc::c_int, mut data: &[u8]) {
    while !data.is_empty() {
        let n = unsafe { libc::write(fd, data.as_ptr() as *const libc::c_void, data.len()) };
        if n > 0 {
            data = &data[n as usize..];
            continue;
        }
        let err = io::Error::last_os_error();
        let again =
            matches!(err.raw_os_error(), Some(e) if e == libc::EAGAIN || e == libc::EWOULDBLOCK);
        if err.kind() == io::ErrorKind::Interrupted {
            continue;
        } else if again {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        } else {
            break;
        }
    }
}

/// Index of the detach chord in `buf`, if present.
fn detach_at(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == DETACH_CHORD)
}

/// Restores the caller's terminal settings when dropped.
struct RawMode {
    saved: Option<libc::termios>,
}

impl RawMode {
    fn enable() -> Self {
        if !io::stdin().is_terminal() {
            return Self { saved: None };
        }
        unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(0, &mut t) != 0 {
                return Self { saved: None };
            }
            let saved = t.clone();
            t.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG | libc::IEXTEN);
            t.c_iflag &= !(libc::IXON | libc::ICRNL | libc::BRKINT | libc::INPCK | libc::ISTRIP);
            t.c_oflag &= !libc::OPOST;
            t.c_cc[libc::VMIN as usize] = 1;
            t.c_cc[libc::VTIME as usize] = 0;
            libc::tcsetattr(0, libc::TCSANOW, &t);
            Self { saved: Some(saved) }
        }
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        if let Some(t) = self.saved {
            unsafe {
                libc::tcsetattr(0, libc::TCSANOW, &t);
            }
        }
    }
}

/// Start `container` in the background under a supervised PTY (`run --detach`).
///
/// The supervisor is re-invoked from this same binary and, because it is given
/// its own session, is not in this process's group and does not hold this
/// terminal - so it outlives this process. Returns once the session has
/// registered, having printed the supervisor pid, so an immediate `attach` finds
/// it.
pub async fn spawn_detached_session(
    container: &str,
    command: Vec<String>,
    isolated: bool,
    mounts: Vec<proot_distro_core::container::BindMount>,
) -> Result<()> {
    let exe = std::env::current_exe().context("resolve the podnext binary path")?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("attach").arg(container);
    cmd.env(SESSION_HOST_ENV, "1");
    cmd.env(SESSION_CMD_ENV, serde_json::to_string(&command)?);
    cmd.env(SESSION_ISOLATED_ENV, if isolated { "1" } else { "0" });
    cmd.env(SESSION_MOUNTS_ENV, serde_json::to_string(&mounts)?);
    if let Some((rows, cols)) = terminal_size() {
        cmd.env(SESSION_WINSIZE_ENV, format!("{}x{}", rows, cols));
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // Own session: not the caller's group, so no hangup on the caller's terminal
    // reaches the supervisor (#68 is the reason the group matters).
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = cmd
        .spawn()
        .context("spawn the detached session supervisor")?;
    let pid = child.id();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let live = proot_distro_core::session::active_sessions()
            .map(|s| s.iter().any(|r| r.pid == pid))
            .unwrap_or(false);
        if live {
            break;
        }
        match child.try_wait() {
            Ok(Some(status)) => bail!(
                "the detached session ended before it could be attached (pid {}, status {}); \
                 its output, if any, is in {}",
                pid,
                status,
                proot_distro_core::session::session_log_path(pid).display()
            ),
            Ok(None) => {}
            Err(e) => bail!("wait on the detached session supervisor failed: {e}"),
        }
        if std::time::Instant::now() > deadline {
            bail!(
                "the detached session (pid {}) did not register within 5s",
                pid
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    println!("{}", pid);
    Ok(())
}

/// Reconnect to the running detached session for `container` (`podnext attach`).
///
/// Copies this terminal to and from the session's socket, in raw mode, until the
/// guest exits or the detach chord is pressed. It never starts a session: a
/// session is `login` (foreground) or `run --detach` (background).
pub async fn attach_session(
    container: &str,
    user: Option<String>,
    command: Vec<String>,
) -> Result<()> {
    if user.is_some() || !command.is_empty() {
        bail!(
            "`attach {container}` reconnects to a running session and does not start one. \
             Use `podnext login {container} -- <command>` for a new foreground session, or \
             `podnext run --detach {container} -- <command>` to start one in the background."
        );
    }
    let pid = proot_distro_core::session::active_sessions()
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.container_name == container)
        .max_by_key(|s| s.started_at)
        .map(|s| s.pid)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No running session for container '{}'. Start one with \
                 `podnext run --detach {} -- <command>`.",
                container,
                container
            )
        })?;
    let sock = proot_distro_core::session::session_socket_path(pid);
    let stream = tokio::net::UnixStream::connect(&sock)
        .await
        .with_context(|| {
            format!(
                "session {} for '{}' has no reachable rendezvous socket ({})",
                pid,
                container,
                sock.display()
            )
        })?;

    let raw = RawMode::enable();
    let (rows, cols) = terminal_size().unwrap_or((24, 80));
    let (mut rd, mut wr) = stream.into_split();
    let mut hdr = [0u8; 6];
    hdr[0] = b'W';
    hdr[1] = b'S';
    hdr[2..4].copy_from_slice(&rows.to_le_bytes());
    hdr[4..6].copy_from_slice(&cols.to_le_bytes());
    wr.write_all(&hdr).await?;

    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut inbuf = [0u8; 4096];
    let mut outbuf = [0u8; 4096];
    loop {
        tokio::select! {
            n = stdin.read(&mut inbuf) => match n {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if let Some(pos) = detach_at(&inbuf[..n]) {
                        if pos > 0 {
                            wr.write_all(&inbuf[..pos]).await?;
                        }
                        break;
                    }
                    wr.write_all(&inbuf[..n]).await?;
                }
            },
            n = rd.read(&mut outbuf) => match n {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    stdout.write_all(&outbuf[..n]).await?;
                    stdout.flush().await?;
                }
            },
        }
    }
    drop(raw);
    Ok(())
}

/// The detached supervisor: owns the PTY for the guest and serves `attach`.
///
/// Invoked as `attach <container>` with `PODNEXT_SESSION_HOST=1`; the guest
/// command and options arrive through the environment so no new subcommand is
/// needed.
pub async fn run_session_host(container: &str) -> Result<()> {
    use std::os::fd::FromRawFd;

    let command: Vec<String> =
        serde_json::from_str(&std::env::var(SESSION_CMD_ENV).unwrap_or_default())
            .unwrap_or_default();
    let isolated = std::env::var(SESSION_ISOLATED_ENV).as_deref() == Ok("1");
    let mounts: Vec<proot_distro_core::container::BindMount> =
        serde_json::from_str(&std::env::var(SESSION_MOUNTS_ENV).unwrap_or_default())
            .unwrap_or_default();
    let (rows, cols) = parse_winsize(std::env::var(SESSION_WINSIZE_ENV).ok());

    // A real PTY for the guest: terminal fidelity (#56) only holds on a tty, and
    // a reattach needs the master to keep existing after the launcher is gone.
    let mut master: libc::c_int = -1;
    let mut slave: libc::c_int = -1;
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if rc != 0 {
        bail!("openpty failed: {}", io::Error::last_os_error());
    }
    set_winsize(master, rows, cols);
    // Own session, slave as controlling terminal, guest stdio on the slave: the
    // guest reads a tty in its foreground group, exactly as an interactive
    // `podnext login` does (issue #68).
    unsafe {
        libc::setsid();
        libc::ioctl(slave, libc::TIOCSCTTY as _, 0);
        libc::dup2(slave, 0);
        libc::dup2(slave, 1);
        libc::dup2(slave, 2);
        if slave > 2 {
            libc::close(slave);
        }
    }

    let pid = std::process::id();
    let sock_path = proot_distro_core::session::session_socket_path(pid);
    // Reclaim a stale path: a supervisor that was SIGKILLed leaves its socket
    // file behind, and a second bind at the same path would otherwise fail.
    proot_distro_core::session::reap_stale_rendezvous();
    let _ = std::fs::remove_file(&sock_path);
    let listener = tokio::net::UnixListener::bind(&sock_path)
        .with_context(|| format!("bind session socket {}", sock_path.display()))?;
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(proot_distro_core::session::session_log_path(pid))?;

    // The master is drained on the async reactor; a guest blocked on a full pty
    // buffer would otherwise never become reattachable.
    set_nonblocking(master)?;
    let master_file = unsafe { std::fs::File::from_raw_fd(master) };
    let afd = tokio::io::unix::AsyncFd::new(master_file)?;

    let guest = login_container_native(container, isolated, command, mounts);
    tokio::pin!(guest);

    let (in_tx, mut in_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let mut client_tx: Option<tokio::sync::mpsc::UnboundedSender<Vec<u8>>> = None;

    let result = loop {
        tokio::select! {
            r = &mut guest => break r,
            ac = listener.accept() => {
                if let Ok((stream, _)) = ac {
                    let (mut rd, wr) = stream.into_split();
                    let tx = in_tx.clone();
                    // Reader: client -> master. The first frame is the winsize.
                    tokio::spawn(async move {
                        let mut hdr = [0u8; 6];
                        if rd.read_exact(&mut hdr).await.is_ok() && &hdr[..2] == b"WS" {
                            let rows = u16::from_le_bytes([hdr[2], hdr[3]]);
                            let cols = u16::from_le_bytes([hdr[4], hdr[5]]);
                            set_winsize(master, rows, cols);
                        }
                        let mut buf = [0u8; 4096];
                        loop {
                            match rd.read(&mut buf).await {
                                Ok(0) | Err(_) => break,
                                Ok(n) => {
                                    if tx.send(buf[..n].to_vec()).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    });
                    // Writer: master -> client. Assigning a new sender drops the
                    // previous one, so a superseded attachment is closed.
                    let (new_tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
                    tokio::spawn(async move {
                        let mut wr = wr;
                        while let Some(buf) = rx.recv().await {
                            if wr.write_all(&buf).await.is_err() {
                                break;
                            }
                        }
                    });
                    client_tx = Some(new_tx);
                }
            }
            ready = afd.readable() => {
                if let Ok(mut g) = ready {
                    let mut buf = [0u8; 8192];
                    let n = unsafe {
                        libc::read(
                            master,
                            buf.as_mut_ptr() as *mut libc::c_void,
                            buf.len(),
                        )
                    };
                    if n > 0 {
                        let _ = log.write_all(&buf[..n as usize]);
                        let _ = log.flush();
                        if let Some(tx) = client_tx.as_ref() {
                            let _ = tx.send(buf[..n as usize].to_vec());
                        }
                    } else if n < 0 {
                        let err = io::Error::last_os_error();
                        if err.kind() == io::ErrorKind::WouldBlock {
                            // Readiness was for the future; the read did not
                            // consume it, so it must be cleared or the loop spins.
                            g.clear_ready();
                        }
                    }
                }
            }
            chunk = in_rx.recv() => {
                if let Some(data) = chunk {
                    write_master(master, &data).await;
                }
            }
        }
    };

    let _ = std::fs::remove_file(proot_distro_core::session::session_socket_path(pid));
    result
}
