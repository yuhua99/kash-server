/// Tests A1-A4: Single shared DB schema
///
/// These tests verify the *target* schema after migration.
/// They are expected to FAIL (red) until the migration is implemented.
mod common;

// ---------------------------------------------------------------------------
// A1: Single DB init creates all required tables
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a1_single_db_init_creates_all_required_tables() {
    let app = common::setup_test_app().await.expect("setup failed");
    let conn = app.state.main_db.read().await;

    // All tables must exist in the single shared DB
    for table in &[
        "users",
        "friendship",
        "idempotency_keys",
        "records",
        "categories",
        "telegram_users",
    ] {
        let mut rows = conn
            .query(
                "SELECT name FROM sqlite_master WHERE type='table' AND name=?",
                [*table],
            )
            .await
            .unwrap_or_else(|e| panic!("query sqlite_master for table {table}: {e}"));
        let row = rows
            .next()
            .await
            .unwrap_or_else(|e| panic!("next row for table {table}: {e}"));
        assert!(row.is_some(), "table '{table}' must exist in the single DB");
    }
}

// ---------------------------------------------------------------------------
// A2: records.owner_user_id is NOT NULL and an index exists
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a2_records_owner_user_id_column_not_null_and_index_exists() {
    let app = common::setup_test_app().await.expect("setup failed");
    let conn = app.state.main_db.read().await;

    // Verify the column exists via PRAGMA table_info
    let mut rows = conn
        .query("PRAGMA table_info(records)", ())
        .await
        .expect("PRAGMA table_info(records)");

    let mut found_owner = false;
    let mut owner_notnull = false;
    while let Some(row) = rows.next().await.expect("next table_info row") {
        let col_name: String = row.get(1).expect("col name");
        if col_name == "owner_user_id" {
            found_owner = true;
            let notnull: i64 = row.get(3).expect("notnull flag");
            owner_notnull = notnull != 0;
        }
    }
    assert!(found_owner, "records.owner_user_id column must exist");
    assert!(owner_notnull, "records.owner_user_id must be NOT NULL");

    // Verify that at least one index covers owner_user_id
    let mut idx_rows = conn
        .query(
            "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='records' AND sql LIKE '%owner_user_id%'",
            (),
        )
        .await
        .expect("query indexes on records");
    let idx_row = idx_rows.next().await.expect("next index row");
    assert!(
        idx_row.is_some(),
        "an index on records(owner_user_id) must exist"
    );
}

// ---------------------------------------------------------------------------
// A3: categories uniqueness is per-user (same name across users OK,
//     duplicate name within same user rejected)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a3_categories_uniqueness_is_per_user() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice_a3", "pw")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, "bob_a3", "pw")
        .await
        .expect("create bob");

    let alice_cookie = common::login_user(&app.router, "alice_a3", "pw")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob_a3", "pw")
        .await
        .expect("login bob");

    // Both Alice and Bob create a category with the same name — must succeed
    let alice_status = create_category_status(&app, &alice_cookie, "Dining").await;
    let bob_status = create_category_status(&app, &bob_cookie, "Dining").await;
    assert_eq!(
        alice_status,
        axum::http::StatusCode::CREATED,
        "alice should create Dining"
    );
    assert_eq!(
        bob_status,
        axum::http::StatusCode::CREATED,
        "bob should create Dining (different owner)"
    );

    // Alice tries to create a second "Dining" — must be rejected
    let alice_dup_status = create_category_status(&app, &alice_cookie, "Dining").await;
    assert_eq!(
        alice_dup_status,
        axum::http::StatusCode::CONFLICT,
        "duplicate category name within same user must be rejected"
    );

    let _ = (alice_id, bob_id); // suppress unused warnings
}

// ---------------------------------------------------------------------------
// A4: idempotency_keys uniqueness is (user_id, endpoint, key) not global key
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a4_idempotency_keys_uniqueness_is_per_user_and_endpoint() {
    let app = common::setup_test_app().await.expect("setup failed");
    let conn = app.state.main_db.read().await;

    // The table must NOT have a simple PRIMARY KEY on `key` alone.
    // Instead uniqueness must be on (user_id, endpoint, key).
    // We verify via PRAGMA index_list / index_info.

    let mut idx_rows = conn
        .query(
            "SELECT name, origin FROM pragma_index_list('idempotency_keys')",
            (),
        )
        .await
        .expect("pragma index_list idempotency_keys");

    let mut found_compound = false;
    while let Some(row) = idx_rows.next().await.expect("next index_list row") {
        let idx_name: String = row.get(0).expect("index name");
        // Check the columns in this index
        let mut col_rows = conn
            .query("SELECT name FROM pragma_index_info(?)", [idx_name.as_str()])
            .await
            .expect("pragma index_info");
        let mut cols = Vec::new();
        while let Some(col_row) = col_rows.next().await.expect("next col row") {
            let col_name: String = col_row.get(0).expect("col name");
            cols.push(col_name);
        }
        if cols.contains(&"user_id".to_string())
            && cols.contains(&"endpoint".to_string())
            && cols.contains(&"key".to_string())
        {
            found_compound = true;
        }
    }
    assert!(
        found_compound,
        "idempotency_keys must have a unique index on (user_id, endpoint, key)"
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::util::ServiceExt;

async fn create_category_status(app: &common::TestApp, cookie: &str, name: &str) -> StatusCode {
    let payload = serde_json::json!({ "name": name, "is_income": false });
    let request = Request::builder()
        .uri("/categories")
        .method("POST")
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .expect("build category request");
    app.router
        .clone()
        .oneshot(request)
        .await
        .expect("execute category request")
        .status()
}
