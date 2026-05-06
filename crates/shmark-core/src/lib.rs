pub mod device;
pub mod identity;
pub mod node;
pub mod paths;
pub mod state;

pub use device::{Device, DeviceCert, SignedDeviceCert};
pub use identity::Identity;
pub use state::AppState;

pub const SHMARK_ALPN: &[u8] = b"shmark/0";

pub fn now_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
