mod common;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::util::ServiceExt;

fn parse_body(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8(bytes.to_vec()).expect("utf8 body")))
}

async fn request(
    app: &common::TestApp,
    method: &str,
    uri: String,
    cookie: Option<&str>,
    payload: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    let body = if let Some(payload) = payload {
        builder = builder.header("content-type", "application/json");
        Body::from(payload.to_string())
    } else {
        Body::empty()
    };

    let response = app
        .router
        .clone()
        .oneshot(builder.body(body).expect("build request"))
        .await
        .expect("execute request");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    (status, parse_body(&bytes))
}

async fn register_and_login(app: &common::TestApp, username: &str) -> (String, String) {
    let (status, body) = request(
        app,
        "POST",
        "/auth/register".to_string(),
        None,
        Some(json!({ "username": username, "password": "password123" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let user_id = body["id"].as_str().expect("registered user id").to_string();
    let cookie = common::login_user(&app.router, username, "password123")
        .await
        .expect("login user");
    (user_id, cookie)
}

async fn create_category(app: &common::TestApp, cookie: &str, name: &str) -> String {
    let (status, body) = request(
        app,
        "POST",
        "/categories".to_string(),
        Some(cookie),
        Some(json!({ "name": name, "is_income": false })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body["id"].as_str().expect("category id").to_string()
}

async fn make_friends(
    app: &common::TestApp,
    requester_cookie: &str,
    accepter_cookie: &str,
    requester_id: &str,
    accepter_username: &str,
) {
    let (status, _) = request(
        app,
        "POST",
        "/friends/request".to_string(),
        Some(requester_cookie),
        Some(json!({ "friend_username": accepter_username })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = request(
        app,
        "POST",
        "/friends/accept".to_string(),
        Some(accepter_cookie),
        Some(json!({ "friend_id": requester_id })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

async fn count_records(app: &common::TestApp, owner_user_id: &str) -> i64 {
    let conn = app.state.main_db.connect().expect("connect db");
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM records WHERE owner_user_id = ?",
            [owner_user_id],
        )
        .await
        .expect("count records");
    rows.next()
        .await
        .expect("next count")
        .expect("count row")
        .get(0)
        .expect("record count")
}

#[tokio::test]
async fn full_split_lifecycle_uses_participants_pending_finalize_unsettled_and_settle() {
    let app = common::setup_test_app().await.expect("setup failed");
    let (alice_id, alice_cookie) = register_and_login(&app, "alice_e2e_life").await;
    let (bob_id, bob_cookie) = register_and_login(&app, "bob_e2e_life").await;

    make_friends(&app, &alice_cookie, &bob_cookie, &alice_id, "bob_e2e_life").await;
    let alice_category_id = create_category(&app, &alice_cookie, "Dining e2e").await;
    let bob_category_id = create_category(&app, &bob_cookie, "Shared e2e").await;

    let (status, split) = request(
        &app,
        "POST",
        "/splits".to_string(),
        Some(&alice_cookie),
        Some(json!({
            "idempotency_key": "e2e-lifecycle-new-model-1",
            "total_amount": 100.0,
            "currency": "TWD",
            "description": "E2E dinner",
            "date": "2026-02-16",
            "category_id": alice_category_id,
            "splits": [{ "user_id": bob_id, "amount": 40.0 }]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let split_id = split["split_id"].as_str().expect("split id");
    let creditor_record_id = split["creditor_record_id"]
        .as_str()
        .expect("creditor record id");
    let participant_id = split["participants"][0]["id"]
        .as_str()
        .expect("participant id");
    assert_eq!(split["participants"][0]["debtor_user_id"], bob_id);
    assert_eq!(split["participants"][0]["amount"], 40.0);
    assert_eq!(count_records(&app, &alice_id).await, 1);
    assert_eq!(count_records(&app, &bob_id).await, 0);

    let (status, pending) = request(
        &app,
        "GET",
        "/splits/pending?limit=1000&offset=0".to_string(),
        Some(&bob_cookie),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pending["total_count"], 1);
    assert_eq!(pending["shares"][0]["participant_id"], participant_id);
    assert_eq!(pending["shares"][0]["split_id"], split_id);

    let (status, record) = request(
        &app,
        "POST",
        format!("/splits/participants/{participant_id}/finalize"),
        Some(&bob_cookie),
        Some(json!({ "category_id": bob_category_id })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let debtor_record_id = record["id"].as_str().expect("debtor record id");
    assert_eq!(record["amount"], -40.0);
    assert_eq!(count_records(&app, &alice_id).await, 1);
    assert_eq!(count_records(&app, &bob_id).await, 1);

    let (status, pending_after) = request(
        &app,
        "GET",
        "/splits/pending?limit=1000&offset=0".to_string(),
        Some(&bob_cookie),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pending_after["total_count"], 0);

    let (status, unsettled) = request(
        &app,
        "GET",
        format!("/splits/unsettled?friend_id={alice_id}&limit=1000&offset=0"),
        Some(&bob_cookie),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(unsettled["total_count"], 1);
    assert_eq!(unsettled["shares"][0]["participant_id"], participant_id);
    assert_eq!(unsettled["shares"][0]["direction"], "you_owe");
    assert_eq!(unsettled["shares"][0]["finalized"], true);

    let (status, settle) = request(
        &app,
        "PUT",
        format!("/splits/participants/{participant_id}/settle"),
        Some(&bob_cookie),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(settle["participant_id"], participant_id);
    assert_eq!(settle["settled"], true);
    assert_eq!(settle["finalized"], true);

    let (status, unsettled_after) = request(
        &app,
        "GET",
        format!("/splits/unsettled?friend_id={alice_id}&limit=1000&offset=0"),
        Some(&bob_cookie),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(unsettled_after["total_count"], 0);

    let conn = app.state.main_db.connect().expect("connect db");
    let mut rows = conn
        .query(
            "SELECT settled, finalized_record_id FROM split_participants WHERE id = ?",
            [participant_id],
        )
        .await
        .expect("query participant");
    let row = rows
        .next()
        .await
        .expect("next participant")
        .expect("participant");
    let settled: bool = row.get(0).expect("settled");
    let finalized_record_id: String = row.get(1).expect("finalized record id");
    assert!(settled);
    assert_eq!(finalized_record_id, debtor_record_id);

    let mut rows = conn
        .query(
            "SELECT id, amount FROM records WHERE owner_user_id = ?",
            [alice_id.as_str()],
        )
        .await
        .expect("query creditor ledger");
    let row = rows
        .next()
        .await
        .expect("next creditor record")
        .expect("creditor record");
    assert_eq!(
        row.get::<String>(0).expect("creditor record"),
        creditor_record_id
    );
    assert_eq!(row.get::<i64>(1).expect("creditor amount"), -6000);
}
