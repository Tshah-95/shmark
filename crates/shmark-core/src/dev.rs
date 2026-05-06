//! Test-driver bridge between shmark-api dispatch and shmark-tauri's webview.
//!
//! shmark-api can't depend on Tauri — it has to work in the standalone
//! daemon path too. But test scripts hitting the unix socket want to
//! `dev_emit` events the frontend listens for and `dev_run` arbitrary JS
//! against the webview.
//!
//! Solution: a tokio mpsc channel of `DevRequest`s. shmark-api's dispatch
//! sends requests; the Tauri side spawns a consumer task that fulfils them
//! using AppHandle / Webview APIs. The standalone daemon never installs a
//! consumer, so dev_* methods return an error there.

use anyhow::Result;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

pub type DevSender = mpsc::UnboundedSender<DevRequest>;
pub type DevReceiver = mpsc::UnboundedReceiver<DevRequest>;

#[derive(Debug)]
pub enum DevRequest {
    /// Re-emit a Tauri event so the frontend's `listen` handler fires
    /// without a real OS-level trigger (hotkey, tray, etc).
    Emit {
        event: String,
        payload: Value,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Snapshot of the main window's state.
    WindowState {
        reply: oneshot::Sender<Result<Value>>,
    },
    /// Fire-and-forget JS evaluation in the main webview.
    RunJs {
        js: String,
        reply: oneshot::Sender<Result<()>>,
    },
    /// JS evaluation that returns a JSON-encoded string via Tauri's
    /// eval_with_callback.
    RunJsGet {
        js: String,
        reply: oneshot::Sender<Result<String>>,
    },
}

pub fn channel() -> (DevSender, DevReceiver) {
    mpsc::unbounded_channel()
}
