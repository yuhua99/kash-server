mod common;

use axum::{
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
    app: &common::TestApp,
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

    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute json request");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let json = parse_body_as_json_or_string(&body);
    (status, json)
}

async fn create_category(app: &common::TestApp, cookie: &str, name: &str) -> String {
    let (status, body) = json_request(
        app,
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
        app,
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
        app,
        "POST",
        "/friends/accept",
        cookie,
        json!({ "friend_id": friend_id }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

async fn create_split(
    app: &common::TestApp,
    cookie: &str,
    idempotency_key: &str,
    total_amount: f64,
    category_id: &str,
    splits: Value,
    description: &str,
) -> (StatusCode, Value) {
    json_request(
        app,
        "POST",
        "/splits/create",
        cookie,
        json!({
            "idempotency_key": idempotency_key,
            "total_amount": total_amount,
            "description": description,
            "date": "2026-02-16",
            "category_id": category_id,
            "splits": splits
        }),
    )
    .await
}

async fn finalize_pending(
    app: &common::TestApp,
    cookie: &str,
    record_id: &str,
    category_id: &str,
) -> (StatusCode, Value) {
    json_request(
        app,
        "POST",
        "/records/finalize-pending",
        cookie,
        json!({
            "record_id": record_id,
            "category_id": category_id,
        }),
    )
    .await
}

async fn update_settle(
    app: &common::TestApp,
    cookie: &str,
    record_id: &str,
    split_id: &str,
) -> (StatusCode, Value) {
    json_request(
        app,
        "PUT",
        &format!("/records/{record_id}/settle"),
        cookie,
        json!({ "split_id": split_id }),
    )
    .await
}

async fn get_records(app: &common::TestApp, cookie: &str, query: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("GET")
        .uri(format!("/records{query}"))
        .header("cookie", cookie)
        .body(Body::empty())
        .expect("build records request");
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute records request");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read records response body");
    let json = parse_body_as_json_or_string(&body);
    (status, json)
}

#[tokio::test]
async fn test_full_lifecycle_happy_path_e2e_lifecycle() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob");
    let charlie_id = common::create_test_user(&app.state, "charlie", "password123")
        .await
        .expect("create charlie");

    let alice_cookie = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob", "password123")
        .await
        .expect("login bob");
    let charlie_cookie = common::login_user(&app.router, "charlie", "password123")
        .await
        .expect("login charlie");

    send_friend_request(&app, &alice_cookie, "bob").await;
    send_friend_request(&app, &alice_cookie, "charlie").await;
    accept_friend_request(&app, &bob_cookie, &alice_id).await;
    accept_friend_request(&app, &charlie_cookie, &alice_id).await;

    let alice_category_id = create_category(&app, &alice_cookie, "Dining").await;
    let bob_category_id = create_category(&app, &bob_cookie, "Shared").await;
    let charlie_category_id = create_category(&app, &charlie_cookie, "Trip").await;

    let (split_status, split_body) = create_split(
        &app,
        &alice_cookie,
        "e2e-lifecycle-happy-1",
        100.0,
        &alice_category_id,
        json!([
            { "user_id": bob_id, "amount": 20.0 },
            { "user_id": charlie_id, "amount": 20.0 }
        ]),
        "E2E lifecycle split",
    )
    .await;
    assert_eq!(split_status, StatusCode::CREATED);

    let split_id = split_body["split_id"]
        .as_str()
        .expect("split_id string")
        .to_string();
    let payer_record_id = split_body["payer_record_id"]
        .as_str()
        .expect("payer_record_id string")
        .to_string();
    let pending_record_ids = split_body["pending_record_ids"]
        .as_array()
        .expect("pending_record_ids array");
    assert_eq!(pending_record_ids.len(), 2);

    let bob_pending_record_id = {
        let conn = app.state.main_db.read().await;
        let mut rows = conn
            .query(
                "SELECT id, amount, category_id, pending, split_id, settle, debtor_user_id, creditor_user_id FROM records WHERE split_id = ? AND owner_user_id = ?",
                (split_id.as_str(), bob_id.as_str()),
            )
            .await
            .expect("query bob split record");
        let row = rows
            .next()
            .await
            .expect("next bob row")
            .expect("bob split row exists");
        let id: String = row.get(0).expect("bob id");
        let amount: f64 = row.get(1).expect("bob amount");
        let category_id: Option<String> = row.get(2).expect("bob category");
        let pending: bool = row.get(3).expect("bob pending");
        let split_id_db: Option<String> = row.get(4).expect("bob split_id");
        let settle: bool = row.get(5).expect("bob settle");
        let debtor_user_id: Option<String> = row.get(6).expect("bob debtor");
        let creditor_user_id: Option<String> = row.get(7).expect("bob creditor");
        assert_eq!(amount, -20.0);
        assert_eq!(category_id, None);
        assert!(pending);
        assert_eq!(split_id_db, Some(split_id.clone()));
        assert!(!settle);
        assert_eq!(debtor_user_id, Some(bob_id.clone()));
        assert_eq!(creditor_user_id, Some(alice_id.clone()));
        id
    };

    let charlie_pending_record_id = {
        let conn = app.state.main_db.read().await;
        let mut rows = conn
            .query(
                "SELECT id, amount, category_id, pending, split_id, settle, debtor_user_id, creditor_user_id FROM records WHERE split_id = ? AND owner_user_id = ?",
                (split_id.as_str(), charlie_id.as_str()),
            )
            .await
            .expect("query charlie split record");
        let row = rows
            .next()
            .await
            .expect("next charlie row")
            .expect("charlie split row exists");
        let id: String = row.get(0).expect("charlie id");
        let amount: f64 = row.get(1).expect("charlie amount");
        let category_id: Option<String> = row.get(2).expect("charlie category");
        let pending: bool = row.get(3).expect("charlie pending");
        let split_id_db: Option<String> = row.get(4).expect("charlie split_id");
        let settle: bool = row.get(5).expect("charlie settle");
        let debtor_user_id: Option<String> = row.get(6).expect("charlie debtor");
        let creditor_user_id: Option<String> = row.get(7).expect("charlie creditor");
        assert_eq!(amount, -20.0);
        assert_eq!(category_id, None);
        assert!(pending);
        assert_eq!(split_id_db, Some(split_id.clone()));
        assert!(!settle);
        assert_eq!(debtor_user_id, Some(charlie_id.clone()));
        assert_eq!(creditor_user_id, Some(alice_id.clone()));
        id
    };

    {
        let conn = app.state.main_db.read().await;
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
            .expect("payer row exists");
        let amount: f64 = row.get(0).expect("payer amount");
        let category_id: String = row.get(1).expect("payer category");
        let pending: bool = row.get(2).expect("payer pending");
        let split_id_db: Option<String> = row.get(3).expect("payer split_id");
        let settle: bool = row.get(4).expect("payer settle");
        let debtor_user_id: Option<String> = row.get(5).expect("payer debtor");
        let creditor_user_id: Option<String> = row.get(6).expect("payer creditor");
        assert_eq!(amount, -60.0);
        assert_eq!(category_id, alice_category_id);
        assert!(!pending);
        assert_eq!(split_id_db, Some(split_id.clone()));
        assert!(!settle);
        assert_eq!(debtor_user_id, Some(alice_id.clone()));
        assert_eq!(creditor_user_id, Some(alice_id.clone()));
    }

    assert!(
        pending_record_ids
            .iter()
            .any(|id| id.as_str() == Some(bob_pending_record_id.as_str()))
    );
    assert!(
        pending_record_ids
            .iter()
            .any(|id| id.as_str() == Some(charlie_pending_record_id.as_str()))
    );

    let (finalize_b_status, finalize_b_body) =
        finalize_pending(&app, &bob_cookie, &bob_pending_record_id, &bob_category_id).await;
    assert_eq!(finalize_b_status, StatusCode::OK);
    assert_eq!(finalize_b_body["id"], bob_pending_record_id);
    assert_eq!(finalize_b_body["category_id"], bob_category_id);

    let (finalize_c_status, finalize_c_body) = finalize_pending(
        &app,
        &charlie_cookie,
        &charlie_pending_record_id,
        &charlie_category_id,
    )
    .await;
    assert_eq!(finalize_c_status, StatusCode::OK);
    assert_eq!(finalize_c_body["id"], charlie_pending_record_id);
    assert_eq!(finalize_c_body["category_id"], charlie_category_id);

    let (settle_status, settle_body) =
        update_settle(&app, &bob_cookie, &bob_pending_record_id, &split_id).await;
    assert_eq!(settle_status, StatusCode::OK);
    assert_eq!(settle_body["id"], bob_pending_record_id);

    let (settle_status_2, settle_body_2) =
        update_settle(&app, &bob_cookie, &bob_pending_record_id, &split_id).await;
    assert_eq!(settle_status_2, StatusCode::OK);
    assert_eq!(settle_body_2["id"], bob_pending_record_id);

    let (alice_records_status, alice_records_body) =
        get_records(&app, &alice_cookie, "?pending=false").await;
    assert_eq!(alice_records_status, StatusCode::OK);
    assert_eq!(alice_records_body["total_count"], 1);
    assert_eq!(alice_records_body["records"][0]["id"], payer_record_id);

    let (bob_settled_status, bob_settled_body) =
        get_records(&app, &bob_cookie, "?settle=true").await;
    assert_eq!(bob_settled_status, StatusCode::OK);
    assert_eq!(bob_settled_body["total_count"], 1);
    assert_eq!(bob_settled_body["records"][0]["id"], bob_pending_record_id);

    let (bob_combined_status, bob_combined_body) =
        get_records(&app, &bob_cookie, "?pending=false&settle=true").await;
    assert_eq!(bob_combined_status, StatusCode::OK);
    assert_eq!(bob_combined_body["total_count"], 1);
    assert_eq!(bob_combined_body["records"][0]["id"], bob_pending_record_id);

    let (charlie_unsettled_status, charlie_unsettled_body) =
        get_records(&app, &charlie_cookie, "?settle=false").await;
    assert_eq!(charlie_unsettled_status, StatusCode::OK);
    assert_eq!(charlie_unsettled_body["total_count"], 1);
    assert_eq!(
        charlie_unsettled_body["records"][0]["id"],
        charlie_pending_record_id
    );

    let (charlie_combined_status, charlie_combined_body) =
        get_records(&app, &charlie_cookie, "?pending=false&settle=false").await;
    assert_eq!(charlie_combined_status, StatusCode::OK);
    assert_eq!(charlie_combined_body["total_count"], 1);
    assert_eq!(
        charlie_combined_body["records"][0]["id"],
        charlie_pending_record_id
    );

    let (charlie_settled_status, charlie_settled_body) =
        get_records(&app, &charlie_cookie, "?settle=true").await;
    assert_eq!(charlie_settled_status, StatusCode::OK);
    assert_eq!(charlie_settled_body["total_count"], 0);
}

#[tokio::test]
async fn test_pending_relation_prevents_split_e2e_lifecycle() {
    let app = common::setup_test_app().await.expect("setup failed");

    let _alice_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob");

    let alice_cookie = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("login alice");
    let _bob_cookie = common::login_user(&app.router, "bob", "password123")
        .await
        .expect("login bob");

    send_friend_request(&app, &alice_cookie, "bob").await;

    let alice_category_id = create_category(&app, &alice_cookie, "Dining").await;
    let (split_status, split_body) = create_split(
        &app,
        &alice_cookie,
        "e2e-lifecycle-pending-1",
        100.0,
        &alice_category_id,
        json!([{ "user_id": bob_id, "amount": 40.0 }]),
        "pending relation should fail",
    )
    .await;

    assert_eq!(split_status, StatusCode::BAD_REQUEST);
    let error_message = split_body
        .as_str()
        .expect("split error as string response body");
    assert!(error_message.contains("not an accepted friend"));

    {
        let conn = app.state.main_db.read().await;
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM records WHERE name = ? AND owner_user_id = ?",
                ("pending relation should fail", bob_id.as_str()),
            )
            .await
            .expect("query bob records count");
        let row = rows
            .next()
            .await
            .expect("next bob record row")
            .expect("bob record count row exists");
        let bob_record_count: i64 = row.get(0).expect("bob record count");
        assert_eq!(bob_record_count, 0);
    }
}
