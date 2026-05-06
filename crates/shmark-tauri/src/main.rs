// Quiet the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::{anyhow, Result};
use serde_json::Value;
use shmark_core::{paths, AppState};
use std::sync::Arc;
use tauri::Manager;
use tracing::info;

struct ShmarkAppState {
    inner: Arc<AppState>,
}

#[tauri::command]
async fn rpc(
    state: tauri::State<'_, ShmarkAppState>,
    method: String,
    #[allow(non_snake_case)] params: Option<Value>,
) -> Result<Value, String> {
    shmark_api::dispatch(&method, params.unwrap_or(Value::Null), &state.inner)
        .await
        .map_err(|e| {
            let mut msg = format!("{e}");
            for cause in e.chain().skip(1) {
                msg.push_str(&format!(": {cause}"));
            }
            msg
        })
}

fn init_logging() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,iroh=warn"));
    let _ = fmt().with_env_filter(filter).with_target(false).try_init();
}

fn main() {
    init_logging();

    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();

            // Boot synchronously so commands can rely on the state being
            // managed before any UI calls land.
            tauri::async_runtime::block_on(async move {
                paths::ensure_data_dir()?;

                let socket = paths::socket_path()?;
                if socket.exists() {
                    if tokio::net::UnixStream::connect(&socket).await.is_ok() {
                        return Err(anyhow!(
                            "shmark daemon already running on {} — stop it with `shmark daemon stop` before launching the app",
                            socket.display()
                        ));
                    }
                    let _ = std::fs::remove_file(&socket);
                }

                let app_state = AppState::boot("shmark").await?;
                info!(
                    identity = %app_state.identity.pubkey_hex(),
                    endpoint = %app_state.node.endpoint.id(),
                    "shmark-desktop ready"
                );
                let arc = Arc::new(app_state);

                // Run the unix-socket control plane in the background so the
                // CLI and any agents can hit the same daemon while the GUI is
                // running.
                let serve_arc = arc.clone();
                let socket_for_serve = socket.clone();
                tauri::async_runtime::spawn(async move {
                    let serve_state = (*serve_arc).clone();
                    if let Err(e) = shmark_api::serve(serve_state, &socket_for_serve).await {
                        tracing::error!(error = ?e, "socket server exited with error");
                    }
                });

                handle.manage(ShmarkAppState { inner: arc });
                Ok::<_, anyhow::Error>(())
            })
            .map_err(|e| -> Box<dyn std::error::Error> { format!("{e:#}").into() })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![rpc])
        .run(tauri::generate_context!())
        .expect("run tauri app");
}
