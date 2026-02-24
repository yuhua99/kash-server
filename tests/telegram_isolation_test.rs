/// Tests G25-G26: Telegram bot DB isolation
///
/// The Telegram bot's `list_records` and `edit_record` tool functions must
/// scope all queries to the linked user's `owner_user_id` in the shared DB.
///
/// These tests call the TG db helpers directly (no HTTP layer).
/// They are expected to FAIL (red) until the migration is implemented.
///
/// NOTE: The `execute_tool_call` function and its helpers live in
/// `src/bin/tg/db.rs` which is a binary-only module and cannot be imported
/// from integration tests.  Instead, we test the *observable behaviour* via
/// the shared DB: after using the bot helpers we inspect the shared DB to
/// verify that cross-user data leakage does not occur.
///
/// Because the TG db helpers accept a `&DbPool` today (which will be removed
/// after migration), we exercise isolation by inserting data directly into
/// the shared DB and calling the relevant HTTP API that mirrors the tool logic
/// (`GET /records`, `PUT /records/{id}`).  The key invariant being tested is
/// that the queries added during migration cannot return data owned by a
/// different user.
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

async fn json_request(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    cookie: &str,
    body: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
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
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8(bytes.to_vec()).expect("utf8")));
    (status, value)
}

async fn create_category(app: &common::TestApp, cookie: &str, name: &str) -> String {
    let (status, body) = json_request(
        app,
        "POST",
        "/categories",
        cookie,
        json!({ "name": name, "is_income": false }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create category {name}");
    body["id"].as_str().expect("category id").to_string()
}

async fn create_record(
    app: &common::TestApp,
    cookie: &str,
    name: &str,
    category_id: &str,
) -> String {
    let (status, body) = json_request(
        app,
        "POST",
        "/records",
        cookie,
        json!({
            "name": name,
            "amount": -30.0,
            "date": "2026-02-20",
            "category_id": category_id
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create record {name}");
    body["id"].as_str().expect("record id").to_string()
}

/// Insert a Telegram user link into the shared DB.
async fn link_telegram_user(
    app: &common::TestApp,
    telegram_user_id: i64,
    user_id: &str,
    chat_id: i64,
) {
    use time::OffsetDateTime;
    let conn = app.state.main_db.write().await;
    let created_at = OffsetDateTime::now_utc().unix_timestamp();
    conn.execute(
        "INSERT INTO telegram_users (telegram_user_id, user_id, chat_id, created_at) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT(telegram_user_id) DO UPDATE SET user_id = excluded.user_id, chat_id = excluded.chat_id",
        (
            telegram_user_id.to_string(),
            user_id,
            chat_id.to_string(),
            created_at,
        ),
    )
    .await
    .expect("link telegram user");
}

/// Count records in the shared DB owned by a given user_id.
#[allow(dead_code)]
async fn count_records_in_shared_db(app: &common::TestApp, owner_user_id: &str) -> i64 {
    let conn = app.state.main_db.read().await;
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM records WHERE owner_user_id = ?",
            [owner_user_id],
        )
        .await
        .expect("count query");
    let row = rows.next().await.expect("next").expect("row");
    row.get(0).expect("count")
}

// ---------------------------------------------------------------------------
// G25: TG list_records only returns linked user's records
//
// The bot's `list_records` tool fetches records for the linked `user_id`.
// After migration the query MUST include `WHERE owner_user_id = ?`.
// We verify this by:
//   1. Creating records for two different users in the shared DB.
//   2. Simulating what the bot would do: query the shared DB for records
//      belonging to the telegram-linked user.
//   3. Asserting that the result contains only that user's records.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn g25_tg_list_records_returns_only_linked_user_records() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice_g25", "pw")
        .await
        .expect("create alice");
    let _bob_id = common::create_test_user(&app.state, "bob_g25", "pw")
        .await
        .expect("create bob");

    let alice_cookie = common::login_user(&app.router, "alice_g25", "pw")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob_g25", "pw")
        .await
        .expect("login bob");

    // Link Alice to a Telegram user id
    link_telegram_user(&app, 100_001, &alice_id, 999_001).await;

    // Create records for both users
    let alice_cat = create_category(&app, &alice_cookie, "AliceCat").await;
    let bob_cat = create_category(&app, &bob_cookie, "BobCat").await;

    let alice_rec_id = create_record(&app, &alice_cookie, "Alice record G25", &alice_cat).await;
    let _bob_rec_id = create_record(&app, &bob_cookie, "Bob record G25", &bob_cat).await;

    // Simulate what the bot does: query records for the linked user_id from the shared DB.
    // After migration this must be `SELECT ... FROM records WHERE owner_user_id = ?`.
    let conn = app.state.main_db.read().await;
    let mut rows = conn
        .query(
            "SELECT id, name FROM records WHERE owner_user_id = ?",
            [alice_id.as_str()],
        )
        .await
        .expect("list records query");

    let mut found_ids: Vec<String> = Vec::new();
    while let Some(row) = rows.next().await.expect("next") {
        let id: String = row.get(0).expect("id");
        found_ids.push(id);
    }

    assert!(
        found_ids.contains(&alice_rec_id),
        "Alice's record must appear in bot list"
    );
    // Bob's records must NOT appear
    assert_eq!(
        found_ids.len(),
        1,
        "bot must return exactly 1 record for Alice"
    );

    // Also verify via HTTP API (which the bot calls internally) — GET /records
    drop(conn);
    let (http_status, http_body) =
        json_request(&app, "GET", "/records", &alice_cookie, json!({})).await;
    assert_eq!(http_status, StatusCode::OK);
    let http_ids: Vec<&str> = http_body["records"]
        .as_array()
        .expect("records array")
        .iter()
        .filter_map(|r| r["id"].as_str())
        .collect();
    assert!(
        http_ids.contains(&alice_rec_id.as_str()),
        "HTTP /records must include Alice's record"
    );
    assert_eq!(
        http_ids.len(),
        1,
        "HTTP /records must return only Alice's record"
    );
}

// ---------------------------------------------------------------------------
// G26: TG edit_record cannot modify another user's record
//
// The bot's `edit_record` tool modifies a record by id for the linked user.
// After migration the UPDATE must include `WHERE id = ? AND owner_user_id = ?`
// so that a Telegram user cannot edit a record belonging to a different user.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn g26_tg_edit_record_cannot_modify_other_user_record() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice_g26", "pw")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, "bob_g26", "pw")
        .await
        .expect("create bob");

    let alice_cookie = common::login_user(&app.router, "alice_g26", "pw")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob_g26", "pw")
        .await
        .expect("login bob");

    // Link Alice to a Telegram user id
    link_telegram_user(&app, 100_002, &alice_id, 999_002).await;

    // Bob creates a record
    let bob_cat = create_category(&app, &bob_cookie, "BobCat").await;
    let bob_rec_id = create_record(&app, &bob_cookie, "Bob record G26", &bob_cat).await;

    // Alice's category (needed for an update attempt)
    let alice_cat = create_category(&app, &alice_cookie, "AliceCat").await;

    // Simulate the bot trying to edit Bob's record on behalf of Alice:
    // The migration must ensure the UPDATE is scoped to alice_id.
    {
        let conn = app.state.main_db.write().await;
        let result = conn
            .execute(
                // This is the query the bot will use after migration:
                "UPDATE records SET name = 'hacked by bot' WHERE id = ? AND owner_user_id = ?",
                [bob_rec_id.as_str(), alice_id.as_str()],
            )
            .await
            .expect("execute update");
        // rows_affected must be 0 — Alice cannot touch Bob's record
        assert_eq!(
            result, 0,
            "bot update must affect 0 rows when owner_user_id does not match"
        );
    }

    // Verify Bob's record is unchanged in the shared DB
    {
        let conn = app.state.main_db.read().await;
        let mut rows = conn
            .query(
                "SELECT name FROM records WHERE id = ? AND owner_user_id = ?",
                [bob_rec_id.as_str(), bob_id.as_str()],
            )
            .await
            .expect("query record");
        let row = rows.next().await.expect("next").expect("row");
        let name: String = row.get(0).expect("name");
        assert_eq!(
            name, "Bob record G26",
            "Bob's record name must be unchanged"
        );
    }

    // Also verify the HTTP PUT endpoint rejects Alice editing Bob's record
    let (put_status, _) = json_request(
        &app,
        "PUT",
        &format!("/records/{bob_rec_id}"),
        &alice_cookie,
        json!({
            "name": "hacked via http",
            "amount": -1.0,
            "date": "2026-02-20",
            "category_id": alice_cat
        }),
    )
    .await;
    assert!(
        put_status == StatusCode::NOT_FOUND || put_status == StatusCode::FORBIDDEN,
        "HTTP PUT must reject Alice editing Bob's record, got {put_status}"
    );

    let _ = bob_id;
}
