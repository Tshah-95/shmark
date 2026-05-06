use anyhow::{anyhow, bail, Context, Result};
use shmark_api::{Request, Response};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

pub async fn call(socket_path: &Path, method: &str) -> Result<serde_json::Value> {
    call_with_params(socket_path, method, serde_json::Value::Null).await
}

pub async fn call_with_params(
    socket_path: &Path,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value> {
    let req = Request {
        method: method.to_string(),
        params,
    };

    let stream = UnixStream::connect(socket_path).await.with_context(|| {
        format!(
            "connect to daemon socket at {} (is the daemon running?)",
            socket_path.display()
        )
    })?;
    let (read_half, mut write_half) = stream.into_split();

    let mut buf = serde_json::to_string(&req)?;
    buf.push('\n');
    write_half.write_all(buf.as_bytes()).await?;
    write_half.shutdown().await?;

    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        bail!("daemon closed the connection without responding");
    }
    let resp: Response = serde_json::from_str(line.trim())
        .with_context(|| format!("parse daemon response: {}", line.trim()))?;
    match resp {
        Response::Ok { ok } => Ok(ok),
        Response::Err { err } => Err(anyhow!("{}: {}", err.code, err.message)),
    }
}
