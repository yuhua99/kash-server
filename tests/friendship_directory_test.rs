mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use kash_server::models::FriendshipRelation;
use serde_json::json;
use tower::util::ServiceExt;

#[tokio::test]
async fn test_search_users_happy_path() {
    let app = common::setup_test_app().await.expect("setup failed");

    // Create multiple users with various usernames
    common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    common::create_test_user(&app.state, "alice_smith", "password123")
        .await
        .expect("create alice_smith failed");
    common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");
    common::create_test_user(&app.state, "charlie", "password123")
        .await
        .expect("create charlie failed");

    let cookie = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");

    // Search for users starting with "ali"
    let request = Request::builder()
        .uri("/friends/search?query=ali")
        .method("GET")
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap();

    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let results: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();

    // Should find both alice and alice_smith
    assert_eq!(results.len(), 2);

    // Verify no password hash in results
    for user in results {
        assert!(user.get("id").is_some());
        assert!(user.get("username").is_some());
        assert!(user.get("password_hash").is_none());
    }
}

#[tokio::test]
async fn test_search_users_query_too_short() {
    let app = common::setup_test_app().await.expect("setup failed");

    common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");

    let cookie = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");

    // Try to search with query shorter than 3 characters
    let request = Request::builder()
        .uri("/friends/search?query=ab")
        .method("GET")
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap();

    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error_msg = String::from_utf8(body.to_vec()).unwrap();
    assert!(error_msg.contains("at least") || error_msg.contains("minimum"));
}

#[tokio::test]
async fn test_search_users_pagination() {
    let app = common::setup_test_app().await.expect("setup failed");

    // Create users with similar prefix
    for i in 1..=5 {
        common::create_test_user(&app.state, &format!("user{}", i), "password123")
            .await
            .expect("create user failed");
    }

    let cookie = common::login_user(&app.router, "user1", "password123")
        .await
        .expect("user1 login failed");

    // Search with limit
    let request = Request::builder()
        .uri("/friends/search?query=user&limit=3&offset=0")
        .method("GET")
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap();

    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let results: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();

    // Should return exactly 3 results
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_nickname_isolation_happy_path() {
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
    let cookie_b = common::login_user(&app.router, "bob", "password123")
        .await
        .expect("bob login failed");

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

    let accept_request = Request::builder()
        .uri("/friends/accept")
        .method("POST")
        .header("cookie", cookie_b.clone())
        .header("content-type", "application/json")
        .body(Body::from(json!({"friend_id": user_a_id}).to_string()))
        .unwrap();
    let accept_response = app.router.clone().oneshot(accept_request).await.unwrap();
    assert_eq!(accept_response.status(), StatusCode::OK);

    let nickname_payload = json!({
        "friend_id": user_b_id,
        "nickname": "Gym buddy"
    });
    let request = Request::builder()
        .uri("/friends/nickname")
        .method("PATCH")
        .header("cookie", cookie_a.clone())
        .header("content-type", "application/json")
        .body(Body::from(nickname_payload.to_string()))
        .unwrap();

    let response = app.router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    if status != StatusCode::OK {
        let error_msg = String::from_utf8(body.to_vec()).unwrap();
        panic!(
            "Failed to update nickname: status={}, error={}",
            status, error_msg
        );
    }

    let relation: FriendshipRelation = serde_json::from_slice(&body).unwrap();
    assert_eq!(relation.nickname, "Gym buddy", "Alice should see nickname");

    let request = Request::builder()
        .uri("/friends/list?pending=false")
        .method("GET")
        .header("cookie", cookie_a.clone())
        .body(Body::empty())
        .unwrap();

    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let list_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let friends = list_response["friends"]
        .as_array()
        .expect("friends should be array");
    assert_eq!(friends.len(), 1);
    assert_eq!(friends[0]["nickname"], "Gym buddy");

    let request = Request::builder()
        .uri("/friends/list?pending=false")
        .method("GET")
        .header("cookie", cookie_b)
        .body(Body::empty())
        .unwrap();

    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let list_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let friends = list_response["friends"]
        .as_array()
        .expect("friends should be array");
    assert_eq!(friends.len(), 1);
    assert_eq!(
        friends[0]["nickname"], "alice",
        "Bob should see Alice's username as default nickname"
    );
}

#[tokio::test]
async fn test_nickname_oversize_error() {
    let app = common::setup_test_app().await.expect("setup failed");

    let _user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    let user_b_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");

    let cookie_a = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");

    // Send friend request
    let payload = json!({"friend_username": "bob"});
    let request = Request::builder()
        .uri("/friends/request")
        .method("POST")
        .header("cookie", cookie_a.clone())
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let _ = app.router.clone().oneshot(request).await.unwrap();

    // Try to set nickname > 100 chars
    let long_nickname = "a".repeat(101);
    let nickname_payload = json!({
        "friend_id": user_b_id,
        "nickname": long_nickname
    });
    let request = Request::builder()
        .uri("/friends/nickname")
        .method("PATCH")
        .header("cookie", cookie_a)
        .header("content-type", "application/json")
        .body(Body::from(nickname_payload.to_string()))
        .unwrap();

    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Should reject oversized nickname"
    );
}

#[tokio::test]
async fn test_list_friends_with_status_filter() {
    let app = common::setup_test_app().await.expect("setup failed");

    let user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    let user_b_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");
    let _user_c_id = common::create_test_user(&app.state, "charlie", "password123")
        .await
        .expect("create charlie failed");

    let cookie_a = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");
    let cookie_b = common::login_user(&app.router, "bob", "password123")
        .await
        .expect("bob login failed");
    let cookie_c = common::login_user(&app.router, "charlie", "password123")
        .await
        .expect("charlie login failed");

    // Alice sends request to Bob
    let request = Request::builder()
        .uri("/friends/request")
        .method("POST")
        .header("cookie", cookie_a.clone())
        .header("content-type", "application/json")
        .body(Body::from(json!({"friend_username": "bob"}).to_string()))
        .unwrap();
    let _ = app.router.clone().oneshot(request).await.unwrap();

    // Charlie sends request to Alice
    let request = Request::builder()
        .uri("/friends/request")
        .method("POST")
        .header("cookie", cookie_c.clone())
        .header("content-type", "application/json")
        .body(Body::from(json!({"friend_username": "alice"}).to_string()))
        .unwrap();
    let _ = app.router.clone().oneshot(request).await.unwrap();

    // Bob accepts Alice's request
    let accept_request = Request::builder()
        .uri("/friends/accept")
        .method("POST")
        .header("cookie", cookie_b.clone())
        .header("content-type", "application/json")
        .body(Body::from(json!({"friend_id": user_a_id}).to_string()))
        .unwrap();
    let accept_response = app.router.clone().oneshot(accept_request).await.unwrap();
    assert_eq!(accept_response.status(), StatusCode::OK);

    // Alice: pending=false (default) → 1 accepted friend (Bob)
    let request = Request::builder()
        .uri("/friends/list?pending=false")
        .method("GET")
        .header("cookie", cookie_a.clone())
        .body(Body::empty())
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let list_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let friends = list_response["friends"]
        .as_array()
        .expect("friends should be array");
    assert_eq!(friends.len(), 1, "Alice should have 1 accepted friend");
    assert_eq!(friends[0]["user_id"], user_b_id);

    // Alice: pending=true → only incoming requests (Charlie sent to Alice) → 1
    let request = Request::builder()
        .uri("/friends/list?pending=true")
        .method("GET")
        .header("cookie", cookie_a.clone())
        .body(Body::empty())
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let list_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let friends = list_response["friends"]
        .as_array()
        .expect("friends should be array");
    assert_eq!(
        friends.len(),
        1,
        "Alice should see 1 incoming request (from Charlie)"
    );
    assert_eq!(friends[0]["user_id"], _user_c_id);

    // Bob: pending=true → Bob sent no requests, and Alice's request to Bob is accepted, so 0 incoming
    let request = Request::builder()
        .uri("/friends/list?pending=true")
        .method("GET")
        .header("cookie", cookie_b)
        .body(Body::empty())
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let list_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let friends = list_response["friends"]
        .as_array()
        .expect("friends should be array");
    assert_eq!(friends.len(), 0, "Bob should see 0 incoming requests");
}

#[tokio::test]
async fn test_list_friends_pagination() {
    let app = common::setup_test_app().await.expect("setup failed");

    let user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");

    for i in 0..10 {
        let username = format!("friend{}", i);
        common::create_test_user(&app.state, &username, "password123")
            .await
            .expect("create friend failed");
    }

    let cookie_a = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");

    // Alice sends requests to all 10 friends
    for i in 0..10 {
        let username = format!("friend{}", i);
        let payload = json!({"friend_username": username});
        let request = Request::builder()
            .uri("/friends/request")
            .method("POST")
            .header("cookie", cookie_a.clone())
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap();
        let _ = app.router.clone().oneshot(request).await.unwrap();
    }

    // All 10 friends accept Alice's request
    for i in 0..10 {
        let username = format!("friend{}", i);
        let cookie = common::login_user(&app.router, &username, "password123")
            .await
            .expect("friend login failed");
        let accept_payload = json!({"friend_id": user_a_id});
        let request = Request::builder()
            .uri("/friends/accept")
            .method("POST")
            .header("cookie", cookie)
            .header("content-type", "application/json")
            .body(Body::from(accept_payload.to_string()))
            .unwrap();
        let response = app.router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    // Page 1: 5 of 10 accepted friends
    let request = Request::builder()
        .uri("/friends/list?limit=5&offset=0")
        .method("GET")
        .header("cookie", cookie_a.clone())
        .body(Body::empty())
        .unwrap();

    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let list_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let friends = list_response["friends"]
        .as_array()
        .expect("friends should be array");
    let total_count = list_response["total_count"]
        .as_u64()
        .expect("total_count should exist");

    assert_eq!(friends.len(), 5, "Should return exactly 5 friends");
    assert_eq!(total_count, 10, "Total count should be 10");

    // Page 2: next 5
    let request = Request::builder()
        .uri("/friends/list?limit=5&offset=5")
        .method("GET")
        .header("cookie", cookie_a)
        .body(Body::empty())
        .unwrap();

    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let list_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let friends = list_response["friends"]
        .as_array()
        .expect("friends should be array");

    assert_eq!(friends.len(), 5, "Should return next 5 friends");
}
