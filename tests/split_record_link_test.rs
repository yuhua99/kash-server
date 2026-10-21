mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::util::ServiceExt;

struct Fixture {
    app: common::TestApp,
    alice: String,
    bob: String,
    bob_id: String,
    dining: String,
    salary: String,
}

async fn request(
    app: &common::TestApp,
    cookie: &str,
    method: &str,
    uri: &str,
    payload: Option<Value>,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body(payload.map_or_else(Body::empty, |value| Body::from(value.to_string())))
        .expect("build request");
    let response = app.router.clone().oneshot(request).await.expect("request");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response");
    let value = serde_json::from_slice(&body)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&body).into_owned()));
    (status, value)
}

async fn fixture(suffix: &str) -> Fixture {
    let app = common::setup_test_app().await.expect("setup app");
    let alice_name = format!("alice_{suffix}");
    let bob_name = format!("bob_{suffix}");
    let alice_id = common::create_test_user(&app.state, &alice_name, "password123")
        .await
        .expect("create alice");
    let bob_id = common::create_test_user(&app.state, &bob_name, "password123")
        .await
        .expect("create bob");
    let alice = common::login_user(&app.router, &alice_name, "password123")
        .await
        .expect("login alice");
    let bob = common::login_user(&app.router, &bob_name, "password123")
        .await
        .expect("login bob");

    let (status, _) = request(
        &app,
        &alice,
        "POST",
        "/friends/request",
        Some(json!({"friend_username": bob_name})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = request(
        &app,
        &bob,
        "POST",
        "/friends/accept",
        Some(json!({"friend_id": alice_id})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let mut category_ids = Vec::new();
    for (name, is_income) in [("Dining", false), ("Salary", true)] {
        let (status, category) = request(
            &app,
            &alice,
            "POST",
            "/categories",
            Some(json!({"name": name, "is_income": is_income})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        category_ids.push(category["id"].as_str().expect("category id").to_string());
    }

    Fixture {
        app,
        alice,
        bob,
        bob_id,
        dining: category_ids[0].clone(),
        salary: category_ids[1].clone(),
    }
}

impl Fixture {
    async fn create_split(&self, key: &str, total: f64, bob_share: f64) -> Value {
        let (status, split) = request(
            &self.app,
            &self.alice,
            "POST",
            "/splits",
            Some(json!({
                "idempotency_key": key,
                "total_amount": total,
                "currency": "TWD",
                "description": "Dinner",
                "date": "2026-02-16",
                "category_id": self.dining,
                "splits": [{"user_id": self.bob_id, "amount": bob_share}]
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        split
    }

    async fn alice_record(&self, record_id: &str) -> Option<Value> {
        let (status, body) = request(&self.app, &self.alice, "GET", "/records", None).await;
        assert_eq!(status, StatusCode::OK);
        body["records"]
            .as_array()
            .expect("records")
            .iter()
            .find(|record| record["id"] == record_id)
            .cloned()
    }

    async fn bob_pending(&self) -> Value {
        let (status, body) = request(&self.app, &self.bob, "GET", "/splits/pending", None).await;
        assert_eq!(status, StatusCode::OK);
        body
    }
}

fn record_uri(split: &Value) -> String {
    format!(
        "/records/{}",
        split["creditor_record_id"].as_str().expect("record id")
    )
}

#[tokio::test]
async fn split_record_stores_total_and_reports_my_share() {
    let f = fixture("srl_share").await;
    let split = f.create_split("srl-share", 100.0, 60.0).await;
    let record = f
        .alice_record(split["creditor_record_id"].as_str().expect("record id"))
        .await
        .expect("creditor record listed");
    assert_eq!(record["amount"], -100.0);
    assert_eq!(record["my_amount"], -40.0);
    assert_eq!(record["split_id"], split["split_id"]);

    let full = f.create_split("srl-full", 50.0, 50.0).await;
    let record = f
        .alice_record(full["creditor_record_id"].as_str().expect("record id"))
        .await
        .expect("full-share record listed");
    assert_eq!(record["amount"], -50.0);
    assert_eq!(record["my_amount"], 0.0);
}

#[tokio::test]
async fn editing_split_record_validates_and_syncs_split() {
    let f = fixture("srl_edit").await;
    let split = f.create_split("srl-edit", 100.0, 60.0).await;
    let uri = record_uri(&split);

    let (status, _) = request(&f.app, &f.alice, "PUT", &uri, Some(json!({"amount": 50.0}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = request(
        &f.app,
        &f.alice,
        "PUT",
        &uri,
        Some(json!({"category_id": f.salary})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, updated) = request(
        &f.app,
        &f.alice,
        "PUT",
        &uri,
        Some(json!({"amount": 150.0, "name": "Team dinner", "date": "2026-02-20"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["amount"], -150.0);
    assert_eq!(updated["my_amount"], -90.0);
    assert_eq!(updated["split_id"], split["split_id"]);

    let (status, updated) =
        request(&f.app, &f.alice, "PUT", &uri, Some(json!({"amount": 60.0}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["my_amount"], 0.0);

    let (status, updated) = request(
        &f.app,
        &f.alice,
        "PUT",
        &uri,
        Some(json!({"amount": 150.0})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["my_amount"], -90.0);

    let pending = f.bob_pending().await;
    assert_eq!(pending["shares"][0]["description"], "Team dinner");
    assert_eq!(pending["shares"][0]["date"], "2026-02-20");

    let conn = f.app.state.main_db.connect().expect("connect db");
    let mut rows = conn
        .query(
            "SELECT total_amount FROM splits WHERE id = ?",
            [split["split_id"].as_str().expect("split id")],
        )
        .await
        .expect("query split");
    let row = rows.next().await.expect("next row").expect("split row");
    assert_eq!(row.get::<i64>(0).expect("total"), 15000);
}

#[tokio::test]
async fn deleting_unsettled_split_record_revokes_split() {
    let f = fixture("srl_delete").await;
    let split = f.create_split("srl-delete", 100.0, 60.0).await;
    let finalize_uri = format!(
        "/splits/participants/{}/finalize",
        split["participants"][0]["id"]
            .as_str()
            .expect("participant id")
    );
    let (status, bob_category) = request(
        &f.app,
        &f.bob,
        "POST",
        "/categories",
        Some(json!({"name": "Dining", "is_income": false})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, finalized) = request(
        &f.app,
        &f.bob,
        "POST",
        &finalize_uri,
        Some(json!({"category_id": bob_category["id"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = request(&f.app, &f.alice, "DELETE", &record_uri(&split), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(f.bob_pending().await["total_count"], 0);

    // Bob's finalized record is his own and survives the revoke.
    let (status, bob_records) = request(&f.app, &f.bob, "GET", "/records", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bob_records["records"][0]["id"], finalized["id"]);

    let split_uri = format!("/splits/{}", split["split_id"].as_str().expect("split id"));
    let (status, _) = request(&f.app, &f.alice, "DELETE", &split_uri, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Replaying the original create reports the split as revoked.
    let (status, _) = request(
        &f.app,
        &f.alice,
        "POST",
        "/splits",
        Some(json!({
            "idempotency_key": "srl-delete",
            "total_amount": 100.0,
            "currency": "TWD",
            "description": "Dinner",
            "date": "2026-02-16",
            "category_id": f.dining,
            "splits": [{"user_id": f.bob_id, "amount": 60.0}]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn deleting_split_record_with_settled_share_conflicts() {
    let f = fixture("srl_settled").await;
    let split = f.create_split("srl-settled", 100.0, 60.0).await;
    let settle_uri = format!(
        "/splits/participants/{}/settle",
        split["participants"][0]["id"]
            .as_str()
            .expect("participant id")
    );
    let (status, _) = request(&f.app, &f.bob, "PUT", &settle_uri, None).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = request(&f.app, &f.alice, "DELETE", &record_uri(&split), None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        f.alice_record(split["creditor_record_id"].as_str().expect("record id"))
            .await
            .is_some()
    );
}
