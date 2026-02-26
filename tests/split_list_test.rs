mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use kash_server::models::Category;
use serde_json::{Value, json};
use tower::util::ServiceExt;
use uuid::Uuid;

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

async fn finalize_pending(app: &common::TestApp, cookie: &str, record_id: &str, category_id: &str) {
    let payload = json!({
        "record_id": record_id,
        "category_id": category_id,
    });

    let request = Request::builder()
        .uri("/records/finalize-pending")
        .method("POST")
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .expect("build finalize request");

    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute finalize request");

    assert_eq!(response.status(), StatusCode::OK);
}

async fn settle_record(app: &common::TestApp, cookie: &str, record_id: &str, split_id: &str) {
    let payload = json!({ "split_id": split_id });
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/records/{}/settle", record_id))
        .header("content-type", "application/json")
        .header("cookie", cookie)
        .body(Body::from(payload.to_string()))
        .expect("build settle request");

    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute settle request");

    assert_eq!(response.status(), StatusCode::OK);
}

fn pending_record_id(split_response: &Value) -> String {
    split_response["pending_record_ids"]
        .as_array()
        .expect("pending_record_ids should be array")
        .first()
        .expect("pending record id exists")
        .as_str()
        .expect("pending record id should be string")
        .to_string()
}

#[tokio::test]
async fn split_pending_list_returns_actionable_pending_items() {
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

    let split_response = create_split(
        &app,
        &alice_cookie,
        &bob_id,
        &alice_category.id,
        "split-pending-list-1",
    )
    .await;
    let bob_pending_record_id = pending_record_id(&split_response);

    let list_request = Request::builder()
        .uri("/splits/pending?limit=1000&offset=0")
        .method("GET")
        .header("cookie", &bob_cookie)
        .body(Body::empty())
        .expect("build pending list request");

    let list_response = app
        .router
        .clone()
        .oneshot(list_request)
        .await
        .expect("execute pending list request");

    assert_eq!(list_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(list_response.into_body(), usize::MAX)
        .await
        .expect("read pending list body");
    let json_body: Value = serde_json::from_slice(&body).expect("parse pending list body");

    let splits = json_body["splits"]
        .as_array()
        .expect("splits should be array");
    assert_eq!(json_body["total_count"], 1);
    assert_eq!(splits.len(), 1);
    assert_eq!(splits[0]["record_id"], bob_pending_record_id);
    assert_eq!(splits[0]["requested_by_user_id"], alice_id);
    assert_eq!(splits[0]["requested_by_name"], "alice");
    assert_eq!(splits[0]["counterparty_user_id"], alice_id);
    assert_eq!(splits[0]["counterparty_name"], "alice");
    assert_eq!(splits[0]["direction"], "you_owe");
    assert_eq!(splits[0]["pending"], true);
    assert_eq!(splits[0]["settle"], false);
    assert_eq!(splits[0]["amount"], 35.0);
}

#[tokio::test]
async fn split_unsettled_with_friend_returns_non_pending_and_excludes_settled() {
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
        "split-unsettled-list-1",
    )
    .await;

    let split_id = split_response["split_id"]
        .as_str()
        .expect("split_id should be string");
    let bob_pending_record_id = pending_record_id(&split_response);

    finalize_pending(&app, &bob_cookie, &bob_pending_record_id, &bob_category.id).await;

    let unsettled_request = Request::builder()
        .uri(format!(
            "/splits/unsettled?friend_id={}&limit=1000&offset=0",
            alice_id
        ))
        .method("GET")
        .header("cookie", &bob_cookie)
        .body(Body::empty())
        .expect("build unsettled request");

    let unsettled_response = app
        .router
        .clone()
        .oneshot(unsettled_request)
        .await
        .expect("execute unsettled request");

    assert_eq!(unsettled_response.status(), StatusCode::OK);

    let unsettled_body = axum::body::to_bytes(unsettled_response.into_body(), usize::MAX)
        .await
        .expect("read unsettled body");
    let unsettled_json: Value =
        serde_json::from_slice(&unsettled_body).expect("parse unsettled body");

    let unsettled_splits = unsettled_json["splits"]
        .as_array()
        .expect("splits should be array");
    assert_eq!(unsettled_json["total_count"], 1);
    assert_eq!(unsettled_splits.len(), 1);
    assert_eq!(unsettled_splits[0]["record_id"], bob_pending_record_id);
    assert_eq!(unsettled_splits[0]["pending"], false);
    assert_eq!(unsettled_splits[0]["settle"], false);
    assert_eq!(unsettled_splits[0]["direction"], "you_owe");

    let unknown_friend_id = Uuid::new_v4().to_string();
    let unknown_friend_request = Request::builder()
        .uri(format!(
            "/splits/unsettled?friend_id={}&limit=1000&offset=0",
            unknown_friend_id
        ))
        .method("GET")
        .header("cookie", &bob_cookie)
        .body(Body::empty())
        .expect("build unknown-friend unsettled request");

    let unknown_friend_response = app
        .router
        .clone()
        .oneshot(unknown_friend_request)
        .await
        .expect("execute unknown-friend unsettled request");

    assert_eq!(unknown_friend_response.status(), StatusCode::BAD_REQUEST);

    let self_friend_request = Request::builder()
        .uri(format!(
            "/splits/unsettled?friend_id={}&limit=1000&offset=0",
            bob_id
        ))
        .method("GET")
        .header("cookie", &bob_cookie)
        .body(Body::empty())
        .expect("build self-id unsettled request");

    let self_friend_response = app
        .router
        .clone()
        .oneshot(self_friend_request)
        .await
        .expect("execute self-id unsettled request");

    assert_eq!(self_friend_response.status(), StatusCode::BAD_REQUEST);

    settle_record(&app, &bob_cookie, &bob_pending_record_id, split_id).await;

    let settled_request = Request::builder()
        .uri(format!(
            "/splits/unsettled?friend_id={}&limit=1000&offset=0",
            alice_id
        ))
        .method("GET")
        .header("cookie", &bob_cookie)
        .body(Body::empty())
        .expect("build post-settle unsettled request");

    let settled_response = app
        .router
        .clone()
        .oneshot(settled_request)
        .await
        .expect("execute post-settle unsettled request");

    assert_eq!(settled_response.status(), StatusCode::OK);

    let settled_body = axum::body::to_bytes(settled_response.into_body(), usize::MAX)
        .await
        .expect("read post-settle body");
    let settled_json: Value =
        serde_json::from_slice(&settled_body).expect("parse post-settle body");

    assert_eq!(settled_json["total_count"], 0);
    assert_eq!(
        settled_json["splits"]
            .as_array()
            .expect("splits should be array")
            .len(),
        0
    );
}
