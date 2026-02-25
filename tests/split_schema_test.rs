use kash_server::init_main_db;
use std::path::PathBuf;

#[tokio::test]
async fn split_schema_tables_exist() {
    // Setup: create temp dir for test database
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let data_path = temp_dir.path().to_str().expect("temp path to string");

    // Initialize main database - should create schema
    init_main_db(data_path).await.expect("init_main_db failed");

    // Connect to the created database for verification
    let db_path = PathBuf::from(data_path).join("users.db");
    let db = libsql::Builder::new_local(&db_path)
        .build()
        .await
        .expect("failed to build db");
    let conn = db.connect().expect("failed to connect");

    // Test 1: Verify friendship table exists
    let mut rows = conn
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='friendship'",
            (),
        )
        .await
        .expect("query failed");

    assert!(
        rows.next().await.expect("rows.next failed").is_some(),
        "friendship table should exist"
    );

    // Test 2: Verify idempotency_keys table exists
    let mut rows = conn
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='idempotency_keys'",
            (),
        )
        .await
        .expect("query failed");

    assert!(
        rows.next().await.expect("rows.next failed").is_some(),
        "idempotency_keys table should exist"
    );

    // Test 4: Verify friendship_relations indexes exist
    let mut rows = conn
        .query(
            "SELECT name FROM sqlite_master WHERE type='index' AND name='idx_friendship_from'",
            (),
        )
        .await
        .expect("query failed");

    assert!(
        rows.next().await.expect("rows.next failed").is_some(),
        "idx_friendship_from index should exist"
    );

    let mut rows = conn
        .query(
            "SELECT name FROM sqlite_master WHERE type='index' AND name='idx_friendship_to'",
            (),
        )
        .await
        .expect("query failed");

    assert!(
        rows.next().await.expect("rows.next failed").is_some(),
        "idx_friendship_to index should exist"
    );

    // Test 6: Verify idempotency_keys index exists
    let mut rows = conn
        .query(
            "SELECT name FROM sqlite_master WHERE type='index' AND name='idx_idempotency_user'",
            (),
        )
        .await
        .expect("query failed");

    assert!(
        rows.next().await.expect("rows.next failed").is_some(),
        "idx_idempotency_user index should exist"
    );

    // Test 7: Verify UNIQUE constraint on idempotency_keys is enforced
    // Insert a test key
    conn.execute(
        "INSERT INTO idempotency_keys (id, key, user_id, endpoint, payload_hash, response_status, created_at, expires_at) 
         VALUES ('idem-id-1', 'test_key_1', 'user_123', '/api/test', 'hash_abc', 200, '2026-02-16', '2026-03-16')",
        (),
    )
    .await
    .expect("initial insert failed");

    // Try to insert duplicate key - should fail with UNIQUE constraint
    let result = conn
        .execute(
            "INSERT INTO idempotency_keys (id, key, user_id, endpoint, payload_hash, response_status, created_at, expires_at) 
             VALUES ('idem-id-2', 'test_key_1', 'user_123', '/api/test', 'hash_xyz', 201, '2026-02-16', '2026-03-16')",
            (),
        )
        .await;

    assert!(
        result.is_err(),
        "UNIQUE constraint on idempotency_keys(user_id, endpoint, key) should reject duplicate entry"
    );

    println!("✓ All split schema tests passed");
}
