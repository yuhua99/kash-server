mod common;

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::util::ServiceExt;

async fn json_request(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    cookie: &str,
    payload: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
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
    let body = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8(bytes.to_vec()).expect("utf8")));
    (status, body)
}

async fn create_category(app: &common::TestApp, cookie: &str, name: &str) -> String {
    let (status, body) = json_request(
        app,
        "POST",
        "/categories",
        cookie,
        json!({ "name": name, "is_income": false }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create category");
    body["id"].as_str().expect("category id").to_string()
}

async fn send_friend_request(app: &common::TestApp, cookie: &str, friend_username: &str) {
    let (status, _) = json_request(
        app,
        "POST",
        "/friends/request",
        cookie,
        json!({ "friend_username": friend_username }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "send friend request");
}

async fn accept_friend(app: &common::TestApp, cookie: &str, friend_id: &str) {
    let (status, _) = json_request(
        app,
        "POST",
        "/friends/accept",
        cookie,
        json!({ "friend_id": friend_id }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "accept friend");
}

async fn setup_friends(suffix: &str) -> (common::TestApp, String, String, String, String, String) {
    let app = common::setup_test_app().await.expect("setup failed");
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

    send_friend_request(&app, &alice_cookie, &bob_name).await;
    accept_friend(&app, &bob_cookie, &alice_id).await;
    let category_id = create_category(&app, &alice_cookie, &format!("Dining {suffix}")).await;

    (app, alice_id, bob_id, alice_cookie, bob_cookie, category_id)
}

fn split_payload(idempotency_key: &str, debtor_id: &str, category_id: &str, amount: f64) -> Value {
    json!({
        "idempotency_key": idempotency_key,
        "total_amount": 60.0,
        "currency": "TWD",
        "description": "split test",
        "date": "2026-02-20",
        "category_id": category_id,
        "splits": [{ "user_id": debtor_id, "amount": amount }]
    })
}

async fn count_table(app: &common::TestApp, table: &str) -> i64 {
    let conn = app.state.main_db.connect().expect("connect db");
    let sql = format!("SELECT COUNT(*) FROM {table}");
    let mut rows = conn.query(&sql, ()).await.expect("count query");
    let row = rows.next().await.expect("next row").expect("row exists");
    row.get(0).expect("count")
}

async fn count_records_for_user(app: &common::TestApp, user_id: &str) -> i64 {
    let conn = app.state.main_db.connect().expect("connect db");
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM records WHERE owner_user_id = ?",
            [user_id],
        )
        .await
        .expect("count records query");
    let row = rows.next().await.expect("next row").expect("row exists");
    row.get(0).expect("count")
}

async fn count_idempotency_rows(app: &common::TestApp, user_id: &str, key: &str) -> i64 {
    let conn = app.state.main_db.connect().expect("connect db");
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM idempotency_keys WHERE key = ? AND user_id = ? AND endpoint = ?",
            (key, user_id, "/splits/create"),
        )
        .await
        .expect("count idempotency rows");
    let row = rows.next().await.expect("next row").expect("row exists");
    row.get(0).expect("count")
}

async fn creditor_record_amount(
    app: &common::TestApp,
    record_id: &str,
    owner_user_id: &str,
) -> i64 {
    let conn = app.state.main_db.connect().expect("connect db");
    let mut rows = conn
        .query(
            "SELECT amount FROM records WHERE id = ? AND owner_user_id = ?",
            (record_id, owner_user_id),
        )
        .await
        .expect("query creditor record");
    let row = rows.next().await.expect("next row").expect("row exists");
    row.get(0).expect("amount")
}

async fn participant_count_for_split(app: &common::TestApp, split_id: &str) -> i64 {
    let conn = app.state.main_db.connect().expect("connect db");
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM split_participants WHERE split_id = ? AND amount > 0 AND settled = 0 AND finalized_record_id IS NULL",
            [split_id],
        )
        .await
        .expect("count participants");
    let row = rows.next().await.expect("next row").expect("row exists");
    row.get(0).expect("count")
}

#[tokio::test]
async fn create_split_writes_split_creditor_record_and_participants_only() {
    let (app, alice_id, bob_id, alice_cookie, _bob_cookie, category_id) =
        setup_friends("atomic_create").await;

    let (status, body) = json_request(
        &app,
        "POST",
        "/splits",
        &alice_cookie,
        split_payload("atomic-create-key", &bob_id, &category_id, 30.0),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let split_id = body["split_id"].as_str().expect("split_id");
    let creditor_record_id = body["creditor_record_id"]
        .as_str()
        .expect("creditor_record_id");
    let participants = body["participants"].as_array().expect("participants");
    assert_eq!(participants.len(), 1);
    assert_eq!(participants[0]["debtor_user_id"], bob_id);
    assert_eq!(participants[0]["amount"], 30.0);

    assert_eq!(count_table(&app, "splits").await, 1);
    assert_eq!(participant_count_for_split(&app, split_id).await, 1);
    assert_eq!(count_records_for_user(&app, &alice_id).await, 1);
    assert_eq!(count_records_for_user(&app, &bob_id).await, 0);
    assert_eq!(
        creditor_record_amount(&app, creditor_record_id, &alice_id).await,
        -3000
    );
}

#[tokio::test]
async fn failed_create_persists_nothing_and_clears_reservation_for_retry() {
    let (app, alice_id, bob_id, alice_cookie, _bob_cookie, category_id) =
        setup_friends("atomic_fail").await;
    let key = "atomic-failure-key";

    let (status, _) = json_request(
        &app,
        "POST",
        "/splits",
        &alice_cookie,
        split_payload(key, &bob_id, "missing-category", 30.0),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(count_table(&app, "splits").await, 0);
    assert_eq!(count_table(&app, "split_participants").await, 0);
    assert_eq!(count_records_for_user(&app, &alice_id).await, 0);
    assert_eq!(count_records_for_user(&app, &bob_id).await, 0);
    assert_eq!(count_idempotency_rows(&app, &alice_id, key).await, 0);

    let (retry_status, _) = json_request(
        &app,
        "POST",
        "/splits",
        &alice_cookie,
        split_payload(key, &bob_id, &category_id, 30.0),
    )
    .await;

    assert_eq!(retry_status, StatusCode::CREATED);
    assert_eq!(count_table(&app, "splits").await, 1);
    assert_eq!(count_table(&app, "split_participants").await, 1);
    assert_eq!(count_records_for_user(&app, &alice_id).await, 1);
    assert_eq!(count_records_for_user(&app, &bob_id).await, 0);
}

#[tokio::test]
async fn concurrent_same_key_same_payload_creates_one_split() {
    let (app, alice_id, bob_id, alice_cookie, _bob_cookie, category_id) =
        setup_friends("atomic_concurrent").await;
    let payload = Arc::new(split_payload(
        "atomic-concurrent-key",
        &bob_id,
        &category_id,
        30.0,
    ));

    let mut handles = Vec::new();
    for _ in 0..2 {
        let router = app.router.clone();
        let cookie = alice_cookie.clone();
        let payload = payload.clone();
        handles.push(tokio::spawn(async move {
            let request = Request::builder()
                .method("POST")
                .uri("/splits")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .expect("build request");
            let response = router.oneshot(request).await.expect("execute request");
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("read body");
            let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            (status, body)
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await.expect("join request"));
    }

    let created: Vec<_> = results
        .iter()
        .filter(|(status, _)| *status == StatusCode::CREATED)
        .collect();
    assert!(!created.is_empty(), "at least one request returns 201");
    assert!(
        results
            .iter()
            .all(|(status, _)| *status == StatusCode::CREATED || *status == StatusCode::CONFLICT),
        "other request may replay cached 201 or report in-progress 409"
    );
    let first_body = &created[0].1;
    for (_, body) in created.iter().skip(1) {
        assert_eq!(body, first_body, "cached 201 bodies must match");
    }
    assert_eq!(count_table(&app, "splits").await, 1);
    assert_eq!(count_table(&app, "split_participants").await, 1);
    assert_eq!(count_records_for_user(&app, &alice_id).await, 1);
    assert_eq!(count_records_for_user(&app, &bob_id).await, 0);
}

#[tokio::test]
async fn same_key_different_payload_conflicts_without_duplicate_rows() {
    let (app, alice_id, bob_id, alice_cookie, _bob_cookie, category_id) =
        setup_friends("atomic_conflict").await;
    let key = "atomic-conflict-key";

    let (first_status, _) = json_request(
        &app,
        "POST",
        "/splits",
        &alice_cookie,
        split_payload(key, &bob_id, &category_id, 30.0),
    )
    .await;
    assert_eq!(first_status, StatusCode::CREATED);

    let (second_status, _) = json_request(
        &app,
        "POST",
        "/splits",
        &alice_cookie,
        split_payload(key, &bob_id, &category_id, 20.0),
    )
    .await;

    assert_eq!(second_status, StatusCode::CONFLICT);
    assert_eq!(count_table(&app, "splits").await, 1);
    assert_eq!(count_table(&app, "split_participants").await, 1);
    assert_eq!(count_records_for_user(&app, &alice_id).await, 1);
}
