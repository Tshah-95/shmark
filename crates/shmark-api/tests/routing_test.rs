//! Step 7 — routing notes / agent surface tests.

use serde_json::{json, Value};
use serial_test::serial;
use shmark_api::dispatch;
use shmark_core::AppState;
use tempfile::TempDir;

async fn boot_in_tempdir() -> (AppState, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("SHMARK_DATA_DIR", dir.path());
    let state = AppState::boot("test").await.unwrap();
    (state, dir)
}

async fn rpc(state: &AppState, method: &str, params: Value) -> Value {
    dispatch(method, params, state)
        .await
        .unwrap_or_else(|e| panic!("{method} failed: {e:#}"))
}

#[tokio::test]
#[serial]
async fn contacts_crud_and_notes() {
    let (state, _dir) = boot_in_tempdir().await;

    // Empty list to start
    let list0 = rpc(&state, "contacts_list", Value::Null).await;
    assert_eq!(list0.as_array().unwrap().len(), 0);

    // Add a contact
    let pubkey = "a".repeat(64);
    let added = rpc(
        &state,
        "contacts_upsert",
        json!({ "identity_pubkey": pubkey, "display_name": "Garrett" }),
    )
    .await;
    assert_eq!(added["display_name"].as_str().unwrap_or(""), "Garrett");

    // Set a note
    let noted = rpc(
        &state,
        "contacts_set_note",
        json!({ "name_or_pubkey": "Garrett", "note": "prefers high-level summaries" }),
    )
    .await;
    assert_eq!(
        noted["note"].as_str().unwrap_or(""),
        "prefers high-level summaries"
    );

    // List sees it
    let list1 = rpc(&state, "contacts_list", Value::Null).await;
    let arr = list1.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["display_name"].as_str().unwrap_or(""), "Garrett");

    // Clear note
    rpc(
        &state,
        "contacts_set_note",
        json!({ "name_or_pubkey": "Garrett", "note": null }),
    )
    .await;
    let list2 = rpc(&state, "contacts_list", Value::Null).await;
    assert!(list2[0]["note"].is_null());

    // Remove
    rpc(
        &state,
        "contacts_remove",
        json!({ "name_or_pubkey": "Garrett" }),
    )
    .await;
    let list3 = rpc(&state, "contacts_list", Value::Null).await;
    assert_eq!(list3.as_array().unwrap().len(), 0);

    state.node.shutdown().await.ok();
}

#[tokio::test]
#[serial]
async fn groups_set_note_persists() {
    let (state, _dir) = boot_in_tempdir().await;
    rpc(&state, "groups_new", json!({ "alias": "design" })).await;

    rpc(
        &state,
        "groups_set_note",
        json!({ "group": "design", "note": "share infra docs here, no customer data" }),
    )
    .await;

    // context_dump should reflect the note
    let dump = rpc(&state, "context_dump", Value::Null).await;
    let md = dump["markdown"].as_str().unwrap_or("");
    assert!(md.contains("### design"));
    assert!(md.contains("share infra docs here, no customer data"));

    state.node.shutdown().await.ok();
}

#[tokio::test]
#[serial]
async fn context_dump_assembles_groups_and_contacts() {
    let (state, _dir) = boot_in_tempdir().await;

    rpc(&state, "groups_new", json!({ "alias": "engineering" })).await;
    rpc(
        &state,
        "groups_set_note",
        json!({ "group": "engineering", "note": "infra + product engineering" }),
    )
    .await;

    let pubkey = "b".repeat(64);
    rpc(
        &state,
        "contacts_upsert",
        json!({ "identity_pubkey": pubkey, "display_name": "Garrett" }),
    )
    .await;
    rpc(
        &state,
        "contacts_set_note",
        json!({ "name_or_pubkey": "Garrett", "note": "east coast, prefers summaries" }),
    )
    .await;

    let dump = rpc(&state, "context_dump", Value::Null).await;
    let md = dump["markdown"].as_str().unwrap_or("");
    assert!(md.starts_with("# shmark context"));
    assert!(md.contains("## Identity"));
    assert!(md.contains("## Groups"));
    assert!(md.contains("### engineering"));
    assert!(md.contains("infra + product engineering"));
    assert!(md.contains("## Contacts"));
    assert!(md.contains("### Garrett"));
    assert!(md.contains("east coast, prefers summaries"));

    state.node.shutdown().await.ok();
}

#[tokio::test]
#[serial]
async fn resolve_recipient_routes_groups_contacts_and_ambiguity() {
    let (state, _dir) = boot_in_tempdir().await;
    rpc(&state, "groups_new", json!({ "alias": "garrett-1on1" })).await;
    let pubkey = "c".repeat(64);
    rpc(
        &state,
        "contacts_upsert",
        json!({ "identity_pubkey": pubkey, "display_name": "Garrett" }),
    )
    .await;

    // Group-only match
    let g = rpc(&state, "resolve_recipient", json!({ "query": "garrett-1on1" })).await;
    assert_eq!(g["kind"].as_str().unwrap_or(""), "group");

    // Contact-only match
    let c = rpc(&state, "resolve_recipient", json!({ "query": "Garrett" })).await;
    assert_eq!(c["kind"].as_str().unwrap_or(""), "contact");

    // Nothing matches
    let n = rpc(
        &state,
        "resolve_recipient",
        json!({ "query": "doesnotexist" }),
    )
    .await;
    assert_eq!(n["kind"].as_str().unwrap_or(""), "none");

    state.node.shutdown().await.ok();
}
