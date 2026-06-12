mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::util::ServiceExt;
use uuid::Uuid;

async fn json_request(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    cookie: &str,
    payload: Value,
) -> (StatusCode, Vec<u8>) {
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
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body")
        .to_vec();
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
    assert_eq!(status, StatusCode::CREATED);
    let body: Value = serde_json::from_slice(&body).expect("parse category response");
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
    assert_eq!(status, StatusCode::CREATED);
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
    assert_eq!(status, StatusCode::OK);
}

async fn setup_split_fixture(
    suffix: &str,
) -> anyhow::Result<(common::TestApp, String, String, String, String)> {
    let app = common::setup_test_app().await?;
    let alice_name = format!("alice_{suffix}_ir");
    let bob_name = format!("bob_{suffix}_ir");
    let alice_id = common::create_test_user(&app.state, &alice_name, "pw").await?;
    let bob_id = common::create_test_user(&app.state, &bob_name, "pw").await?;
    let alice_cookie = common::login_user(&app.router, &alice_name, "pw").await?;
    let bob_cookie = common::login_user(&app.router, &bob_name, "pw").await?;

    send_friend_request(&app, &alice_cookie, &bob_name).await;
    accept_friend(&app, &bob_cookie, &alice_id).await;
    let category_id = create_category(&app, &alice_cookie, &format!("Dining {suffix}")).await;

    Ok((app, alice_id, bob_id, alice_cookie, category_id))
}

fn split_payload(idempotency_key: &str, bob_id: &str, category_id: &str) -> Value {
    json!({
        "idempotency_key": idempotency_key,
        "total_amount": 60.0,
        "currency": "TWD",
        "description": "reservation split",
        "date": "2026-02-20",
        "category_id": category_id,
        "splits": [{ "user_id": bob_id, "amount": 30.0 }]
    })
}

async fn insert_null_reservation(
    app: &common::TestApp,
    user_id: &str,
    key: &str,
    created_at: time::OffsetDateTime,
) {
    let created_at = created_at
        .format(&time::format_description::well_known::Rfc3339)
        .expect("format created_at");
    let expires_at = (time::OffsetDateTime::now_utc() + time::Duration::hours(24))
        .format(&time::format_description::well_known::Rfc3339)
        .expect("format expires_at");
    let conn = app.state.main_db.connect().expect("connect db");
    conn.execute(
        "INSERT INTO idempotency_keys (id, key, user_id, endpoint, payload_hash, response_status, response_body, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, NULL, ?, ?)",
        (
            Uuid::new_v4().to_string(),
            key,
            user_id,
            "/splits/create",
            "seed-hash",
            0i64,
            created_at.as_str(),
            expires_at.as_str(),
        ),
    )
    .await
    .expect("insert null reservation");
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

async fn response_body_is_cached(app: &common::TestApp, user_id: &str, key: &str) -> bool {
    let conn = app.state.main_db.connect().expect("connect db");
    let mut rows = conn
        .query(
            "SELECT response_body IS NOT NULL FROM idempotency_keys WHERE key = ? AND user_id = ? AND endpoint = ?",
            (key, user_id, "/splits/create"),
        )
        .await
        .expect("query idempotency response body");
    let row = rows.next().await.expect("next row").expect("row exists");
    row.get(0).expect("response body cached")
}

#[tokio::test]
async fn fresh_null_reservation_returns_conflict_without_creating_records() {
    let (app, alice_id, bob_id, alice_cookie, category_id) =
        setup_split_fixture("fresh").await.expect("setup fixture");
    let key = "fresh-null-reservation-ir";
    insert_null_reservation(&app, &alice_id, key, time::OffsetDateTime::now_utc()).await;

    let (status, _) = json_request(
        &app,
        "POST",
        "/splits/create",
        &alice_cookie,
        split_payload(key, &bob_id, &category_id),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(count_records_for_user(&app, &alice_id).await, 0);
}

#[tokio::test]
async fn stale_null_reservation_is_replaced_and_create_succeeds() {
    let (app, alice_id, bob_id, alice_cookie, category_id) =
        setup_split_fixture("stale").await.expect("setup fixture");
    let key = "stale-null-reservation-ir";
    insert_null_reservation(
        &app,
        &alice_id,
        key,
        time::OffsetDateTime::now_utc() - time::Duration::seconds(600),
    )
    .await;

    let (status, _) = json_request(
        &app,
        "POST",
        "/splits/create",
        &alice_cookie,
        split_payload(key, &bob_id, &category_id),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(count_records_for_user(&app, &alice_id).await, 1);
    assert!(response_body_is_cached(&app, &alice_id, key).await);
}
