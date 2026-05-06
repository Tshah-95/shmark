// Quiet the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::{anyhow, Result};
use serde_json::Value;
use shmark_core::{paths, AppState};
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tracing::{info, warn};

struct ShmarkAppState {
    inner: Arc<AppState>,
}

#[tauri::command]
async fn rpc(
    state: tauri::State<'_, ShmarkAppState>,
    method: String,
    params: Option<Value>,
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

const SHARE_HOTKEY: &str = "CmdOrCtrl+Shift+P";

fn main() {
    init_logging();

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init());

    #[cfg(desktop)]
    {
        use tauri_plugin_global_shortcut::{Code, Modifiers, ShortcutState};

        builder = builder.plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts([SHARE_HOTKEY])
                .expect("register shmark hotkey")
                .with_handler(|app, shortcut, event| {
                    // Only react on key-down. The handler also fires on
                    // release; firing the modal twice would be jarring.
                    if event.state != ShortcutState::Pressed {
                        return;
                    }
                    // Be defensive — match by keycode + modifier so any future
                    // hotkey rebind funnels through the same predicate.
                    if !shortcut.matches(Modifiers::SUPER | Modifiers::SHIFT, Code::KeyP)
                        && !shortcut.matches(Modifiers::CONTROL | Modifiers::SHIFT, Code::KeyP)
                    {
                        return;
                    }
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                    }
                    if let Err(e) = app.emit("shmark://hotkey/share", ()) {
                        warn!(error = ?e, "failed to emit hotkey event");
                    }
                })
                .build(),
        );
    }

    builder
        .setup(|app| {
            let handle = app.handle().clone();

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
