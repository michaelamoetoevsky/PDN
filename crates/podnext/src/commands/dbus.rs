//! D-Bus broker commands: `podnext dbus serve`, `list`, `call` and `monitor`.
//!
//! Split out of `commands/mod.rs` for roadmap section 9.2. Re-exported with
//! `pub use dbus::*;` in `commands/mod.rs`.

use anyhow::{Context, Result};
use serde::Serialize;
use tabled::Tabled;

use pn_next::{DBusBroker, DBusBrokerConfig};

use super::{output_json, output_table, output_yaml};
pub async fn execute_dbus_command(cmd: crate::DbusCommands) -> Result<()> {
    match cmd {
        crate::DbusCommands::Serve { socket } => cmd_dbus_serve(socket).await,
        crate::DbusCommands::List { format } => cmd_dbus_list(format).await,
        crate::DbusCommands::Call {
            bus,
            socket,
            path,
            interface,
            method,
            args,
        } => cmd_dbus_call(bus, socket, path, interface, method, args).await,
        crate::DbusCommands::Monitor { bus } => cmd_dbus_monitor(bus).await,
    }
}

/// Listen on the broker socket until interrupted.
pub async fn cmd_dbus_serve(socket: Option<String>) -> Result<()> {
    let mut config = DBusBrokerConfig::default();
    if let Some(path) = socket {
        config.socket_path = path;
    }
    let path = config.socket_path.clone();

    let broker = DBusBroker::new(config);
    println!("D-Bus broker listening on {}", path);
    println!("Press Ctrl-C to stop.");

    // The signature is `&self`, so the handle has to outlive the call.
    tokio::select! {
        result = broker.start() => result,
        _ = tokio::signal::ctrl_c() => {
            println!("Stopped.");
            Ok(())
        }
    }
}

pub async fn cmd_dbus_list(format: crate::OutputFormat) -> Result<()> {
    let config = DBusBrokerConfig::default();
    let broker = DBusBroker::new(config);
    let services = broker.list_services(None).await;

    #[derive(Tabled, Serialize)]
    struct DbusRow {
        #[tabled(rename = "NAME")]
        name: String,
        #[tabled(rename = "PID")]
        pid: u32,
        #[tabled(rename = "CONTAINER")]
        container: String,
    }

    let rows: Vec<DbusRow> = services
        .iter()
        .map(|s| DbusRow {
            name: s.name.clone(),
            pid: s.pid,
            container: s.container.clone().unwrap_or("-".to_string()),
        })
        .collect();

    match format {
        crate::OutputFormat::Table => output_table(&rows),
        crate::OutputFormat::Json => output_json(&services)?,
        crate::OutputFormat::Yaml => output_yaml(&services)?,
        crate::OutputFormat::Text => {
            for s in rows {
                println!("{} {} {}", s.name, s.pid, s.container);
            }
        }
    }
    Ok(())
}

/// Speak PDN's own broker framing: a 4-byte little-endian length prefix
/// followed by one JSON message. This is *not* the binary D-Bus protocol, so
/// `ZBusClient` cannot talk to this broker.
pub async fn dbus_roundtrip(
    socket_path: &str,
    message: &serde_json::Value,
) -> Result<Vec<serde_json::Value>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = tokio::net::UnixStream::connect(socket_path)
        .await
        .with_context(|| {
            format!(
                "Cannot reach the D-Bus broker at {}. Start it first (it is started by \
                 'podnext daemon', or run 'podnext dbus serve' in another shell).",
                socket_path
            )
        })?;

    let payload = serde_json::to_vec(message)?;
    stream
        .write_all(&(payload.len() as u32).to_le_bytes())
        .await?;
    stream.write_all(&payload).await?;
    stream.flush().await?;

    // The broker pushes an unsolicited Hello on connect, then one message per
    // request. Read until we have seen a reply to something other than the
    // handshake, or the peer closes.
    let mut replies = Vec::new();
    // The broker pushes an unsolicited Hello before it reads anything. Hello
    // carries a destination (the client's unique name) just like a real reply
    // does, so it cannot be told apart that way — it is matched by member.
    for _ in 0..5 {
        let mut len_buf = [0u8; 4];
        match stream.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }
        let len = u32::from_le_bytes(len_buf) as usize;
        if len == 0 || len > 16 * 1024 * 1024 {
            anyhow::bail!("Broker sent a nonsensical frame length ({})", len);
        }
        let mut buf = vec![0u8; len];
        stream.read_exact(&mut buf).await?;
        let value: serde_json::Value = serde_json::from_slice(&buf).context(
            "The broker's reply is not the JSON framing this client speaks. A real \
             D-Bus daemon answers in the binary protocol; 'podnext dbus call' targets \
             PDN's own broker only.",
        )?;

        if value["header"]["member"].as_str() == Some("Hello") {
            continue;
        }

        replies.push(value);
        // One request, one reply: anything past this is unsolicited and is
        // reported to the caller rather than looped on forever.
        break;
    }

    Ok(replies)
}

pub async fn cmd_dbus_call(
    bus: String,
    socket: Option<String>,
    path: String,
    interface: String,
    method: String,
    args: Vec<String>,
) -> Result<()> {
    // Without this the client always dialled the default path, so a broker
    // started with `--socket` was unreachable even though it was running.
    let socket_path = socket.unwrap_or_else(|| DBusBrokerConfig::default().socket_path);
    let _ = &bus;

    // The broker decodes the body as a single JSON value, so `args` are joined
    // into one and passed through as-is when they already parse as JSON.
    let raw = args.join(" ");
    let body: serde_json::Value = if raw.is_empty() {
        serde_json::Value::String(String::new())
    } else {
        serde_json::from_str(&raw)
            .unwrap_or(serde_json::Value::String(raw.trim_matches('"').to_string()))
    };

    let message = serde_json::json!({
        "header": {
            // MessageType derives `rename_all = "lowercase"`, so MethodCall
            // serialises to "methodcall" — "method_call" fails to parse at the
            // broker, which then closes the connection with no reply.
            "message_type": "methodcall",
            "flags": 0,
            "version": 1,
            "serial": 1,
            "path": path,
            "interface": interface,
            "member": method,
            "destination": serde_json::Value::Null,
            "sender": ":1.client",
            "signature": serde_json::Value::Null,
            "unix_fds": 0,
        },
        "body": serde_json::to_vec(&body)?,
    });

    let replies = dbus_roundtrip(&socket_path, &message).await?;
    let reply = replies
        .last()
        .ok_or_else(|| anyhow::anyhow!("Broker closed without answering"))?;

    let msg_type = reply["header"]["message_type"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    // The body is JSON bytes inside JSON; unwrap it so the caller sees the
    // payload rather than an array of numbers.
    let decoded = reply["body"]
        .as_array()
        .and_then(|bytes| {
            let bytes: Vec<u8> = bytes
                .iter()
                .filter_map(|b| b.as_u64().map(|n| n as u8))
                .collect();
            serde_json::from_slice::<serde_json::Value>(&bytes).ok()
        })
        .unwrap_or_else(|| reply["body"].clone());

    match msg_type.as_str() {
        "error" => {
            let (name, text) = match &decoded {
                serde_json::Value::Array(items) => (
                    items.first().and_then(|v| v.as_str()).unwrap_or("Error"),
                    items.get(1).and_then(|v| v.as_str()).unwrap_or(""),
                ),
                _ => ("Error", ""),
            };
            anyhow::bail!("{}: {}", name, text);
        }
        _ => {
            println!("{}", serde_json::to_string_pretty(&decoded)?);
        }
    }
    Ok(())
}

pub async fn cmd_dbus_monitor(bus: String) -> Result<()> {
    let _ = bus;
    anyhow::bail!(
        "'dbus monitor' is not implemented: PDN's broker logs signals but never \
         forwards them to a watcher, so there is nothing to stream yet. Watching \
         would need a BecomeMonitor verb in the broker protocol; it does not exist. \
         Use 'podnext log tail' for service output instead."
    )
}
