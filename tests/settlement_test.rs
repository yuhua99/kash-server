mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use kash_server::models::Category;
use serde_json::{Value, json};
use tower::util::ServiceExt;

struct SplitFixture {
    app: common::TestApp,
    creditor_cookie: String,
    debtor_cookie: String,
    third_cookie: String,
    debtor_category: Category,
    participant_id: String,
}

async fn create_category(
    app: &common::TestApp,
    cookie: &str,
    name: &str,
    is_income: bool,
) -> Category {
    let request = Request::builder()
        .uri("/categories")
        .method("POST")
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "name": name, "is_income": is_income }).to_string(),
        ))
        .expect("build category request");
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute category request");
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read category body");
    serde_json::from_slice(&body).expect("parse category response")
}

async fn send_friend_request(app: &common::TestApp, cookie: &str, friend_username: &str) {
    let request = Request::builder()
        .uri("/friends/request")
        .method("POST")
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "friend_username": friend_username }).to_string(),
        ))
        .expect("build friend request");
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute friend request");
    assert_eq!(response.status(), StatusCode::CREATED);
}

async fn accept_friend_request(app: &common::TestApp, cookie: &str, friend_id: &str) {
    let request = Request::builder()
        .uri("/friends/accept")
        .method("POST")
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(json!({ "friend_id": friend_id }).to_string()))
        .expect("build accept request");
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute accept request");
    assert_eq!(response.status(), StatusCode::OK);
}

async fn create_split_fixture(suffix: &str) -> SplitFixture {
    let app = common::setup_test_app().await.expect("setup failed");
    let creditor_name = format!("creditor_settle_{suffix}");
    let debtor_name = format!("debtor_settle_{suffix}");
    let third_name = format!("third_settle_{suffix}");

    let creditor_id = common::create_test_user(&app.state, &creditor_name, "password123")
        .await
        .expect("create creditor");
    let debtor_id = common::create_test_user(&app.state, &debtor_name, "password123")
        .await
        .expect("create debtor");
    common::create_test_user(&app.state, &third_name, "password123")
        .await
        .expect("create third");

    let creditor_cookie = common::login_user(&app.router, &creditor_name, "password123")
        .await
        .expect("creditor login");
    let debtor_cookie = common::login_user(&app.router, &debtor_name, "password123")
        .await
        .expect("debtor login");
    let third_cookie = common::login_user(&app.router, &third_name, "password123")
        .await
        .expect("third login");

    send_friend_request(&app, &creditor_cookie, &debtor_name).await;
    accept_friend_request(&app, &debtor_cookie, &creditor_id).await;

    let creditor_category = create_category(&app, &creditor_cookie, "Dining", false).await;
    let debtor_category = create_category(&app, &debtor_cookie, "Shared", false).await;

    let request = Request::builder()
        .uri("/splits")
        .method("POST")
        .header("cookie", &creditor_cookie)
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "idempotency_key": format!("settlement-{suffix}"),
                "total_amount": 100.0,
                "currency": "TWD",
                "description": "Dinner",
                "date": "2026-02-16",
                "category_id": creditor_category.id,
                "splits": [{ "user_id": debtor_id, "amount": 50.0 }]
            })
            .to_string(),
        ))
        .expect("build split request");
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute split request");
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read split body");
    let split: Value = serde_json::from_slice(&body).expect("parse split response");
    let participant_id = split["participants"][0]["id"]
        .as_str()
        .expect("participant id")
        .to_string();

    SplitFixture {
        app,
        creditor_cookie,
        debtor_cookie,
        third_cookie,
        debtor_category,
        participant_id,
    }
}

async fn settle(
    app: &common::TestApp,
    cookie: &str,
    participant_id: &str,
) -> (StatusCode, Value, String) {
    let request = Request::builder()
        .uri(format!("/splits/participants/{participant_id}/settle"))
        .method("PUT")
        .header("cookie", cookie)
        .body(Body::empty())
        .expect("build settle request");
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute settle request");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read settle body");
    let body_text = String::from_utf8(body.to_vec()).expect("utf8 body");
    let body_json = serde_json::from_str(&body_text).unwrap_or(Value::Null);
    (status, body_json, body_text)
}

async fn finalize(
    app: &common::TestApp,
    cookie: &str,
    participant_id: &str,
    category_id: &str,
) -> Value {
    let request = Request::builder()
        .uri(format!("/splits/participants/{participant_id}/finalize"))
        .method("POST")
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "category_id": category_id }).to_string(),
        ))
        .expect("build finalize request");
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute finalize request");
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read finalize body");
    serde_json::from_slice(&body).expect("parse finalize body")
}

async fn record_count(app: &common::TestApp) -> i64 {
    let conn = app.state.main_db.connect().expect("connect db");
    let mut rows = conn
        .query("SELECT COUNT(*) FROM records", ())
        .await
        .expect("count records");
    let row = rows.next().await.expect("next row").expect("count row");
    row.get(0).expect("count")
}

async fn assert_participant_settled(app: &common::TestApp, participant_id: &str) {
    let conn = app.state.main_db.connect().expect("connect db");
    let mut rows = conn
        .query(
            "SELECT settled FROM split_participants WHERE id = ?",
            [participant_id],
        )
        .await
        .expect("query participant");
    let row = rows
        .next()
        .await
        .expect("next row")
        .expect("participant row");
    let settled: bool = row.get(0).expect("settled");
    assert!(settled);
}

fn assert_settled_response(body: &Value, participant_id: &str, finalized: bool) {
    assert_eq!(body["participant_id"], participant_id);
    assert_eq!(body["settled"], true);
    assert_eq!(body["finalized"], finalized);
    assert!(body.get("category_id").is_none());
}

#[tokio::test]
async fn debtor_settles_own_share() {
    let fx = create_split_fixture("debtor").await;

    let (status, body, _) = settle(&fx.app, &fx.debtor_cookie, &fx.participant_id).await;

    assert_eq!(status, StatusCode::OK);
    assert_settled_response(&body, &fx.participant_id, false);
    assert_participant_settled(&fx.app, &fx.participant_id).await;
}

#[tokio::test]
async fn creditor_settles_debtors_share() {
    let fx = create_split_fixture("creditor").await;

    let (status, body, _) = settle(&fx.app, &fx.creditor_cookie, &fx.participant_id).await;

    assert_eq!(status, StatusCode::OK);
    assert_settled_response(&body, &fx.participant_id, false);
    assert_participant_settled(&fx.app, &fx.participant_id).await;
}

#[tokio::test]
async fn settle_before_finalize_is_allowed() {
    let fx = create_split_fixture("before_finalize").await;

    let (status, body, _) = settle(&fx.app, &fx.debtor_cookie, &fx.participant_id).await;
    assert_eq!(status, StatusCode::OK);
    assert_settled_response(&body, &fx.participant_id, false);

    let record = finalize(
        &fx.app,
        &fx.debtor_cookie,
        &fx.participant_id,
        &fx.debtor_category.id,
    )
    .await;
    assert_eq!(record["amount"], -50.0);
    assert_participant_settled(&fx.app, &fx.participant_id).await;
}

#[tokio::test]
async fn settle_after_finalize_is_allowed() {
    let fx = create_split_fixture("after_finalize").await;
    finalize(
        &fx.app,
        &fx.debtor_cookie,
        &fx.participant_id,
        &fx.debtor_category.id,
    )
    .await;

    let (status, body, _) = settle(&fx.app, &fx.debtor_cookie, &fx.participant_id).await;

    assert_eq!(status, StatusCode::OK);
    assert_settled_response(&body, &fx.participant_id, true);
    assert_participant_settled(&fx.app, &fx.participant_id).await;
}

#[tokio::test]
async fn double_settle_is_idempotent() {
    let fx = create_split_fixture("idempotent").await;

    let (status, body, _) = settle(&fx.app, &fx.debtor_cookie, &fx.participant_id).await;
    assert_eq!(status, StatusCode::OK);
    assert_settled_response(&body, &fx.participant_id, false);

    let (status, body, _) = settle(&fx.app, &fx.debtor_cookie, &fx.participant_id).await;
    assert_eq!(status, StatusCode::OK);
    assert_settled_response(&body, &fx.participant_id, false);
}

#[tokio::test]
async fn unauthorized_third_user_gets_404() {
    let fx = create_split_fixture("unauthorized").await;

    let (status, _, body) = settle(&fx.app, &fx.third_cookie, &fx.participant_id).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, "Share not found");
}

#[tokio::test]
async fn settling_does_not_change_records() {
    let fx = create_split_fixture("records_unchanged").await;
    let before = record_count(&fx.app).await;

    let (status, body, _) = settle(&fx.app, &fx.creditor_cookie, &fx.participant_id).await;

    assert_eq!(status, StatusCode::OK);
    assert_settled_response(&body, &fx.participant_id, false);
    assert_eq!(record_count(&fx.app).await, before);
}
