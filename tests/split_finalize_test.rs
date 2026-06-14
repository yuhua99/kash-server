mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use kash_server::models::Category;
use serde_json::{Value, json};
use tower::util::ServiceExt;
use uuid::Uuid;

struct SplitFixture {
    app: common::TestApp,
    #[allow(dead_code)]
    alice_cookie: String,
    bob_cookie: String,
    charlie_cookie: String,
    bob_id: String,
    bob_category: Category,
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
    let alice_name = format!("alice_finalize_{suffix}");
    let bob_name = format!("bob_finalize_{suffix}");
    let charlie_name = format!("charlie_finalize_{suffix}");

    let alice_id = common::create_test_user(&app.state, &alice_name, "password123")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, &bob_name, "password123")
        .await
        .expect("create bob");
    common::create_test_user(&app.state, &charlie_name, "password123")
        .await
        .expect("create charlie");

    let alice_cookie = common::login_user(&app.router, &alice_name, "password123")
        .await
        .expect("alice login");
    let bob_cookie = common::login_user(&app.router, &bob_name, "password123")
        .await
        .expect("bob login");
    let charlie_cookie = common::login_user(&app.router, &charlie_name, "password123")
        .await
        .expect("charlie login");

    send_friend_request(&app, &alice_cookie, &bob_name).await;
    accept_friend_request(&app, &bob_cookie, &alice_id).await;

    let alice_category = create_category(&app, &alice_cookie, "Dining", false).await;
    let bob_category = create_category(&app, &bob_cookie, "Shared", false).await;

    let split_payload = json!({
        "idempotency_key": format!("split-finalize-{suffix}"),
        "total_amount": 100.0,
        "currency": "TWD",
        "description": "Dinner",
        "date": "2026-02-16",
        "category_id": alice_category.id,
        "splits": [{ "user_id": bob_id, "amount": 35.0 }]
    });
    let request = Request::builder()
        .uri("/splits")
        .method("POST")
        .header("cookie", &alice_cookie)
        .header("content-type", "application/json")
        .body(Body::from(split_payload.to_string()))
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
        alice_cookie,
        bob_cookie,
        charlie_cookie,
        bob_id,
        bob_category,
        participant_id,
    }
}

async fn finalize(
    app: &common::TestApp,
    cookie: &str,
    participant_id: &str,
    category_id: &str,
) -> (StatusCode, Value, String) {
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
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read finalize body");
    let body_text = String::from_utf8(body.to_vec()).expect("utf8 body");
    let body_json = serde_json::from_str(&body_text).unwrap_or(Value::Null);
    (status, body_json, body_text)
}

#[tokio::test]
async fn happy_finalize_creates_debtor_record_and_removes_pending_share() {
    let fx = create_split_fixture("happy").await;

    let (status, record, _) = finalize(
        &fx.app,
        &fx.bob_cookie,
        &fx.participant_id,
        &fx.bob_category.id,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(record["name"], "Dinner");
    assert_eq!(record["amount"], -35.0);
    assert_eq!(record["currency"], "TWD");
    assert_eq!(record["category_id"], fx.bob_category.id);
    assert_eq!(record["date"], "2026-02-16");
    let record_id = record["id"].as_str().expect("record id");

    let conn = fx.app.state.main_db.connect().expect("connect db");
    let mut rows = conn
        .query(
            "SELECT finalized_record_id FROM split_participants WHERE id = ?",
            [fx.participant_id.as_str()],
        )
        .await
        .expect("query participant");
    let row = rows
        .next()
        .await
        .expect("next row")
        .expect("participant row");
    let finalized_record_id: Option<String> = row.get(0).expect("finalized_record_id");
    assert_eq!(finalized_record_id.as_deref(), Some(record_id));

    let mut rows = conn
        .query(
            "SELECT owner_user_id, amount, category_id FROM records WHERE id = ?",
            [record_id],
        )
        .await
        .expect("query record");
    let row = rows.next().await.expect("next row").expect("record row");
    let owner_user_id: String = row.get(0).expect("owner_user_id");
    let amount: i64 = row.get(1).expect("amount");
    let category_id: String = row.get(2).expect("category_id");
    assert_eq!(owner_user_id, fx.bob_id);
    assert_eq!(amount, -3500);
    assert_eq!(category_id, fx.bob_category.id);

    let request = Request::builder()
        .uri("/splits/pending")
        .method("GET")
        .header("cookie", &fx.bob_cookie)
        .body(Body::empty())
        .expect("build pending request");
    let response = fx
        .app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute pending request");
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read pending body");
    let pending: Value = serde_json::from_slice(&body).expect("parse pending body");
    assert!(
        pending["shares"]
            .as_array()
            .expect("shares array")
            .is_empty()
    );
}

#[tokio::test]
async fn other_user_finalize_returns_404() {
    let fx = create_split_fixture("other_user").await;

    let (status, _, body) = finalize(
        &fx.app,
        &fx.charlie_cookie,
        &fx.participant_id,
        &fx.bob_category.id,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, "Share not found");
}

#[tokio::test]
async fn double_finalize_returns_409() {
    let fx = create_split_fixture("double").await;

    let (status, _, _) = finalize(
        &fx.app,
        &fx.bob_cookie,
        &fx.participant_id,
        &fx.bob_category.id,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _, body) = finalize(
        &fx.app,
        &fx.bob_cookie,
        &fx.participant_id,
        &fx.bob_category.id,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body, "Share already finalized");
}

#[tokio::test]
async fn bad_category_returns_400() {
    let fx = create_split_fixture("bad_category").await;
    let missing_category_id = Uuid::new_v4().to_string();

    let (status, _, body) = finalize(
        &fx.app,
        &fx.bob_cookie,
        &fx.participant_id,
        &missing_category_id,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, "Category does not exist");
}
