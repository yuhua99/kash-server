mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use common::{auth_request, create_test_user, login_user, setup_test_app};
use my_budget_server::{constants::*, database, with_transaction};
use serde_json::json;
use tower::util::ServiceExt;
use uuid::Uuid;

async fn create_split_scenario(
    app: &common::TestApp,
    payer_id: &str,
    debtor_id: &str,
    payer_cookie: &str,
) -> anyhow::Result<(String, String, String)> {
    let split_id = Uuid::new_v4().to_string();
    let payer_record_id = Uuid::new_v4().to_string();
    let debtor_record_id = Uuid::new_v4().to_string();

    {
        let conn = app.state.main_db.write().await;
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)?;
        conn.execute(
            "INSERT INTO split_coordination (id, initiator_user_id, idempotency_key, status, total_amount, participant_count, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (
                split_id.as_str(),
                payer_id,
                "test_idem_key",
                SPLIT_STATUS_INITIATED,
                100.0,
                2,
                now.as_str(),
                now.as_str(),
            ),
        )
        .await?;
    }

    let category_id = Uuid::new_v4().to_string();
    {
        let user_db = app.state.db_pool.get_user_db(payer_id).await?;
        let conn = user_db.write().await;
        conn.execute(
            "INSERT INTO categories (id, name, is_income) VALUES (?, ?, ?)",
            (category_id.as_str(), "Test Category", false),
        )
        .await?;
    }

    {
        let user_db = app.state.db_pool.get_user_db(payer_id).await?;
        let conn = user_db.write().await;
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)?;
        let date = time::Date::parse(
            "2026-02-16",
            &time::format_description::parse("[year]-[month]-[day]")?,
        )?
        .to_string();
        conn.execute(
            "INSERT INTO records (id, name, amount, category_id, date, pending, split_id, settle, debtor_user_id, creditor_user_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                payer_record_id.as_str(),
                "Split Payment",
                -100.0,
                category_id.as_str(),
                date.as_str(),
                false,
                split_id.as_str(),
                false,
                payer_id,
                payer_id,
            ),
        )
        .await?;
    }

    {
        let user_db = app.state.db_pool.get_user_db(debtor_id).await?;
        let conn = user_db.write().await;
        let date = time::Date::parse(
            "2026-02-16",
            &time::format_description::parse("[year]-[month]-[day]")?,
        )?
        .to_string();
        conn.execute(
            "INSERT INTO records (id, name, amount, category_id, date, pending, split_id, settle, debtor_user_id, creditor_user_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                debtor_record_id.as_str(),
                "Split Payment",
                50.0,
                "",
                date.as_str(),
                true,
                split_id.as_str(),
                false,
                debtor_id,
                payer_id,
            ),
        )
        .await?;
    }

    Ok((split_id, payer_record_id, debtor_record_id))
}

#[tokio::test]
async fn test_settle_happy_path_owner() -> anyhow::Result<()> {
    let app = setup_test_app().await?;

    let payer_id = create_test_user(&app.state, "payer", "password").await?;
    let debtor_id = create_test_user(&app.state, "debtor", "password").await?;

    let payer_cookie = login_user(&app.router, "payer", "password").await?;

    let (_split_id, payer_record_id, _debtor_record_id) =
        create_split_scenario(&app, &payer_id, &debtor_id, &payer_cookie).await?;

    // Payer (owner) settles their own record
    let payload = json!({
        "split_id": _split_id
    });

    let request = Request::builder()
        .method("PUT")
        .uri(format!("/records/{}/settle", payer_record_id))
        .header("content-type", "application/json")
        .header("cookie", &payer_cookie)
        .body(Body::from(payload.to_string()))?;

    let response = app.router.clone().oneshot(request).await?;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Owner should be able to settle their own record"
    );

    // Verify record is settled
    let user_db = app.state.db_pool.get_user_db(&payer_id).await?;
    let conn = user_db.read().await;
    let mut rows = conn
        .query(
            "SELECT settle FROM records WHERE id = ?",
            [payer_record_id.as_str()],
        )
        .await?;
    let row = rows.next().await?.expect("Record should exist");
    let settle: bool = row.get(0)?;
    assert!(settle, "Record should be marked as settled");

    Ok(())
}

#[tokio::test]
async fn test_settle_happy_path_debtor() -> anyhow::Result<()> {
    let app = setup_test_app().await?;

    let payer_id = create_test_user(&app.state, "payer", "password").await?;
    let debtor_id = create_test_user(&app.state, "debtor", "password").await?;

    let debtor_cookie = login_user(&app.router, "debtor", "password").await?;

    let (_split_id, _payer_record_id, debtor_record_id) =
        create_split_scenario(&app, &payer_id, &debtor_id, &debtor_cookie).await?;

    // Debtor settles the record where they are debtor
    let payload = json!({
        "split_id": _split_id
    });

    let request = Request::builder()
        .method("PUT")
        .uri(format!("/records/{}/settle", debtor_record_id))
        .header("content-type", "application/json")
        .header("cookie", &debtor_cookie)
        .body(Body::from(payload.to_string()))?;

    let response = app.router.clone().oneshot(request).await?;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Debtor should be able to settle record"
    );

    // Verify record is settled
    let user_db = app.state.db_pool.get_user_db(&debtor_id).await?;
    let conn = user_db.read().await;
    let mut rows = conn
        .query(
            "SELECT settle FROM records WHERE id = ?",
            [debtor_record_id.as_str()],
        )
        .await?;
    let row = rows.next().await?.expect("Record should exist");
    let settle: bool = row.get(0)?;
    assert!(settle, "Record should be marked as settled");

    Ok(())
}

#[tokio::test]
async fn test_settle_happy_path_creditor_sees_debtor_settled() -> anyhow::Result<()> {
    let app = setup_test_app().await?;

    let payer_id = create_test_user(&app.state, "payer", "password").await?;
    let debtor_id = create_test_user(&app.state, "debtor", "password").await?;

    let payer_cookie = login_user(&app.router, "payer", "password").await?;
    let debtor_cookie = login_user(&app.router, "debtor", "password").await?;

    let (_split_id, _payer_record_id, debtor_record_id) =
        create_split_scenario(&app, &payer_id, &debtor_id, &payer_cookie).await?;

    let payload = json!({
        "split_id": _split_id
    });

    let request = Request::builder()
        .method("PUT")
        .uri(format!("/records/{}/settle", debtor_record_id))
        .header("content-type", "application/json")
        .header("cookie", &debtor_cookie)
        .body(Body::from(payload.to_string()))?;

    let response = app.router.clone().oneshot(request).await?;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Debtor should be able to settle their own record"
    );

    let user_db = app.state.db_pool.get_user_db(&debtor_id).await?;
    let conn = user_db.read().await;
    let mut rows = conn
        .query(
            "SELECT settle FROM records WHERE id = ?",
            [debtor_record_id.as_str()],
        )
        .await?;
    let row = rows.next().await?.expect("Record should exist");
    let settle: bool = row.get(0)?;
    assert!(settle, "Debtor's record should be marked as settled");

    Ok(())
}

#[tokio::test]
async fn test_settle_unauthorized_third_party() -> anyhow::Result<()> {
    let app = setup_test_app().await?;

    let payer_id = create_test_user(&app.state, "payer", "password").await?;
    let debtor_id = create_test_user(&app.state, "debtor", "password").await?;
    let _third_party_id = create_test_user(&app.state, "thirdparty", "password").await?;

    let payer_cookie = login_user(&app.router, "payer", "password").await?;
    let third_party_cookie = login_user(&app.router, "thirdparty", "password").await?;

    let (_split_id, _payer_record_id, debtor_record_id) =
        create_split_scenario(&app, &payer_id, &debtor_id, &payer_cookie).await?;

    let payload = json!({
        "split_id": _split_id
    });

    let request = Request::builder()
        .method("PUT")
        .uri(format!("/records/{}/settle", debtor_record_id))
        .header("content-type", "application/json")
        .header("cookie", &third_party_cookie)
        .body(Body::from(payload.to_string()))?;

    let response = app.router.clone().oneshot(request).await?;
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Third party should not be able to see or settle record (404 not 403 to avoid leaking existence)"
    );

    Ok(())
}

#[tokio::test]
async fn test_settle_idempotent() -> anyhow::Result<()> {
    let app = setup_test_app().await?;

    let payer_id = create_test_user(&app.state, "payer", "password").await?;
    let debtor_id = create_test_user(&app.state, "debtor", "password").await?;

    let payer_cookie = login_user(&app.router, "payer", "password").await?;

    let (_split_id, payer_record_id, _debtor_record_id) =
        create_split_scenario(&app, &payer_id, &debtor_id, &payer_cookie).await?;

    let payload = json!({
        "split_id": _split_id
    });

    // First settle
    let request1 = Request::builder()
        .method("PUT")
        .uri(format!("/records/{}/settle", payer_record_id))
        .header("content-type", "application/json")
        .header("cookie", &payer_cookie)
        .body(Body::from(payload.to_string()))?;

    let response1 = app.router.clone().oneshot(request1).await?;
    assert_eq!(response1.status(), StatusCode::OK);

    // Second settle (idempotent)
    let request2 = Request::builder()
        .method("PUT")
        .uri(format!("/records/{}/settle", payer_record_id))
        .header("content-type", "application/json")
        .header("cookie", &payer_cookie)
        .body(Body::from(payload.to_string()))?;

    let response2 = app.router.clone().oneshot(request2).await?;
    assert_eq!(
        response2.status(),
        StatusCode::OK,
        "Settling already-settled record should succeed (idempotent)"
    );

    Ok(())
}

#[tokio::test]
async fn test_settle_record_not_found() -> anyhow::Result<()> {
    let app = setup_test_app().await?;

    let payer_id = create_test_user(&app.state, "payer", "password").await?;
    let payer_cookie = login_user(&app.router, "payer", "password").await?;

    let non_existent_record_id = Uuid::new_v4().to_string();
    let payload = json!({
        "split_id": Uuid::new_v4().to_string()
    });

    let request = Request::builder()
        .method("PUT")
        .uri(format!("/records/{}/settle", non_existent_record_id))
        .header("content-type", "application/json")
        .header("cookie", &payer_cookie)
        .body(Body::from(payload.to_string()))?;

    let response = app.router.clone().oneshot(request).await?;
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Should return 404 for non-existent record"
    );

    Ok(())
}

#[tokio::test]
async fn test_settle_filters_out_settled_records() -> anyhow::Result<()> {
    let app = setup_test_app().await?;

    let payer_id = create_test_user(&app.state, "payer", "password").await?;
    let debtor_id = create_test_user(&app.state, "debtor", "password").await?;

    let debtor_cookie = login_user(&app.router, "debtor", "password").await?;

    let (_split_id, _payer_record_id, debtor_record_id) =
        create_split_scenario(&app, &payer_id, &debtor_id, &debtor_cookie).await?;

    let category_id = Uuid::new_v4().to_string();
    {
        let user_db = app.state.db_pool.get_user_db(&debtor_id).await?;
        let conn = user_db.write().await;
        conn.execute(
            "INSERT INTO categories (id, name, is_income) VALUES (?, ?, ?)",
            (category_id.as_str(), "Debtor Category", false),
        )
        .await?;
    }

    let finalize_payload = json!({
        "record_id": debtor_record_id,
        "category_id": category_id
    });

    let finalize_request = Request::builder()
        .method("POST")
        .uri("/records/finalize-pending")
        .header("content-type", "application/json")
        .header("cookie", &debtor_cookie)
        .body(Body::from(finalize_payload.to_string()))?;

    app.router.clone().oneshot(finalize_request).await?;

    // Query unsettled records (settle=false)
    let query_request = Request::builder()
        .method("GET")
        .uri("/records?settle=false")
        .header("cookie", &debtor_cookie)
        .body(Body::empty())?;

    let response = app.router.clone().oneshot(query_request).await?;
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    let body: serde_json::Value = serde_json::from_slice(&body_bytes)?;
    let records = body["records"]
        .as_array()
        .expect("Should have records array");

    // Should see 1 unsettled record
    assert_eq!(records.len(), 1, "Should see 1 unsettled record");

    // Now settle the record
    let settle_payload = json!({
        "split_id": _split_id
    });

    let settle_request = Request::builder()
        .method("PUT")
        .uri(format!("/records/{}/settle", debtor_record_id))
        .header("content-type", "application/json")
        .header("cookie", &debtor_cookie)
        .body(Body::from(settle_payload.to_string()))?;

    app.router.clone().oneshot(settle_request).await?;

    // Query unsettled records again
    let query_request2 = Request::builder()
        .method("GET")
        .uri("/records?settle=false")
        .header("cookie", &debtor_cookie)
        .body(Body::empty())?;

    let response2 = app.router.clone().oneshot(query_request2).await?;
    let body_bytes2 = axum::body::to_bytes(response2.into_body(), usize::MAX).await?;
    let body2: serde_json::Value = serde_json::from_slice(&body_bytes2)?;
    let records2 = body2["records"]
        .as_array()
        .expect("Should have records array");

    // Should see 0 unsettled records now
    assert_eq!(
        records2.len(),
        0,
        "Settled record should be filtered out by settle=false"
    );

    Ok(())
}
