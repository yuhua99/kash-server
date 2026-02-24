mod common;

use axum::http::StatusCode;
use common::{auth_request, create_test_user, login_user, setup_test_app};

#[tokio::test]
async fn test_records_filter_pending_only() -> anyhow::Result<()> {
    let test_app = setup_test_app().await?;
    let user_id = create_test_user(&test_app.state, "user1", "pass").await?;
    let cookie = login_user(&test_app.router, "user1", "pass").await?;

    // Create a category first
    {
        let conn = test_app.state.main_db.write().await;
        conn.execute(
            "INSERT INTO categories (id, owner_user_id, name, is_income) VALUES (?, ?, ?, ?)",
            ("cat1", user_id.as_str(), "Food", false),
        )
        .await?;
    }

    // Insert test records with different pending states
    {
        let conn = test_app.state.main_db.write().await;
        // Pending record
        conn.execute(
            "INSERT INTO records (id, owner_user_id, name, amount, category_id, date, pending) VALUES (?, ?, ?, ?, ?, ?, ?)",
            (
                "rec1",
                user_id.as_str(),
                "Lunch",
                -50.0,
                "cat1",
                "2024-01-01",
                true,
            ),
        )
        .await?;

        // Non-pending record
        conn.execute(
            "INSERT INTO records (id, owner_user_id, name, amount, category_id, date, pending) VALUES (?, ?, ?, ?, ?, ?, ?)",
            (
                "rec2",
                user_id.as_str(),
                "Dinner",
                -100.0,
                "cat1",
                "2024-01-02",
                false,
            ),
        )
        .await?;
    }

    // Query with pending=true filter
    let (status, body) =
        auth_request(&test_app.router, "GET", "/records?pending=true", &cookie).await?;
    assert_eq!(status, StatusCode::OK);

    let response: serde_json::Value = serde_json::from_str(&body)?;
    let records = response["records"]
        .as_array()
        .expect("should have records array");

    // Should only return the pending record
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "rec1");
    assert_eq!(records[0]["name"], "Lunch");

    Ok(())
}

#[tokio::test]
async fn test_records_filter_settle_only() -> anyhow::Result<()> {
    let test_app = setup_test_app().await?;
    let user_id = create_test_user(&test_app.state, "user1", "pass").await?;
    let cookie = login_user(&test_app.router, "user1", "pass").await?;

    // Create a category first
    {
        let conn = test_app.state.main_db.write().await;
        conn.execute(
            "INSERT INTO categories (id, owner_user_id, name, is_income) VALUES (?, ?, ?, ?)",
            ("cat1", user_id.as_str(), "Food", false),
        )
        .await?;
    }

    // Insert test records with different settle states
    {
        // Settled record
        let conn = test_app.state.main_db.write().await;
        conn.execute(
            "INSERT INTO records (id, owner_user_id, name, amount, category_id, date, settle) VALUES (?, ?, ?, ?, ?, ?, ?)",
            (
                "rec1",
                user_id.as_str(),
                "Lunch",
                -50.0,
                "cat1",
                "2024-01-01",
                true,
            ),
        )
        .await?;

        // Unsettled record
        conn.execute(
            "INSERT INTO records (id, owner_user_id, name, amount, category_id, date, settle) VALUES (?, ?, ?, ?, ?, ?, ?)",
            (
                "rec2",
                user_id.as_str(),
                "Dinner",
                -100.0,
                "cat1",
                "2024-01-02",
                false,
            ),
        )
        .await?;
    }

    // Query with settle=true filter
    let (status, body) =
        auth_request(&test_app.router, "GET", "/records?settle=true", &cookie).await?;
    assert_eq!(status, StatusCode::OK);

    let response: serde_json::Value = serde_json::from_str(&body)?;
    let records = response["records"]
        .as_array()
        .expect("should have records array");

    // Should only return the settled record
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "rec1");
    assert_eq!(records[0]["name"], "Lunch");

    Ok(())
}

#[tokio::test]
async fn test_records_filter_combined_pending_and_settle() -> anyhow::Result<()> {
    let test_app = setup_test_app().await?;
    let user_id = create_test_user(&test_app.state, "user1", "pass").await?;
    let cookie = login_user(&test_app.router, "user1", "pass").await?;

    // Create a category first
    {
        let conn = test_app.state.main_db.write().await;
        conn.execute(
            "INSERT INTO categories (id, owner_user_id, name, is_income) VALUES (?, ?, ?, ?)",
            ("cat1", user_id.as_str(), "Food", false),
        )
        .await?;
    }

    // Insert test records with different combinations
    {
        let conn = test_app.state.main_db.write().await;
        // pending=true, settle=false
        conn.execute(
            "INSERT INTO records (id, owner_user_id, name, amount, category_id, date, pending, settle) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (
                "rec1",
                user_id.as_str(),
                "Lunch",
                -50.0,
                "cat1",
                "2024-01-01",
                true,
                false,
            ),
        )
        .await?;

        // pending=true, settle=true (both true)
        conn.execute(
            "INSERT INTO records (id, owner_user_id, name, amount, category_id, date, pending, settle) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (
                "rec2",
                user_id.as_str(),
                "Dinner",
                -100.0,
                "cat1",
                "2024-01-02",
                true,
                true,
            ),
        )
        .await?;

        // pending=false, settle=false
        conn.execute(
            "INSERT INTO records (id, owner_user_id, name, amount, category_id, date, pending, settle) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (
                "rec3",
                user_id.as_str(),
                "Breakfast",
                -30.0,
                "cat1",
                "2024-01-03",
                false,
                false,
            ),
        )
        .await?;

        // pending=false, settle=true
        conn.execute(
            "INSERT INTO records (id, owner_user_id, name, amount, category_id, date, pending, settle) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (
                "rec4",
                user_id.as_str(),
                "Snack",
                -20.0,
                "cat1",
                "2024-01-04",
                false,
                true,
            ),
        )
        .await?;
    }

    // Query with both filters: pending=true AND settle=false
    let (status, body) = auth_request(
        &test_app.router,
        "GET",
        "/records?pending=true&settle=false",
        &cookie,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);

    let response: serde_json::Value = serde_json::from_str(&body)?;
    let records = response["records"]
        .as_array()
        .expect("should have records array");

    // Should only return rec1 (pending=true AND settle=false)
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "rec1");
    assert_eq!(records[0]["name"], "Lunch");

    Ok(())
}

#[tokio::test]
async fn test_records_filter_backward_compatibility_no_filters() -> anyhow::Result<()> {
    let test_app = setup_test_app().await?;
    let user_id = create_test_user(&test_app.state, "user1", "pass").await?;
    let cookie = login_user(&test_app.router, "user1", "pass").await?;

    // Create a category first
    {
        let conn = test_app.state.main_db.write().await;
        conn.execute(
            "INSERT INTO categories (id, owner_user_id, name, is_income) VALUES (?, ?, ?, ?)",
            ("cat1", user_id.as_str(), "Food", false),
        )
        .await?;
    }

    // Insert test records with different states
    {
        let conn = test_app.state.main_db.write().await;
        conn.execute(
            "INSERT INTO records (id, owner_user_id, name, amount, category_id, date, pending, settle) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (
                "rec1",
                user_id.as_str(),
                "Lunch",
                -50.0,
                "cat1",
                "2024-01-01",
                true,
                false,
            ),
        )
        .await?;

        conn.execute(
            "INSERT INTO records (id, owner_user_id, name, amount, category_id, date, pending, settle) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (
                "rec2",
                user_id.as_str(),
                "Dinner",
                -100.0,
                "cat1",
                "2024-01-02",
                false,
                true,
            ),
        )
        .await?;
    }

    // Query without any filters - should return all records
    let (status, body) = auth_request(&test_app.router, "GET", "/records", &cookie).await?;
    assert_eq!(status, StatusCode::OK);

    let response: serde_json::Value = serde_json::from_str(&body)?;
    let records = response["records"]
        .as_array()
        .expect("should have records array");

    // Should return both records (no filtering)
    assert_eq!(records.len(), 2);
    assert_eq!(response["total_count"], 2);

    Ok(())
}

#[tokio::test]
async fn test_records_filter_with_date_filters() -> anyhow::Result<()> {
    let test_app = setup_test_app().await?;
    let user_id = create_test_user(&test_app.state, "user1", "pass").await?;
    let cookie = login_user(&test_app.router, "user1", "pass").await?;

    // Create a category first
    {
        let conn = test_app.state.main_db.write().await;
        conn.execute(
            "INSERT INTO categories (id, owner_user_id, name, is_income) VALUES (?, ?, ?, ?)",
            ("cat1", user_id.as_str(), "Food", false),
        )
        .await?;
    }

    // Insert test records with different dates and pending states
    {
        let conn = test_app.state.main_db.write().await;
        conn.execute(
            "INSERT INTO records (id, owner_user_id, name, amount, category_id, date, pending) VALUES (?, ?, ?, ?, ?, ?, ?)",
            (
                "rec1",
                user_id.as_str(),
                "Lunch",
                -50.0,
                "cat1",
                "2024-01-01",
                true,
            ),
        )
        .await?;

        conn.execute(
            "INSERT INTO records (id, owner_user_id, name, amount, category_id, date, pending) VALUES (?, ?, ?, ?, ?, ?, ?)",
            (
                "rec2",
                user_id.as_str(),
                "Dinner",
                -100.0,
                "cat1",
                "2024-01-05",
                false,
            ),
        )
        .await?;

        conn.execute(
            "INSERT INTO records (id, owner_user_id, name, amount, category_id, date, pending) VALUES (?, ?, ?, ?, ?, ?, ?)",
            (
                "rec3",
                user_id.as_str(),
                "Breakfast",
                -30.0,
                "cat1",
                "2024-01-10",
                true,
            ),
        )
        .await?;
    }

    // Query with both date and pending filters
    let (status, body) = auth_request(
        &test_app.router,
        "GET",
        "/records?start_date=2024-01-05&end_date=2024-01-10&pending=true",
        &cookie,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);

    let response: serde_json::Value = serde_json::from_str(&body)?;
    let records = response["records"]
        .as_array()
        .expect("should have records array");

    // Should return only rec3 (in date range AND pending=true)
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "rec3");

    Ok(())
}

#[tokio::test]
async fn test_records_filter_pending_false() -> anyhow::Result<()> {
    let test_app = setup_test_app().await?;
    let user_id = create_test_user(&test_app.state, "user1", "pass").await?;
    let cookie = login_user(&test_app.router, "user1", "pass").await?;

    // Create a category first
    {
        let conn = test_app.state.main_db.write().await;
        conn.execute(
            "INSERT INTO categories (id, owner_user_id, name, is_income) VALUES (?, ?, ?, ?)",
            ("cat1", user_id.as_str(), "Food", false),
        )
        .await?;
    }

    // Insert test records
    {
        let conn = test_app.state.main_db.write().await;
        conn.execute(
            "INSERT INTO records (id, owner_user_id, name, amount, category_id, date, pending) VALUES (?, ?, ?, ?, ?, ?, ?)",
            (
                "rec1",
                user_id.as_str(),
                "Lunch",
                -50.0,
                "cat1",
                "2024-01-01",
                true,
            ),
        )
        .await?;

        conn.execute(
            "INSERT INTO records (id, owner_user_id, name, amount, category_id, date, pending) VALUES (?, ?, ?, ?, ?, ?, ?)",
            (
                "rec2",
                user_id.as_str(),
                "Dinner",
                -100.0,
                "cat1",
                "2024-01-02",
                false,
            ),
        )
        .await?;
    }

    // Query with pending=false
    let (status, body) =
        auth_request(&test_app.router, "GET", "/records?pending=false", &cookie).await?;
    assert_eq!(status, StatusCode::OK);

    let response: serde_json::Value = serde_json::from_str(&body)?;
    let records = response["records"]
        .as_array()
        .expect("should have records array");

    // Should only return rec2 (pending=false)
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "rec2");

    Ok(())
}
