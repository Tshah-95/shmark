//! End-to-end dispatch tests against a real `AppState` rooted in a tempdir.
//!
//! Each test sets SHMARK_DATA_DIR to a fresh tempdir before booting so it
//! doesn't read or write the real `~/Library/Application Support/shmark/`.
//! `serial_test` keeps the env var changes single-threaded across the file.

use serde_json::{json, Value};
use serial_test::serial;
use shmark_api::dispatch;
use shmark_core::AppState;
use tempfile::TempDir;

async fn boot_in_tempdir() -> (AppState, TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    std::env::set_var("SHMARK_DATA_DIR", dir.path());
    let state = AppState::boot("test").await.expect("boot AppState");
    (state, dir)
}

async fn rpc(state: &AppState, method: &str, params: Value) -> Value {
    dispatch(method, params, state)
        .await
        .unwrap_or_else(|e| panic!("dispatch {method} failed: {e:#}"))
}

#[tokio::test]
#[serial]
async fn identity_show_returns_pubkey_and_device() {
    let (state, _dir) = boot_in_tempdir().await;
    let v = rpc(&state, "identity_show", Value::Null).await;
    assert_eq!(
        v["display_name"].as_str().unwrap_or(""),
        "test",
        "default display name should match what we passed to boot"
    );
    let identity_pk = v["identity_pubkey"].as_str().expect("identity_pubkey");
    assert_eq!(identity_pk.len(), 64, "ed25519 pubkey hex is 32 bytes");
    let node_pk = v["device"]["node_pubkey"]
        .as_str()
        .expect("node pubkey");
    assert_eq!(node_pk.len(), 64);
    state.node.shutdown().await.ok();
}

#[tokio::test]
#[serial]
async fn daemon_status_reports_running() {
    let (state, _dir) = boot_in_tempdir().await;
    let v = rpc(&state, "daemon_status", Value::Null).await;
    assert_eq!(v["status"].as_str().unwrap_or(""), "running");
    state.node.shutdown().await.ok();
}

#[tokio::test]
#[serial]
async fn groups_create_list_and_share_flow() {
    let (state, _dir) = boot_in_tempdir().await;

    // Create
    let g = rpc(&state, "groups_new", json!({ "alias": "engineering" })).await;
    assert_eq!(g["local_alias"].as_str().unwrap_or(""), "engineering");
    assert_eq!(g["created_locally"].as_bool().unwrap_or(false), true);
    let ns = g["namespace_id"].as_str().expect("namespace_id").to_string();
    assert_eq!(ns.len(), 64);

    // List sees it
    let list = rpc(&state, "groups_list", Value::Null).await;
    let arr = list.as_array().expect("groups list array");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["local_alias"].as_str().unwrap_or(""), "engineering");

    // Share code (write mode)
    let code = rpc(
        &state,
        "groups_share_code",
        json!({ "name_or_id": "engineering" }),
    )
    .await;
    assert_eq!(code["mode"].as_str().unwrap_or(""), "write");
    let code_str = code["code"].as_str().expect("ticket code");
    assert!(code_str.starts_with("doc"), "ticket should serialize to a 'doc...' base32 string");

    // Read-only share code path
    let ro = rpc(
        &state,
        "groups_share_code",
        json!({ "name_or_id": "engineering", "read_only": true }),
    )
    .await;
    assert_eq!(ro["mode"].as_str().unwrap_or(""), "read");

    // Rename
    let renamed = rpc(
        &state,
        "groups_rename",
        json!({ "name_or_id": "engineering", "new_alias": "eng" }),
    )
    .await;
    assert_eq!(renamed["local_alias"].as_str().unwrap_or(""), "eng");

    // Remove
    let removed = rpc(&state, "groups_remove", json!({ "name_or_id": "eng" })).await;
    assert_eq!(removed["local_alias"].as_str().unwrap_or(""), "eng");
    let list2 = rpc(&state, "groups_list", Value::Null).await;
    assert_eq!(list2.as_array().unwrap().len(), 0);

    state.node.shutdown().await.ok();
}

#[tokio::test]
#[serial]
async fn share_create_file_and_read_back_bytes() {
    let (state, _dir) = boot_in_tempdir().await;

    // Group
    rpc(&state, "groups_new", json!({ "alias": "t" })).await;

    // Write a markdown file to a tempdir
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("hello.md");
    std::fs::write(&path, b"# hi\n\nworld").unwrap();

    // Share
    let rec = rpc(
        &state,
        "share_create",
        json!({ "group": "t", "path": path.display().to_string(), "description": "test" }),
    )
    .await;
    let share_id = rec["share_id"].as_str().expect("share_id").to_string();
    assert_eq!(rec["name"].as_str().unwrap_or(""), "hello.md");
    let items = rec["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert!(items[0]["path"].is_null(), "single-file share has null path");
    assert_eq!(items[0]["size_bytes"].as_u64().unwrap_or(0), 11);

    // shares_list sees it
    let list = rpc(&state, "shares_list", json!({ "group": "t" })).await;
    let arr = list.as_array().expect("shares list array");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["share"]["share_id"].as_str().unwrap_or(""), share_id);

    // share_get_bytes returns the actual content
    let bytes_resp = rpc(
        &state,
        "share_get_bytes",
        json!({ "group": "t", "share_id": share_id, "item": 0 }),
    )
    .await;
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(bytes_resp["bytes_b64"].as_str().unwrap())
        .unwrap();
    assert_eq!(decoded, b"# hi\n\nworld");

    state.node.shutdown().await.ok();
}

#[tokio::test]
#[serial]
async fn share_create_folder_packs_multiple_items() {
    let (state, _dir) = boot_in_tempdir().await;
    rpc(&state, "groups_new", json!({ "alias": "t" })).await;

    let scratch = tempfile::tempdir().unwrap();
    std::fs::write(scratch.path().join("a.md"), b"a").unwrap();
    std::fs::write(scratch.path().join("b.md"), b"bb").unwrap();
    std::fs::create_dir_all(scratch.path().join("sub")).unwrap();
    std::fs::write(scratch.path().join("sub/c.md"), b"ccc").unwrap();

    let rec = rpc(
        &state,
        "share_create",
        json!({ "group": "t", "path": scratch.path().display().to_string() }),
    )
    .await;
    let items = rec["items"].as_array().expect("items");
    assert_eq!(items.len(), 3, "three files in folder");
    let paths: Vec<&str> = items
        .iter()
        .map(|i| i["path"].as_str().unwrap_or(""))
        .collect();
    assert!(paths.contains(&"a.md"));
    assert!(paths.contains(&"b.md"));
    assert!(paths.contains(&"sub/c.md"));

    state.node.shutdown().await.ok();
}

#[tokio::test]
#[serial]
async fn paths_resolve_classifies_inputs() {
    let (state, _dir) = boot_in_tempdir().await;

    let empty = rpc(&state, "paths_resolve", json!({ "raw": "" })).await;
    assert_eq!(empty["kind"].as_str().unwrap_or(""), "empty");

    let url = rpc(&state, "paths_resolve", json!({ "raw": "https://example.com/x.md" })).await;
    assert_eq!(url["kind"].as_str().unwrap_or(""), "url");
    assert_eq!(url["url"].as_str().unwrap_or(""), "https://example.com/x.md");

    let unsupported = rpc(&state, "paths_resolve", json!({ "raw": "hello world" })).await;
    assert_eq!(unsupported["kind"].as_str().unwrap_or(""), "unsupported");

    let scratch = tempfile::tempdir().unwrap();
    let target = scratch.path().join("file.md");
    std::fs::write(&target, b"x").unwrap();
    let abs = rpc(&state, "paths_resolve", json!({ "raw": target.display().to_string() })).await;
    assert_eq!(abs["kind"].as_str().unwrap_or(""), "path");

    state.node.shutdown().await.ok();
}

#[tokio::test]
#[serial]
async fn settings_get_set_persists_across_lookups() {
    let (state, _dir) = boot_in_tempdir().await;

    // Defaults
    let g = rpc(&state, "settings_get", Value::Null).await;
    assert_eq!(
        g["settings"]["hotkey"].as_str().unwrap_or(""),
        "CmdOrCtrl+Shift+P"
    );
    assert_eq!(g["settings"]["auto_pin"].as_bool().unwrap_or(false), true);

    // Mutate
    let s = rpc(
        &state,
        "settings_set",
        json!({ "hotkey": "CmdOrCtrl+Shift+M", "search_roots": ["/tmp/foo"], "auto_pin": false }),
    )
    .await;
    assert_eq!(s["hotkey"].as_str().unwrap_or(""), "CmdOrCtrl+Shift+M");
    assert_eq!(s["search_roots"][0].as_str().unwrap_or(""), "/tmp/foo");
    assert_eq!(s["auto_pin"].as_bool().unwrap_or(true), false);

    // Re-read
    let g2 = rpc(&state, "settings_get", Value::Null).await;
    assert_eq!(
        g2["settings"]["hotkey"].as_str().unwrap_or(""),
        "CmdOrCtrl+Shift+M"
    );
    assert_eq!(
        g2["effective_search_roots"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>(),
        vec!["/tmp/foo"]
    );

    state.node.shutdown().await.ok();
}

#[tokio::test]
#[serial]
async fn unknown_method_returns_error() {
    let (state, _dir) = boot_in_tempdir().await;
    let err = dispatch("not_a_real_method", Value::Null, &state)
        .await
        .expect_err("should error");
    assert!(format!("{err}").contains("unknown method"));
    state.node.shutdown().await.ok();
}
