use crate::client;
use anyhow::{bail, Context, Result};
use clap::Subcommand;
use shmark_core::{paths, AppState};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::info;

#[derive(Subcommand)]
pub enum DaemonCmd {
    /// Spawn the daemon as a detached child process and return.
    Start {
        /// Initial display name to seed the identity if it doesn't exist yet.
        #[arg(long)]
        display_name: Option<String>,
    },

    /// Ask the running daemon to shut down.
    Stop,

    /// Print daemon status.
    Status,

    /// Run the daemon in the foreground (blocks).
    Foreground {
        /// Initial display name to seed the identity if it doesn't exist yet.
        #[arg(long)]
        display_name: Option<String>,
    },
}

pub async fn run(cmd: DaemonCmd) -> Result<()> {
    match cmd {
        DaemonCmd::Start { display_name } => start(display_name).await,
        DaemonCmd::Stop => stop().await,
        DaemonCmd::Status => status().await,
        DaemonCmd::Foreground { display_name } => foreground(display_name).await,
    }
}

async fn foreground(display_name: Option<String>) -> Result<()> {
    init_logging();
    let name = display_name.unwrap_or_else(default_display_name);
    info!(default_display_name = %name, "booting shmark daemon");

    let state = AppState::boot(&name).await.context("boot AppState")?;
    info!(
        identity_pubkey = %state.identity.pubkey_hex(),
        display_name = %state.identity.display_name,
        node_pubkey = %state.device.node_pubkey_hex(),
        endpoint_id = %state.endpoint.id(),
        "daemon ready"
    );

    let socket = paths::socket_path()?;

    let serve_state = state.clone();
    let serve_socket = socket.clone();
    let mut serve_task =
        tokio::spawn(async move { shmark_api::serve(serve_state, &serve_socket).await });

    tokio::select! {
        res = &mut serve_task => match res {
            Ok(inner) => inner?,
            Err(join_err) => return Err(anyhow::anyhow!("daemon task panicked: {join_err}")),
        },
        _ = tokio::signal::ctrl_c() => {
            info!("ctrl-c received, shutting down");
            state.signal_shutdown();
            match serve_task.await {
                Ok(inner) => inner?,
                Err(join_err) => return Err(anyhow::anyhow!("daemon task panicked: {join_err}")),
            }
        }
    }

    state.endpoint.close().await;
    info!("daemon stopped cleanly");
    Ok(())
}

async fn start(display_name: Option<String>) -> Result<()> {
    let socket = paths::socket_path()?;
    paths::ensure_data_dir()?;

    if socket.exists() {
        if client::call(&socket, "daemon_status").await.is_ok() {
            println!("daemon already running");
            return Ok(());
        }
        let _ = std::fs::remove_file(&socket);
    }

    spawn_detached(display_name)?;

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if socket.exists() {
            if let Ok(value) = client::call(&socket, "daemon_status").await {
                println!("daemon started: {}", serde_json::to_string(&value)?);
                return Ok(());
            }
        }
        sleep(Duration::from_millis(100)).await;
    }

    bail!(
        "spawned daemon but it didn't start accepting connections within 5s — check logs at {}",
        paths::log_path()?.display()
    );
}

async fn stop() -> Result<()> {
    let socket = paths::socket_path()?;
    if !socket.exists() {
        println!("daemon not running");
        return Ok(());
    }
    let value = client::call(&socket, "daemon_stop").await?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

async fn status() -> Result<()> {
    let socket = paths::socket_path()?;
    if !socket.exists() {
        println!("not running");
        return Ok(());
    }
    match client::call(&socket, "daemon_status").await {
        Ok(value) => {
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        Err(e) => {
            println!("not running ({e})");
            Ok(())
        }
    }
}

fn spawn_detached(display_name: Option<String>) -> Result<()> {
    use std::process::{Command, Stdio};
    let exe = std::env::current_exe().context("locate current exe")?;
    let log_path = paths::log_path()?;
    paths::ensure_data_dir()?;
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("open log {}", log_path.display()))?;
    let log_clone = log_file.try_clone()?;

    let mut cmd = Command::new(exe);
    cmd.arg("daemon").arg("foreground");
    if let Some(name) = display_name {
        cmd.arg("--display-name").arg(name);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_clone));

    #[cfg(unix)]
    detach_child(&mut cmd);

    let _child = cmd.spawn().context("spawn daemon process")?;
    Ok(())
}

#[cfg(unix)]
fn detach_child(cmd: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    // setsid() on the child — new session, no controlling terminal — so closing
    // the parent shell doesn't deliver SIGHUP.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

fn default_display_name() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "shmark user".to_string())
}

fn init_logging() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,iroh=warn"));
    fmt().with_env_filter(filter).with_target(false).init();
}
