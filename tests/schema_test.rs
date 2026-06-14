/// Tests A1-A4: Single shared DB schema
///
/// These tests verify the *target* schema.
mod common;

// ---------------------------------------------------------------------------
// A1: Single DB init creates all required tables
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a1_single_db_init_creates_all_required_tables() {
    let app = common::setup_test_app().await.expect("setup failed");
    let conn = kash_server::database::db_conn(&app.state.main_db)
        .await
        .expect("connect db");

    // All tables must exist in the single shared DB
    for table in &[
        "users",
        "friendship",
        "idempotency_keys",
        "records",
        "categories",
        "splits",
        "split_participants",
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
// A2: records.owner_user_id is NOT NULL, references users, and has the composite hot-path index
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a2_records_owner_user_id_column_not_null_fk_and_index_exists() {
    let app = common::setup_test_app().await.expect("setup failed");
    let conn = kash_server::database::db_conn(&app.state.main_db)
        .await
        .expect("connect db");

    // Verify the column exists via PRAGMA table_info
    let mut rows = conn
        .query("PRAGMA table_info(records)", ())
        .await
        .expect("PRAGMA table_info(records)");

    let mut found_owner = false;
    let mut owner_notnull = false;
    let mut date_notnull = false;
    while let Some(row) = rows.next().await.expect("next table_info row") {
        let col_name: String = row.get(1).expect("col name");
        if col_name == "owner_user_id" {
            found_owner = true;
            let notnull: i64 = row.get(3).expect("notnull flag");
            owner_notnull = notnull != 0;
        }
        if col_name == "date" {
            let notnull: i64 = row.get(3).expect("notnull flag");
            date_notnull = notnull != 0;
        }
    }
    assert!(found_owner, "records.owner_user_id column must exist");
    assert!(owner_notnull, "records.owner_user_id must be NOT NULL");
    assert!(date_notnull, "records.date must be NOT NULL");

    let mut fk_rows = conn
        .query(
            "SELECT \"table\" FROM pragma_foreign_key_list('records') WHERE \"from\" = 'owner_user_id'",
            (),
        )
        .await
        .expect("query records foreign keys");
    let fk_row = fk_rows.next().await.expect("next fk row");
    assert!(
        fk_row.is_some(),
        "records.owner_user_id must reference users(id)"
    );

    let mut idx_rows = conn
        .query(
            "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='records' AND name='idx_records_owner_date'",
            (),
        )
        .await
        .expect("query indexes on records");
    let idx_row = idx_rows.next().await.expect("next index row");
    assert!(idx_row.is_some(), "idx_records_owner_date must exist");
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

    // Alice tries to create a second "Dining" with different case — must be rejected
    let alice_dup_status = create_category_status(&app, &alice_cookie, "dining").await;
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
    let conn = kash_server::database::db_conn(&app.state.main_db)
        .await
        .expect("connect db");

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
// A5: Constraints are enforced at runtime, not just declared
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a5_orphan_record_insert_is_rejected_by_foreign_key() {
    let app = common::setup_test_app().await.expect("setup failed");
    let conn = kash_server::database::db_conn(&app.state.main_db)
        .await
        .expect("connect db");

    let result = conn
        .execute(
            "INSERT INTO records (id, owner_user_id, name, amount, currency, date) VALUES (?, ?, ?, ?, ?, ?)",
            ("rec-a5", "no-such-user", "orphan", 100i64, "TWD", "2026-01-01"),
        )
        .await;
    assert!(
        result.is_err(),
        "insert with nonexistent owner_user_id must violate the foreign key"
    );
}

#[tokio::test]
async fn a5_split_participants_foreign_keys_are_declared() {
    let app = common::setup_test_app().await.expect("setup failed");
    let conn = kash_server::database::db_conn(&app.state.main_db)
        .await
        .expect("connect db");

    let mut rows = conn
        .query(
            "SELECT \"from\", \"table\" FROM pragma_foreign_key_list('split_participants')",
            (),
        )
        .await
        .expect("query split_participants foreign keys");

    let mut foreign_keys = Vec::new();
    while let Some(row) = rows.next().await.expect("next fk row") {
        let from: String = row.get(0).expect("from column");
        let table: String = row.get(1).expect("table column");
        foreign_keys.push((from, table));
    }

    assert!(
        foreign_keys.contains(&("split_id".to_string(), "splits".to_string())),
        "split_participants.split_id must reference splits(id)"
    );
    assert!(
        foreign_keys.contains(&("debtor_user_id".to_string(), "users".to_string())),
        "split_participants.debtor_user_id must reference users(id)"
    );
    assert!(
        foreign_keys.contains(&("finalized_record_id".to_string(), "records".to_string())),
        "split_participants.finalized_record_id must reference records(id)"
    );
}

#[tokio::test]
async fn a5_split_participants_constraints_are_enforced_in_db() {
    let app = common::setup_test_app().await.expect("setup failed");
    let user_id = common::create_test_user(&app.state, "alice_split_a5", "password123")
        .await
        .expect("create user");
    let conn = kash_server::database::db_conn(&app.state.main_db)
        .await
        .expect("connect db");

    conn.execute(
        "INSERT INTO splits (id, creditor_user_id, description, currency, date, total_amount, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        (
            "split-a5",
            user_id.as_str(),
            "Dinner",
            "TWD",
            "2026-01-01",
            100i64,
            "2026-01-01T00:00:00Z",
        ),
    )
    .await
    .expect("insert split");

    let bad_split_result = conn
        .execute(
            "INSERT INTO split_participants (id, split_id, debtor_user_id, amount) VALUES (?, ?, ?, ?)",
            ("part-a5-bad", "no-such-split", user_id.as_str(), 100i64),
        )
        .await;
    assert!(
        bad_split_result.is_err(),
        "insert with nonexistent split_id must violate the foreign key"
    );

    conn.execute(
        "INSERT INTO split_participants (id, split_id, debtor_user_id, amount) VALUES (?, ?, ?, ?)",
        ("part-a5-1", "split-a5", user_id.as_str(), 100i64),
    )
    .await
    .expect("insert first participant");

    let duplicate_result = conn
        .execute(
            "INSERT INTO split_participants (id, split_id, debtor_user_id, amount) VALUES (?, ?, ?, ?)",
            ("part-a5-2", "split-a5", user_id.as_str(), 100i64),
        )
        .await;
    assert!(
        duplicate_result.is_err(),
        "duplicate split_id and debtor_user_id must violate the unique constraint"
    );
}

#[tokio::test]
async fn a5_category_name_uniqueness_is_case_insensitive_in_db() {
    let app = common::setup_test_app().await.expect("setup failed");
    let user_id = common::create_test_user(&app.state, "alice_a5", "password123")
        .await
        .expect("create user");
    let conn = kash_server::database::db_conn(&app.state.main_db)
        .await
        .expect("connect db");

    conn.execute(
        "INSERT INTO categories (id, owner_user_id, name, is_income) VALUES (?, ?, ?, 0)",
        ("cat-a5-1", user_id.as_str(), "Food"),
    )
    .await
    .expect("insert first category");

    let result = conn
        .execute(
            "INSERT INTO categories (id, owner_user_id, name, is_income) VALUES (?, ?, ?, 0)",
            ("cat-a5-2", user_id.as_str(), "food"),
        )
        .await;
    assert!(
        result.is_err(),
        "case-insensitive duplicate category name must violate the unique index"
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
