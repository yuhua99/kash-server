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

async fn create_accepted_friendship(
    app: &common::TestApp,
    alice_cookie: &str,
    friend_cookie: &str,
    alice_id: &str,
    friend_username: &str,
) {
    send_friend_request(app, alice_cookie, friend_username).await;
    accept_friend_request(app, friend_cookie, alice_id).await;
}

async fn post_split(app: &common::TestApp, cookie: &str, payload: &Value) -> (StatusCode, Vec<u8>) {
    let request = Request::builder()
        .uri("/splits")
        .method("POST")
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .expect("build split request");

    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute split create");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read split create response body")
        .to_vec();
    (status, body)
}

fn extract_split_create_ids(body: &Value) -> (String, String, Vec<String>) {
    let split_id = body["split_id"]
        .as_str()
        .expect("missing split_id")
        .to_string();
    let creditor_record_id = body["creditor_record_id"]
        .as_str()
        .expect("missing creditor_record_id")
        .to_string();
    let participant_ids = body["participants"]
        .as_array()
        .expect("missing participants")
        .iter()
        .map(|participant| {
            participant["id"]
                .as_str()
                .expect("participant id should be string")
                .to_string()
        })
        .collect();

    (split_id, creditor_record_id, participant_ids)
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

#[tokio::test]
async fn split_create_happy_path_creates_split_participants_and_creditor_record() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice_sc", "password123")
        .await
        .expect("create alice failed");
    let bob_id = common::create_test_user(&app.state, "bob_sc", "password123")
        .await
        .expect("create bob failed");
    let charlie_id = common::create_test_user(&app.state, "charlie_sc", "password123")
        .await
        .expect("create charlie failed");

    let alice_cookie = common::login_user(&app.router, "alice_sc", "password123")
        .await
        .expect("alice login failed");
    let bob_cookie = common::login_user(&app.router, "bob_sc", "password123")
        .await
        .expect("bob login failed");
    let charlie_cookie = common::login_user(&app.router, "charlie_sc", "password123")
        .await
        .expect("charlie login failed");

    create_accepted_friendship(&app, &alice_cookie, &bob_cookie, &alice_id, "bob_sc").await;
    create_accepted_friendship(
        &app,
        &alice_cookie,
        &charlie_cookie,
        &alice_id,
        "charlie_sc",
    )
    .await;

    let expense_category = create_category(&app, &alice_cookie, "Dining", false).await;
    let payload = json!({
        "idempotency_key": "split-create-happy-1",
        "total_amount": 100.0,
        "currency": "TWD",
        "description": "Dinner split",
        "date": "2026-02-16",
        "category_id": expense_category.id,
        "splits": [
            { "user_id": bob_id, "amount": 30.0 },
            { "user_id": charlie_id, "amount": 30.0 }
        ]
    });

    let (status, body) = post_split(&app, &alice_cookie, &payload).await;
    assert_eq!(status, StatusCode::CREATED);
    let json_body: Value = serde_json::from_slice(&body).expect("parse split create response");
    let (split_id, creditor_record_id, participant_ids) = extract_split_create_ids(&json_body);

    let participants = json_body["participants"]
        .as_array()
        .expect("participants array");
    assert_eq!(participants.len(), 2);
    assert_eq!(participants[0]["debtor_user_id"], bob_id);
    assert_eq!(participants[0]["amount"], 30.0);
    assert_eq!(participants[1]["debtor_user_id"], charlie_id);
    assert_eq!(participants[1]["amount"], 30.0);

    let conn = app.state.main_db.connect().expect("connect db");
    let mut split_rows = conn
        .query(
            "SELECT creditor_user_id, description, currency, date, total_amount FROM splits WHERE id = ?",
            [split_id.as_str()],
        )
        .await
        .expect("query split row");
    let split_row = split_rows
        .next()
        .await
        .expect("next split row")
        .expect("split row exists");
    assert_eq!(split_row.get::<String>(0).expect("creditor"), alice_id);
    assert_eq!(
        split_row.get::<String>(1).expect("description"),
        "Dinner split"
    );
    assert_eq!(split_row.get::<String>(2).expect("currency"), "TWD");
    assert_eq!(split_row.get::<String>(3).expect("date"), "2026-02-16");
    assert_eq!(split_row.get::<i64>(4).expect("total amount"), 10000);
    assert!(split_rows.next().await.expect("no second split").is_none());

    for id in &participant_ids {
        let mut participant_rows = conn
            .query(
                "SELECT debtor_user_id, amount, settled, finalized_record_id FROM split_participants WHERE id = ? AND split_id = ?",
                (id.as_str(), split_id.as_str()),
            )
            .await
            .expect("query participant");
        let row = participant_rows
            .next()
            .await
            .expect("next participant")
            .expect("participant exists");
        let debtor_user_id: String = row.get(0).expect("debtor user id");
        let amount: i64 = row.get(1).expect("participant amount");
        let settled: i64 = row.get(2).expect("settled");
        let finalized_record_id: Option<String> = row.get(3).expect("finalized record id");

        assert!(debtor_user_id == bob_id || debtor_user_id == charlie_id);
        assert_eq!(amount, 3000);
        assert_eq!(settled, 0);
        assert_eq!(finalized_record_id, None);
    }

    let mut participant_count_rows = conn
        .query(
            "SELECT COUNT(*) FROM split_participants WHERE split_id = ?",
            [split_id.as_str()],
        )
        .await
        .expect("count participants");
    let participant_count_row = participant_count_rows
        .next()
        .await
        .expect("next participant count")
        .expect("participant count row");
    assert_eq!(participant_count_row.get::<i64>(0).expect("count"), 2);

    let mut record_rows = conn
        .query(
            "SELECT owner_user_id, name, amount, currency, category_id, date FROM records WHERE id = ?",
            [creditor_record_id.as_str()],
        )
        .await
        .expect("query creditor record");
    let record_row = record_rows
        .next()
        .await
        .expect("next creditor record")
        .expect("creditor record exists");
    assert_eq!(record_row.get::<String>(0).expect("owner"), alice_id);
    assert_eq!(record_row.get::<String>(1).expect("name"), "Dinner split");
    assert_eq!(record_row.get::<i64>(2).expect("amount"), -4000);
    assert_eq!(record_row.get::<String>(3).expect("currency"), "TWD");
    assert_eq!(
        record_row.get::<String>(4).expect("category"),
        expense_category.id
    );
    assert_eq!(
        record_row.get::<String>(5).expect("record date"),
        "2026-02-16"
    );

    assert_eq!(count_records_for_user(&app, &alice_id).await, 1);
    assert_eq!(count_records_for_user(&app, &bob_id).await, 0);
    assert_eq!(count_records_for_user(&app, &charlie_id).await, 0);
}

#[tokio::test]
async fn split_create_idempotency_same_key_same_payload_returns_same_response() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice_sc_idem", "password123")
        .await
        .expect("create alice failed");
    let bob_id = common::create_test_user(&app.state, "bob_sc_idem", "password123")
        .await
        .expect("create bob failed");

    let alice_cookie = common::login_user(&app.router, "alice_sc_idem", "password123")
        .await
        .expect("alice login failed");
    let bob_cookie = common::login_user(&app.router, "bob_sc_idem", "password123")
        .await
        .expect("bob login failed");

    create_accepted_friendship(&app, &alice_cookie, &bob_cookie, &alice_id, "bob_sc_idem").await;
    let expense_category = create_category(&app, &alice_cookie, "Dining", false).await;

    let payload = json!({
        "idempotency_key": "split-create-idempotency-1",
        "total_amount": 99.99,
        "currency": "TWD",
        "description": "Lunch",
        "date": "2026-02-16",
        "category_id": expense_category.id,
        "splits": [{ "user_id": bob_id, "amount": 33.33 }]
    });

    let (first_status, first_body) = post_split(&app, &alice_cookie, &payload).await;
    let (second_status, second_body) = post_split(&app, &alice_cookie, &payload).await;

    assert_eq!(first_status, StatusCode::CREATED);
    assert_eq!(second_status, first_status);
    assert_eq!(second_body, first_body);
    assert_eq!(count_records_for_user(&app, &alice_id).await, 1);
    assert_eq!(count_records_for_user(&app, &bob_id).await, 0);

    let response: Value = serde_json::from_slice(&first_body).expect("parse idempotent response");
    let split_id = response["split_id"].as_str().expect("split id");
    let conn = app.state.main_db.connect().expect("connect db");
    let mut rows = conn
        .query(
            "SELECT (SELECT COUNT(*) FROM splits WHERE id = ?), (SELECT COUNT(*) FROM split_participants WHERE split_id = ?)",
            (split_id, split_id),
        )
        .await
        .expect("count split rows");
    let row = rows.next().await.expect("next counts").expect("counts row");
    assert_eq!(row.get::<i64>(0).expect("split count"), 1);
    assert_eq!(row.get::<i64>(1).expect("participant count"), 1);
}
