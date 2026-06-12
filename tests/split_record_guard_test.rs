mod common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
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

    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/categories")
                .method(Method::POST)
                .header("cookie", cookie)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .expect("build category request"),
        )
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
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/friends/request")
                .method(Method::POST)
                .header("cookie", cookie)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .expect("build friend request"),
        )
        .await
        .expect("execute friend request");
    assert_eq!(response.status(), StatusCode::CREATED);
}

async fn accept_friend_request(app: &common::TestApp, cookie: &str, friend_id: &str) {
    let payload = json!({ "friend_id": friend_id });
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/friends/accept")
                .method(Method::POST)
                .header("cookie", cookie)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .expect("build accept request"),
        )
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
        "currency": "TWD",
        "description": "Dinner split",
        "date": "2026-02-16",
        "category_id": payer_category_id,
        "splits": [
            { "user_id": participant_id, "amount": 35.0 }
        ]
    });

    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/splits/create")
                .method(Method::POST)
                .header("cookie", payer_cookie)
                .header("content-type", "application/json")
                .header("idempotency-key", idempotency_key)
                .body(Body::from(payload.to_string()))
                .expect("build split request"),
        )
        .await
        .expect("execute split request");
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read split body");
    serde_json::from_slice(&body).expect("parse split response")
}

async fn create_friend_split(
    suffix: &str,
) -> (
    common::TestApp,
    String,
    String,
    Category,
    Category,
    String,
    String,
) {
    let app = common::setup_test_app().await.expect("setup failed");
    let alice_name = format!("alice_{suffix}");
    let bob_name = format!("bob_{suffix}");

    let alice_id = common::create_test_user(&app.state, &alice_name, "password123")
        .await
        .expect("create alice failed");
    let bob_id = common::create_test_user(&app.state, &bob_name, "password123")
        .await
        .expect("create bob failed");

    let alice_cookie = common::login_user(&app.router, &alice_name, "password123")
        .await
        .expect("alice login failed");
    let bob_cookie = common::login_user(&app.router, &bob_name, "password123")
        .await
        .expect("bob login failed");

    send_friend_request(&app, &alice_cookie, &bob_name).await;
    accept_friend_request(&app, &bob_cookie, &alice_id).await;

    let alice_category = create_category(&app, &alice_cookie, "Dining", false).await;
    let bob_category = create_category(&app, &bob_cookie, "Shared", false).await;
    let split_response = create_split(
        &app,
        &alice_cookie,
        &bob_id,
        &alice_category.id,
        &format!("split-guard-{suffix}"),
    )
    .await;

    let payer_record_id = split_response["payer_record_id"]
        .as_str()
        .expect("payer_record_id string")
        .to_string();
    let pending_record_id = split_response["pending_record_ids"]
        .as_array()
        .expect("pending ids array")
        .first()
        .expect("pending id exists")
        .as_str()
        .expect("pending id string")
        .to_string();

    (
        app,
        alice_cookie,
        bob_cookie,
        alice_category,
        bob_category,
        payer_record_id,
        pending_record_id,
    )
}

async fn update_record(
    app: &common::TestApp,
    cookie: &str,
    record_id: &str,
    payload: Value,
) -> axum::response::Response {
    app.router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/records/{record_id}"))
                .method(Method::PUT)
                .header("cookie", cookie)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .expect("build update request"),
        )
        .await
        .expect("execute update request")
}

async fn delete_record(
    app: &common::TestApp,
    cookie: &str,
    record_id: &str,
) -> axum::response::Response {
    app.router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/records/{record_id}"))
                .method(Method::DELETE)
                .header("cookie", cookie)
                .body(Body::empty())
                .expect("build delete request"),
        )
        .await
        .expect("execute delete request")
}

async fn get_records(app: &common::TestApp, cookie: &str) -> Value {
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/records?limit=100")
                .method(Method::GET)
                .header("cookie", cookie)
                .body(Body::empty())
                .expect("build get records request"),
        )
        .await
        .expect("execute get records request");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read records body");
    serde_json::from_slice(&body).expect("parse records response")
}

fn find_record(records_response: &Value, record_id: &str) -> Value {
    records_response["records"]
        .as_array()
        .expect("records array")
        .iter()
        .find(|record| record["id"].as_str() == Some(record_id))
        .expect("record exists")
        .clone()
}

async fn finalize_record(app: &common::TestApp, cookie: &str, record_id: &str, category_id: &str) {
    let payload = json!({
        "record_id": record_id,
        "category_id": category_id
    });
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/records/finalize-pending")
                .method(Method::POST)
                .header("cookie", cookie)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .expect("build finalize request"),
        )
        .await
        .expect("execute finalize request");
    assert_eq!(response.status(), StatusCode::OK);
}

async fn create_record(
    app: &common::TestApp,
    cookie: &str,
    category_id: &str,
    name: &str,
) -> Value {
    let payload = json!({
        "name": name,
        "amount": 12.5,
        "currency": "TWD",
        "category_id": category_id,
        "date": "2026-03-01"
    });
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/records")
                .method(Method::POST)
                .header("cookie", cookie)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .expect("build create record request"),
        )
        .await
        .expect("execute create record request");
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read create record body");
    serde_json::from_slice(&body).expect("parse create record response")
}

#[tokio::test]
async fn sg1_debtor_cannot_update_pending_split_record() {
    let (app, _alice_cookie, bob_cookie, _alice_category, bob_category, _payer_id, pending_id) =
        create_friend_split("sg1").await;

    let before = find_record(&get_records(&app, &bob_cookie).await, &pending_id);
    let response = update_record(
        &app,
        &bob_cookie,
        &pending_id,
        json!({
            "name": "Tampered",
            "amount": 1.0,
            "category_id": bob_category.id,
            "date": "2026-04-01"
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let after = find_record(&get_records(&app, &bob_cookie).await, &pending_id);
    assert_eq!(after, before);
}

#[tokio::test]
async fn sg2_debtor_cannot_delete_pending_split_record() {
    let (app, _alice_cookie, bob_cookie, _alice_category, _bob_category, _payer_id, pending_id) =
        create_friend_split("sg2").await;

    let response = delete_record(&app, &bob_cookie, &pending_id).await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    find_record(&get_records(&app, &bob_cookie).await, &pending_id);
}

#[tokio::test]
async fn sg3_debtor_cannot_update_or_delete_finalized_split_record() {
    let (app, _alice_cookie, bob_cookie, _alice_category, bob_category, _payer_id, pending_id) =
        create_friend_split("sg3").await;
    finalize_record(&app, &bob_cookie, &pending_id, &bob_category.id).await;

    let update_response = update_record(
        &app,
        &bob_cookie,
        &pending_id,
        json!({ "name": "Tampered" }),
    )
    .await;
    let delete_response = delete_record(&app, &bob_cookie, &pending_id).await;

    assert_eq!(update_response.status(), StatusCode::CONFLICT);
    assert_eq!(delete_response.status(), StatusCode::CONFLICT);
    find_record(&get_records(&app, &bob_cookie).await, &pending_id);
}

#[tokio::test]
async fn sg4_creditor_cannot_update_or_delete_payer_split_record() {
    let (app, alice_cookie, _bob_cookie, _alice_category, _bob_category, payer_id, _pending_id) =
        create_friend_split("sg4").await;

    let update_response = update_record(
        &app,
        &alice_cookie,
        &payer_id,
        json!({ "name": "Tampered payer" }),
    )
    .await;
    let delete_response = delete_record(&app, &alice_cookie, &payer_id).await;

    assert_eq!(update_response.status(), StatusCode::CONFLICT);
    assert_eq!(delete_response.status(), StatusCode::CONFLICT);
    find_record(&get_records(&app, &alice_cookie).await, &payer_id);
}

#[tokio::test]
async fn sg5_normal_record_update_still_succeeds() {
    let app = common::setup_test_app().await.expect("setup failed");
    common::create_test_user(&app.state, "alice_sg5", "password123")
        .await
        .expect("create alice failed");
    let cookie = common::login_user(&app.router, "alice_sg5", "password123")
        .await
        .expect("alice login failed");
    let category = create_category(&app, &cookie, "Groceries", false).await;
    let record = create_record(&app, &cookie, &category.id, "Milk").await;
    let record_id = record["id"].as_str().expect("record id string");

    let response = update_record(&app, &cookie, record_id, json!({ "name": "Eggs" })).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read update body");
    let updated: Value = serde_json::from_slice(&body).expect("parse update body");
    assert_eq!(updated["name"], "Eggs");
}

#[tokio::test]
async fn sg6_normal_record_delete_still_succeeds() {
    let app = common::setup_test_app().await.expect("setup failed");
    common::create_test_user(&app.state, "alice_sg6", "password123")
        .await
        .expect("create alice failed");
    let cookie = common::login_user(&app.router, "alice_sg6", "password123")
        .await
        .expect("alice login failed");
    let category = create_category(&app, &cookie, "Groceries", false).await;
    let record = create_record(&app, &cookie, &category.id, "Milk").await;
    let record_id = record["id"].as_str().expect("record id string");

    let response = delete_record(&app, &cookie, record_id).await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let records = get_records(&app, &cookie).await;
    assert!(
        records["records"]
            .as_array()
            .expect("records array")
            .is_empty()
    );
}
