mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use my_budget_server::models::FriendshipRelation;
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
    assert_eq!(relation.status, "pending");
    assert!(relation.nickname.is_none());

    // Verify both directed rows exist in database
    let conn = app.state.main_db.read().await;
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM friendship_relations WHERE from_user_id = ? OR to_user_id = ?",
            (_user_a_id.as_str(), _user_a_id.as_str()),
        )
        .await
        .unwrap();

    if let Some(row) = rows.next().await.unwrap() {
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 2, "Expected two directed rows (A->B and B->A)");
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

    // Create User A and User B
    let user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    let user_b_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");

    // Login both users
    let cookie_a = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");
    let cookie_b = common::login_user(&app.router, "bob", "password123")
        .await
        .expect("bob login failed");

    // Alice sends friend request to Bob
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

    // Accept friend request (Bob accepts Alice's request)
    // Insert acceptance logic here when accept endpoint exists
    // For now, manually update database to accepted status
    {
        let conn = app.state.main_db.write().await;
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();

        conn.execute(
            "UPDATE friendship_relations SET status = 'accepted', updated_at = ? WHERE from_user_id = ? AND to_user_id = ?",
            (now.as_str(), user_a_id.as_str(), user_b_id.as_str()),
        )
        .await
        .unwrap();

        conn.execute(
            "UPDATE friendship_relations SET status = 'accepted', updated_at = ? WHERE from_user_id = ? AND to_user_id = ?",
            (now.as_str(), user_b_id.as_str(), user_a_id.as_str()),
        )
        .await
        .unwrap();
    }

    // Alice sets nickname for Bob to "Gym buddy"
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
    assert_eq!(
        relation.nickname,
        Some("Gym buddy".to_string()),
        "Alice should see nickname"
    );

    // Alice lists friends and sees the nickname
    let request = Request::builder()
        .uri("/friends/list?status=accepted")
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

    // Bob lists friends and should NOT see a nickname (only Alice's view has it)
    let request = Request::builder()
        .uri("/friends/list?status=accepted")
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
    assert!(
        friends[0]["nickname"].is_null(),
        "Bob should not see nickname"
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

    // Create 3 users
    let user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    let user_b_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");
    let user_c_id = common::create_test_user(&app.state, "charlie", "password123")
        .await
        .expect("create charlie failed");

    let cookie_a = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");

    // Alice sends requests to both Bob and Charlie
    let payload_b = json!({"friend_username": "bob"});
    let request = Request::builder()
        .uri("/friends/request")
        .method("POST")
        .header("cookie", cookie_a.clone())
        .header("content-type", "application/json")
        .body(Body::from(payload_b.to_string()))
        .unwrap();
    let _ = app.router.clone().oneshot(request).await.unwrap();

    let payload_c = json!({"friend_username": "charlie"});
    let request = Request::builder()
        .uri("/friends/request")
        .method("POST")
        .header("cookie", cookie_a.clone())
        .header("content-type", "application/json")
        .body(Body::from(payload_c.to_string()))
        .unwrap();
    let _ = app.router.clone().oneshot(request).await.unwrap();

    // Accept only Bob's request
    {
        let conn = app.state.main_db.write().await;
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();

        conn.execute(
            "UPDATE friendship_relations SET status = 'accepted', updated_at = ? WHERE from_user_id = ? AND to_user_id = ?",
            (now.as_str(), user_a_id.as_str(), user_b_id.as_str()),
        )
        .await
        .unwrap();
    }

    // Query with status=accepted should return only Bob
    let request = Request::builder()
        .uri("/friends/list?status=accepted")
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

    assert_eq!(friends.len(), 1, "Should have 1 accepted friend");
    assert_eq!(friends[0]["user_id"], user_b_id);

    // Query with status=pending should return only Charlie
    let request = Request::builder()
        .uri("/friends/list?status=pending")
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

    assert_eq!(friends.len(), 1, "Should have 1 pending friend");
    assert_eq!(friends[0]["user_id"], user_c_id);
}

#[tokio::test]
async fn test_list_friends_pagination() {
    let app = common::setup_test_app().await.expect("setup failed");

    let _user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");

    // Create 10 friends for Alice
    for i in 0..10 {
        let username = format!("friend{}", i);
        common::create_test_user(&app.state, &username, "password123")
            .await
            .expect("create friend failed");
    }

    let cookie_a = common::login_user(&app.router, "alice", "password123")
        .await
        .expect("alice login failed");

    // Send requests to all 10 users
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

    // Query with limit=5, offset=0
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

    // Query with limit=5, offset=5
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

// ===== TASK 7: ACCEPT FRIEND TESTS =====

#[tokio::test]
async fn test_accept_friend_happy_path() {
    let app = common::setup_test_app().await.expect("setup failed");

    // Create two users
    let user_a_id = common::create_test_user(&app.state, "alice", "password123")
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

    // Login as Bob
    let cookie_b = common::login_user(&app.router, "bob", "password123")
        .await
        .expect("bob login failed");

    // Bob accepts the friend request
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

    // Verify response

    // Verify response - Bob's view should show Alice as accepted
    assert_eq!(relation.user_id, user_a_id);
    assert_eq!(relation.status, "accepted");

    // Verify both directed rows are now "accepted" in database
    let conn = app.state.main_db.read().await;

    // Check A->B row
    let mut rows_ab = conn
        .query(
            "SELECT status FROM friendship_relations WHERE from_user_id = ? AND to_user_id = ?",
            (user_a_id.as_str(), user_b_id.as_str()),
        )
        .await
        .unwrap();

    if let Some(row) = rows_ab.next().await.unwrap() {
        let status: String = row.get(0).unwrap();
        assert_eq!(status, "accepted", "A->B row should be accepted");
    } else {
        panic!("A->B row not found");
    }

    // Check B->A row
    let mut rows_ba = conn
        .query(
            "SELECT status FROM friendship_relations WHERE from_user_id = ? AND to_user_id = ?",
            (user_b_id.as_str(), user_a_id.as_str()),
        )
        .await
        .unwrap();

    if let Some(row) = rows_ba.next().await.unwrap() {
        let status: String = row.get(0).unwrap();
        assert_eq!(status, "accepted", "B->A row should be accepted");
    } else {
        panic!("B->A row not found");
    }
}

#[tokio::test]
async fn test_accept_friend_unauthorized() {
    let app = common::setup_test_app().await.expect("setup failed");

    // Create three users
    let user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");
    common::create_test_user(&app.state, "charlie", "password123")
        .await
        .expect("create charlie failed");

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

    // Login as Charlie (unauthorized third party)
    let cookie_c = common::login_user(&app.router, "charlie", "password123")
        .await
        .expect("charlie login failed");

    // Charlie tries to accept Alice's request to Bob
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

    // Create two users
    common::create_test_user(&app.state, "alice", "password123")
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
        .header("cookie", cookie_a.clone())
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // Alice (requester) tries to accept her own request
    let accept_payload = json!({"friend_id": user_b_id});
    let accept_request = Request::builder()
        .uri("/friends/accept")
        .method("POST")
        .header("cookie", cookie_a)
        .header("content-type", "application/json")
        .body(Body::from(accept_payload.to_string()))
        .unwrap();

    let accept_response = app.router.clone().oneshot(accept_request).await.unwrap();
    // Should fail because Alice is the requester, not the recipient
    assert_eq!(accept_response.status(), StatusCode::NOT_FOUND);
}

// ===== TASK 7: BLOCK FRIEND TESTS =====

#[tokio::test]
async fn test_block_friend_happy_path() {
    let app = common::setup_test_app().await.expect("setup failed");

    // Create two users
    common::create_test_user(&app.state, "alice", "password123")
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
        .header("cookie", cookie_a.clone())
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // Alice blocks the request
    let block_payload = json!({"friend_id": user_b_id});
    let block_request = Request::builder()
        .uri("/friends/block")
        .method("POST")
        .header("cookie", cookie_a)
        .header("content-type", "application/json")
        .body(Body::from(block_payload.to_string()))
        .unwrap();

    let block_response = app.router.clone().oneshot(block_request).await.unwrap();
    assert_eq!(block_response.status(), StatusCode::OK);

    // Parse response
    let body = axum::body::to_bytes(block_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let relation: FriendshipRelation = serde_json::from_slice(&body).unwrap();

    // Verify response
    assert_eq!(relation.user_id, user_b_id);
    assert_eq!(relation.status, "blocked");
}

#[tokio::test]
async fn test_block_accepted_friend_fails() {
    let app = common::setup_test_app().await.expect("setup failed");

    // Create two users
    let user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    let user_b_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");

    // Alice sends friend request
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

    // Bob accepts
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

    // Alice tries to block the accepted friend (invalid FSM transition: accepted->blocked)
    let block_payload = json!({"friend_id": user_b_id});
    let block_request = Request::builder()
        .uri("/friends/block")
        .method("POST")
        .header("cookie", cookie_a)
        .header("content-type", "application/json")
        .body(Body::from(block_payload.to_string()))
        .unwrap();

    let block_response = app.router.clone().oneshot(block_request).await.unwrap();
    // Should fail - cannot block an accepted friend (must unfriend first)
    assert_eq!(block_response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(block_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error_msg = String::from_utf8(body.to_vec()).unwrap();
    assert!(error_msg.contains("transition") || error_msg.contains("Invalid"));
}

// ===== TASK 7: UNFRIEND TESTS =====

#[tokio::test]
async fn test_unfriend_happy_path() {
    let app = common::setup_test_app().await.expect("setup failed");

    // Create two users and establish friendship
    let user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    let user_b_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");

    // Alice sends friend request
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

    // Bob accepts
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

    // Alice unfriends Bob
    let unfriend_payload = json!({"friend_id": user_b_id});
    let unfriend_request = Request::builder()
        .uri("/friends/unfriend")
        .method("POST")
        .header("cookie", cookie_a)
        .header("content-type", "application/json")
        .body(Body::from(unfriend_payload.to_string()))
        .unwrap();

    let unfriend_response = app.router.clone().oneshot(unfriend_request).await.unwrap();
    assert_eq!(unfriend_response.status(), StatusCode::OK);

    // Parse response
    let body = axum::body::to_bytes(unfriend_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let relation: FriendshipRelation = serde_json::from_slice(&body).unwrap();

    // Verify response
    assert_eq!(relation.user_id, user_b_id);
    assert_eq!(relation.status, "unfriended");

    // Verify both directed rows are now "unfriended" in database
    let conn = app.state.main_db.read().await;

    // Check A->B row
    let mut rows_ab = conn
        .query(
            "SELECT status FROM friendship_relations WHERE from_user_id = ? AND to_user_id = ?",
            (user_a_id.as_str(), user_b_id.as_str()),
        )
        .await
        .unwrap();

    if let Some(row) = rows_ab.next().await.unwrap() {
        let status: String = row.get(0).unwrap();
        assert_eq!(status, "unfriended", "A->B row should be unfriended");
    }

    // Check B->A row
    let mut rows_ba = conn
        .query(
            "SELECT status FROM friendship_relations WHERE from_user_id = ? AND to_user_id = ?",
            (user_b_id.as_str(), user_a_id.as_str()),
        )
        .await
        .unwrap();

    if let Some(row) = rows_ba.next().await.unwrap() {
        let status: String = row.get(0).unwrap();
        assert_eq!(status, "unfriended", "B->A row should be unfriended");
    }
}

#[tokio::test]
async fn test_unfriend_from_blocked_state() {
    let app = common::setup_test_app().await.expect("setup failed");

    // Create two users
    common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    let user_b_id = common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");

    // Alice sends friend request
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

    // Alice blocks the request
    let block_payload = json!({"friend_id": user_b_id});
    let block_request = Request::builder()
        .uri("/friends/block")
        .method("POST")
        .header("cookie", cookie_a.clone())
        .header("content-type", "application/json")
        .body(Body::from(block_payload.to_string()))
        .unwrap();

    let block_response = app.router.clone().oneshot(block_request).await.unwrap();
    assert_eq!(block_response.status(), StatusCode::OK);

    // Alice unfriends (transitions from blocked to unfriended)
    let unfriend_payload = json!({"friend_id": user_b_id});
    let unfriend_request = Request::builder()
        .uri("/friends/unfriend")
        .method("POST")
        .header("cookie", cookie_a)
        .header("content-type", "application/json")
        .body(Body::from(unfriend_payload.to_string()))
        .unwrap();

    let unfriend_response = app.router.clone().oneshot(unfriend_request).await.unwrap();
    assert_eq!(unfriend_response.status(), StatusCode::OK);

    // Parse response
    let body = axum::body::to_bytes(unfriend_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let relation: FriendshipRelation = serde_json::from_slice(&body).unwrap();

    assert_eq!(relation.status, "unfriended");
}

#[tokio::test]
async fn test_unfriend_either_party_can_initiate() {
    let app = common::setup_test_app().await.expect("setup failed");

    // Create two users and establish friendship
    let user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");

    // Alice sends friend request
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

    // Bob accepts
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

    // Bob unfriends Alice (recipient initiating unfriend)
    let unfriend_payload = json!({"friend_id": user_a_id});
    let unfriend_request = Request::builder()
        .uri("/friends/unfriend")
        .method("POST")
        .header("cookie", cookie_b)
        .header("content-type", "application/json")
        .body(Body::from(unfriend_payload.to_string()))
        .unwrap();

    let unfriend_response = app.router.clone().oneshot(unfriend_request).await.unwrap();
    assert_eq!(unfriend_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(unfriend_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let relation: FriendshipRelation = serde_json::from_slice(&body).unwrap();

    assert_eq!(relation.status, "unfriended");
}

// ===== TASK 7: INVALID FSM TRANSITION TESTS =====

#[tokio::test]
async fn test_invalid_fsm_transition_unfriended_to_accepted() {
    let app = common::setup_test_app().await.expect("setup failed");

    // Create two users and establish then unfriend
    let user_a_id = common::create_test_user(&app.state, "alice", "password123")
        .await
        .expect("create alice failed");
    common::create_test_user(&app.state, "bob", "password123")
        .await
        .expect("create bob failed");

    // Establish and accept friendship
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

    // Unfriend
    let unfriend_payload = json!({"friend_id": user_a_id});
    let unfriend_request = Request::builder()
        .uri("/friends/unfriend")
        .method("POST")
        .header("cookie", cookie_b)
        .header("content-type", "application/json")
        .body(Body::from(unfriend_payload.to_string()))
        .unwrap();

    let unfriend_response = app.router.clone().oneshot(unfriend_request).await.unwrap();
    assert_eq!(unfriend_response.status(), StatusCode::OK);

    // Try to accept again (unfriended -> accepted is invalid)
    let cookie_b2 = common::login_user(&app.router, "bob", "password123")
        .await
        .expect("bob login failed");

    let reaccept_payload = json!({"friend_id": user_a_id});
    let reaccept_request = Request::builder()
        .uri("/friends/accept")
        .method("POST")
        .header("cookie", cookie_b2)
        .header("content-type", "application/json")
        .body(Body::from(reaccept_payload.to_string()))
        .unwrap();

    let reaccept_response = app.router.clone().oneshot(reaccept_request).await.unwrap();
    assert_eq!(reaccept_response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(reaccept_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error_msg = String::from_utf8(body.to_vec()).unwrap();
    assert!(error_msg.contains("transition") || error_msg.contains("Invalid"));
}
