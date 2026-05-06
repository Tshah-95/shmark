//! Local control plane for the shmark daemon.
//!
//! Two transports share one dispatch function:
//!
//! - Unix domain socket, line-delimited JSON. One request per line, one
//!   response per line. Used by the CLI and any non-Tauri client.
//! - Direct in-process call from the Tauri app, which embeds shmark-core
//!   and serves its UI commands by routing them through `dispatch`.
//!
//! Wire format (over the socket):
//!   Request:  {"method": "<name>", "params": <value>}
//!   Response: {"ok": <value>}  or  {"err": {"code": "...", "message": "..."}}

pub mod dispatch;
pub mod protocol;
mod server;

pub use dispatch::dispatch;
pub use protocol::{Request, Response};
pub use server::serve;
