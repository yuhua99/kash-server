mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::util::ServiceExt;

async fn send_friend_request(app: &common::TestApp, cookie: &str, friend_username: &str) {
    let payload = json!({ "friend_username": friend_username });
    let request = Request::builder()
        .uri("/friends/request")
        .method("POST")
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .expect("build friend request payload");

    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute friend request");
    assert_eq!(response.status(), StatusCode::CREATED);
}

async fn accept_friend_request(app: &common::TestApp, cookie: &str, friend_id: &str) {
    let payload = json!({ "friend_id": friend_id });
    let request = Request::builder()
        .uri("/friends/accept")
        .method("POST")
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .expect("build accept request payload");

    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute accept request");
    assert_eq!(response.status(), StatusCode::OK);
}

async fn post_split_body(app: &common::TestApp, cookie: &str, body: String) -> StatusCode {
    let request = Request::builder()
        .uri("/splits")
        .method("POST")
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .expect("build split request");

    app.router
        .clone()
        .oneshot(request)
        .await
        .expect("execute split request")
        .status()
}

async fn post_split(app: &common::TestApp, cookie: &str, payload: Value) -> StatusCode {
    post_split_body(app, cookie, payload.to_string()).await
}

async fn assert_no_split_rows(app: &common::TestApp) {
    let conn = app.state.main_db.connect().expect("connect db");
    let mut rows = conn
        .query(
            "SELECT (SELECT COUNT(*) FROM splits), (SELECT COUNT(*) FROM split_participants)",
            (),
        )
        .await
        .expect("count split rows");
    let row = rows.next().await.expect("next counts").expect("counts row");
    assert_eq!(row.get::<i64>(0).expect("splits count"), 0);
    assert_eq!(row.get::<i64>(1).expect("participants count"), 0);
}

async fn setup_users() -> (common::TestApp, String, String, String, String) {
    let app = common::setup_test_app().await.expect("setup failed");
    let alice_id = common::create_test_user(&app.state, "alice_sv", "password123")
        .await
        .expect("create alice failed");
    let bob_id = common::create_test_user(&app.state, "bob_sv", "password123")
        .await
        .expect("create bob failed");
    let alice_cookie = common::login_user(&app.router, "alice_sv", "password123")
        .await
        .expect("alice login failed");
    let bob_cookie = common::login_user(&app.router, "bob_sv", "password123")
        .await
        .expect("bob login failed");
    (app, alice_id, bob_id, alice_cookie, bob_cookie)
}

fn valid_payload(key: &str, bob_id: &str) -> Value {
    json!({
        "idempotency_key": key,
        "total_amount": 100.0,
        "currency": "TWD",
        "description": "Dinner",
        "date": "2026-02-16",
        "category_id": "cat-placeholder",
        "splits": [{ "user_id": bob_id, "amount": 25.0 }]
    })
}

#[tokio::test]
async fn split_create_rejects_bad_currency_without_rows() {
    let (app, _alice_id, bob_id, alice_cookie, _bob_cookie) = setup_users().await;
    let mut payload = valid_payload("split-validation-bad-currency", &bob_id);
    payload["currency"] = json!("BAD");

    assert_eq!(
        post_split(&app, &alice_cookie, payload).await,
        StatusCode::BAD_REQUEST
    );
    assert_no_split_rows(&app).await;
}

#[tokio::test]
async fn split_create_rejects_bad_date_without_rows() {
    let (app, _alice_id, bob_id, alice_cookie, _bob_cookie) = setup_users().await;
    let mut payload = valid_payload("split-validation-bad-date", &bob_id);
    payload["date"] = json!("02/16/2026");

    assert_eq!(
        post_split(&app, &alice_cookie, payload).await,
        StatusCode::BAD_REQUEST
    );
    assert_no_split_rows(&app).await;
}

#[tokio::test]
async fn split_create_rejects_duplicate_participant_without_rows() {
    let (app, _alice_id, bob_id, alice_cookie, _bob_cookie) = setup_users().await;
    let mut payload = valid_payload("split-validation-duplicate", &bob_id);
    payload["splits"] = json!([
        { "user_id": bob_id, "amount": 25.0 },
        { "user_id": bob_id, "amount": 15.0 }
    ]);

    assert_eq!(
        post_split(&app, &alice_cookie, payload).await,
        StatusCode::BAD_REQUEST
    );
    assert_no_split_rows(&app).await;
}

#[tokio::test]
async fn split_create_rejects_self_as_participant_without_rows() {
    let (app, alice_id, _bob_id, alice_cookie, _bob_cookie) = setup_users().await;
    let payload = json!({
        "idempotency_key": "split-validation-self",
        "total_amount": 100.0,
        "currency": "TWD",
        "description": "Dinner",
        "date": "2026-02-16",
        "category_id": "cat-placeholder",
        "splits": [{ "user_id": alice_id, "amount": 25.0 }]
    });

    assert_eq!(
        post_split(&app, &alice_cookie, payload).await,
        StatusCode::BAD_REQUEST
    );
    assert_no_split_rows(&app).await;
}

#[tokio::test]
async fn split_create_rejects_zero_amount_without_rows() {
    let (app, _alice_id, bob_id, alice_cookie, _bob_cookie) = setup_users().await;
    let mut payload = valid_payload("split-validation-zero", &bob_id);
    payload["splits"] = json!([{ "user_id": bob_id, "amount": 0.0 }]);

    assert_eq!(
        post_split(&app, &alice_cookie, payload).await,
        StatusCode::BAD_REQUEST
    );
    assert_no_split_rows(&app).await;
}

#[tokio::test]
async fn split_create_rejects_negative_amount_without_rows() {
    let (app, _alice_id, bob_id, alice_cookie, _bob_cookie) = setup_users().await;
    let mut payload = valid_payload("split-validation-negative", &bob_id);
    payload["splits"] = json!([{ "user_id": bob_id, "amount": -1.0 }]);

    assert_eq!(
        post_split(&app, &alice_cookie, payload).await,
        StatusCode::BAD_REQUEST
    );
    assert_no_split_rows(&app).await;
}

#[tokio::test]
async fn split_create_rejects_non_finite_amount_without_rows() {
    let (app, _alice_id, bob_id, alice_cookie, _bob_cookie) = setup_users().await;
    let body = format!(
        r#"{{"idempotency_key":"split-validation-non-finite","total_amount":100.0,"currency":"TWD","description":"Dinner","date":"2026-02-16","category_id":"cat-placeholder","splits":[{{"user_id":"{}","amount":1e999}}]}}"#,
        bob_id
    );

    assert_eq!(
        post_split_body(&app, &alice_cookie, body).await,
        StatusCode::BAD_REQUEST
    );
    assert_no_split_rows(&app).await;
}

#[tokio::test]
async fn split_create_rejects_over_max_amount_without_rows() {
    let (app, _alice_id, bob_id, alice_cookie, _bob_cookie) = setup_users().await;
    let mut payload = valid_payload("split-validation-over-max", &bob_id);
    payload["splits"] = json!([{ "user_id": bob_id, "amount": 1_000_000_001.0 }]);

    assert_eq!(
        post_split(&app, &alice_cookie, payload).await,
        StatusCode::BAD_REQUEST
    );
    assert_no_split_rows(&app).await;
}

#[tokio::test]
async fn split_create_rejects_participant_not_a_friend_without_rows() {
    let (app, _alice_id, bob_id, alice_cookie, _bob_cookie) = setup_users().await;

    assert_eq!(
        post_split(
            &app,
            &alice_cookie,
            valid_payload("split-validation-not-friend", &bob_id),
        )
        .await,
        StatusCode::BAD_REQUEST
    );
    assert_no_split_rows(&app).await;
}

#[tokio::test]
async fn split_create_rejects_participant_sum_exceeding_total_without_rows() {
    let (app, alice_id, bob_id, alice_cookie, bob_cookie) = setup_users().await;
    send_friend_request(&app, &alice_cookie, "bob_sv").await;
    accept_friend_request(&app, &bob_cookie, &alice_id).await;

    let mut payload = valid_payload("split-validation-exceeds-total", &bob_id);
    payload["splits"] = json!([{ "user_id": bob_id, "amount": 125.0 }]);

    assert_eq!(
        post_split(&app, &alice_cookie, payload).await,
        StatusCode::BAD_REQUEST
    );
    assert_no_split_rows(&app).await;
}
