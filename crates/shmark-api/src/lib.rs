//! Local control plane for the shmark daemon.
//!
//! Wire format: line-delimited JSON over a Unix domain socket. One request
//! per line, one response per line. Multiple requests can share a connection
//! but the CLI uses one-request-per-connection for simplicity.
//!
//! Request:  {"method": "<name>", "params": <value>}
//! Response: {"ok": <value>}  or  {"err": {"code": "...", "message": "..."}}

pub mod protocol;
mod server;

pub use protocol::{Request, Response};
pub use server::serve;
