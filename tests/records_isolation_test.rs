/// Tests B5-B10: Records isolation and authorization
///
/// After migration to a single DB, every query must be scoped to the
/// calling user's `owner_user_id`.  These tests are expected to FAIL
/// (red) until the migration is implemented.
mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::util::ServiceExt;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

async fn json_post(
    app: &common::TestApp,
    uri: &str,
    cookie: &str,
    payload: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .expect("build request");
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute request");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        Value::String(String::from_utf8(bytes.to_vec()).expect("utf8"))
    });
    (status, body)
}

async fn json_put(
    app: &common::TestApp,
    uri: &str,
    cookie: &str,
    payload: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("PUT")
        .uri(uri)
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .expect("build request");
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute request");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        Value::String(String::from_utf8(bytes.to_vec()).expect("utf8"))
    });
    (status, body)
}

async fn json_delete(
    app: &common::TestApp,
    uri: &str,
    cookie: &str,
) -> StatusCode {
    let request = Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("cookie", cookie)
        .body(Body::empty())
        .expect("build request");
    app.router
        .clone()
        .oneshot(request)
        .await
        .expect("execute request")
        .status()
}

async fn json_get(
    app: &common::TestApp,
    uri: &str,
    cookie: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .header("cookie", cookie)
        .body(Body::empty())
        .expect("build request");
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute request");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        Value::String(String::from_utf8(bytes.to_vec()).expect("utf8"))
    });
    (status, body)
}

/// Create a category via API and return its id.
async fn create_category(app: &common::TestApp, cookie: &str, name: &str) -> String {
    let (status, body) = json_post(
        app,
        "/categories",
        cookie,
        json!({ "name": name, "is_income": false }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create category {name}");
    body["id"].as_str().expect("category id").to_string()
}

/// Create a plain (non-split) record via API and return its id.
async fn create_record(
    app: &common::TestApp,
    cookie: &str,
    name: &str,
    category_id: &str,
) -> String {
    let (status, body) = json_post(
        app,
        "/records",
        cookie,
        json!({
            "name": name,
            "amount": -50.0,
            "date": "2026-02-20",
            "category_id": category_id
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create record {name}");
    body["id"].as_str().expect("record id").to_string()
}

/// Set up two users (alice + bob) who are friends, each with one category
/// and one record; returns (alice_id, bob_id, alice_cookie, bob_cookie,
///  alice_cat_id, bob_cat_id, alice_record_id, bob_record_id).
#[allow(clippy::type_complexity)]
async fn setup_two_users(
    app: &common::TestApp,
    suffix: &str,
) -> (String, String, String, String, String, String, String, String) {
    let alice_name = format!("alice_{suffix}");
    let bob_name = format!("bob_{suffix}");

    let alice_id = common::create_test_user(&app.state, &alice_name, "pw")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, &bob_name, "pw")
        .await
        .expect("create bob");

    let alice_cookie = common::login_user(&app.router, &alice_name, "pw")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, &bob_name, "pw")
        .await
        .expect("login bob");

    let alice_cat = create_category(app, &alice_cookie, "AliceCat").await;
    let bob_cat = create_category(app, &bob_cookie, "BobCat").await;

    let alice_rec = create_record(app, &alice_cookie, "AliceRecord", &alice_cat).await;
    let bob_rec = create_record(app, &bob_cookie, "BobRecord", &bob_cat).await;

    (
        alice_id,
        bob_id,
        alice_cookie,
        bob_cookie,
        alice_cat,
        bob_cat,
        alice_rec,
        bob_rec,
    )
}

// ---------------------------------------------------------------------------
// B5: User A creates record; User B cannot see it in GET /records
// ---------------------------------------------------------------------------

#[tokio::test]
async fn b5_user_a_record_not_visible_to_user_b() {
    let app = common::setup_test_app().await.expect("setup failed");
    let (_alice_id, _bob_id, alice_cookie, bob_cookie, _ac, _bc, alice_rec_id, _bob_rec) =
        setup_two_users(&app, "b5").await;

    // Bob lists records — must not see Alice's record
    let (status, body) = json_get(&app, "/records", &bob_cookie).await;
    assert_eq!(status, StatusCode::OK);
    let records = body["records"].as_array().expect("records array");
    let ids: Vec<&str> = records
        .iter()
        .filter_map(|r| r["id"].as_str())
        .collect();
    assert!(
        !ids.contains(&alice_rec_id.as_str()),
        "Bob must not see Alice's record"
    );
}

// ---------------------------------------------------------------------------
// B6: User A cannot UPDATE User B's record
// ---------------------------------------------------------------------------

#[tokio::test]
async fn b6_user_a_cannot_update_user_b_record() {
    let app = common::setup_test_app().await.expect("setup failed");
    let (_alice_id, _bob_id, alice_cookie, _bob_cookie, alice_cat, _bc, _alice_rec, bob_rec_id) =
        setup_two_users(&app, "b6").await;

    // Alice tries to update Bob's record
    let (status, _) = json_put(
        &app,
        &format!("/records/{bob_rec_id}"),
        &alice_cookie,
        json!({
            "name": "hacked",
            "amount": -1.0,
            "date": "2026-02-20",
            "category_id": alice_cat
        }),
    )
    .await;

    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::FORBIDDEN,
        "Alice must not be able to update Bob's record, got {status}"
    );
}

// ---------------------------------------------------------------------------
// B7: User A cannot DELETE User B's record
// ---------------------------------------------------------------------------

#[tokio::test]
async fn b7_user_a_cannot_delete_user_b_record() {
    let app = common::setup_test_app().await.expect("setup failed");
    let (_aid, _bid, alice_cookie, _bob_cookie, _ac, _bc, _alice_rec, bob_rec_id) =
        setup_two_users(&app, "b7").await;

    let status = json_delete(&app, &format!("/records/{bob_rec_id}"), &alice_cookie).await;
    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::FORBIDDEN,
        "Alice must not be able to delete Bob's record, got {status}"
    );
}

// ---------------------------------------------------------------------------
// B8: GET /records returns only current user's data (with filters applied)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn b8_get_records_returns_only_current_user_data() {
    let app = common::setup_test_app().await.expect("setup failed");
    let (_aid, _bid, alice_cookie, bob_cookie, _ac, _bc, alice_rec_id, bob_rec_id) =
        setup_two_users(&app, "b8").await;

    // Alice sees only her record
    let (status_a, body_a) = json_get(&app, "/records", &alice_cookie).await;
    assert_eq!(status_a, StatusCode::OK);
    let alice_ids: Vec<&str> = body_a["records"]
        .as_array()
        .expect("records")
        .iter()
        .filter_map(|r| r["id"].as_str())
        .collect();
    assert!(alice_ids.contains(&alice_rec_id.as_str()), "Alice must see her own record");
    assert!(!alice_ids.contains(&bob_rec_id.as_str()), "Alice must not see Bob's record");

    // Bob sees only his record
    let (status_b, body_b) = json_get(&app, "/records", &bob_cookie).await;
    assert_eq!(status_b, StatusCode::OK);
    let bob_ids: Vec<&str> = body_b["records"]
        .as_array()
        .expect("records")
        .iter()
        .filter_map(|r| r["id"].as_str())
        .collect();
    assert!(bob_ids.contains(&bob_rec_id.as_str()), "Bob must see his own record");
    assert!(!bob_ids.contains(&alice_rec_id.as_str()), "Bob must not see Alice's record");

    // With filter applied — still scoped to the caller
    let (status_fa, body_fa) = json_get(&app, "/records?pending=false", &alice_cookie).await;
    assert_eq!(status_fa, StatusCode::OK);
    let filtered_ids: Vec<&str> = body_fa["records"]
        .as_array()
        .expect("filtered records")
        .iter()
        .filter_map(|r| r["id"].as_str())
        .collect();
    assert!(!filtered_ids.contains(&bob_rec_id.as_str()), "filter must not expose Bob's records");
}

// ---------------------------------------------------------------------------
// B9: finalize_pending cannot be used on another user's record
// ---------------------------------------------------------------------------

#[tokio::test]
async fn b9_finalize_pending_blocked_for_other_user_record() {
    let app = common::setup_test_app().await.expect("setup failed");

    // We need a pending record — create a split, then have a third user try to finalize it
    let alice_id = common::create_test_user(&app.state, "alice_b9", "pw")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, "bob_b9", "pw")
        .await
        .expect("create bob");
    let eve_id = common::create_test_user(&app.state, "eve_b9", "pw")
        .await
        .expect("create eve");

    let alice_cookie = common::login_user(&app.router, "alice_b9", "pw")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob_b9", "pw")
        .await
        .expect("login bob");
    let eve_cookie = common::login_user(&app.router, "eve_b9", "pw")
        .await
        .expect("login eve");

    // Alice and Bob become friends
    let _ = json_post(
        &app,
        "/friends/request",
        &alice_cookie,
        json!({ "friend_username": "bob_b9" }),
    )
    .await;
    let _ = json_post(
        &app,
        "/friends/accept",
        &bob_cookie,
        json!({ "friend_id": alice_id }),
    )
    .await;

    // Eve has her own category
    let eve_cat = create_category(&app, &eve_cookie, "EveCat").await;

    // Alice creates split with Bob
    let alice_cat = create_category(&app, &alice_cookie, "AliceCat").await;
    let (split_status, split_body) = json_post(
        &app,
        "/splits/create",
        &alice_cookie,
        json!({
            "idempotency_key": "b9-split-1",
            "total_amount": 60.0,
            "description": "b9 split",
            "date": "2026-02-20",
            "category_id": alice_cat,
            "splits": [{ "user_id": bob_id, "amount": 30.0 }]
        }),
    )
    .await;
    assert_eq!(split_status, StatusCode::CREATED, "create split");

    let bob_pending_id = split_body["pending_record_ids"][0]
        .as_str()
        .expect("pending id")
        .to_string();

    // Eve tries to finalize Bob's pending record — must be rejected
    let (status, _) = json_post(
        &app,
        "/records/finalize-pending",
        &eve_cookie,
        json!({ "record_id": bob_pending_id, "category_id": eve_cat }),
    )
    .await;
    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::FORBIDDEN,
        "Eve must not be able to finalize Bob's pending record, got {status}"
    );

    let _ = (eve_id,); // suppress unused warnings
}

// ---------------------------------------------------------------------------
// B10: settle permissions — owner/debtor/creditor OK; cross-user blocked
// ---------------------------------------------------------------------------

#[tokio::test]
async fn b10_settle_cross_user_blocked() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice_b10", "pw")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, "bob_b10", "pw")
        .await
        .expect("create bob");
    let eve_id = common::create_test_user(&app.state, "eve_b10", "pw")
        .await
        .expect("create eve");

    let alice_cookie = common::login_user(&app.router, "alice_b10", "pw")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob_b10", "pw")
        .await
        .expect("login bob");
    let eve_cookie = common::login_user(&app.router, "eve_b10", "pw")
        .await
        .expect("login eve");

    // Alice and Bob become friends
    let _ = json_post(
        &app,
        "/friends/request",
        &alice_cookie,
        json!({ "friend_username": "bob_b10" }),
    )
    .await;
    let _ = json_post(
        &app,
        "/friends/accept",
        &bob_cookie,
        json!({ "friend_id": alice_id }),
    )
    .await;

    let alice_cat = create_category(&app, &alice_cookie, "AliceCat").await;
    let (split_status, split_body) = json_post(
        &app,
        "/splits/create",
        &alice_cookie,
        json!({
            "idempotency_key": "b10-split-1",
            "total_amount": 60.0,
            "description": "b10 split",
            "date": "2026-02-20",
            "category_id": alice_cat,
            "splits": [{ "user_id": bob_id, "amount": 30.0 }]
        }),
    )
    .await;
    assert_eq!(split_status, StatusCode::CREATED, "create split");

    let split_id = split_body["split_id"].as_str().expect("split_id").to_string();
    let bob_record_id = split_body["pending_record_ids"][0]
        .as_str()
        .expect("pending id")
        .to_string();

    // Eve (unrelated) tries to settle Bob's record — must be 404
    let (eve_settle_status, _) = json_put(
        &app,
        &format!("/records/{bob_record_id}/settle"),
        &eve_cookie,
        json!({ "split_id": split_id }),
    )
    .await;
    assert_eq!(
        eve_settle_status,
        StatusCode::NOT_FOUND,
        "Eve must not be able to settle Bob's record (404 to avoid leaking existence)"
    );

    // Bob (debtor) CAN settle his own record
    let (bob_settle_status, _) = json_put(
        &app,
        &format!("/records/{bob_record_id}/settle"),
        &bob_cookie,
        json!({ "split_id": split_id }),
    )
    .await;
    assert_eq!(bob_settle_status, StatusCode::OK, "Bob (debtor) must be able to settle");

    let _ = (eve_id, bob_id);
}
