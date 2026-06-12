mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use kash_server::models::FriendshipRelation;
use serde_json::json;
use tower::util::ServiceExt;

#[tokio::test]
async fn test_send_friend_request_happy_path() {
    let app = common::setup_test_app().await.expect("setup failed");

    // Create two users
    let _user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    let user_b_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");

    // Login as alice
    let cookie_a = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");

    // Alice sends friend request to Bob
    let payload = json!({"friend_username": "bob"});
    let request = Request::builder()
        .uri("/friends/request")
        .method("POST")
        .header("cookie", cookie_a)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // Parse response
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let relation: FriendshipRelation = serde_json::from_slice(&body).unwrap();

    // Verify response structure
    assert_eq!(relation.user_id, user_b_id);
    assert!(relation.pending);
    assert_eq!(
        relation.nickname, "bob",
        "default nickname should be the friend's username"
    );

    // Verify a single canonical row exists in database
    let conn = app.state.main_db.connect().expect("connect db");
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM friendship WHERE user_low_id = ? OR user_high_id = ?",
            (_user_a_id.as_str(), _user_a_id.as_str()),
        )
        .await
        .unwrap();

    if let Some(row) = rows.next().await.unwrap() {
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 1, "Expected one friendship row for the pair");
    }
}

#[tokio::test]
async fn test_send_friend_request_duplicate_error() {
    let app = common::setup_test_app().await.expect("setup failed");

    let _user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    let _user_b_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");

    let cookie_a = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");

    // Send first request
    let payload = json!({"friend_username": "bob"});
    let request1 = Request::builder()
        .uri("/friends/request")
        .method("POST")
        .header("cookie", cookie_a.clone())
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let response1 = app.router.clone().oneshot(request1).await.unwrap();
    assert_eq!(response1.status(), StatusCode::CREATED);

    // Send duplicate request
    let request2 = Request::builder()
        .uri("/friends/request")
        .method("POST")
        .header("cookie", cookie_a)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let response2 = app.router.clone().oneshot(request2).await.unwrap();
    assert_eq!(response2.status(), StatusCode::CONFLICT);

    let body = axum::body::to_bytes(response2.into_body(), usize::MAX)
        .await
        .unwrap();
    let error_msg = String::from_utf8(body.to_vec()).unwrap();
    assert!(error_msg.contains("already exists") || error_msg.contains("duplicate"));
}

#[tokio::test]
async fn test_send_friend_request_self_error() {
    let app = common::setup_test_app().await.expect("setup failed");

    let _user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");

    let cookie_a = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");

    // Try to send friend request to self
    let payload = json!({"friend_username": "alice"});
    let request = Request::builder()
        .uri("/friends/request")
        .method("POST")
        .header("cookie", cookie_a)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error_msg = String::from_utf8(body.to_vec()).unwrap();
    assert!(error_msg.contains("self") || error_msg.contains("yourself"));
}

#[tokio::test]
async fn test_send_friend_request_user_not_found() {
    let app = common::setup_test_app().await.expect("setup failed");

    let _user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");

    let cookie_a = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");

    // Try to send friend request to non-existent user
    let payload = json!({"friend_username": "nonexistent"});
    let request = Request::builder()
        .uri("/friends/request")
        .method("POST")
        .header("cookie", cookie_a)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error_msg = String::from_utf8(body.to_vec()).unwrap();
    assert!(error_msg.contains("not found") || error_msg.contains("does not exist"));
}

#[tokio::test]
async fn test_accept_friend_happy_path() {
    let app = common::setup_test_app().await.expect("setup failed");

    let user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    let user_b_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");

    let cookie_a = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");

    let payload = json!({"friend_username": "bob"});
    let request = Request::builder()
        .uri("/friends/request")
        .method("POST")
        .header("cookie", cookie_a)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let cookie_b = common::login_user(&app.router, "bob", "password123")
        .await
        .expect("bob login failed");

    let accept_payload = json!({"friend_id": user_a_id});
    let accept_request = Request::builder()
        .uri("/friends/accept")
        .method("POST")
        .header("cookie", cookie_b)
        .header("content-type", "application/json")
        .body(Body::from(accept_payload.to_string()))
        .unwrap();

    let accept_response = app.router.clone().oneshot(accept_request).await.unwrap();
    assert_eq!(accept_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(accept_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let relation: FriendshipRelation = serde_json::from_slice(&body).unwrap();

    assert_eq!(relation.user_id, user_a_id);
    assert!(!relation.pending);

    let conn = app.state.main_db.connect().expect("connect db");

    let mut rows = conn
        .query(
            "SELECT pending FROM friendship WHERE user_low_id = MIN(?1, ?2) AND user_high_id = MAX(?1, ?2)",
            (user_a_id.as_str(), user_b_id.as_str()),
        )
        .await
        .unwrap();

    if let Some(row) = rows.next().await.unwrap() {
        let pending: i64 = row.get(0).unwrap();
        assert_eq!(
            pending, 0i64,
            "friendship row should be accepted (pending=0)"
        );
    } else {
        panic!("friendship row not found");
    }
}

#[tokio::test]
async fn test_accept_friend_unauthorized() {
    let app = common::setup_test_app().await.expect("setup failed");

    let user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");
    common::create_test_user(&app.state, "charlie", "password123")
        .await
        .expect("create charlie failed");

    let cookie_a = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");

    let payload = json!({"friend_username": "bob"});
    let request = Request::builder()
        .uri("/friends/request")
        .method("POST")
        .header("cookie", cookie_a)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let cookie_c = common::login_user(&app.router, "charlie", "password123")
        .await
        .expect("charlie login failed");

    let accept_payload = json!({"friend_id": user_a_id});
    let accept_request = Request::builder()
        .uri("/friends/accept")
        .method("POST")
        .header("cookie", cookie_c)
        .header("content-type", "application/json")
        .body(Body::from(accept_payload.to_string()))
        .unwrap();

    let accept_response = app.router.clone().oneshot(accept_request).await.unwrap();
    assert_eq!(accept_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_accept_friend_requester_cannot_accept() {
    let app = common::setup_test_app().await.expect("setup failed");

    common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    let user_b_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");

    let cookie_a = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");

    let payload = json!({"friend_username": "bob"});
    let request = Request::builder()
        .uri("/friends/request")
        .method("POST")
        .header("cookie", cookie_a.clone())
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let accept_payload = json!({"friend_id": user_b_id});
    let accept_request = Request::builder()
        .uri("/friends/accept")
        .method("POST")
        .header("cookie", cookie_a)
        .header("content-type", "application/json")
        .body(Body::from(accept_payload.to_string()))
        .unwrap();

    let accept_response = app.router.clone().oneshot(accept_request).await.unwrap();
    assert_eq!(accept_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_remove_friend_happy_path() {
    let app = common::setup_test_app().await.expect("setup failed");

    let user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    let user_b_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");

    let cookie_a = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");

    let payload = json!({"friend_username": "bob"});
    let request = Request::builder()
        .uri("/friends/request")
        .method("POST")
        .header("cookie", cookie_a.clone())
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let cookie_b = common::login_user(&app.router, "bob", "password123")
        .await
        .expect("bob login failed");

    let accept_payload = json!({"friend_id": user_a_id});
    let accept_request = Request::builder()
        .uri("/friends/accept")
        .method("POST")
        .header("cookie", cookie_b)
        .header("content-type", "application/json")
        .body(Body::from(accept_payload.to_string()))
        .unwrap();

    let accept_response = app.router.clone().oneshot(accept_request).await.unwrap();
    assert_eq!(accept_response.status(), StatusCode::OK);

    let remove_payload = json!({"friend_id": user_b_id});
    let remove_request = Request::builder()
        .uri("/friends/remove")
        .method("POST")
        .header("cookie", cookie_a)
        .header("content-type", "application/json")
        .body(Body::from(remove_payload.to_string()))
        .unwrap();

    let remove_response = app.router.clone().oneshot(remove_request).await.unwrap();
    assert_eq!(remove_response.status(), StatusCode::OK);

    let conn = app.state.main_db.connect().expect("connect db");
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM friendship WHERE user_low_id = MIN(?1, ?2) AND user_high_id = MAX(?1, ?2)",
            (user_a_id.as_str(), user_b_id.as_str()),
        )
        .await
        .unwrap();

    if let Some(row) = rows.next().await.unwrap() {
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 0, "Friendship row should be deleted");
    }
}

#[tokio::test]
async fn test_remove_friend_either_party_can_initiate() {
    let app = common::setup_test_app().await.expect("setup failed");

    let user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");

    let cookie_a = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");

    let payload = json!({"friend_username": "bob"});
    let request = Request::builder()
        .uri("/friends/request")
        .method("POST")
        .header("cookie", cookie_a)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let cookie_b = common::login_user(&app.router, "bob", "password123")
        .await
        .expect("bob login failed");

    let accept_payload = json!({"friend_id": user_a_id});
    let accept_request = Request::builder()
        .uri("/friends/accept")
        .method("POST")
        .header("cookie", cookie_b.clone())
        .header("content-type", "application/json")
        .body(Body::from(accept_payload.to_string()))
        .unwrap();

    let accept_response = app.router.clone().oneshot(accept_request).await.unwrap();
    assert_eq!(accept_response.status(), StatusCode::OK);

    let remove_payload = json!({"friend_id": user_a_id});
    let remove_request = Request::builder()
        .uri("/friends/remove")
        .method("POST")
        .header("cookie", cookie_b)
        .header("content-type", "application/json")
        .body(Body::from(remove_payload.to_string()))
        .unwrap();

    let remove_response = app.router.clone().oneshot(remove_request).await.unwrap();
    assert_eq!(remove_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_remove_then_accept_returns_not_found() {
    let app = common::setup_test_app().await.expect("setup failed");

    let user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");

    let cookie_a = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");

    let payload = json!({"friend_username": "bob"});
    let request = Request::builder()
        .uri("/friends/request")
        .method("POST")
        .header("cookie", cookie_a.clone())
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    app.router.clone().oneshot(request).await.unwrap();

    let cookie_b = common::login_user(&app.router, "bob", "password123")
        .await
        .expect("bob login failed");

    let accept_payload = json!({"friend_id": user_a_id});
    let accept_request = Request::builder()
        .uri("/friends/accept")
        .method("POST")
        .header("cookie", cookie_b.clone())
        .header("content-type", "application/json")
        .body(Body::from(accept_payload.to_string()))
        .unwrap();

    app.router.clone().oneshot(accept_request).await.unwrap();

    let remove_payload = json!({"friend_id": user_a_id});
    let remove_request = Request::builder()
        .uri("/friends/remove")
        .method("POST")
        .header("cookie", cookie_b.clone())
        .header("content-type", "application/json")
        .body(Body::from(remove_payload.to_string()))
        .unwrap();

    let remove_response = app.router.clone().oneshot(remove_request).await.unwrap();
    assert_eq!(remove_response.status(), StatusCode::OK);

    let reaccept_request = Request::builder()
        .uri("/friends/accept")
        .method("POST")
        .header("cookie", cookie_b)
        .header("content-type", "application/json")
        .body(Body::from(accept_payload.to_string()))
        .unwrap();

    let reaccept_response = app.router.clone().oneshot(reaccept_request).await.unwrap();
    assert_eq!(reaccept_response.status(), StatusCode::NOT_FOUND);
}
