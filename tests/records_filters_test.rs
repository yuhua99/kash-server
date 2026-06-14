mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use common::{TestApp, create_test_user, login_user, setup_test_app};
use serde_json::{Value, json};
use tower::util::ServiceExt;

async fn json_request(
    app: &TestApp,
    method: &str,
    uri: &str,
    cookie: &str,
    payload: Option<Value>,
) -> anyhow::Result<(StatusCode, Value)> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("cookie", cookie);
    let body = if let Some(payload) = payload {
        builder = builder.header("content-type", "application/json");
        Body::from(payload.to_string())
    } else {
        Body::empty()
    };
    let response = app.router.clone().oneshot(builder.body(body)?).await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    let body = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8(bytes.to_vec()).expect("utf8")));
    Ok((status, body))
}

async fn create_category(app: &TestApp, cookie: &str, name: &str) -> anyhow::Result<String> {
    let (status, body) = json_request(
        app,
        "POST",
        "/categories",
        cookie,
        Some(json!({ "name": name, "is_income": false })),
    )
    .await?;
    assert_eq!(status, StatusCode::CREATED);
    Ok(body["id"].as_str().expect("category id").to_string())
}

async fn create_record(
    app: &TestApp,
    cookie: &str,
    category_id: &str,
    name: &str,
    amount: f64,
    date: &str,
) -> anyhow::Result<String> {
    let (status, body) = json_request(
        app,
        "POST",
        "/records",
        cookie,
        Some(json!({
            "name": name,
            "amount": amount,
            "currency": "TWD",
            "date": date,
            "category_id": category_id
        })),
    )
    .await?;
    assert_eq!(status, StatusCode::CREATED);
    Ok(body["id"].as_str().expect("record id").to_string())
}

async fn setup_user_with_category(username: &str) -> anyhow::Result<(TestApp, String, String)> {
    let app = setup_test_app().await?;
    create_test_user(&app.state, username, "pass").await?;
    let cookie = login_user(&app.router, username, "pass").await?;
    let category_id = create_category(&app, &cookie, "Food").await?;
    Ok((app, cookie, category_id))
}

#[tokio::test]
async fn test_records_filter_no_filters_returns_all_ordinary_records() -> anyhow::Result<()> {
    let (app, cookie, category_id) = setup_user_with_category("filters_all").await?;
    create_record(&app, &cookie, &category_id, "Lunch", 50.0, "2024-01-01").await?;
    create_record(&app, &cookie, &category_id, "Dinner", 100.0, "2024-01-02").await?;

    let (status, body) = json_request(&app, "GET", "/records", &cookie, None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["records"].as_array().expect("records").len(), 2);
    assert_eq!(body["total_count"], 2);

    Ok(())
}

#[tokio::test]
async fn test_records_filter_with_date_range() -> anyhow::Result<()> {
    let (app, cookie, category_id) = setup_user_with_category("filters_date").await?;
    create_record(&app, &cookie, &category_id, "Early", 50.0, "2024-01-01").await?;
    let middle_id =
        create_record(&app, &cookie, &category_id, "Middle", 75.0, "2024-01-05").await?;
    let late_id = create_record(&app, &cookie, &category_id, "Late", 100.0, "2024-01-10").await?;

    let (status, body) = json_request(
        &app,
        "GET",
        "/records?start_date=2024-01-05&end_date=2024-01-10",
        &cookie,
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let ids: Vec<&str> = body["records"]
        .as_array()
        .expect("records")
        .iter()
        .filter_map(|record| record["id"].as_str())
        .collect();
    assert_eq!(ids, vec![late_id.as_str(), middle_id.as_str()]);
    assert_eq!(body["total_count"], 2);

    Ok(())
}

#[tokio::test]
async fn test_records_filter_limit_offset_and_total_count() -> anyhow::Result<()> {
    let (app, cookie, category_id) = setup_user_with_category("filters_page").await?;
    create_record(&app, &cookie, &category_id, "First", 10.0, "2024-01-01").await?;
    let second_id =
        create_record(&app, &cookie, &category_id, "Second", 20.0, "2024-01-02").await?;
    create_record(&app, &cookie, &category_id, "Third", 30.0, "2024-01-03").await?;

    let (status, body) =
        json_request(&app, "GET", "/records?limit=1&offset=1", &cookie, None).await?;
    assert_eq!(status, StatusCode::OK);
    let records = body["records"].as_array().expect("records");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], second_id);
    assert_eq!(body["total_count"], 3);

    Ok(())
}

#[tokio::test]
async fn test_records_filter_ordering_is_date_descending() -> anyhow::Result<()> {
    let (app, cookie, category_id) = setup_user_with_category("filters_order").await?;
    let oldest_id =
        create_record(&app, &cookie, &category_id, "Oldest", 10.0, "2024-01-01").await?;
    let newest_id =
        create_record(&app, &cookie, &category_id, "Newest", 20.0, "2024-01-03").await?;
    let middle_id =
        create_record(&app, &cookie, &category_id, "Middle", 30.0, "2024-01-02").await?;

    let (status, body) = json_request(&app, "GET", "/records", &cookie, None).await?;
    assert_eq!(status, StatusCode::OK);
    let ids: Vec<&str> = body["records"]
        .as_array()
        .expect("records")
        .iter()
        .filter_map(|record| record["id"].as_str())
        .collect();
    assert_eq!(
        ids,
        vec![newest_id.as_str(), middle_id.as_str(), oldest_id.as_str()]
    );

    Ok(())
}
