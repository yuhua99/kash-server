mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use kash_server::models::Category;
use serde_json::{Value, json};
use tower::util::ServiceExt;

async fn create_category(app: &common::TestApp, cookie: &str, name: &str) -> Category {
    let request = Request::builder()
        .uri("/categories")
        .method("POST")
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "name": name, "is_income": false }).to_string(),
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

async fn create_friendship(
    app: &common::TestApp,
    requester_cookie: &str,
    friend_cookie: &str,
    requester_id: &str,
    friend_username: &str,
) {
    send_friend_request(app, requester_cookie, friend_username).await;
    accept_friend_request(app, friend_cookie, requester_id).await;
}

#[allow(clippy::too_many_arguments)]
async fn create_split(
    app: &common::TestApp,
    creditor_cookie: &str,
    debtor_id: &str,
    category_id: &str,
    idempotency_key: &str,
    description: &str,
    date: &str,
    amount: f64,
) -> Value {
    let payload = json!({
        "idempotency_key": idempotency_key,
        "total_amount": 100.0,
        "currency": "TWD",
        "description": description,
        "date": date,
        "category_id": category_id,
        "splits": [{ "user_id": debtor_id, "amount": amount }]
    });

    let request = Request::builder()
        .uri("/splits")
        .method("POST")
        .header("cookie", creditor_cookie)
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

fn first_participant_id(split_response: &Value) -> String {
    split_response["participants"][0]["id"]
        .as_str()
        .expect("participant id should be string")
        .to_string()
}

async fn get_json(app: &common::TestApp, cookie: &str, uri: String) -> (StatusCode, Value) {
    let request = Request::builder()
        .uri(uri)
        .method("GET")
        .header("cookie", cookie)
        .body(Body::empty())
        .expect("build get request");

    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute get request");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read get body");
    let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, value)
}

async fn finalize_share(
    app: &common::TestApp,
    cookie: &str,
    participant_id: &str,
    category_id: &str,
) {
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
    assert_eq!(response.status(), StatusCode::OK);
}

async fn settle_share(app: &common::TestApp, cookie: &str, participant_id: &str) {
    let request = Request::builder()
        .uri(format!("/splits/participants/{participant_id}/settle"))
        .method("PUT")
        .header("cookie", cookie)
        .body(Body::empty())
        .expect("build settle request");

    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute settle request");
    assert_eq!(response.status(), StatusCode::OK);
}

async fn settle_all_with_friend(app: &common::TestApp, cookie: &str, friend_id: &str) -> Value {
    let request = Request::builder()
        .uri(format!("/splits/with/{friend_id}/settle-all"))
        .method("PUT")
        .header("cookie", cookie)
        .body(Body::empty())
        .expect("build settle-all request");

    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute settle-all request");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read settle-all body");
    serde_json::from_slice(&body).expect("parse settle-all body")
}

#[tokio::test]
async fn split_pending_list_returns_caller_shares_and_finalize_removes_one() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice_sl1", "password123")
        .await
        .expect("create alice failed");
    let bob_id = common::create_test_user(&app.state, "bob_sl1", "password123")
        .await
        .expect("create bob failed");
    let charlie_id = common::create_test_user(&app.state, "charlie_sl1", "password123")
        .await
        .expect("create charlie failed");
    let dana_id = common::create_test_user(&app.state, "dana_sl1", "password123")
        .await
        .expect("create dana failed");

    let alice_cookie = common::login_user(&app.router, "alice_sl1", "password123")
        .await
        .expect("alice login failed");
    let bob_cookie = common::login_user(&app.router, "bob_sl1", "password123")
        .await
        .expect("bob login failed");
    let charlie_cookie = common::login_user(&app.router, "charlie_sl1", "password123")
        .await
        .expect("charlie login failed");
    let dana_cookie = common::login_user(&app.router, "dana_sl1", "password123")
        .await
        .expect("dana login failed");

    create_friendship(&app, &alice_cookie, &bob_cookie, &alice_id, "bob_sl1").await;
    create_friendship(&app, &charlie_cookie, &dana_cookie, &charlie_id, "dana_sl1").await;

    let alice_category = create_category(&app, &alice_cookie, "Dining sl1").await;
    let bob_category = create_category(&app, &bob_cookie, "Shared sl1").await;
    let charlie_category = create_category(&app, &charlie_cookie, "Other sl1").await;

    let older = create_split(
        &app,
        &alice_cookie,
        &bob_id,
        &alice_category.id,
        "split-list-pending-older",
        "Older dinner",
        "2026-03-01",
        25.0,
    )
    .await;
    create_split(
        &app,
        &alice_cookie,
        &bob_id,
        &alice_category.id,
        "split-list-pending-middle",
        "Middle dinner",
        "2026-03-02",
        35.0,
    )
    .await;
    create_split(
        &app,
        &alice_cookie,
        &bob_id,
        &alice_category.id,
        "split-list-pending-newest",
        "Newest dinner",
        "2026-03-03",
        45.0,
    )
    .await;
    create_split(
        &app,
        &charlie_cookie,
        &dana_id,
        &charlie_category.id,
        "split-list-pending-tenant",
        "Tenant dinner",
        "2026-03-04",
        55.0,
    )
    .await;

    let (status, page) = get_json(
        &app,
        &bob_cookie,
        "/splits/pending?limit=1&offset=1".to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["total_count"], 3);
    assert_eq!(page["limit"], 1);
    assert_eq!(page["offset"], 1);
    let page_shares = page["shares"].as_array().expect("shares should be array");
    assert_eq!(page_shares.len(), 1);
    assert_eq!(page_shares[0]["description"], "Middle dinner");
    assert_eq!(page_shares[0]["date"], "2026-03-02");

    let (status, list) = get_json(
        &app,
        &bob_cookie,
        "/splits/pending?limit=1000&offset=0".to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let shares = list["shares"].as_array().expect("shares should be array");
    assert_eq!(shares.len(), 3);
    assert!(
        shares
            .iter()
            .all(|share| share["creditor_user_id"] == alice_id)
    );
    assert!(
        shares
            .iter()
            .all(|share| share["creditor_name"] == "alice_sl1")
    );
    assert!(shares.iter().all(|share| share["currency"] == "TWD"));
    assert!(shares.iter().all(|share| share["settled"] == false));
    assert!(
        shares
            .iter()
            .any(|share| { share["description"] == "Newest dinner" && share["amount"] == 45.0 })
    );
    assert!(
        !shares
            .iter()
            .any(|share| share["description"] == "Tenant dinner")
    );

    finalize_share(
        &app,
        &bob_cookie,
        &first_participant_id(&older),
        &bob_category.id,
    )
    .await;

    let (status, after_finalize) = get_json(
        &app,
        &bob_cookie,
        "/splits/pending?limit=1000&offset=0".to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let after_shares = after_finalize["shares"]
        .as_array()
        .expect("shares should be array");
    assert_eq!(after_finalize["total_count"], 2);
    assert_eq!(after_shares.len(), 2);
    assert!(
        !after_shares
            .iter()
            .any(|share| share["description"] == "Older dinner")
    );
}

#[tokio::test]
async fn split_unsettled_list_returns_both_directions_and_settle_removes_one() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice_sl2", "password123")
        .await
        .expect("create alice failed");
    let bob_id = common::create_test_user(&app.state, "bob_sl2", "password123")
        .await
        .expect("create bob failed");
    let alice_cookie = common::login_user(&app.router, "alice_sl2", "password123")
        .await
        .expect("alice login failed");
    let bob_cookie = common::login_user(&app.router, "bob_sl2", "password123")
        .await
        .expect("bob login failed");

    create_friendship(&app, &alice_cookie, &bob_cookie, &alice_id, "bob_sl2").await;
    let alice_category = create_category(&app, &alice_cookie, "Dining sl2").await;
    let bob_category = create_category(&app, &bob_cookie, "Shared sl2").await;

    let bob_owes = create_split(
        &app,
        &alice_cookie,
        &bob_id,
        &alice_category.id,
        "split-list-unsettled-bob-owes",
        "Alice paid",
        "2026-04-01",
        40.0,
    )
    .await;
    let bob_owes_participant_id = first_participant_id(&bob_owes);
    finalize_share(
        &app,
        &bob_cookie,
        &bob_owes_participant_id,
        &bob_category.id,
    )
    .await;

    let alice_owes = create_split(
        &app,
        &bob_cookie,
        &alice_id,
        &bob_category.id,
        "split-list-unsettled-alice-owes",
        "Bob paid",
        "2026-04-02",
        30.0,
    )
    .await;
    let alice_owes_participant_id = first_participant_id(&alice_owes);
    finalize_share(
        &app,
        &alice_cookie,
        &alice_owes_participant_id,
        &alice_category.id,
    )
    .await;

    let (self_status, _) = get_json(
        &app,
        &alice_cookie,
        format!("/splits/unsettled?friend_id={alice_id}&limit=1000&offset=0"),
    )
    .await;
    assert_eq!(self_status, StatusCode::BAD_REQUEST);

    let (status, list) = get_json(
        &app,
        &alice_cookie,
        format!("/splits/unsettled?friend_id={bob_id}&limit=1000&offset=0"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["total_count"], 2);
    assert_eq!(list["limit"], 1000);
    assert_eq!(list["offset"], 0);
    let shares = list["shares"].as_array().expect("shares should be array");
    assert_eq!(shares.len(), 2);

    let they_owe_you = shares
        .iter()
        .find(|share| share["participant_id"] == bob_owes_participant_id)
        .expect("alice-creditor share should appear");
    assert_eq!(they_owe_you["direction"], "they_owe_you");
    assert_eq!(they_owe_you["counterparty_user_id"], bob_id);
    assert_eq!(they_owe_you["counterparty_name"], "bob_sl2");
    assert_eq!(they_owe_you["description"], "Alice paid");
    assert_eq!(they_owe_you["amount"], 40.0);
    assert_eq!(they_owe_you["finalized"], true);
    assert_eq!(they_owe_you["settled"], false);

    let you_owe = shares
        .iter()
        .find(|share| share["participant_id"] == alice_owes_participant_id)
        .expect("alice-debtor share should appear");
    assert_eq!(you_owe["direction"], "you_owe");
    assert_eq!(you_owe["counterparty_user_id"], bob_id);
    assert_eq!(you_owe["counterparty_name"], "bob_sl2");
    assert_eq!(you_owe["description"], "Bob paid");
    assert_eq!(you_owe["amount"], 30.0);
    assert_eq!(you_owe["finalized"], true);
    assert_eq!(you_owe["settled"], false);

    settle_share(&app, &alice_cookie, &alice_owes_participant_id).await;

    let (status, after_settle) = get_json(
        &app,
        &alice_cookie,
        format!("/splits/unsettled?friend_id={bob_id}&limit=1000&offset=0"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let remaining = after_settle["shares"]
        .as_array()
        .expect("shares should be array");
    assert_eq!(after_settle["total_count"], 1);
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0]["participant_id"], bob_owes_participant_id);
}

#[tokio::test]
async fn split_settle_all_updates_both_directions_once_and_rejects_self() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice_sl3", "password123")
        .await
        .expect("create alice failed");
    let bob_id = common::create_test_user(&app.state, "bob_sl3", "password123")
        .await
        .expect("create bob failed");
    let alice_cookie = common::login_user(&app.router, "alice_sl3", "password123")
        .await
        .expect("alice login failed");
    let bob_cookie = common::login_user(&app.router, "bob_sl3", "password123")
        .await
        .expect("bob login failed");

    create_friendship(&app, &alice_cookie, &bob_cookie, &alice_id, "bob_sl3").await;
    let alice_category = create_category(&app, &alice_cookie, "Dining sl3").await;
    let bob_category = create_category(&app, &bob_cookie, "Shared sl3").await;

    let bob_owes = create_split(
        &app,
        &alice_cookie,
        &bob_id,
        &alice_category.id,
        "split-list-settle-all-bob-owes",
        "Alice paid settle all",
        "2026-05-01",
        20.0,
    )
    .await;
    let bob_owes_participant_id = first_participant_id(&bob_owes);
    finalize_share(
        &app,
        &bob_cookie,
        &bob_owes_participant_id,
        &bob_category.id,
    )
    .await;

    let alice_owes = create_split(
        &app,
        &bob_cookie,
        &alice_id,
        &bob_category.id,
        "split-list-settle-all-alice-owes",
        "Bob paid settle all",
        "2026-05-02",
        60.0,
    )
    .await;
    let alice_owes_participant_id = first_participant_id(&alice_owes);
    finalize_share(
        &app,
        &alice_cookie,
        &alice_owes_participant_id,
        &alice_category.id,
    )
    .await;

    let (self_status, _) = get_json(
        &app,
        &alice_cookie,
        format!("/splits/unsettled?friend_id={alice_id}&limit=1000&offset=0"),
    )
    .await;
    assert_eq!(self_status, StatusCode::BAD_REQUEST);

    let self_settle_request = Request::builder()
        .uri(format!("/splits/with/{alice_id}/settle-all"))
        .method("PUT")
        .header("cookie", &alice_cookie)
        .body(Body::empty())
        .expect("build self settle-all request");
    let self_settle_response = app
        .router
        .clone()
        .oneshot(self_settle_request)
        .await
        .expect("execute self settle-all request");
    assert_eq!(self_settle_response.status(), StatusCode::BAD_REQUEST);

    let first = settle_all_with_friend(&app, &alice_cookie, &bob_id).await;
    assert_eq!(first["updated_count"], 2);

    let (status, after_first) = get_json(
        &app,
        &alice_cookie,
        format!("/splits/unsettled?friend_id={bob_id}&limit=1000&offset=0"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(after_first["total_count"], 0);
    assert_eq!(
        after_first["shares"]
            .as_array()
            .expect("shares should be array")
            .len(),
        0
    );

    let second = settle_all_with_friend(&app, &alice_cookie, &bob_id).await;
    assert_eq!(second["updated_count"], 0);
}
