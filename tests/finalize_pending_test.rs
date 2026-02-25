mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use kash_server::models::Category;
use serde_json::{Value, json};
use tower::util::ServiceExt;

async fn create_category(
    app: &common::TestApp,
    cookie: &str,
    name: &str,
    is_income: bool,
) -> Category {
    let payload = json!({
        "name": name,
        "is_income": is_income
    });

    let request = Request::builder()
        .uri("/categories")
        .method("POST")
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
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

async fn create_split(
    app: &common::TestApp,
    payer_cookie: &str,
    participant_id: &str,
    payer_category_id: &str,
    idempotency_key: &str,
) -> Value {
    let payload = json!({
        "idempotency_key": idempotency_key,
        "total_amount": 100.0,
        "description": "Dinner",
        "date": "2026-02-16",
        "category_id": payer_category_id,
        "splits": [
            { "user_id": participant_id, "amount": 35.0 }
        ]
    });

    let request = Request::builder()
        .uri("/splits/create")
        .method("POST")
        .header("cookie", payer_cookie)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
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
    serde_json::from_slice(&body).expect("parse split response")
}

fn extract_pending_record_id(split_response: &Value) -> String {
    split_response["pending_record_ids"]
        .as_array()
        .expect("pending_record_ids array")
        .first()
        .expect("pending id exists")
        .as_str()
        .expect("pending id string")
        .to_string()
}

#[tokio::test]
async fn finalize_pending_happy_path_finalizes_record_and_updates_split_status() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    let bob_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");

    let alice_cookie = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");
    let bob_cookie = common::login_user(&app.router, "bob", "password123")
        .await
        .expect("bob login failed");

    send_friend_request(&app, &alice_cookie, "bob").await;
    accept_friend_request(&app, &bob_cookie, &alice_id).await;

    let alice_category = create_category(&app, &alice_cookie, "Dining", false).await;
    let bob_category = create_category(&app, &bob_cookie, "Shared", false).await;

    let split_response = create_split(
        &app,
        &alice_cookie,
        &bob_id,
        &alice_category.id,
        "finalize-pending-happy-1",
    )
    .await;

    let pending_record_id = extract_pending_record_id(&split_response);

    let finalize_payload = json!({
        "record_id": pending_record_id,
        "category_id": bob_category.id,
    });
    let finalize_request = Request::builder()
        .uri("/records/finalize-pending")
        .method("POST")
        .header("cookie", bob_cookie)
        .header("content-type", "application/json")
        .body(Body::from(finalize_payload.to_string()))
        .expect("build finalize request");

    let finalize_response = app
        .router
        .clone()
        .oneshot(finalize_request)
        .await
        .expect("execute finalize request");

    assert_eq!(finalize_response.status(), StatusCode::OK);

    let finalize_body = axum::body::to_bytes(finalize_response.into_body(), usize::MAX)
        .await
        .expect("read finalize body");
    let finalize_json: Value = serde_json::from_slice(&finalize_body).expect("parse finalize body");
    assert_eq!(finalize_json["id"], pending_record_id);
    assert_eq!(finalize_json["category_id"], bob_category.id);

    {
        let conn = app.state.main_db.read().await;
        let mut rows = conn
            .query(
                "SELECT pending, category_id FROM records WHERE id = ?",
                [pending_record_id.as_str()],
            )
            .await
            .expect("query finalized record");

        let row = rows
            .next()
            .await
            .expect("next row")
            .expect("record row exists");
        let pending: bool = row.get(0).expect("pending column");
        let category_id: Option<String> = row.get(1).expect("category_id column");

        assert!(!pending);
        assert_eq!(category_id, Some(bob_category.id));
    }
}

#[tokio::test]
async fn finalize_pending_concurrent_requests_allow_only_one_success() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    let bob_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");

    let alice_cookie = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");
    let bob_cookie = common::login_user(&app.router, "bob", "password123")
        .await
        .expect("bob login failed");

    send_friend_request(&app, &alice_cookie, "bob").await;
    accept_friend_request(&app, &bob_cookie, &alice_id).await;

    let alice_category = create_category(&app, &alice_cookie, "Dining", false).await;
    let bob_category = create_category(&app, &bob_cookie, "Shared", false).await;

    let split_response = create_split(
        &app,
        &alice_cookie,
        &bob_id,
        &alice_category.id,
        "finalize-pending-race-1",
    )
    .await;
    let pending_record_id = extract_pending_record_id(&split_response);

    let payload = json!({
        "record_id": pending_record_id,
        "category_id": bob_category.id,
    });

    let request1 = Request::builder()
        .uri("/records/finalize-pending")
        .method("POST")
        .header("cookie", bob_cookie.clone())
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .expect("build request1");

    let request2 = Request::builder()
        .uri("/records/finalize-pending")
        .method("POST")
        .header("cookie", bob_cookie)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .expect("build request2");

    let router1 = app.router.clone();
    let router2 = app.router.clone();

    let (response1, response2) = tokio::join!(router1.oneshot(request1), router2.oneshot(request2));
    let response1 = response1.expect("execute request1");
    let response2 = response2.expect("execute request2");

    let statuses = [response1.status(), response2.status()];
    let success_count = statuses.iter().filter(|&&s| s == StatusCode::OK).count();
    let conflict_count = statuses
        .iter()
        .filter(|&&s| s == StatusCode::CONFLICT)
        .count();

    assert_eq!(success_count, 1);
    assert_eq!(conflict_count, 1);
}
