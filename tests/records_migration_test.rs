use libsql::Builder;
use std::path::Path;
use tokio::fs;

const CREATE_RECORDS_TABLE_OLD: &str = r#"
CREATE TABLE IF NOT EXISTS records (
    id          TEXT    PRIMARY KEY,
    name        TEXT    NOT NULL,
    amount      REAL    NOT NULL,
    category_id TEXT    NOT NULL,
    date        TEXT    NOT NULL
);
"#;

async fn ensure_records_split_columns(conn: &libsql::Connection) -> anyhow::Result<()> {
    let mut rows = conn.query("PRAGMA table_info(records)", ()).await?;
    let mut has_pending = false;
    let mut has_split_id = false;
    let mut has_settle = false;
    let mut has_debtor_user_id = false;
    let mut has_creditor_user_id = false;

    while let Some(row) = rows.next().await? {
        let name: String = row.get(1)?;
        match name.as_str() {
            "pending" => has_pending = true,
            "split_id" => has_split_id = true,
            "settle" => has_settle = true,
            "debtor_user_id" => has_debtor_user_id = true,
            "creditor_user_id" => has_creditor_user_id = true,
            _ => {}
        }
    }

    if !has_pending {
        conn.execute(
            "ALTER TABLE records ADD COLUMN pending BOOLEAN NOT NULL DEFAULT 0",
            (),
        )
        .await?;
    }

    if !has_split_id {
        conn.execute("ALTER TABLE records ADD COLUMN split_id TEXT", ())
            .await?;
    }

    if !has_settle {
        conn.execute(
            "ALTER TABLE records ADD COLUMN settle BOOLEAN NOT NULL DEFAULT 0",
            (),
        )
        .await?;
    }

    if !has_debtor_user_id {
        conn.execute("ALTER TABLE records ADD COLUMN debtor_user_id TEXT", ())
            .await?;
    }

    if !has_creditor_user_id {
        conn.execute("ALTER TABLE records ADD COLUMN creditor_user_id TEXT", ())
            .await?;
    }

    Ok(())
}

#[tokio::test]
async fn test_records_additive_migration_existing_db() -> anyhow::Result<()> {
    let test_dir = "./test_db_migration";
    fs::create_dir_all(test_dir).await?;
    let db_path = Path::new(test_dir).join("user_test.db");

    if db_path.exists() {
        fs::remove_file(&db_path).await?;
    }

    let db = Builder::new_local(&db_path).build().await?;
    let conn = db.connect()?;

    println!("=== STEP 1: Create old schema without split columns ===");
    conn.execute(CREATE_RECORDS_TABLE_OLD, ()).await?;

    let mut rows = conn.query("PRAGMA table_info(records)", ()).await?;
    let mut columns_before = Vec::new();
    while let Some(row) = rows.next().await? {
        let name: String = row.get(1)?;
        columns_before.push(name.clone());
        println!("  Column: {}", name);
    }

    assert!(
        !columns_before.contains(&"pending".to_string()),
        "pending column should not exist before migration"
    );
    assert!(
        !columns_before.contains(&"split_id".to_string()),
        "split_id column should not exist before migration"
    );
    assert!(
        !columns_before.contains(&"settle".to_string()),
        "settle column should not exist before migration"
    );
    assert!(
        !columns_before.contains(&"debtor_user_id".to_string()),
        "debtor_user_id column should not exist before migration"
    );
    assert!(
        !columns_before.contains(&"creditor_user_id".to_string()),
        "creditor_user_id column should not exist before migration"
    );

    println!("=== STEP 2: Insert legacy record without split fields ===");
    conn.execute(
        "INSERT INTO records (id, name, amount, category_id, date) VALUES (?, ?, ?, ?, ?)",
        ("rec_123", "Old record", 100.0, "cat_food", "2024-01-01"),
    )
    .await?;

    let mut legacy_rows = conn
        .query(
            "SELECT id, name, amount FROM records WHERE id = ?",
            ["rec_123"],
        )
        .await?;
    let legacy_exists = legacy_rows.next().await?.is_some();
    assert!(legacy_exists, "Legacy record should exist before migration");
    println!("  Inserted legacy record: rec_123");

    println!("=== STEP 3: Run migration ===");
    ensure_records_split_columns(&conn).await?;

    println!("=== STEP 4: Verify columns exist after migration ===");
    let mut rows = conn.query("PRAGMA table_info(records)", ()).await?;
    let mut columns_after = Vec::new();
    while let Some(row) = rows.next().await? {
        let name: String = row.get(1)?;
        columns_after.push(name.clone());
        println!("  Column: {}", name);
    }

    assert!(
        columns_after.contains(&"pending".to_string()),
        "pending column should exist after migration"
    );
    assert!(
        columns_after.contains(&"split_id".to_string()),
        "split_id column should exist after migration"
    );
    assert!(
        columns_after.contains(&"settle".to_string()),
        "settle column should exist after migration"
    );
    assert!(
        columns_after.contains(&"debtor_user_id".to_string()),
        "debtor_user_id column should exist after migration"
    );
    assert!(
        columns_after.contains(&"creditor_user_id".to_string()),
        "creditor_user_id column should exist after migration"
    );

    println!("=== STEP 5: Verify legacy record still queryable ===");
    let mut legacy_rows = conn
        .query(
            "SELECT id, name, amount, category_id, date FROM records WHERE id = ?",
            ["rec_123"],
        )
        .await?;

    if let Some(row) = legacy_rows.next().await? {
        let id: String = row.get(0)?;
        let name: String = row.get(1)?;
        let amount: f64 = row.get(2)?;
        let category_id: String = row.get(3)?;
        let date: String = row.get(4)?;

        println!(
            "  Legacy record retrieved: id={}, name={}, amount={}, category_id={}, date={}",
            id, name, amount, category_id, date
        );

        assert_eq!(id, "rec_123");
        assert_eq!(name, "Old record");
        assert_eq!(amount, 100.0);
        assert_eq!(category_id, "cat_food");
        assert_eq!(date, "2024-01-01");
    } else {
        panic!("Legacy record should exist after migration");
    }

    println!("=== STEP 6: Insert new record with split fields ===");
    conn.execute(
        "INSERT INTO records (id, name, amount, category_id, date, pending, split_id, settle, debtor_user_id, creditor_user_id) 
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        (
            "rec_456",
            "Split record",
            50.0,
            "cat_food",
            "2024-01-02",
            true,
            "split_xyz",
            false,
            "user_alice",
            "user_bob",
        ),
    )
    .await?;

    let mut split_rows = conn
        .query(
            "SELECT id, pending, split_id, settle, debtor_user_id, creditor_user_id FROM records WHERE id = ?",
            ["rec_456"],
        )
        .await?;

    if let Some(row) = split_rows.next().await? {
        let id: String = row.get(0)?;
        let pending: bool = row.get(1)?;
        let split_id: String = row.get(2)?;
        let settle: bool = row.get(3)?;
        let debtor_user_id: String = row.get(4)?;
        let creditor_user_id: String = row.get(5)?;

        println!(
            "  New split record: id={}, pending={}, split_id={}, settle={}, debtor={}, creditor={}",
            id, pending, split_id, settle, debtor_user_id, creditor_user_id
        );

        assert_eq!(id, "rec_456");
        assert!(pending);
        assert_eq!(split_id, "split_xyz");
        assert!(!settle);
        assert_eq!(debtor_user_id, "user_alice");
        assert_eq!(creditor_user_id, "user_bob");
    } else {
        panic!("New split record should exist");
    }

    println!("=== STEP 7: Verify idempotency - run migration again ===");
    ensure_records_split_columns(&conn).await?;
    println!("  Migration ran successfully again (idempotent)");

    let mut rows = conn.query("PRAGMA table_info(records)", ()).await?;
    let mut columns_final = Vec::new();
    while let Some(row) = rows.next().await? {
        let name: String = row.get(1)?;
        columns_final.push(name);
    }

    assert_eq!(
        columns_after, columns_final,
        "Columns should be identical after re-running migration"
    );

    println!("=== STEP 8: Verify both records still intact ===");
    let mut all_rows = conn.query("SELECT COUNT(*) FROM records", ()).await?;
    if let Some(row) = all_rows.next().await? {
        let count: u32 = row.get(0)?;
        assert_eq!(count, 2, "Should have exactly 2 records");
        println!("  Total records: {}", count);
    }

    fs::remove_dir_all(test_dir).await?;

    println!("\n✓ Test passed: records migration is idempotent and preserves existing data");

    Ok(())
}
