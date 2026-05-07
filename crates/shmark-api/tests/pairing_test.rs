//! End-to-end pairing test against two real AppStates in separate tempdirs.
//!
//! Both endpoints bind to iroh's default preset (which uses n0's public
//! relay network), so this test requires internet access to find each
//! other through the relay. Locally that's the same path users hit; in
//! CI it can be flaky if the relays are slow or blocked.

use serde_json::{json, Value};
use serial_test::serial;
use shmark_api::dispatch;
use shmark_core::{paths, AppState};
use std::path::Path;
use tempfile::TempDir;

fn init_logging() {
    use tracing_subscriber::{fmt, EnvFilter};
    let _ = fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("warn,shmark_core=debug,shmark_api=debug")),
        )
        .with_test_writer()
        .try_init();
}

async fn boot_in(dir: &Path, name: &str) -> AppState {
    init_logging();
    std::env::set_var("SHMARK_DATA_DIR", dir);
    AppState::boot(name).await.expect("boot")
}

async fn rpc(state: &AppState, method: &str, params: Value) -> Value {
    dispatch(method, params, state)
        .await
        .unwrap_or_else(|e| panic!("{method} failed: {e:#}"))
}

#[tokio::test]
#[serial]
async fn pair_two_devices_replicates_identity_and_groups() {
    let dir_a: TempDir = tempfile::tempdir().unwrap();
    let dir_b: TempDir = tempfile::tempdir().unwrap();

    // Boot A and create a group.
    let state_a = boot_in(dir_a.path(), "device-a").await;
    rpc(&state_a, "groups_new", json!({ "alias": "alpha" })).await;
    let identity_a_pk = state_a.identity.pubkey_hex();

    // Boot B in a separate tempdir.
    let state_b = boot_in(dir_b.path(), "device-b").await;
    let identity_b_before = state_b.identity.pubkey_hex();
    assert_ne!(
        identity_a_pk, identity_b_before,
        "fresh AppStates must have different identities"
    );

    // Mint code on A.
    std::env::set_var("SHMARK_DATA_DIR", dir_a.path());
    let code_resp = rpc(&state_a, "devices_pair_create", Value::Null).await;
    let code = code_resp["code"]
        .as_str()
        .expect("code in response")
        .to_string();
    assert!(code.starts_with("shpair-"), "code prefix");

    // Wait briefly so A's group is reachable for ticket generation.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Pair-join on B.
    std::env::set_var("SHMARK_DATA_DIR", dir_b.path());
    let join_resp = rpc(
        &state_b,
        "devices_pair_join",
        json!({ "code": code, "display_name": "device-b" }),
    )
    .await;

    assert_eq!(
        join_resp["identity_pubkey"].as_str().unwrap_or(""),
        identity_a_pk,
        "B should now report A's identity_pubkey"
    );
    assert_eq!(
        join_resp["reload_requested"].as_bool().unwrap_or(false),
        true
    );
    let imported = join_resp["imported_groups"].as_array().unwrap();
    assert_eq!(imported.len(), 1, "exactly one group imported");
    assert_eq!(imported[0].as_str().unwrap_or(""), "alpha");

    // The persisted identity on disk for B should match A's identity now.
    std::env::set_var("SHMARK_DATA_DIR", dir_b.path());
    let identity_b_after = shmark_core::Identity::load(&paths::identity_path().unwrap())
        .expect("load identity B after pair");
    assert_eq!(
        identity_b_after.pubkey_hex(),
        identity_a_pk,
        "persisted B identity must equal A"
    );

    // Groups on disk for B include "alpha".
    let groups_b =
        shmark_core::Groups::load(&paths::groups_state_path().unwrap()).expect("load groups B");
    let aliases: Vec<String> = groups_b
        .list()
        .into_iter()
        .map(|g| g.local_alias)
        .collect();
    assert!(
        aliases.contains(&"alpha".to_string()),
        "expected 'alpha' in {aliases:?}"
    );

    state_a.node.shutdown().await.ok();
    state_b.node.shutdown().await.ok();
}
