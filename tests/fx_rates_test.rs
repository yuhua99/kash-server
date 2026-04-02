mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::util::ServiceExt;

#[tokio::test]
async fn fx_rates_returns_cached_rows() {
    let app = common::setup_test_app().await.expect("setup failed");
    common::create_test_user(&app.state, "alice_fx_a1", "pw")
        .await
        .expect("create user");
    let cookie = common::login_user(&app.router, "alice_fx_a1", "pw")
        .await
        .expect("login user");

    let conn = app.state.main_db.write().await;
    conn.execute(
        "INSERT INTO exchange_rates_daily (date, currency, rate) VALUES (?, ?, ?)",
        ("2026-04-01", "JPY", 154.24),
    )
    .await
    .expect("insert jpy rate");
    conn.execute(
        "INSERT INTO exchange_rates_daily (date, currency, rate) VALUES (?, ?, ?)",
        ("2026-04-01", "TWD", 32.0),
    )
    .await
    .expect("insert twd rate");
    drop(conn);

    let request = Request::builder()
        .uri("/fx/rates?from=2026-04-01&to=2026-04-01&quotes=JPY,TWD")
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
    let rates = parsed["rates"].as_array().expect("rates array");
    assert_eq!(rates.len(), 2);
    assert_eq!(rates[0]["date"], "2026-04-01");
    assert_eq!(rates[0]["currency"], "JPY");
    assert_eq!(rates[1]["currency"], "TWD");
}

#[tokio::test]
async fn fx_rates_rejects_invalid_quotes() {
    let app = common::setup_test_app().await.expect("setup failed");
    common::create_test_user(&app.state, "alice_fx_a2", "pw")
        .await
        .expect("create user");
    let cookie = common::login_user(&app.router, "alice_fx_a2", "pw")
        .await
        .expect("login user");

    let request = Request::builder()
        .uri("/fx/rates?from=2026-04-01&to=2026-04-01&quotes=TWD,GBP")
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

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
