/// Tests C11-C14: Category isolation
///
/// After migration to a single DB, category queries must be scoped by
/// owner_user_id.  These tests are expected to FAIL (red) until the
/// migration is implemented.
mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::util::ServiceExt;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn json_post(
    app: &common::TestApp,
    uri: &str,
    cookie: &str,
    payload: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .expect("build request");
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute request");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        Value::String(String::from_utf8(bytes.to_vec()).expect("utf8"))
    });
    (status, body)
}

async fn json_put(
    app: &common::TestApp,
    uri: &str,
    cookie: &str,
    payload: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("PUT")
        .uri(uri)
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .expect("build request");
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute request");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        Value::String(String::from_utf8(bytes.to_vec()).expect("utf8"))
    });
    (status, body)
}

async fn json_delete(
    app: &common::TestApp,
    uri: &str,
    cookie: &str,
) -> StatusCode {
    let request = Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("cookie", cookie)
        .body(Body::empty())
        .expect("build request");
    app.router
        .clone()
        .oneshot(request)
        .await
        .expect("execute request")
        .status()
}

/// Create a category via API; return (id, status).
async fn create_category(
    app: &common::TestApp,
    cookie: &str,
    name: &str,
) -> (StatusCode, String) {
    let (status, body) = json_post(
        app,
        "/categories",
        cookie,
        json!({ "name": name, "is_income": false }),
    )
    .await;
    let id = body["id"]
        .as_str()
        .unwrap_or("")
        .to_string();
    (status, id)
}

// ---------------------------------------------------------------------------
// C11: Same category name allowed across different users
//      (previously blocked because of a global UNIQUE constraint on name)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn c11_same_category_name_allowed_for_different_users() {
    let app = common::setup_test_app().await.expect("setup failed");

    common::create_test_user(&app.state, "alice_c11", "pw")
        .await
        .expect("create alice");
    common::create_test_user(&app.state, "bob_c11", "pw")
        .await
        .expect("create bob");

    let alice_cookie = common::login_user(&app.router, "alice_c11", "pw")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob_c11", "pw")
        .await
        .expect("login bob");

    let (alice_status, alice_id) = create_category(&app, &alice_cookie, "Groceries").await;
    let (bob_status, bob_id) = create_category(&app, &bob_cookie, "Groceries").await;

    assert_eq!(alice_status, StatusCode::CREATED, "alice should create Groceries");
    assert_eq!(bob_status, StatusCode::CREATED, "bob should create Groceries (different owner)");
    assert!(!alice_id.is_empty(), "alice category must have an id");
    assert!(!bob_id.is_empty(), "bob category must have an id");
    assert_ne!(alice_id, bob_id, "each user gets a distinct category id");
}

// ---------------------------------------------------------------------------
// C12: User A cannot update User B's category
// ---------------------------------------------------------------------------

#[tokio::test]
async fn c12_user_a_cannot_update_user_b_category() {
    let app = common::setup_test_app().await.expect("setup failed");

    common::create_test_user(&app.state, "alice_c12", "pw")
        .await
        .expect("create alice");
    common::create_test_user(&app.state, "bob_c12", "pw")
        .await
        .expect("create bob");

    let alice_cookie = common::login_user(&app.router, "alice_c12", "pw")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob_c12", "pw")
        .await
        .expect("login bob");

    let (_, bob_cat_id) = create_category(&app, &bob_cookie, "BobCat").await;

    // Alice tries to update Bob's category
    let (status, _) = json_put(
        &app,
        &format!("/categories/{bob_cat_id}"),
        &alice_cookie,
        json!({ "name": "HackedName", "is_income": true }),
    )
    .await;

    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::FORBIDDEN,
        "Alice must not be able to update Bob's category, got {status}"
    );
}

// ---------------------------------------------------------------------------
// C13: User A cannot delete User B's category
// ---------------------------------------------------------------------------

#[tokio::test]
async fn c13_user_a_cannot_delete_user_b_category() {
    let app = common::setup_test_app().await.expect("setup failed");

    common::create_test_user(&app.state, "alice_c13", "pw")
        .await
        .expect("create alice");
    common::create_test_user(&app.state, "bob_c13", "pw")
        .await
        .expect("create bob");

    let alice_cookie = common::login_user(&app.router, "alice_c13", "pw")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob_c13", "pw")
        .await
        .expect("login bob");

    let (_, bob_cat_id) = create_category(&app, &bob_cookie, "BobCat").await;

    let status = json_delete(&app, &format!("/categories/{bob_cat_id}"), &alice_cookie).await;
    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::FORBIDDEN,
        "Alice must not be able to delete Bob's category, got {status}"
    );
}

// ---------------------------------------------------------------------------
// C14: Category in-use check is scoped to owner (Bob's category with same
//      name as Alice's should not falsely block Alice from deleting hers
//      when Bob has records referencing Bob's category)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn c14_category_in_use_check_scoped_to_owner() {
    let app = common::setup_test_app().await.expect("setup failed");

    common::create_test_user(&app.state, "alice_c14", "pw")
        .await
        .expect("create alice");
    common::create_test_user(&app.state, "bob_c14", "pw")
        .await
        .expect("create bob");

    let alice_cookie = common::login_user(&app.router, "alice_c14", "pw")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob_c14", "pw")
        .await
        .expect("login bob");

    // Both users create a category with the same name
    let (_, alice_cat_id) = create_category(&app, &alice_cookie, "Transport").await;
    let (_, bob_cat_id) = create_category(&app, &bob_cookie, "Transport").await;

    // Bob creates a record referencing his category — making Bob's category "in use"
    let (rec_status, _) = json_post(
        &app,
        "/records",
        &bob_cookie,
        json!({
            "name": "taxi",
            "amount": -20.0,
            "date": "2026-02-20",
            "category_id": bob_cat_id
        }),
    )
    .await;
    assert_eq!(rec_status, StatusCode::CREATED, "bob creates record");

    // Alice's category has no records — she should be able to delete it
    let delete_status = json_delete(
        &app,
        &format!("/categories/{alice_cat_id}"),
        &alice_cookie,
    )
    .await;
    assert_eq!(
        delete_status,
        StatusCode::NO_CONTENT,
        "Alice must be able to delete her unused category even if Bob's same-named category is in use"
    );

    // Bob's category IS in use — deleting it must fail
    let bob_delete_status =
        json_delete(&app, &format!("/categories/{bob_cat_id}"), &bob_cookie).await;
    assert!(
        bob_delete_status == StatusCode::CONFLICT || bob_delete_status == StatusCode::BAD_REQUEST,
        "Bob's in-use category must not be deletable, got {bob_delete_status}"
    );
}
