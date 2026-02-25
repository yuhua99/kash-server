use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use kash_server::{AppState, auth, constants::*, database};
use time::Duration;
use tower::util::ServiceExt;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer, cookie::Key};
use uuid::Uuid;

#[derive(Clone)]
pub struct TestConfig {
    pub temp_dir_path: String,
}

impl TestConfig {
    pub fn new() -> anyhow::Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let temp_dir_path = temp_dir.path().to_string_lossy().to_string();
        std::mem::forget(temp_dir);
        Ok(Self { temp_dir_path })
    }

    pub fn data_path(&self) -> String {
        self.temp_dir_path.clone()
    }
}

pub struct TestApp {
    pub router: Router,
    pub state: AppState,
}

pub async fn setup_test_app() -> anyhow::Result<TestApp> {
    let test_config = TestConfig::new()?;

    let data_path = test_config.data_path();
    std::fs::create_dir_all(&data_path)?;

    let main_db = database::init_main_db(&data_path)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize main database: {}", e))?;

    let app_state = AppState { main_db };

    let store = MemoryStore::default();

    let session_secret = "test_secret_key_at_least_64_chars_long_test_secret_key_at_least_64_";
    let session_key = Key::try_from(session_secret.as_bytes())
        .map_err(|e| anyhow::anyhow!("Invalid session secret: {}", e))?;

    let session_layer = SessionManagerLayer::new(store)
        .with_secure(false)
        .with_name(SESSION_NAME)
        .with_expiry(Expiry::OnInactivity(Duration::days(SESSION_EXPIRY_DAYS)))
        .with_signed(session_key);

    let router = Router::new()
        .route("/", axum::routing::get(root_handler))
        .route("/auth/register", axum::routing::post(auth::register))
        .route("/auth/login", axum::routing::post(auth::login))
        .route("/auth/me", axum::routing::get(auth::me))
        .route("/auth/logout", axum::routing::post(auth::logout))
        .route(
            "/records",
            axum::routing::post(kash_server::records::create_record)
                .get(kash_server::records::get_records),
        )
        .route(
            "/records/{id}",
            axum::routing::put(kash_server::records::update_record)
                .delete(kash_server::records::delete_record),
        )
        .route(
            "/records/{id}/settle",
            axum::routing::put(kash_server::records::update_settle),
        )
        .route(
            "/records/finalize-pending",
            axum::routing::post(kash_server::records::finalize_pending_record),
        )
        .route(
            "/categories",
            axum::routing::post(kash_server::categories::create_category)
                .get(kash_server::categories::get_categories),
        )
        .route(
            "/categories/{id}",
            axum::routing::put(kash_server::categories::update_category)
                .delete(kash_server::categories::delete_category),
        )
        .route(
            "/friends/request",
            axum::routing::post(kash_server::friends::send_friend_request),
        )
        .route(
            "/friends/search",
            axum::routing::get(kash_server::friends::search_users),
        )
        .route(
            "/friends/nickname",
            axum::routing::patch(kash_server::friends::update_nickname),
        )
        .route(
            "/friends/list",
            axum::routing::get(kash_server::friends::list_friends),
        )
        .route(
            "/friends/accept",
            axum::routing::post(kash_server::friends::accept_friend),
        )
        .route(
            "/friends/block",
            axum::routing::post(kash_server::friends::block_friend),
        )
        .route(
            "/friends/unfriend",
            axum::routing::post(kash_server::friends::unfriend),
        )
        .route(
            "/splits/create",
            axum::routing::post(kash_server::splits::create_split),
        )
        .layer(session_layer)
        .with_state(app_state.clone());

    Ok(TestApp {
        router,
        state: app_state,
    })
}

async fn root_handler(_session: tower_sessions::Session) -> axum::response::Html<String> {
    axum::response::Html("<h1>Test Server</h1>".to_string())
}

pub async fn create_test_user(
    app_state: &AppState,
    username: &str,
    password: &str,
) -> anyhow::Result<String> {
    use argon2::{
        Argon2,
        password_hash::{PasswordHasher, SaltString},
    };
    use password_hash::rand_core::OsRng;

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Failed to hash password: {}", e))?
        .to_string();

    let user_id = Uuid::new_v4().to_string();

    let conn = app_state.main_db.write().await;
    conn.execute(
        "INSERT INTO users (id, name, password_hash) VALUES (?, ?, ?)",
        (user_id.as_str(), username, hash.as_str()),
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to create test user: {}", e))?;

    Ok(user_id)
}

pub async fn login_user(app: &Router, username: &str, password: &str) -> anyhow::Result<String> {
    let payload = serde_json::json!({
        "username": username,
        "password": password
    });

    let request = Request::builder()
        .method("POST")
        .uri("/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .map_err(|e| anyhow::anyhow!("Failed to build request: {}", e))?;

    let response = app
        .clone()
        .oneshot(request)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to execute request: {}", e))?;

    let set_cookie = response
        .headers()
        .get("set-cookie")
        .and_then(|v: &axum::http::HeaderValue| v.to_str().ok())
        .ok_or_else(|| anyhow::anyhow!("No session cookie in response"))?;

    Ok(set_cookie.to_string())
}

#[allow(dead_code)]
pub async fn auth_request(
    app: &Router,
    method: &str,
    uri: &str,
    cookie: &str,
) -> anyhow::Result<(StatusCode, String)> {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("cookie", cookie)
        .body(Body::empty())
        .map_err(|e| anyhow::anyhow!("Failed to build request: {}", e))?;

    let response = app
        .clone()
        .oneshot(request)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to execute request: {}", e))?;

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read response body: {}", e))?;
    let body_str = String::from_utf8(body.to_vec())?;

    Ok((status, body_str))
}
