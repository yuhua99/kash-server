mod common;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::util::ServiceExt;

fn parse_body(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8(bytes.to_vec()).expect("utf8 body")))
}

async fn request(
    router: Router,
    method: &str,
    uri: String,
    cookie: Option<&str>,
    payload: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    let body = if let Some(payload) = payload {
        builder = builder.header("content-type", "application/json");
        Body::from(payload.to_string())
    } else {
        Body::empty()
    };

    let response = router
        .oneshot(builder.body(body).expect("build request"))
        .await
        .expect("execute request");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    (status, parse_body(&bytes))
}

async fn register_and_login(app: &common::TestApp, username: &str) -> (String, String) {
    let (status, body) = request(
        app.router.clone(),
        "POST",
        "/auth/register".to_string(),
        None,
        Some(json!({ "username": username, "password": "password123" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let user_id = body["id"].as_str().expect("registered user id").to_string();
    let cookie = common::login_user(&app.router, username, "password123")
        .await
        .expect("login user");
    (user_id, cookie)
}

async fn create_category(app: &common::TestApp, cookie: &str, name: &str) -> String {
    let (status, body) = request(
        app.router.clone(),
        "POST",
        "/categories".to_string(),
        Some(cookie),
        Some(json!({ "name": name, "is_income": false })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body["id"].as_str().expect("category id").to_string()
}

async fn make_friends(
    app: &common::TestApp,
    requester_cookie: &str,
    accepter_cookie: &str,
    requester_id: &str,
    accepter_username: &str,
) {
    let (status, _) = request(
        app.router.clone(),
        "POST",
        "/friends/request".to_string(),
        Some(requester_cookie),
        Some(json!({ "friend_username": accepter_username })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = request(
        app.router.clone(),
        "POST",
        "/friends/accept".to_string(),
        Some(accepter_cookie),
        Some(json!({ "friend_id": requester_id })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

async fn split_fixture(
    suffix: &str,
) -> (
    common::TestApp,
    String,
    String,
    String,
    String,
    String,
    String,
) {
    let app = common::setup_test_app().await.expect("setup failed");
    let alice_name = format!("alice_e2e_conc_{suffix}");
    let bob_name = format!("bob_e2e_conc_{suffix}");
    let (alice_id, alice_cookie) = register_and_login(&app, &alice_name).await;
    let (bob_id, bob_cookie) = register_and_login(&app, &bob_name).await;
    make_friends(&app, &alice_cookie, &bob_cookie, &alice_id, &bob_name).await;
    let alice_category_id = create_category(&app, &alice_cookie, "Dining conc").await;
    let bob_category_id = create_category(&app, &bob_cookie, "Shared conc").await;
    (
        app,
        alice_id,
        bob_id,
        alice_cookie,
        bob_cookie,
        alice_category_id,
        bob_category_id,
    )
}

async fn create_one_split(
    app: &common::TestApp,
    alice_cookie: &str,
    bob_id: &str,
    alice_category_id: &str,
    key: &str,
) -> Value {
    let (status, body) = request(
        app.router.clone(),
        "POST",
        "/splits".to_string(),
        Some(alice_cookie),
        Some(json!({
            "idempotency_key": key,
            "total_amount": 100.0,
            "currency": "TWD",
            "description": "Concurrent split",
            "date": "2026-02-16",
            "category_id": alice_category_id,
            "splits": [{ "user_id": bob_id, "amount": 35.0 }]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body
}

#[tokio::test]
async fn concurrent_split_creation_with_same_idempotency_key_creates_one_split() {
    let (app, alice_id, bob_id, alice_cookie, _bob_cookie, alice_category_id, _bob_category_id) =
        split_fixture("create").await;

    let payload = json!({
        "idempotency_key": "e2e-concurrency-create-same-key",
        "total_amount": 100.0,
        "currency": "TWD",
        "description": "Concurrent create",
        "date": "2026-02-16",
        "category_id": alice_category_id,
        "splits": [{ "user_id": bob_id, "amount": 40.0 }]
    });

    let mut handles = Vec::new();
    for _ in 0..5 {
        let router = app.router.clone();
        let cookie = alice_cookie.clone();
        let payload = payload.clone();
        handles.push(tokio::spawn(async move {
            request(
                router,
                "POST",
                "/splits".to_string(),
                Some(&cookie),
                Some(payload),
            )
            .await
        }));
    }

    let mut created = Vec::new();
    for handle in handles {
        let (status, body) = handle.await.expect("join create task");
        if status == StatusCode::CREATED {
            created.push(body);
        } else {
            assert!(
                status == StatusCode::CONFLICT || status == StatusCode::TOO_EARLY,
                "unexpected status {status}: {body:?}"
            );
        }
    }
    assert!(
        !created.is_empty(),
        "one request should create or replay response"
    );

    let split_id = created[0]["split_id"].as_str().expect("split id");
    for body in &created {
        assert_eq!(body["split_id"], split_id);
        assert_eq!(body["creditor_record_id"], created[0]["creditor_record_id"]);
        assert_eq!(
            body["participants"][0]["id"],
            created[0]["participants"][0]["id"]
        );
    }

    let conn = app.state.main_db.connect().expect("connect db");
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM splits WHERE creditor_user_id = ?",
            [alice_id.as_str()],
        )
        .await
        .expect("count splits");
    let split_count: i64 = rows
        .next()
        .await
        .expect("next split count")
        .expect("split count row")
        .get(0)
        .expect("split count");
    assert_eq!(split_count, 1);

    let mut rows = conn
        .query(
            "SELECT COUNT(*), MIN(settled), MAX(settled), MIN(finalized_record_id IS NULL), MAX(finalized_record_id IS NULL), MIN(amount), MAX(amount) FROM split_participants WHERE split_id = ?",
            [split_id],
        )
        .await
        .expect("query participant count");
    let row = rows
        .next()
        .await
        .expect("next participant count")
        .expect("row");
    assert_eq!(row.get::<i64>(0).expect("participant count"), 1);
    assert!(!row.get::<bool>(1).expect("min settled"));
    assert!(!row.get::<bool>(2).expect("max settled"));
    assert!(row.get::<bool>(3).expect("min unfinalized"));
    assert!(row.get::<bool>(4).expect("max unfinalized"));
    assert_eq!(row.get::<i64>(5).expect("min amount"), 4000);
    assert_eq!(row.get::<i64>(6).expect("max amount"), 4000);
}

#[tokio::test]
async fn concurrent_finalize_same_participant_creates_one_debtor_record() {
    let (app, _alice_id, bob_id, alice_cookie, bob_cookie, alice_category_id, bob_category_id) =
        split_fixture("finalize").await;
    let split = create_one_split(
        &app,
        &alice_cookie,
        &bob_id,
        &alice_category_id,
        "e2e-concurrency-finalize-same-participant",
    )
    .await;
    let participant_id = split["participants"][0]["id"]
        .as_str()
        .expect("participant id")
        .to_string();

    let run = |router: Router, cookie: String, participant_id: String, category_id: String| async move {
        request(
            router,
            "POST",
            format!("/splits/participants/{participant_id}/finalize"),
            Some(&cookie),
            Some(json!({ "category_id": category_id })),
        )
        .await
    };
    let first = tokio::spawn(run(
        app.router.clone(),
        bob_cookie.clone(),
        participant_id.clone(),
        bob_category_id.clone(),
    ));
    let second = tokio::spawn(run(
        app.router.clone(),
        bob_cookie.clone(),
        participant_id.clone(),
        bob_category_id.clone(),
    ));
    let (first, second) = tokio::join!(first, second);
    let results = [
        first.expect("join first finalize"),
        second.expect("join second finalize"),
    ];
    assert_eq!(
        results
            .iter()
            .filter(|(status, _)| *status == StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|(status, _)| *status == StatusCode::CONFLICT)
            .count(),
        1
    );

    let conn = app.state.main_db.connect().expect("connect db");
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM records WHERE owner_user_id = ?",
            [bob_id.as_str()],
        )
        .await
        .expect("count debtor records");
    assert_eq!(
        rows.next()
            .await
            .expect("next debtor count")
            .expect("debtor count")
            .get::<i64>(0)
            .expect("debtor record count"),
        1
    );

    let mut rows = conn
        .query(
            "SELECT finalized_record_id IS NOT NULL FROM split_participants WHERE id = ?",
            [participant_id.as_str()],
        )
        .await
        .expect("query participant finalized");
    assert!(
        rows.next()
            .await
            .expect("next finalized")
            .expect("finalized row")
            .get::<bool>(0)
            .expect("finalized bool")
    );
}

#[tokio::test]
async fn concurrent_settle_same_participant_is_idempotent() {
    let (app, _alice_id, bob_id, alice_cookie, bob_cookie, alice_category_id, _bob_category_id) =
        split_fixture("settle").await;
    let split = create_one_split(
        &app,
        &alice_cookie,
        &bob_id,
        &alice_category_id,
        "e2e-concurrency-settle-same-participant",
    )
    .await;
    let participant_id = split["participants"][0]["id"]
        .as_str()
        .expect("participant id")
        .to_string();

    let first = tokio::spawn({
        let router = app.router.clone();
        let cookie = bob_cookie.clone();
        let participant_id = participant_id.clone();
        async move {
            request(
                router,
                "PUT",
                format!("/splits/participants/{participant_id}/settle"),
                Some(&cookie),
                None,
            )
            .await
        }
    });
    let second = tokio::spawn({
        let router = app.router.clone();
        let cookie = alice_cookie.clone();
        let participant_id = participant_id.clone();
        async move {
            request(
                router,
                "PUT",
                format!("/splits/participants/{participant_id}/settle"),
                Some(&cookie),
                None,
            )
            .await
        }
    });
    let (first, second) = tokio::join!(first, second);
    for (status, body) in [
        first.expect("join first settle"),
        second.expect("join second settle"),
    ] {
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["participant_id"], participant_id);
        assert_eq!(body["settled"], true);
    }

    let conn = app.state.main_db.connect().expect("connect db");
    let mut rows = conn
        .query(
            "SELECT settled, COUNT(*) FROM split_participants WHERE id = ?",
            [participant_id.as_str()],
        )
        .await
        .expect("query settled participant");
    let row = rows
        .next()
        .await
        .expect("next settled")
        .expect("settled row");
    assert!(row.get::<bool>(0).expect("settled"));
    assert_eq!(row.get::<i64>(1).expect("participant rows"), 1);
}
