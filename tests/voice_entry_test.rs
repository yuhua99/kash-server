mod common;

use axum::{
    Json, Router,
    body::Body,
    extract::{Multipart, State},
    http::{Request, StatusCode},
    routing::post,
};
use kash_server::models::VoiceEntryResponse;
use kash_server::records::voice::VoiceProviders;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Mutex;
use tower::util::ServiceExt;

#[derive(Default)]
struct Mock {
    requests: Vec<(String, Value)>,
    split: bool,
    low_friend_scores: bool,
    category_override: Option<String>,
    extraction: Value,
    fail_transcription: bool,
}
type MockState = Arc<Mutex<Mock>>;

async fn transcribe(
    State(state): State<MockState>,
    mut form: Multipart,
) -> Result<Json<Value>, StatusCode> {
    let mut fields = serde_json::Map::new();
    while let Some(field) = form.next_field().await.expect("multipart field") {
        let name = field.name().expect("field name").to_string();
        let value = field.text().await.expect("field value");
        fields.insert(name, json!(value));
    }
    let mut mock = state.lock().await;
    mock.requests
        .push(("transcribe".into(), Value::Object(fields)));
    if mock.fail_transcription {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    Ok(Json(json!({"text": "昨天晚餐我付九百，跟小明、小華分帳"})))
}

async fn judge(State(state): State<MockState>, Json(body): Json<Value>) -> Json<Value> {
    let mut mock = state.lock().await;
    let category = mock.category_override.clone().unwrap_or_else(|| {
        body["questions"]["category"]["criteria"]
            .as_object()
            .expect("criteria")
            .keys()
            .next()
            .expect("one category")
            .clone()
    });
    let mut answers = serde_json::Map::new();
    answers.insert(
        "category".into(),
        json!({"type":"choice","choice":category}),
    );
    answers.insert(
        "split".into(),
        json!({"type":"noul","noul":if mock.split {0.99} else {0.01}}),
    );
    let friend_keys = body["questions"]
        .as_object()
        .expect("questions")
        .keys()
        .filter(|key| key.starts_with("friend_"));
    for (i, key) in friend_keys.enumerate() {
        // Low scores rise with order, so the last friend question is the likeliest.
        let noul = if mock.low_friend_scores {
            0.1 * (i + 1) as f64
        } else {
            0.99
        };
        answers.insert(key.clone(), json!({"type":"noul","noul":noul}));
    }
    mock.requests.push(("jev".into(), body));
    Json(json!({"answers": answers}))
}

async fn extract(State(state): State<MockState>, Json(body): Json<Value>) -> Json<Value> {
    let mut mock = state.lock().await;
    mock.requests.push(("extract".into(), body));
    Json(json!({"choices":[{"message":{"content":json!({"result":mock.extraction}).to_string()}}]}))
}

struct Fixture {
    app: common::TestApp,
    cookie: String,
    owner_id: String,
    friend_ids: Vec<String>,
    mock: MockState,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl Fixture {
    async fn new(split: bool) -> Self {
        let mock = Arc::new(Mutex::new(Mock {
            split,
            extraction: json!({
                "name":"晚餐","total_amount":"900","date":"2026-10-21",
                "split_mode":if split {"equal"} else {"none"}, "friends":[]
            }),
            ..Mock::default()
        }));
        let router = Router::new()
            .route("/audio/transcriptions", post(transcribe))
            .route("/systemone", post(judge))
            .route("/chat/completions", post(extract))
            .with_state(mock.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock bind");
        let base = format!("http://{}", listener.local_addr().expect("mock address"));
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.expect("mock server");
        });
        let app = common::setup_test_app_with_voice(Some(VoiceProviders {
            openai_api_key: "test-openai".into(),
            typesafe_api_key: "test-jev".into(),
            openai_base_url: base.clone(),
            typesafe_base_url: base,
        }))
        .await
        .expect("test app");
        let owner_id = common::create_test_user(&app.state, "alice_voice", "password123")
            .await
            .expect("owner");
        let cookie = common::login_user(&app.router, "alice_voice", "password123")
            .await
            .expect("login");
        let category = post_json(
            &app.router,
            &cookie,
            "/categories",
            json!({"name":"餐飲","is_income":false}),
        )
        .await;
        assert_eq!(category.0, StatusCode::CREATED);
        let mut friend_ids = Vec::new();
        for (username, nickname, accepted) in [
            ("bob_voice", "小明", true),
            ("carol_voice", "小華", true),
            ("dan_voice", "待接受", false),
        ] {
            let id = common::create_test_user(&app.state, username, "password123")
                .await
                .expect("friend");
            let friend_cookie = common::login_user(&app.router, username, "password123")
                .await
                .expect("friend login");
            assert_eq!(
                post_json(
                    &app.router,
                    &cookie,
                    "/friends/request",
                    json!({"friend_username":username})
                )
                .await
                .0,
                StatusCode::CREATED
            );
            if accepted {
                assert_eq!(
                    post_json(
                        &app.router,
                        &friend_cookie,
                        "/friends/accept",
                        json!({"friend_id":owner_id})
                    )
                    .await
                    .0,
                    StatusCode::OK
                );
                assert_eq!(
                    post_json(
                        &app.router,
                        &cookie,
                        "/friends/nickname",
                        json!({"friend_id":id,"nickname":nickname})
                    )
                    .await
                    .0,
                    StatusCode::OK
                );
                friend_ids.push(id);
            }
        }
        Self {
            app,
            cookie,
            owner_id,
            friend_ids,
            mock,
            server,
        }
    }

    async fn draft(&self) -> (StatusCode, Value) {
        let request = voice_request(
            Some(&self.cookie),
            "audio/webm",
            "2026-10-22",
            "TWD",
            "audio",
        );
        let response = self
            .app
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("voice request");
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        (
            status,
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| json!({"error":String::from_utf8_lossy(&bytes)})),
        )
    }

    async fn assert_no_records(&self) {
        let conn = kash_server::database::db_conn(&self.app.state.main_db)
            .await
            .expect("db");
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM records WHERE owner_user_id = ?",
                [self.owner_id.as_str()],
            )
            .await
            .expect("records");
        assert_eq!(
            rows.next()
                .await
                .expect("row")
                .expect("count")
                .get::<i64>(0)
                .expect("count"),
            0
        );
    }
}

async fn post_json(router: &Router, cookie: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let method = if path == "/friends/nickname" {
        "PATCH"
    } else {
        "POST"
    };
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("cookie", cookie)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"error":String::from_utf8_lossy(&bytes)})),
    )
}

fn voice_request(
    cookie: Option<&str>,
    mime: &str,
    date: &str,
    currency: &str,
    audio: &str,
) -> Request<Body> {
    let boundary = "voice-boundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"reference_date\"\r\n\r\n{date}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"currency\"\r\n\r\n{currency}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"audio\"; filename=\"voice.webm\"\r\nContent-Type: {mime}\r\n\r\n{audio}\r\n--{boundary}--\r\n"
    );
    let mut request = Request::builder()
        .method("POST")
        .uri("/records/voice")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        );
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    request.body(Body::from(body)).expect("voice body")
}

#[tokio::test]
async fn voice_requires_login_and_keys() {
    let app = common::setup_test_app().await.expect("app");
    let response = app
        .router
        .clone()
        .oneshot(voice_request(
            None,
            "audio/webm",
            "2026-10-22",
            "TWD",
            "audio",
        ))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    common::create_test_user(&app.state, "disabled_voice", "password123")
        .await
        .expect("user");
    let cookie = common::login_user(&app.router, "disabled_voice", "password123")
        .await
        .expect("login");
    let response = app
        .router
        .clone()
        .oneshot(voice_request(
            Some(&cookie),
            "audio/webm",
            "2026-10-22",
            "TWD",
            "audio",
        ))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn voice_pipeline_is_scoped_and_does_not_write_records() {
    let fixture = Fixture::new(true).await;
    // Another user's category must never enter Jev's criteria.
    let foreign = common::create_test_user(&fixture.app.state, "foreign_voice", "password123")
        .await
        .expect("foreign");
    let conn = kash_server::database::db_conn(&fixture.app.state.main_db)
        .await
        .expect("db");
    conn.execute(
        "INSERT INTO categories (id, owner_user_id, name, is_income) VALUES (?, ?, ?, ?)",
        ("foreign-category", foreign.as_str(), "秘密", false),
    )
    .await
    .expect("foreign category");
    let (status, body) = fixture.draft().await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let draft: VoiceEntryResponse = serde_json::from_value(body).expect("draft");
    assert_eq!(draft.total_amount, 900.0);
    assert_eq!(draft.date, "2026-10-21");
    assert_eq!(draft.friend_ids.len(), 2);
    assert!(draft.friend_amounts.is_empty());
    let mock = fixture.mock.lock().await;
    assert_eq!(
        mock.requests
            .iter()
            .map(|r| r.0.as_str())
            .collect::<Vec<_>>(),
        ["transcribe", "jev", "extract"]
    );
    assert_eq!(mock.requests[0].1["model"], "gpt-transcribe");
    assert_eq!(mock.requests[0].1["file"], "audio");
    let jev = &mock.requests[1].1;
    assert_eq!(jev["questions"].as_object().expect("questions").len(), 4);
    assert_eq!(
        jev["questions"]["category"]["criteria"]
            .as_object()
            .expect("criteria")
            .len(),
        1
    );
    assert_eq!(
        jev["state"]["friends"].as_array().expect("friends").len(),
        2
    );
    assert!(!jev.to_string().contains("foreign-category"));
    assert!(!jev.to_string().contains("待接受"));
    let input: Value = serde_json::from_str(
        mock.requests[2].1["messages"][1]["content"]
            .as_str()
            .expect("input"),
    )
    .expect("json");
    assert_eq!(input["reference_date"], "2026-10-22");
    assert_eq!(input["weekday"], "Thursday");
    assert!(input.get("timezone").is_none());
    assert_eq!(
        input["jev"]["friends"].as_array().expect("selected").len(),
        2
    );
    drop(mock);
    fixture.assert_no_records().await;
}

#[tokio::test]
async fn voice_split_falls_back_to_likeliest_friend() {
    let fixture = Fixture::new(true).await;
    fixture.mock.lock().await.low_friend_scores = true;
    let (status, body) = fixture.draft().await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mock = fixture.mock.lock().await;
    let likeliest = mock.requests[1].1["questions"]
        .as_object()
        .expect("questions")
        .keys()
        .filter_map(|key| key.strip_prefix("friend_"))
        .next_back()
        .expect("friend question");
    assert_eq!(body["friend_ids"], json!([likeliest]));
}

#[tokio::test]
async fn voice_supports_no_split_and_explicit_amounts() {
    let fixture = Fixture::new(false).await;
    let (status, body) = fixture.draft().await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["split_mode"], "none");
    assert_eq!(body["friend_ids"], json!([]));
    {
        let mut mock = fixture.mock.lock().await;
        mock.split = true;
        mock.extraction["split_mode"] = json!("explicit");
        mock.extraction["friends"] = json!([
            {"user_id":fixture.friend_ids[0],"amount":"250.25"},
            {"user_id":fixture.friend_ids[1],"amount":"300"}
        ]);
    }
    let (status, body) = fixture.draft().await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["friend_amounts"][0]["amount"], 250.25);
    fixture.assert_no_records().await;
}

#[tokio::test]
async fn voice_rejects_invalid_uploads_before_provider_calls() {
    let fixture = Fixture::new(false).await;
    for (mime, date, currency, audio) in [
        ("text/plain", "2026-10-22", "TWD", "audio"),
        ("audio/webm", "yesterday", "TWD", "audio"),
        ("audio/webm", "2026-02-30", "TWD", "audio"),
        ("audio/webm", "2026-10-22", "INVALID", "audio"),
        ("audio/webm", "2026-10-22", "TWD", ""),
    ] {
        let response = fixture
            .app
            .router
            .clone()
            .oneshot(voice_request(
                Some(&fixture.cookie),
                mime,
                date,
                currency,
                audio,
            ))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    let large = "x".repeat(kash_server::records::voice::MAX_VOICE_BODY_BYTES);
    let response = fixture
        .app
        .router
        .clone()
        .oneshot(voice_request(
            Some(&fixture.cookie),
            "audio/webm",
            "2026-10-22",
            "TWD",
            &large,
        ))
        .await
        .expect("large response");
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(fixture.mock.lock().await.requests.is_empty());
}

#[tokio::test]
async fn voice_rejects_foreign_category_provider_failure_and_incomplete_amounts() {
    let fixture = Fixture::new(false).await;
    fixture.mock.lock().await.category_override = Some("foreign-category".into());
    assert_eq!(fixture.draft().await.0, StatusCode::BAD_GATEWAY);
    {
        let mut mock = fixture.mock.lock().await;
        mock.category_override = None;
        mock.fail_transcription = true;
    }
    assert_eq!(fixture.draft().await.0, StatusCode::BAD_GATEWAY);
    fixture.mock.lock().await.fail_transcription = false;
    for invalid in [
        json!({"error":"Missing amount"}),
        json!({"name":"晚餐","total_amount":"NaN","date":"2026-10-21","split_mode":"none","friends":[]}),
        json!({"name":"晚餐","total_amount":"-900","date":"2026-10-21","split_mode":"none","friends":[]}),
        json!({"name":"晚餐","total_amount":"1.001","date":"2026-10-21","split_mode":"none","friends":[]}),
        json!({"name":"晚餐","total_amount":"900","date":"2026-10-23","split_mode":"none","friends":[]}),
        json!({"name":"晚餐","total_amount":"900","date":"2026-02-30","split_mode":"none","friends":[]}),
    ] {
        fixture.mock.lock().await.extraction = invalid;
        let (status, body) = fixture.draft().await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
    fixture.assert_no_records().await;
}

#[tokio::test]
async fn voice_rejects_bad_split_members_and_totals() {
    let fixture = Fixture::new(true).await;
    for participants in [
        json!([]),
        json!([{"user_id":"stranger","amount":"250"}]),
        json!([{"user_id":fixture.friend_ids[0],"amount":"250"},{"user_id":fixture.friend_ids[0],"amount":"300"}]),
        json!([{"user_id":fixture.friend_ids[0],"amount":"800"},{"user_id":fixture.friend_ids[1],"amount":"300"}]),
    ] {
        {
            let mut mock = fixture.mock.lock().await;
            mock.extraction["split_mode"] = json!("explicit");
            mock.extraction["friends"] = participants;
        }
        let (status, body) = fixture.draft().await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
    fixture.assert_no_records().await;
}
