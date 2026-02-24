mod common;

use std::sync::Arc;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::util::ServiceExt;

fn parse_body_as_json_or_string(bytes: &[u8]) -> Value {
    match serde_json::from_slice::<Value>(bytes) {
        Ok(value) => value,
        Err(_) => Value::String(String::from_utf8(bytes.to_vec()).expect("utf8 body")),
    }
}

async fn json_request(
    router: Router,
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
        .expect("build json request");

    let response = router.oneshot(request).await.expect("execute request");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let json = parse_body_as_json_or_string(&body);
    (status, json)
}

async fn create_category(app: &common::TestApp, cookie: &str, name: &str) -> String {
    let (status, body) = json_request(
        app.router.clone(),
        "POST",
        "/categories",
        cookie,
        json!({ "name": name, "is_income": false }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body["id"].as_str().expect("category id string").to_string()
}

async fn send_friend_request(app: &common::TestApp, cookie: &str, friend_username: &str) {
    let (status, _) = json_request(
        app.router.clone(),
        "POST",
        "/friends/request",
        cookie,
        json!({ "friend_username": friend_username }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

async fn accept_friend_request(app: &common::TestApp, cookie: &str, friend_id: &str) {
    let (status, _) = json_request(
        app.router.clone(),
        "POST",
        "/friends/accept",
        cookie,
        json!({ "friend_id": friend_id }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_concurrent_split_creation_idempotency_e2e_concurrency() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob");

    let alice_cookie = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob", "password123")
        .await
        .expect("login bob");

    send_friend_request(&app, &alice_cookie, "bob").await;
    accept_friend_request(&app, &bob_cookie, &alice_id).await;

    let category_id = create_category(&app, &alice_cookie, "Dining").await;

    let shared_payload = Arc::new(json!({
        "idempotency_key": "e2e-concurrency-split-1",
        "total_amount": 100.0,
        "description": "Concurrent split create",
        "date": "2026-02-16",
        "category_id": category_id,
        "splits": [
            { "user_id": bob_id, "amount": 40.0 }
        ]
    }));

    let mut handles = Vec::new();
    for _ in 0..5 {
        let router = app.router.clone();
        let cookie = alice_cookie.clone();
        let payload = shared_payload.clone();
        handles.push(tokio::spawn(async move {
            json_request(
                router,
                "POST",
                "/splits/create",
                &cookie,
                payload.as_ref().clone(),
            )
            .await
        }));
    }

    let mut statuses = Vec::new();
    let mut created_bodies = Vec::new();
    for handle in handles {
        let (status, body) = handle.await.expect("join split create task");
        if status == StatusCode::CREATED {
            created_bodies.push(body.clone());
        }
        statuses.push(status);
    }

    let created_count = statuses
        .iter()
        .filter(|&&s| s == StatusCode::CREATED)
        .count();
    assert!(
        created_count >= 1,
        "at least one concurrent request must succeed"
    );

    let split_id = created_bodies
        .first()
        .expect("at least one successful create")["split_id"]
        .as_str()
        .expect("split_id string")
        .to_string();

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
                "SELECT COUNT(*), MIN(pending), MAX(pending), MIN(amount), MAX(amount) FROM records WHERE split_id = ?",
                [split_id.as_str()],
            )
            .await
            .expect("query bob pending record count");
        let row = rows
            .next()
            .await
            .expect("next bob count row")
            .expect("bob count row exists");
        let pending_count: i64 = row.get(0).expect("pending count");
        let min_pending: Option<bool> = row.get(1).expect("min pending");
        let max_pending: Option<bool> = row.get(2).expect("max pending");
        let min_amount: Option<f64> = row.get(3).expect("min amount");
        let max_amount: Option<f64> = row.get(4).expect("max amount");
        assert_eq!(pending_count, 1);
        assert_eq!(min_pending, Some(true));
        assert_eq!(max_pending, Some(true));
        assert_eq!(min_amount, Some(-40.0));
        assert_eq!(max_amount, Some(-40.0));
    }

    if !created_bodies.is_empty() {
        let canonical_split_id = created_bodies[0]["split_id"]
            .as_str()
            .expect("split id")
            .to_string();
        let canonical_payer_record_id = created_bodies[0]["payer_record_id"]
            .as_str()
            .expect("payer record id")
            .to_string();
        let canonical_pending = created_bodies[0]["pending_record_ids"]
            .as_array()
            .expect("pending array")
            .first()
            .expect("pending id exists")
            .as_str()
            .expect("pending id string")
            .to_string();

        for body in &created_bodies {
            assert_eq!(body["split_id"], canonical_split_id);
            assert_eq!(body["payer_record_id"], canonical_payer_record_id);
            assert_eq!(body["pending_record_ids"][0], canonical_pending);
        }
    }
}

#[tokio::test]
async fn test_concurrent_finalization_race_safety_e2e_concurrency() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob");

    let alice_cookie = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob", "password123")
        .await
        .expect("login bob");

    send_friend_request(&app, &alice_cookie, "bob").await;
    accept_friend_request(&app, &bob_cookie, &alice_id).await;

    let alice_category_id = create_category(&app, &alice_cookie, "Dining").await;
    let bob_category_id = create_category(&app, &bob_cookie, "Shared").await;

    let (split_status, split_body) = json_request(
        app.router.clone(),
        "POST",
        "/splits/create",
        &alice_cookie,
        json!({
            "idempotency_key": "e2e-concurrency-finalize-1",
            "total_amount": 100.0,
            "description": "Concurrent finalize split",
            "date": "2026-02-16",
            "category_id": alice_category_id,
            "splits": [
                { "user_id": bob_id, "amount": 35.0 }
            ]
        }),
    )
    .await;
    assert_eq!(split_status, StatusCode::CREATED);

    let pending_record_id = split_body["pending_record_ids"][0]
        .as_str()
        .expect("pending id")
        .to_string();

    let finalize_payload = Arc::new(json!({
        "record_id": pending_record_id,
        "category_id": bob_category_id,
    }));

    let mut handles = Vec::new();
    for _ in 0..3 {
        let router = app.router.clone();
        let cookie = bob_cookie.clone();
        let payload = finalize_payload.clone();
        handles.push(tokio::spawn(async move {
            json_request(
                router,
                "POST",
                "/records/finalize-pending",
                &cookie,
                payload.as_ref().clone(),
            )
            .await
        }));
    }

    let mut statuses = Vec::new();
    for handle in handles {
        let (status, _) = handle.await.expect("join finalize task");
        statuses.push(status);
    }

    let ok_count = statuses.iter().filter(|&&s| s == StatusCode::OK).count();
    let conflict_count = statuses
        .iter()
        .filter(|&&s| s == StatusCode::CONFLICT)
        .count();
    assert_eq!(ok_count, 1);
    assert_eq!(conflict_count, 2);

    let (post_race_status, post_race_body) = json_request(
        app.router.clone(),
        "POST",
        "/records/finalize-pending",
        &bob_cookie,
        json!({
            "record_id": pending_record_id,
            "category_id": bob_category_id,
        }),
    )
    .await;
    assert_eq!(post_race_status, StatusCode::CONFLICT);
    assert!(
        post_race_body
            .as_str()
            .expect("post-race conflict body string")
            .contains("finalized")
    );

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
                "SELECT pending, category_id, amount, debtor_user_id, creditor_user_id FROM records WHERE id = ?",
                [pending_record_id.as_str()],
            )
            .await
            .expect("query finalized row");
        let row = rows
            .next()
            .await
            .expect("next finalized row")
            .expect("finalized row exists");
        let pending: bool = row.get(0).expect("pending");
        let category_id: Option<String> = row.get(1).expect("category id");
        let amount: f64 = row.get(2).expect("amount");
        let debtor_user_id: Option<String> = row.get(3).expect("debtor id");
        let creditor_user_id: Option<String> = row.get(4).expect("creditor id");
        assert!(!pending);
        assert_eq!(category_id, Some(bob_category_id));
        assert_eq!(amount, -35.0);
        assert_eq!(debtor_user_id, Some(bob_id));
        assert_eq!(creditor_user_id, Some(alice_id));
    }
}
