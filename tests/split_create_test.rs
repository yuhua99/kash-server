mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use my_budget_server::models::Category;
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

fn extract_split_create_ids(body: &Value) -> (String, String, Vec<String>) {
    let split_id = body["split_id"]
        .as_str()
        .expect("missing split_id")
        .to_string();
    let payer_record_id = body["payer_record_id"]
        .as_str()
        .expect("missing payer_record_id")
        .to_string();

    let pending_ids = body["pending_record_ids"]
        .as_array()
        .expect("missing pending_record_ids")
        .iter()
        .map(|v| {
            v.as_str()
                .expect("pending_record_ids item should be string")
                .to_string()
        })
        .collect();

    (split_id, payer_record_id, pending_ids)
}

#[tokio::test]
async fn split_create_happy_path_fans_out_records() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    let bob_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");
    let charlie_id = common::create_test_user(&app.state, "charlie", "password123")
        .await
        .expect("create charlie failed");

    let alice_cookie = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");
    let bob_cookie = common::login_user(&app.router, "bob", "password123")
        .await
        .expect("bob login failed");
    let charlie_cookie = common::login_user(&app.router, "charlie", "password123")
        .await
        .expect("charlie login failed");

    send_friend_request(&app, &alice_cookie, "bob").await;
    send_friend_request(&app, &alice_cookie, "charlie").await;
    accept_friend_request(&app, &bob_cookie, &alice_id).await;
    accept_friend_request(&app, &charlie_cookie, &alice_id).await;

    let expense_category = create_category(&app, &alice_cookie, "Dining", false).await;

    let payload = json!({
        "idempotency_key": "split-create-happy-1",
        "total_amount": 100.0,
        "description": "Dinner split",
        "date": "2026-02-16",
        "category_id": expense_category.id,
        "splits": [
            { "user_id": bob_id, "amount": 30.0 },
            { "user_id": charlie_id, "amount": 30.0 }
        ]
    });

    let request = Request::builder()
        .uri("/splits/create")
        .method("POST")
        .header("cookie", alice_cookie)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .expect("build split request");

    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute split create");
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read split create response body");
    let json_body: Value = serde_json::from_slice(&body).expect("parse split create response");
    let (split_id, payer_record_id, pending_record_ids) = extract_split_create_ids(&json_body);
    assert_eq!(pending_record_ids.len(), 2);

    {
        let alice_db = app
            .state
            .db_pool
            .get_user_db(&alice_id)
            .await
            .expect("alice db");
        let conn = alice_db.read().await;
        let mut rows = conn
            .query(
                "SELECT amount, category_id, pending, split_id, settle, debtor_user_id, creditor_user_id FROM records WHERE id = ?",
                [payer_record_id.as_str()],
            )
            .await
            .expect("query payer record");

        let row = rows
            .next()
            .await
            .expect("next payer row")
            .expect("payer row should exist");
        let amount: f64 = row.get(0).expect("amount");
        let category_id: String = row.get(1).expect("category_id");
        let pending: bool = row.get(2).expect("pending");
        let split_id_db: Option<String> = row.get(3).expect("split_id");
        let settle: bool = row.get(4).expect("settle");
        let debtor_user_id: Option<String> = row.get(5).expect("debtor_user_id");
        let creditor_user_id: Option<String> = row.get(6).expect("creditor_user_id");

        assert_eq!(amount, -100.0);
        assert_eq!(category_id, expense_category.id);
        assert!(!pending);
        assert_eq!(split_id_db, Some(split_id.clone()));
        assert!(!settle);
        assert_eq!(debtor_user_id, Some(alice_id.clone()));
        assert_eq!(creditor_user_id, Some(alice_id.clone()));
    }

    {
        let bob_db = app
            .state
            .db_pool
            .get_user_db(&bob_id)
            .await
            .expect("bob db");
        let conn = bob_db.read().await;
        let mut rows = conn
            .query(
                "SELECT amount, category_id, pending, split_id, settle, debtor_user_id, creditor_user_id FROM records WHERE split_id = ?",
                [split_id.as_str()],
            )
            .await
            .expect("query bob pending");
        let row = rows
            .next()
            .await
            .expect("next bob row")
            .expect("bob pending row should exist");
        let amount: f64 = row.get(0).expect("bob amount");
        let category_id: Option<String> = row.get(1).expect("bob category_id");
        let pending: bool = row.get(2).expect("bob pending");
        let split_id_db: Option<String> = row.get(3).expect("bob split_id");
        let settle: bool = row.get(4).expect("bob settle");
        let debtor_user_id: Option<String> = row.get(5).expect("bob debtor");
        let creditor_user_id: Option<String> = row.get(6).expect("bob creditor");

        assert_eq!(amount, -30.0);
        assert_eq!(category_id, None);
        assert!(pending);
        assert_eq!(split_id_db, Some(split_id.clone()));
        assert!(!settle);
        assert_eq!(debtor_user_id, Some(bob_id.clone()));
        assert_eq!(creditor_user_id, Some(alice_id.clone()));
    }

    {
        let charlie_db = app
            .state
            .db_pool
            .get_user_db(&charlie_id)
            .await
            .expect("charlie db");
        let conn = charlie_db.read().await;
        let mut rows = conn
            .query(
                "SELECT amount, category_id, pending, split_id, settle, debtor_user_id, creditor_user_id FROM records WHERE split_id = ?",
                [split_id.as_str()],
            )
            .await
            .expect("query charlie pending");
        let row = rows
            .next()
            .await
            .expect("next charlie row")
            .expect("charlie pending row should exist");
        let amount: f64 = row.get(0).expect("charlie amount");
        let category_id: Option<String> = row.get(1).expect("charlie category_id");
        let pending: bool = row.get(2).expect("charlie pending");
        let split_id_db: Option<String> = row.get(3).expect("charlie split_id");
        let settle: bool = row.get(4).expect("charlie settle");
        let debtor_user_id: Option<String> = row.get(5).expect("charlie debtor");
        let creditor_user_id: Option<String> = row.get(6).expect("charlie creditor");

        assert_eq!(amount, -30.0);
        assert_eq!(category_id, None);
        assert!(pending);
        assert_eq!(split_id_db, Some(split_id));
        assert!(!settle);
        assert_eq!(debtor_user_id, Some(charlie_id));
        assert_eq!(creditor_user_id, Some(alice_id));
    }
}

#[tokio::test]
async fn split_create_idempotency_same_key_same_payload_returns_same_response() {
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

    let expense_category = create_category(&app, &alice_cookie, "Dining", false).await;

    let payload = json!({
        "idempotency_key": "split-create-idempotency-1",
        "total_amount": 99.99,
        "description": "Lunch",
        "date": "2026-02-16",
        "category_id": expense_category.id,
        "splits": [
            { "user_id": bob_id, "amount": 33.33 }
        ]
    });

    let first_request = Request::builder()
        .uri("/splits/create")
        .method("POST")
        .header("cookie", alice_cookie.clone())
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .expect("build first split request");
    let first_response = app
        .router
        .clone()
        .oneshot(first_request)
        .await
        .expect("execute first split request");
    let first_status = first_response.status();
    let first_body = axum::body::to_bytes(first_response.into_body(), usize::MAX)
        .await
        .expect("read first response body");

    let second_request = Request::builder()
        .uri("/splits/create")
        .method("POST")
        .header("cookie", alice_cookie)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .expect("build second split request");
    let second_response = app
        .router
        .clone()
        .oneshot(second_request)
        .await
        .expect("execute second split request");
    let second_status = second_response.status();
    let second_body = axum::body::to_bytes(second_response.into_body(), usize::MAX)
        .await
        .expect("read second response body");

    assert_eq!(first_status, StatusCode::CREATED);
    assert_eq!(second_status, first_status);
    assert_eq!(second_body, first_body);
}

#[tokio::test]
async fn split_create_idempotency_same_key_different_payload_conflicts() {
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

    let expense_category = create_category(&app, &alice_cookie, "Dining", false).await;

    let first_payload = json!({
        "idempotency_key": "split-create-idempotency-2",
        "total_amount": 100.0,
        "description": "Lunch A",
        "date": "2026-02-16",
        "category_id": expense_category.id,
        "splits": [
            { "user_id": bob_id, "amount": 25.0 }
        ]
    });

    let second_payload = json!({
        "idempotency_key": "split-create-idempotency-2",
        "total_amount": 100.0,
        "description": "Lunch B",
        "date": "2026-02-16",
        "category_id": expense_category.id,
        "splits": [
            { "user_id": bob_id, "amount": 35.0 }
        ]
    });

    let first_request = Request::builder()
        .uri("/splits/create")
        .method("POST")
        .header("cookie", alice_cookie.clone())
        .header("content-type", "application/json")
        .body(Body::from(first_payload.to_string()))
        .expect("build first split request");
    let first_response = app
        .router
        .clone()
        .oneshot(first_request)
        .await
        .expect("execute first split request");
    assert_eq!(first_response.status(), StatusCode::CREATED);

    let second_request = Request::builder()
        .uri("/splits/create")
        .method("POST")
        .header("cookie", alice_cookie)
        .header("content-type", "application/json")
        .body(Body::from(second_payload.to_string()))
        .expect("build second split request");
    let second_response = app
        .router
        .clone()
        .oneshot(second_request)
        .await
        .expect("execute second split request");
    assert_eq!(second_response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn split_create_rejects_non_friend_participant() {
    let app = common::setup_test_app().await.expect("setup failed");

    let _alice_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    let bob_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");

    let alice_cookie = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");
    let expense_category = create_category(&app, &alice_cookie, "Dining", false).await;

    let payload = json!({
        "idempotency_key": "split-create-friendship-1",
        "total_amount": 100.0,
        "description": "Dinner",
        "date": "2026-02-16",
        "category_id": expense_category.id,
        "splits": [
            { "user_id": bob_id, "amount": 25.0 }
        ]
    });

    let request = Request::builder()
        .uri("/splits/create")
        .method("POST")
        .header("cookie", alice_cookie)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .expect("build split request");
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute split request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
