//! Consumes DevRequest messages from shmark-api dispatch and fulfils them
//! using the live AppHandle / WebviewWindow. Spawned once at boot.

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use shmark_core::dev::{DevReceiver, DevRequest};
use tauri::{Emitter, Manager};
use tokio::sync::oneshot;

pub fn spawn<R: tauri::Runtime>(handle: tauri::AppHandle<R>, mut rx: DevReceiver) {
    tauri::async_runtime::spawn(async move {
        while let Some(req) = rx.recv().await {
            handle_request(&handle, req).await;
        }
    });
}

async fn handle_request<R: tauri::Runtime>(handle: &tauri::AppHandle<R>, req: DevRequest) {
    match req {
        DevRequest::Emit { event, payload, reply } => {
            let r = handle.emit(event.as_str(), payload).map_err(|e| anyhow!("{e}"));
            let _ = reply.send(r);
        }
        DevRequest::WindowState { reply } => {
            let r = (|| -> Result<Value> {
                let w = handle
                    .get_webview_window("main")
                    .ok_or_else(|| anyhow!("main window not found"))?;
                Ok(json!({
                    "label": w.label().to_string(),
                    "visible": w.is_visible().unwrap_or(false),
                    "focused": w.is_focused().unwrap_or(false),
                    "minimized": w.is_minimized().unwrap_or(false),
                }))
            })();
            let _ = reply.send(r);
        }
        DevRequest::RunJs { js, reply } => {
            let r = (|| -> Result<()> {
                let w = handle
                    .get_webview_window("main")
                    .ok_or_else(|| anyhow!("main window not found"))?;
                w.eval(js).map_err(|e| anyhow!("{e}"))
            })();
            let _ = reply.send(r);
        }
        DevRequest::RunJsGet { js, reply } => {
            let w = match handle.get_webview_window("main") {
                Some(w) => w,
                None => {
                    let _ = reply.send(Err(anyhow!("main window not found")));
                    return;
                }
            };
            let (cb_tx, cb_rx) = oneshot::channel::<String>();
            let cb_tx = std::sync::Mutex::new(Some(cb_tx));
            let r = w.eval_with_callback(js, move |val| {
                if let Ok(mut guard) = cb_tx.lock() {
                    if let Some(tx) = guard.take() {
                        let _ = tx.send(val);
                    }
                }
            });
            match r {
                Ok(()) => match cb_rx.await {
                    Ok(s) => {
                        let _ = reply.send(Ok(s));
                    }
                    Err(_) => {
                        let _ = reply.send(Err(anyhow!("eval callback dropped")));
                    }
                },
                Err(e) => {
                    let _ = reply.send(Err(anyhow!("{e}")));
                }
            }
        }
    }
}
