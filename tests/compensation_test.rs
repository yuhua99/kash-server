mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::util::ServiceExt;

async fn create_category(
    app: &common::TestApp,
    cookie: &str,
    name: &str,
    is_income: bool,
) -> String {
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
    let json_body: Value = serde_json::from_slice(&body).expect("parse category response");
    json_body["id"]
        .as_str()
        .expect("category id missing")
        .to_string()
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

async fn get_split_status_row(
    app: &common::TestApp,
    split_id: &str,
) -> (String, i64, String, String) {
    let conn = app.state.main_db.read().await;
    let mut rows = conn
        .query(
            "SELECT status, fanout_attempts, succeeded_participant_ids, failed_participant_ids FROM split_coordination WHERE id = ?",
            [split_id],
        )
        .await
        .expect("query split coordination status");

    let row = rows
        .next()
        .await
        .expect("next split row")
        .expect("split row exists");

    let status: String = row.get(0).expect("status");
    let fanout_attempts: i64 = row.get(1).expect("fanout_attempts");
    let succeeded_participant_ids: String = row.get(2).expect("succeeded_participant_ids");
    let failed_participant_ids: String = row.get(3).expect("failed_participant_ids");
    (
        status,
        fanout_attempts,
        succeeded_participant_ids,
        failed_participant_ids,
    )
}

async fn count_split_records_for_user(app: &common::TestApp, user_id: &str, split_id: &str) -> i64 {
    let user_db = app
        .state
        .db_pool
        .get_user_db(user_id)
        .await
        .expect("user db");
    let conn = user_db.read().await;
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM records WHERE split_id = ?",
            [split_id],
        )
        .await
        .expect("query split records count");
    let row = rows
        .next()
        .await
        .expect("next count row")
        .expect("count row");
    row.get(0).expect("count")
}

async fn set_split_fail_once_for_user(app: &common::TestApp, user_id: &str) {
    let conn = app.state.main_db.write().await;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS split_failure_injections (user_id TEXT PRIMARY KEY, fail_once INTEGER NOT NULL)",
        (),
    )
    .await
    .expect("create split_failure_injections table");

    conn.execute(
        "INSERT INTO split_failure_injections (user_id, fail_once) VALUES (?, 1)",
        [user_id],
    )
    .await
    .expect("insert split failure injection");
}

#[tokio::test]
async fn split_compensation_retries_failed_recipient_and_marks_completed() {
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

    let category_id = create_category(&app, &alice_cookie, "Dining", false).await;

    set_split_fail_once_for_user(&app, &charlie_id).await;

    let create_payload = json!({
        "idempotency_key": "split-compensation-create-1",
        "total_amount": 120.0,
        "description": "Dinner compensation flow",
        "date": "2026-02-16",
        "category_id": category_id,
        "splits": [
            { "user_id": bob_id, "amount": 40.0 },
            { "user_id": charlie_id, "amount": 40.0 }
        ]
    });

    let create_request = Request::builder()
        .uri("/splits/create")
        .method("POST")
        .header("cookie", alice_cookie.clone())
        .header("content-type", "application/json")
        .body(Body::from(create_payload.to_string()))
        .expect("build split create request");
    let create_response = app
        .router
        .clone()
        .oneshot(create_request)
        .await
        .expect("execute split create request");

    assert_eq!(create_response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let create_body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
        .await
        .expect("read create response body");
    let create_body_str = String::from_utf8(create_body.to_vec()).expect("create body utf8");
    let split_id = create_body_str
        .split(':')
        .next_back()
        .expect("split id in error body")
        .trim()
        .to_string();

    let (status, fanout_attempts, succeeded_ids, failed_ids) =
        get_split_status_row(&app, &split_id).await;
    assert_eq!(status, "partial_failure");
    assert_eq!(fanout_attempts, 1);
    assert!(succeeded_ids.contains(&alice_id));
    assert!(succeeded_ids.contains(&bob_id));
    assert!(failed_ids.contains(&charlie_id));

    let retry_request = Request::builder()
        .uri(format!("/splits/{}/retry", split_id))
        .method("POST")
        .header("cookie", alice_cookie)
        .body(Body::empty())
        .expect("build retry request");
    let retry_response = app
        .router
        .clone()
        .oneshot(retry_request)
        .await
        .expect("execute retry request");
    assert_eq!(retry_response.status(), StatusCode::OK);

    let (
        status_after_retry,
        fanout_attempts_after_retry,
        _succeeded_after_retry,
        failed_after_retry,
    ) = get_split_status_row(&app, &split_id).await;
    assert_eq!(status_after_retry, "completed");
    assert_eq!(fanout_attempts_after_retry, 2);
    assert_eq!(failed_after_retry, "[]");

    assert_eq!(
        count_split_records_for_user(&app, &alice_id, &split_id).await,
        1
    );
    assert_eq!(
        count_split_records_for_user(&app, &bob_id, &split_id).await,
        1
    );
    assert_eq!(
        count_split_records_for_user(&app, &charlie_id, &split_id).await,
        1
    );
}

#[tokio::test]
async fn split_retry_is_idempotent_when_split_already_completed() {
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

    let category_id = create_category(&app, &alice_cookie, "Dining", false).await;

    let create_payload = json!({
        "idempotency_key": "split-compensation-idempotent-1",
        "total_amount": 90.0,
        "description": "Idempotent retry",
        "date": "2026-02-16",
        "category_id": category_id,
        "splits": [
            { "user_id": bob_id, "amount": 30.0 }
        ]
    });

    let create_request = Request::builder()
        .uri("/splits/create")
        .method("POST")
        .header("cookie", alice_cookie.clone())
        .header("content-type", "application/json")
        .body(Body::from(create_payload.to_string()))
        .expect("build split create request");
    let create_response = app
        .router
        .clone()
        .oneshot(create_request)
        .await
        .expect("execute split create request");
    assert_eq!(create_response.status(), StatusCode::CREATED);

    let create_body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
        .await
        .expect("read create response body");
    let create_json: Value = serde_json::from_slice(&create_body).expect("parse create body json");
    let split_id = create_json["split_id"]
        .as_str()
        .expect("split_id")
        .to_string();

    let retry_request_1 = Request::builder()
        .uri(format!("/splits/{}/retry", split_id))
        .method("POST")
        .header("cookie", alice_cookie.clone())
        .body(Body::empty())
        .expect("build retry request 1");
    let retry_response_1 = app
        .router
        .clone()
        .oneshot(retry_request_1)
        .await
        .expect("execute retry request 1");
    assert_eq!(retry_response_1.status(), StatusCode::OK);

    let retry_request_2 = Request::builder()
        .uri(format!("/splits/{}/retry", split_id))
        .method("POST")
        .header("cookie", alice_cookie)
        .body(Body::empty())
        .expect("build retry request 2");
    let retry_response_2 = app
        .router
        .clone()
        .oneshot(retry_request_2)
        .await
        .expect("execute retry request 2");
    assert_eq!(retry_response_2.status(), StatusCode::OK);

    let (status, _attempts, _succeeded_ids, failed_ids) =
        get_split_status_row(&app, &split_id).await;
    assert_eq!(status, "completed");
    assert_eq!(failed_ids, "[]");

    assert_eq!(
        count_split_records_for_user(&app, &alice_id, &split_id).await,
        1
    );
    assert_eq!(
        count_split_records_for_user(&app, &bob_id, &split_id).await,
        1
    );
}
