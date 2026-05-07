// Quiet the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod dev_consumer;

use anyhow::{anyhow, Result};
use serde_json::Value;
use shmark_core::{dev, paths, AppState};
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
    /// Wrapped so the watcher task can swap a freshly-bootstrapped AppState
    /// into place without restarting the process — used after pairing.
    inner: tokio::sync::RwLock<Arc<AppState>>,
}

#[tauri::command]
async fn rpc(
    state: tauri::State<'_, ShmarkAppState>,
    method: String,
    params: Option<Value>,
) -> Result<Value, String> {
    let app_state = state.inner.read().await.clone();
    shmark_api::dispatch(&method, params.unwrap_or(Value::Null), &app_state)
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

    // CLI flag parsing — skipping clap to keep startup tight. Two flags
    // matter: --headless (hide window after boot, don't build tray) and
    // --display-name=... (seed identity name on first run, ignored after).
    let raw_args: Vec<String> = std::env::args().collect();
    let headless = raw_args.iter().any(|a| a == "--headless");
    info!(headless, "shmark-desktop launching");

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init());

    #[cfg(desktop)]
    {
        use tauri_plugin_global_shortcut::ShortcutState;

        // Register the plugin with a generic handler. The actual accelerator
        // is registered at runtime once we've loaded the user's settings —
        // any fired shortcut is the configured share hotkey.
        builder = builder.plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state != ShortcutState::Pressed {
                        return;
                    }
                    focus_main(&app.app_handle().clone());
                    if let Err(e) = app.emit("shmark://hotkey/share", ()) {
                        warn!(error = ?e, "failed to emit hotkey event");
                    }
                })
                .build(),
        );
    }

    builder
        .setup(move |app| {
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

                // Wire up the dev-bridge channel before booting AppState so
                // the test driver path is live for every operation. The
                // sender is cloned per AppState boot so reloads keep the
                // dev consumer alive.
                let (dev_tx, dev_rx) = dev::channel();
                let dev_tx_for_reload = dev_tx.clone();
                let app_state =
                    AppState::boot_with_dev("shmark", Some(dev_tx)).await?;
                info!(
                    identity = %app_state.identity.pubkey_hex(),
                    endpoint = %app_state.node.endpoint.id(),
                    "shmark-desktop ready"
                );
                let arc = Arc::new(app_state);

                dev_consumer::spawn(handle.clone(), dev_rx);

                let serve_arc = arc.clone();
                let socket_for_serve = socket.clone();
                tauri::async_runtime::spawn(async move {
                    let serve_state = (*serve_arc).clone();
                    if let Err(e) = shmark_api::serve(serve_state, &socket_for_serve).await {
                        tracing::error!(error = ?e, "socket server exited with error");
                    }
                });

                handle.manage(ShmarkAppState {
                    inner: tokio::sync::RwLock::new(arc.clone()),
                });

                // Headless mode: hide the main window so test runs don't
                // appear on the user's screen. Window still has a webview,
                // JS still executes, dev_* RPCs still drive it.
                if headless {
                    if let Some(window) = handle.get_webview_window("main") {
                        let _ = window.hide();
                    }
                }

                // Register the configured hotkey. We re-register
                // dynamically when the user changes it via settings_set.
                #[cfg(desktop)]
                {
                    let initial = arc.settings.read().await.hotkey.clone();
                    register_hotkey(&handle, &initial);
                    spawn_hotkey_watcher(handle.clone(), arc.clone(), initial);
                }

                // Watch for reload requests (signalled by pair_join). When
                // fired, drop and re-bootstrap AppState in place so the
                // new identity from disk takes effect without a process
                // restart.
                spawn_reload_watcher(handle.clone(), arc.clone(), dev_tx_for_reload);
                Ok::<_, anyhow::Error>(())
            })
            .map_err(|e| -> Box<dyn std::error::Error> { format!("{e:#}").into() })?;

            // Tray icon: persistent menu-bar entry. Skip in headless mode
            // — there's no UI a tray icon would lead the user to. The
            // daemon stays alive via the still-running socket server.
            if !headless {
                build_tray(app)?;
            }
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

fn spawn_reload_watcher<R: tauri::Runtime>(
    handle: tauri::AppHandle<R>,
    initial_state: Arc<AppState>,
    dev_tx: shmark_core::dev::DevSender,
) {
    tauri::async_runtime::spawn(async move {
        let mut current = initial_state;
        loop {
            current.reload_requested.notified().await;
            info!("reload requested — re-bootstrapping AppState");
            let new_state = match AppState::boot_with_dev("shmark", Some(dev_tx.clone())).await {
                Ok(s) => Arc::new(s),
                Err(e) => {
                    warn!(error = ?e, "AppState reload failed; keeping old state");
                    continue;
                }
            };

            // Swap in the new state.
            if let Some(managed) = handle.try_state::<ShmarkAppState>() {
                *managed.inner.write().await = new_state.clone();
            }

            // Tell the frontend so it can refresh + show a toast.
            if let Err(e) = handle.emit(
                "shmark://reloaded",
                serde_json::json!({
                    "identity_pubkey": new_state.identity.pubkey_hex(),
                    "display_name": new_state.identity.display_name,
                }),
            ) {
                warn!(error = ?e, "emit shmark://reloaded failed");
            }

            // Shut down the OLD AppState's iroh node so resources are
            // released cleanly before drop. We do this AFTER the swap so
            // any in-flight RPC has migrated.
            let old = std::mem::replace(&mut current, new_state);
            let _ = old.node.shutdown().await;

            info!(
                identity = %current.identity.pubkey_hex(),
                "reload complete"
            );
        }
    });
}

#[cfg(desktop)]
fn register_hotkey<R: tauri::Runtime>(app: &tauri::AppHandle<R>, accel: &str) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    if accel.is_empty() {
        return;
    }
    if let Err(e) = app.global_shortcut().register(accel) {
        warn!(accel = %accel, error = ?e, "register hotkey failed");
    } else {
        info!(accel = %accel, "registered hotkey");
    }
}

#[cfg(desktop)]
fn unregister_hotkey<R: tauri::Runtime>(app: &tauri::AppHandle<R>, accel: &str) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    if accel.is_empty() {
        return;
    }
    if let Err(e) = app.global_shortcut().unregister(accel) {
        warn!(accel = %accel, error = ?e, "unregister hotkey failed");
    }
}

#[cfg(desktop)]
fn spawn_hotkey_watcher<R: tauri::Runtime>(
    handle: tauri::AppHandle<R>,
    state: Arc<AppState>,
    initial: String,
) {
    tauri::async_runtime::spawn(async move {
        let mut last = initial;
        loop {
            state.settings_changed.notified().await;
            let new_hotkey = state.settings.read().await.hotkey.clone();
            if new_hotkey == last {
                continue;
            }
            unregister_hotkey(&handle, &last);
            register_hotkey(&handle, &new_hotkey);
            last = new_hotkey;
        }
    });
}
