mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::util::ServiceExt;

#[tokio::test]
async fn settings_get_returns_default_main_currency() {
    let app = common::setup_test_app().await.expect("setup failed");
    common::create_test_user(&app.state, "alice_settings_a1", "pw")
        .await
        .expect("create user");
    let cookie = common::login_user(&app.router, "alice_settings_a1", "pw")
        .await
        .expect("login user");

    let request = Request::builder()
        .uri("/settings")
        .method("GET")
        .header("cookie", cookie)
        .body(Body::empty())
        .expect("build request");

    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("execute request");

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let parsed: Value = serde_json::from_slice(&body).expect("parse json");
    assert_eq!(parsed["main_currency_code"], "TWD");
}

#[tokio::test]
async fn settings_put_updates_main_currency() {
    let app = common::setup_test_app().await.expect("setup failed");
    let user_id = common::create_test_user(&app.state, "alice_settings_a2", "pw")
        .await
        .expect("create user");
    let cookie = common::login_user(&app.router, "alice_settings_a2", "pw")
        .await
        .expect("login user");

    let payload = serde_json::json!({ "main_currency_code": "USD" });
    let request = Request::builder()
        .uri("/settings")
        .method("PUT")
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

    assert_eq!(response.status(), StatusCode::OK);

    let conn = app.state.main_db.read().await;
    let mut rows = conn
        .query(
            "SELECT main_currency_code FROM users WHERE id = ?",
            [user_id.as_str()],
        )
        .await
        .expect("query user settings");
    let row = rows
        .next()
        .await
        .expect("next row")
        .expect("user row exists");
    let main_currency_code: String = row.get(0).expect("read currency");
    assert_eq!(main_currency_code, "USD");
}

#[tokio::test]
async fn settings_put_rejects_unsupported_currency() {
    let app = common::setup_test_app().await.expect("setup failed");
    common::create_test_user(&app.state, "alice_settings_a3", "pw")
        .await
        .expect("create user");
    let cookie = common::login_user(&app.router, "alice_settings_a3", "pw")
        .await
        .expect("login user");

    let payload = serde_json::json!({ "main_currency_code": "GBP" });
    let request = Request::builder()
        .uri("/settings")
        .method("PUT")
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

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
