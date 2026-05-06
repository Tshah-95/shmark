use crate::protocol::{Request, Response};
use anyhow::{Context, Result};
use serde_json::json;
use shmark_core::AppState;
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::{info, warn};

pub async fn serve(state: AppState, socket_path: &Path) -> Result<()> {
    if socket_path.exists() {
        std::fs::remove_file(socket_path)
            .with_context(|| format!("remove stale socket {}", socket_path.display()))?;
    }
    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("bind unix socket at {}", socket_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(socket_path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(socket_path, perms)?;
    }

    info!(socket = %socket_path.display(), "shmark daemon listening");

    let shutdown = state.shutdown.clone();
    loop {
        tokio::select! {
            biased;
            _ = shutdown.notified() => {
                info!("shutdown requested, draining");
                break;
            }
            accept = listener.accept() => {
                let (stream, _addr) = accept.context("accept unix connection")?;
                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_conn(stream, state).await {
                        warn!(error = ?e, "connection handler errored");
                    }
                });
            }
        }
    }

    // Best-effort cleanup so a fresh daemon can claim the path next start.
    let _ = std::fs::remove_file(socket_path);
    Ok(())
}

async fn handle_conn(stream: UnixStream, state: AppState) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Request>(trimmed) {
            Ok(req) => dispatch(req, &state).await,
            Err(e) => Response::err("parse_error", e.to_string()),
        };
        let mut buf = serde_json::to_string(&resp)?;
        buf.push('\n');
        write_half.write_all(buf.as_bytes()).await?;
    }
    Ok(())
}

async fn dispatch(req: Request, state: &AppState) -> Response {
    match req.method.as_str() {
        "identity_show" => Response::ok(json!({
            "identity_pubkey": state.identity.pubkey_hex(),
            "display_name": state.identity.display_name,
            "created_at": state.identity.created_at,
            "device": {
                "node_pubkey": state.device.node_pubkey_hex(),
                "endpoint_id": state.endpoint.id().to_string(),
                "cert_created_at": state.device.cert.cert.created_at,
            },
        })),
        "daemon_status" => Response::ok(json!({
            "status": "running",
            "pid": std::process::id(),
            "started_at": state.started_at,
            "uptime_secs": shmark_core::now_secs().saturating_sub(state.started_at),
        })),
        "daemon_stop" => {
            state.signal_shutdown();
            Response::ok(json!({ "stopping": true }))
        }
        other => Response::err("unknown_method", format!("unknown method: {other}")),
    }
}
