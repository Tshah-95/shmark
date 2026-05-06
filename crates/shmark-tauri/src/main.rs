// Quiet the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::{anyhow, Result};
use serde_json::Value;
use shmark_core::{paths, AppState};
use std::sync::Arc;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, WindowEvent,
};
use tracing::{info, warn};

const TRAY_ICON_PNG: &[u8] = include_bytes!("../icons/tray-icon.png");

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

            // Tray icon: persistent menu-bar entry. The main window
            // hide-on-close behavior is wired on the WindowEvent below; this
            // is what keeps the daemon reachable after the user closes the
            // window without quitting the process.
            build_tray(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![rpc])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Hide instead of close — the daemon stays alive in the
                // tray. "Quit" from the tray menu is the way out.
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("run tauri app");
}

fn build_tray<R: tauri::Runtime>(app: &tauri::App<R>) -> Result<(), Box<dyn std::error::Error>> {
    let open_item = MenuItem::with_id(app, "tray-open", "Open shmark", true, None::<&str>)?;
    let share_item = MenuItem::with_id(
        app,
        "tray-share",
        "Share from clipboard",
        true,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "tray-quit", "Quit shmark", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_item, &share_item, &separator, &quit_item])?;

    let icon = Image::from_bytes(TRAY_ICON_PNG)?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "tray-open" => focus_main(app),
            "tray-share" => {
                focus_main(app);
                if let Err(e) = app.emit("shmark://hotkey/share", ()) {
                    warn!(error = ?e, "failed to emit share event from tray");
                }
            }
            "tray-quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

fn focus_main<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
