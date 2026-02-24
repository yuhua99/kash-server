/// Tests D15-D18: Split atomicity and idempotency (single-DB target)
/// Tests E19-E21: Regression
/// Tests F22-F24: Concurrency
///
/// These tests are expected to FAIL (red) until the migration is implemented.
mod common;

use std::sync::Arc;

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
    payload: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
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

async fn create_category(app: &common::TestApp, cookie: &str, name: &str) -> String {
    let (status, body) = json_request(
        app,
        "POST",
        "/categories",
        cookie,
        json!({ "name": name, "is_income": false }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create category");
    body["id"].as_str().expect("category id").to_string()
}

async fn send_friend_request(app: &common::TestApp, cookie: &str, friend_username: &str) {
    let (status, _) = json_request(
        app,
        "POST",
        "/friends/request",
        cookie,
        json!({ "friend_username": friend_username }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "send friend request to {friend_username}");
}

async fn accept_friend(app: &common::TestApp, cookie: &str, friend_id: &str) {
    let (status, _) = json_request(
        app,
        "POST",
        "/friends/accept",
        cookie,
        json!({ "friend_id": friend_id }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "accept friend");
}

async fn create_split(
    app: &common::TestApp,
    alice_cookie: &str,
    category_id: &str,
    idempotency_key: &str,
    bob_id: &str,
    amount: f64,
) -> (StatusCode, Value) {
    json_request(
        app,
        "POST",
        "/splits/create",
        alice_cookie,
        json!({
            "idempotency_key": idempotency_key,
            "total_amount": 90.0,
            "description": "split test",
            "date": "2026-02-20",
            "category_id": category_id,
            "splits": [{ "user_id": bob_id, "amount": amount }]
        }),
    )
    .await
}

/// Count records in the shared DB for a given owner_user_id.
/// After migration this queries the single main_db; before migration
/// (per-user DBs) this will always return 0 for the shared DB, which
/// causes the assertion inside the test to fail → intentional red state.
async fn count_records_for_user(app: &common::TestApp, user_id: &str) -> i64 {
    let conn = app.state.main_db.read().await;
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM records WHERE owner_user_id = ?",
            [user_id],
        )
        .await
        .expect("count records query");
    let row = rows.next().await.expect("next row").expect("row exists");
    row.get(0).expect("count")
}

/// Return all record ids for a user from the shared DB.
async fn record_ids_for_user(app: &common::TestApp, user_id: &str) -> Vec<String> {
    let conn = app.state.main_db.read().await;
    let mut rows = conn
        .query(
            "SELECT id FROM records WHERE owner_user_id = ?",
            [user_id],
        )
        .await
        .expect("record ids query");
    let mut ids = Vec::new();
    while let Some(row) = rows.next().await.expect("next row") {
        let id: String = row.get(0).expect("id");
        ids.push(id);
    }
    ids
}

// ---------------------------------------------------------------------------
// D15: create_split writes payer + all participants atomically in one tx
// ---------------------------------------------------------------------------

#[tokio::test]
async fn d15_create_split_writes_all_records_atomically() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice_d15", "pw")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, "bob_d15", "pw")
        .await
        .expect("create bob");
    let charlie_id = common::create_test_user(&app.state, "charlie_d15", "pw")
        .await
        .expect("create charlie");

    let alice_cookie = common::login_user(&app.router, "alice_d15", "pw")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob_d15", "pw")
        .await
        .expect("login bob");
    let charlie_cookie = common::login_user(&app.router, "charlie_d15", "pw")
        .await
        .expect("login charlie");

    send_friend_request(&app, &alice_cookie, "bob_d15").await;
    send_friend_request(&app, &alice_cookie, "charlie_d15").await;
    accept_friend(&app, &bob_cookie, &alice_id).await;
    accept_friend(&app, &charlie_cookie, &alice_id).await;

    let cat = create_category(&app, &alice_cookie, "Dining").await;
    let (status, body) = json_request(
        &app,
        "POST",
        "/splits/create",
        &alice_cookie,
        json!({
            "idempotency_key": "d15-split-1",
            "total_amount": 90.0,
            "description": "d15 split",
            "date": "2026-02-20",
            "category_id": cat,
            "splits": [
                { "user_id": bob_id, "amount": 30.0 },
                { "user_id": charlie_id, "amount": 30.0 }
            ]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "split create");

    let payer_record_id = body["payer_record_id"]
        .as_str()
        .expect("payer_record_id")
        .to_string();
    let pending_ids = body["pending_record_ids"]
        .as_array()
        .expect("pending_record_ids");
    assert_eq!(pending_ids.len(), 2);

    // All records live in the SINGLE shared DB, scoped by owner_user_id
    let alice_count = count_records_for_user(&app, &alice_id).await;
    let bob_count = count_records_for_user(&app, &bob_id).await;
    let charlie_count = count_records_for_user(&app, &charlie_id).await;

    assert_eq!(alice_count, 1, "alice must have 1 record (payer)");
    assert_eq!(bob_count, 1, "bob must have 1 record (pending)");
    assert_eq!(charlie_count, 1, "charlie must have 1 record (pending)");

    // Alice's payer record must be in the shared DB
    let alice_ids = record_ids_for_user(&app, &alice_id).await;
    assert!(
        alice_ids.contains(&payer_record_id),
        "payer record must be owned by alice in shared DB"
    );
}

// ---------------------------------------------------------------------------
// D16: Same key + same payload returns same response, no duplicate writes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn d16_idempotency_same_key_same_payload_no_duplicate_writes() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice_d16", "pw")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, "bob_d16", "pw")
        .await
        .expect("create bob");

    let alice_cookie = common::login_user(&app.router, "alice_d16", "pw")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob_d16", "pw")
        .await
        .expect("login bob");

    send_friend_request(&app, &alice_cookie, "bob_d16").await;
    accept_friend(&app, &bob_cookie, &alice_id).await;

    let cat = create_category(&app, &alice_cookie, "Dining").await;

    let payload = json!({
        "idempotency_key": "d16-split-1",
        "total_amount": 60.0,
        "description": "d16 split",
        "date": "2026-02-20",
        "category_id": cat,
        "splits": [{ "user_id": bob_id, "amount": 30.0 }]
    });

    // First request
    let (s1, b1) = json_request(&app, "POST", "/splits/create", &alice_cookie, payload.clone()).await;
    assert_eq!(s1, StatusCode::CREATED, "first request");

    // Second request — identical key + payload
    let (s2, b2) = json_request(&app, "POST", "/splits/create", &alice_cookie, payload).await;
    assert_eq!(s2, StatusCode::CREATED, "second request (idempotent replay)");
    assert_eq!(b1, b2, "idempotent replay must return identical body");

    // Only ONE payer record and ONE pending record must exist in the shared DB
    let alice_count = count_records_for_user(&app, &alice_id).await;
    let bob_count = count_records_for_user(&app, &bob_id).await;
    assert_eq!(alice_count, 1, "no duplicate payer records");
    assert_eq!(bob_count, 1, "no duplicate pending records for bob");
}

// ---------------------------------------------------------------------------
// D17: Same key + different payload returns 409
// ---------------------------------------------------------------------------

#[tokio::test]
async fn d17_idempotency_same_key_different_payload_conflicts() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice_d17", "pw")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, "bob_d17", "pw")
        .await
        .expect("create bob");

    let alice_cookie = common::login_user(&app.router, "alice_d17", "pw")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob_d17", "pw")
        .await
        .expect("login bob");

    send_friend_request(&app, &alice_cookie, "bob_d17").await;
    accept_friend(&app, &bob_cookie, &alice_id).await;

    let cat = create_category(&app, &alice_cookie, "Dining").await;

    let first_payload = json!({
        "idempotency_key": "d17-split-1",
        "total_amount": 60.0,
        "description": "original split",
        "date": "2026-02-20",
        "category_id": cat,
        "splits": [{ "user_id": bob_id, "amount": 30.0 }]
    });
    let (s1, _) = json_request(&app, "POST", "/splits/create", &alice_cookie, first_payload).await;
    assert_eq!(s1, StatusCode::CREATED);

    // Same key, different payload
    let second_payload = json!({
        "idempotency_key": "d17-split-1",
        "total_amount": 80.0,           // different amount
        "description": "modified split",
        "date": "2026-02-20",
        "category_id": cat,
        "splits": [{ "user_id": bob_id, "amount": 40.0 }]
    });
    let (s2, _) = json_request(&app, "POST", "/splits/create", &alice_cookie, second_payload).await;
    assert_eq!(s2, StatusCode::CONFLICT, "different payload must conflict");
}

// ---------------------------------------------------------------------------
// D18: Any failure in create_split rolls back everything (no partial data)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn d18_create_split_rolls_back_on_failure() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice_d18", "pw")
        .await
        .expect("create alice");
    // bob is intentionally NOT a friend of alice → split must fail after alice
    // is validated but bob validation fails → should produce zero records
    let bob_id = common::create_test_user(&app.state, "bob_d18", "pw")
        .await
        .expect("create bob");

    let alice_cookie = common::login_user(&app.router, "alice_d18", "pw")
        .await
        .expect("login alice");

    let cat = create_category(&app, &alice_cookie, "Dining").await;

    let (status, _) = json_request(
        &app,
        "POST",
        "/splits/create",
        &alice_cookie,
        json!({
            "idempotency_key": "d18-split-1",
            "total_amount": 60.0,
            "description": "d18 failing split",
            "date": "2026-02-20",
            "category_id": cat,
            "splits": [{ "user_id": bob_id, "amount": 30.0 }]  // bob not a friend
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "non-friend split must fail");

    // Zero records must exist in the shared DB for both users
    let alice_count = count_records_for_user(&app, &alice_id).await;
    let bob_count = count_records_for_user(&app, &bob_id).await;
    assert_eq!(alice_count, 0, "alice must have no records after failed split");
    assert_eq!(bob_count, 0, "bob must have no records after failed split");
}

// ---------------------------------------------------------------------------
// E19: No duplicate payer record on retry after partial failure
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e19_no_duplicate_payer_record_on_retry() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice_e19", "pw")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, "bob_e19", "pw")
        .await
        .expect("create bob");

    let alice_cookie = common::login_user(&app.router, "alice_e19", "pw")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob_e19", "pw")
        .await
        .expect("login bob");

    send_friend_request(&app, &alice_cookie, "bob_e19").await;
    accept_friend(&app, &bob_cookie, &alice_id).await;

    let cat = create_category(&app, &alice_cookie, "Dining").await;
    let payload = json!({
        "idempotency_key": "e19-retry-split-1",
        "total_amount": 60.0,
        "description": "e19 retry split",
        "date": "2026-02-20",
        "category_id": cat,
        "splits": [{ "user_id": bob_id, "amount": 30.0 }]
    });

    // Simulate a client retrying three times with the same key
    let (s1, b1) = json_request(&app, "POST", "/splits/create", &alice_cookie, payload.clone()).await;
    let (s2, b2) = json_request(&app, "POST", "/splits/create", &alice_cookie, payload.clone()).await;
    let (s3, b3) = json_request(&app, "POST", "/splits/create", &alice_cookie, payload).await;

    assert_eq!(s1, StatusCode::CREATED);
    assert_eq!(s2, StatusCode::CREATED);
    assert_eq!(s3, StatusCode::CREATED);
    assert_eq!(b1, b2, "all retries must return same body");
    assert_eq!(b2, b3, "all retries must return same body");

    // Exactly 1 payer record for alice in the shared DB
    let alice_count = count_records_for_user(&app, &alice_id).await;
    assert_eq!(alice_count, 1, "no duplicate payer record after retries");

    // Exactly 1 pending record for bob
    let bob_count = count_records_for_user(&app, &bob_id).await;
    assert_eq!(bob_count, 1, "no duplicate pending record after retries");
}

// ---------------------------------------------------------------------------
// E20: No retry/fanout endpoint exists (should 404)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e20_retry_fanout_endpoint_does_not_exist() {
    let app = common::setup_test_app().await.expect("setup failed");

    common::create_test_user(&app.state, "alice_e20", "pw")
        .await
        .expect("create alice");
    let alice_cookie = common::login_user(&app.router, "alice_e20", "pw")
        .await
        .expect("login alice");

    let (status, _) = json_request(
        &app,
        "POST",
        "/splits/retry-fanout",
        &alice_cookie,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "retry-fanout endpoint must not exist");
}

// ---------------------------------------------------------------------------
// E21: Stale/in-progress idempotency key does not trigger replay
//      (a NULL-body entry — which could arise from an old crashed write —
//      must not be replayed, and the endpoint must return a sensible error
//      rather than creating duplicate records)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e21_stale_null_body_idempotency_key_does_not_replay() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice_e21", "pw")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, "bob_e21", "pw")
        .await
        .expect("create bob");

    let alice_cookie = common::login_user(&app.router, "alice_e21", "pw")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob_e21", "pw")
        .await
        .expect("login bob");

    send_friend_request(&app, &alice_cookie, "bob_e21").await;
    accept_friend(&app, &bob_cookie, &alice_id).await;

    let cat = create_category(&app, &alice_cookie, "Dining").await;
    let key = "e21-stale-key-1";

    // Manually insert a "stale" idempotency entry with NULL response_body
    // simulating a crash mid-write.
    {
        use time::OffsetDateTime;
        let now = OffsetDateTime::now_utc();
        let expires = now + time::Duration::hours(24);
        let fmt = time::format_description::well_known::Rfc3339;
        let conn = app.state.main_db.write().await;
        conn.execute(
            "INSERT INTO idempotency_keys \
             (key, user_id, endpoint, payload_hash, response_status, response_body, created_at, expires_at) \
             VALUES (?, ?, ?, ?, ?, NULL, ?, ?)",
            (
                key,
                alice_id.as_str(),
                "/splits/create",
                "somehash",
                201i64,
                now.format(&fmt).unwrap().as_str(),
                expires.format(&fmt).unwrap().as_str(),
            ),
        )
        .await
        .expect("insert stale idempotency row");
    }

    // Now a real request arrives.  It must NOT try to replay the NULL body.
    // It should either proceed with a fresh write OR return a 5xx/4xx.
    // The critical invariant: no duplicate records must be created.
    let payload = json!({
        "idempotency_key": key,
        "total_amount": 60.0,
        "description": "e21 stale key test",
        "date": "2026-02-20",
        "category_id": cat,
        "splits": [{ "user_id": bob_id, "amount": 30.0 }]
    });
    let (status, _) = json_request(&app, "POST", "/splits/create", &alice_cookie, payload).await;

    // Must NOT 500 with a "replay null body" panic
    assert_ne!(status, StatusCode::INTERNAL_SERVER_ERROR, "must not crash on stale key");

    // Must NOT have created duplicate records either
    let alice_count = count_records_for_user(&app, &alice_id).await;
    let bob_count = count_records_for_user(&app, &bob_id).await;
    assert!(alice_count <= 1, "alice must have at most 1 record (no duplicates), got {alice_count}");
    assert!(bob_count <= 1, "bob must have at most 1 record (no duplicates), got {bob_count}");
}

// ---------------------------------------------------------------------------
// F22: Concurrent create_split with same key: only one set of records written
// ---------------------------------------------------------------------------

#[tokio::test]
async fn f22_concurrent_split_same_key_single_write() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice_f22", "pw")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, "bob_f22", "pw")
        .await
        .expect("create bob");

    let alice_cookie = common::login_user(&app.router, "alice_f22", "pw")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob_f22", "pw")
        .await
        .expect("login bob");

    send_friend_request(&app, &alice_cookie, "bob_f22").await;
    accept_friend(&app, &bob_cookie, &alice_id).await;

    let cat = create_category(&app, &alice_cookie, "Dining").await;
    let shared_payload = Arc::new(json!({
        "idempotency_key": "f22-concurrent-split-1",
        "total_amount": 60.0,
        "description": "f22 concurrent",
        "date": "2026-02-20",
        "category_id": cat,
        "splits": [{ "user_id": bob_id, "amount": 30.0 }]
    }));

    // Fire 5 concurrent requests with the same idempotency key
    let mut handles = Vec::new();
    for _ in 0..5 {
        let router = app.router.clone();
        let cookie = alice_cookie.clone();
        let payload = shared_payload.clone();
        handles.push(tokio::spawn(async move {
            let req = Request::builder()
                .method("POST")
                .uri("/splits/create")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .expect("build request");
            let resp = router.oneshot(req).await.expect("execute");
            let status = resp.status();
            let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .expect("read body");
            let body: Value = serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8(bytes.to_vec()).unwrap()));
            (status, body)
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await.expect("join"));
    }

    let created: Vec<_> = results
        .iter()
        .filter(|(s, _)| *s == StatusCode::CREATED)
        .collect();
    assert!(!created.is_empty(), "at least one request must succeed");

    // All successful responses must be identical (idempotent)
    if created.len() > 1 {
        let first_body = &created[0].1;
        for (_, body) in &created[1..] {
            assert_eq!(body, first_body, "all CREATED responses must be identical");
        }
    }

    // Exactly 1 record per user in the shared DB
    let alice_count = count_records_for_user(&app, &alice_id).await;
    let bob_count = count_records_for_user(&app, &bob_id).await;
    assert_eq!(alice_count, 1, "exactly 1 payer record in shared DB");
    assert_eq!(bob_count, 1, "exactly 1 pending record in shared DB");
}

// ---------------------------------------------------------------------------
// F23: Concurrent finalize on same pending record: only one succeeds
// ---------------------------------------------------------------------------

#[tokio::test]
async fn f23_concurrent_finalize_only_one_succeeds() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice_f23", "pw")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, "bob_f23", "pw")
        .await
        .expect("create bob");

    let alice_cookie = common::login_user(&app.router, "alice_f23", "pw")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob_f23", "pw")
        .await
        .expect("login bob");

    send_friend_request(&app, &alice_cookie, "bob_f23").await;
    accept_friend(&app, &bob_cookie, &alice_id).await;

    let alice_cat = create_category(&app, &alice_cookie, "Dining").await;
    let bob_cat = create_category(&app, &bob_cookie, "BobDining").await;

    let (split_status, split_body) = json_request(
        &app,
        "POST",
        "/splits/create",
        &alice_cookie,
        json!({
            "idempotency_key": "f23-finalize-split-1",
            "total_amount": 60.0,
            "description": "f23 split",
            "date": "2026-02-20",
            "category_id": alice_cat,
            "splits": [{ "user_id": bob_id, "amount": 30.0 }]
        }),
    )
    .await;
    assert_eq!(split_status, StatusCode::CREATED);

    let pending_id = split_body["pending_record_ids"][0]
        .as_str()
        .expect("pending id")
        .to_string();

    let finalize_payload = Arc::new(json!({
        "record_id": pending_id,
        "category_id": bob_cat,
    }));

    // Fire 3 concurrent finalize requests
    let mut handles = Vec::new();
    for _ in 0..3 {
        let router = app.router.clone();
        let cookie = bob_cookie.clone();
        let payload = finalize_payload.clone();
        handles.push(tokio::spawn(async move {
            let req = Request::builder()
                .method("POST")
                .uri("/records/finalize-pending")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .expect("build request");
            router.oneshot(req).await.expect("execute").status()
        }));
    }

    let mut statuses: Vec<StatusCode> = Vec::new();
    for handle in handles {
        statuses.push(handle.await.expect("join"));
    }

    let ok_count = statuses.iter().filter(|&&s| s == StatusCode::OK).count();
    let conflict_count = statuses
        .iter()
        .filter(|&&s| s == StatusCode::CONFLICT)
        .count();
    assert_eq!(ok_count, 1, "exactly one finalize must succeed");
    assert_eq!(conflict_count, 2, "the other two must conflict");

    let _ = (alice_id, bob_id);
}

// ---------------------------------------------------------------------------
// F24: Concurrent settle on same record: result is consistent
// ---------------------------------------------------------------------------

#[tokio::test]
async fn f24_concurrent_settle_result_is_consistent() {
    let app = common::setup_test_app().await.expect("setup failed");

    let alice_id = common::create_test_user(&app.state, "alice_f24", "pw")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, "bob_f24", "pw")
        .await
        .expect("create bob");

    let alice_cookie = common::login_user(&app.router, "alice_f24", "pw")
        .await
        .expect("login alice");
    let bob_cookie = common::login_user(&app.router, "bob_f24", "pw")
        .await
        .expect("login bob");

    send_friend_request(&app, &alice_cookie, "bob_f24").await;
    accept_friend(&app, &bob_cookie, &alice_id).await;

    let alice_cat = create_category(&app, &alice_cookie, "Dining").await;

    let (split_status, split_body) = json_request(
        &app,
        "POST",
        "/splits/create",
        &alice_cookie,
        json!({
            "idempotency_key": "f24-settle-split-1",
            "total_amount": 60.0,
            "description": "f24 split",
            "date": "2026-02-20",
            "category_id": alice_cat,
            "splits": [{ "user_id": bob_id, "amount": 30.0 }]
        }),
    )
    .await;
    assert_eq!(split_status, StatusCode::CREATED);

    let split_id = split_body["split_id"].as_str().expect("split_id").to_string();
    let bob_record_id = split_body["pending_record_ids"][0]
        .as_str()
        .expect("pending id")
        .to_string();

    let settle_payload = Arc::new(json!({ "split_id": split_id }));

    // Fire 4 concurrent settle requests from Bob
    let mut handles = Vec::new();
    for _ in 0..4 {
        let router = app.router.clone();
        let cookie = bob_cookie.clone();
        let payload = settle_payload.clone();
        let rec_id = bob_record_id.clone();
        handles.push(tokio::spawn(async move {
            let req = Request::builder()
                .method("PUT")
                .uri(format!("/records/{rec_id}/settle"))
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .expect("build request");
            router.oneshot(req).await.expect("execute").status()
        }));
    }

    let mut statuses: Vec<StatusCode> = Vec::new();
    for handle in handles {
        statuses.push(handle.await.expect("join"));
    }

    // Settle is idempotent — all must succeed with 200
    for s in &statuses {
        assert_eq!(*s, StatusCode::OK, "settle must be idempotent (all 200), got {s}");
    }

    // Record must be settled exactly once in the shared DB
    let conn = app.state.main_db.read().await;
    let mut rows = conn
        .query(
            "SELECT settle FROM records WHERE id = ? AND owner_user_id = ?",
            [bob_record_id.as_str(), bob_id.as_str()],
        )
        .await
        .expect("query settle status");
    let row = rows.next().await.expect("next").expect("row exists");
    let settled: bool = row.get(0).expect("settle flag");
    assert!(settled, "record must be settled in shared DB");

    let _ = alice_id;
}
