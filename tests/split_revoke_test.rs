mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::util::ServiceExt;

struct Fixture {
    app: common::TestApp,
    cookies: Vec<String>,
    users: Vec<String>,
    categories: Vec<String>,
    payload: Value,
    split: Value,
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

async fn fixture() -> Fixture {
    let app = common::setup_test_app().await.expect("setup app");
    let mut cookies = Vec::new();
    let mut users = Vec::new();
    let mut categories = Vec::new();
    for username in ["alice_revoke", "bob_revoke", "carol_revoke"] {
        users.push(
            common::create_test_user(&app.state, username, "password123")
                .await
                .expect("create user"),
        );
        let cookie = common::login_user(&app.router, username, "password123")
            .await
            .expect("login user");
        let (status, category) = request(
            &app,
            &cookie,
            "POST",
            "/categories",
            Some(json!({"name": "Dining", "is_income": false})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        categories.push(category["id"].as_str().expect("category id").to_string());
        cookies.push(cookie);
    }
    for (index, username) in [(1, "bob_revoke"), (2, "carol_revoke")] {
        assert_eq!(
            request(
                &app,
                &cookies[0],
                "POST",
                "/friends/request",
                Some(json!({"friend_username": username})),
            )
            .await
            .0,
            StatusCode::CREATED
        );
        assert_eq!(
            request(
                &app,
                &cookies[index],
                "POST",
                "/friends/accept",
                Some(json!({"friend_id": users[0]})),
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    let payload = json!({
        "idempotency_key": "revoke-test",
        "total_amount": 120,
        "currency": "TWD",
        "description": "Dinner",
        "date": "2026-02-16",
        "category_id": categories[0],
        "splits": [
            {"user_id": users[1], "amount": 40},
            {"user_id": users[2], "amount": 40}
        ]
    });
    let (status, split) =
        request(&app, &cookies[0], "POST", "/splits", Some(payload.clone())).await;
    assert_eq!(status, StatusCode::CREATED);
    Fixture {
        app,
        cookies,
        users,
        categories,
        payload,
        split,
    }
}

impl Fixture {
    fn uri(&self) -> String {
        format!(
            "/splits/{}",
            self.split["split_id"].as_str().expect("split id")
        )
    }

    fn participant(&self, index: usize) -> &str {
        self.split["participants"]
            .as_array()
            .expect("participants")
            .iter()
            .find(|p| p["debtor_user_id"] == self.users[index])
            .expect("participant")["id"]
            .as_str()
            .expect("participant id")
    }

    async fn records(&self, index: usize) -> Value {
        let (status, records) =
            request(&self.app, &self.cookies[index], "GET", "/records", None).await;
        assert_eq!(status, StatusCode::OK);
        records
    }
}

#[tokio::test]
async fn revoke_preserves_records_and_removes_all_shares() {
    let f = fixture().await;
    let finalize_uri = format!("/splits/participants/{}/finalize", f.participant(1));
    let (status, finalized) = request(
        &f.app,
        &f.cookies[1],
        "POST",
        &finalize_uri,
        Some(json!({"category_id": f.categories[1]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let before = [f.records(0).await, f.records(1).await, f.records(2).await];

    // Live idempotent replay still works before revocation.
    let replay = request(
        &f.app,
        &f.cookies[0],
        "POST",
        "/splits",
        Some(f.payload.clone()),
    )
    .await;
    assert_eq!(replay, (StatusCode::CREATED, f.split.clone()));
    assert_eq!(
        request(&f.app, &f.cookies[0], "DELETE", &f.uri(), None)
            .await
            .0,
        StatusCode::OK
    );

    for (index, original) in before.iter().enumerate() {
        assert_eq!(&f.records(index).await, original);
    }
    for index in [1, 2] {
        let pending = request(&f.app, &f.cookies[index], "GET", "/splits/pending", None).await;
        assert_eq!(pending.0, StatusCode::OK);
        assert_eq!(pending.1["total_count"], 0);
        for (viewer, friend) in [(0, index), (index, 0)] {
            let unsettled = request(
                &f.app,
                &f.cookies[viewer],
                "GET",
                &format!("/splits/unsettled?friend_id={}", f.users[friend]),
                None,
            )
            .await;
            assert_eq!(unsettled.0, StatusCode::OK);
            assert_eq!(unsettled.1["total_count"], 0);
        }
        let settle_uri = format!("/splits/participants/{}/settle", f.participant(index));
        assert_eq!(
            request(&f.app, &f.cookies[index], "PUT", &settle_uri, None)
                .await
                .0,
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        request(&f.app, &f.cookies[0], "DELETE", &f.uri(), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(
            &f.app,
            &f.cookies[0],
            "POST",
            "/splits",
            Some(f.payload.clone())
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(f.records(0).await, before[0]);

    // Retained records remain independently editable and deletable.
    let record_uri = format!("/records/{}", finalized["id"].as_str().expect("record id"));
    assert_eq!(
        request(
            &f.app,
            &f.cookies[1],
            "PUT",
            &record_uri,
            Some(json!({"name": "Corrected dinner"}))
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(&f.app, &f.cookies[1], "DELETE", &record_uri, None)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn only_creator_can_revoke() {
    let f = fixture().await;
    assert_eq!(
        request(&f.app, "", "DELETE", &f.uri(), None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&f.app, &f.cookies[1], "DELETE", &f.uri(), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    common::create_test_user(&f.app.state, "outsider_revoke", "password123")
        .await
        .expect("create outsider");
    let outsider = common::login_user(&f.app.router, "outsider_revoke", "password123")
        .await
        .expect("login outsider");
    assert_eq!(
        request(&f.app, &outsider, "DELETE", &f.uri(), None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(&f.app, &f.cookies[0], "DELETE", "/splits/missing", None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(&f.app, &f.cookies[0], "DELETE", &f.uri(), None)
            .await
            .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn one_settled_share_blocks_entire_revocation() {
    let f = fixture().await;
    let settle_uri = format!("/splits/participants/{}/settle", f.participant(2));
    assert_eq!(
        request(&f.app, &f.cookies[2], "PUT", &settle_uri, None)
            .await
            .0,
        StatusCode::OK
    );
    let before = f.records(0).await;
    assert_eq!(
        request(&f.app, &f.cookies[0], "DELETE", &f.uri(), None)
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(f.records(0).await, before);
    let unsettled = request(
        &f.app,
        &f.cookies[0],
        "GET",
        &format!("/splits/unsettled?friend_id={}", f.users[1]),
        None,
    )
    .await;
    assert_eq!(unsettled.0, StatusCode::OK);
    assert_eq!(unsettled.1["total_count"], 1);
    let replay = request(
        &f.app,
        &f.cookies[0],
        "POST",
        "/splits",
        Some(f.payload.clone()),
    )
    .await;
    assert_eq!(replay, (StatusCode::CREATED, f.split));
}
