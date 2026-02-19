mod common;

use common::{auth_request, create_test_user, login_user, setup_test_app};

#[tokio::test]
async fn fixtures_bootstrap_auth() {
    let test_app = setup_test_app().await.expect("Failed to setup test app");

    let user1_id = create_test_user(&test_app.state, "testuser1", "password123")
        .await
        .expect("Failed to create test user 1");

    println!("✓ Created test user 1 with ID: {}", user1_id);

    let user2_id = create_test_user(&test_app.state, "testuser2", "password456")
        .await
        .expect("Failed to create test user 2");

    println!("✓ Created test user 2 with ID: {}", user2_id);

    let cookie1 = login_user(&test_app.router, "testuser1", "password123")
        .await
        .expect("Failed to login user 1");

    println!("✓ Logged in test user 1");

    let cookie2 = login_user(&test_app.router, "testuser2", "password456")
        .await
        .expect("Failed to login user 2");

    println!("✓ Logged in test user 2");

    let (status1, body1) = auth_request(&test_app.router, "GET", "/categories", &cookie1)
        .await
        .expect("Failed to make authenticated request for user 1");

    assert_eq!(status1, 200, "Expected 200 OK for GET /categories");
    println!("✓ User 1 GET /categories: {} OK", status1);
    println!("  Response: {}", body1);

    let (status2, body2) = auth_request(&test_app.router, "GET", "/categories", &cookie2)
        .await
        .expect("Failed to make authenticated request for user 2");

    assert_eq!(status2, 200, "Expected 200 OK for GET /categories");
    println!("✓ User 2 GET /categories: {} OK", status2);
    println!("  Response: {}", body2);

    let (unauth_status, _unauth_body) = auth_request(&test_app.router, "GET", "/categories", "")
        .await
        .expect("Failed to make unauthenticated request");

    assert_eq!(
        unauth_status, 401,
        "Expected 401 Unauthorized for request without session"
    );
    println!("✓ Unauthenticated access properly rejected with 401");

    println!("\n✅ All fixture bootstrap auth tests passed!");
    println!("  - Test users created successfully");
    println!("  - User login workflow verified");
    println!("  - Authenticated requests work");
    println!("  - Unauthorized access properly blocked");
}
