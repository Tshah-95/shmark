pub mod contacts;
pub mod dev;
pub mod device;
pub mod groups;
pub mod identity;
pub mod node;
pub mod pairing;
pub mod paths;
pub mod resolve;
pub mod settings;
pub mod shares;
pub mod state;

pub use settings::Settings;

pub use device::{Device, DeviceCert, SignedDeviceCert};
pub use groups::{make_local_group, Groups, LocalGroup};
pub use identity::Identity;
pub use node::Node;
pub use shares::{ShareItem, ShareRecord, Shares};
pub use state::AppState;

pub fn now_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
