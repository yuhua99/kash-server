use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::{Json, extract::State, http::StatusCode};
use tower_sessions::Session;
use uuid::Uuid;

use crate::AppState;
use crate::constants::*;
use crate::database::Db;
use crate::models::{LoginPayload, PublicUser, RegisterPayload, User};

pub async fn get_user_by_username_public(
    db: &Db,
    username: &str,
) -> anyhow::Result<Option<PublicUser>> {
    let conn = db.read().await;
    let mut rows = conn
        .query("SELECT id, name FROM users WHERE name = ?", [username])
        .await?;

    if let Some(row) = rows.next().await? {
        let id: String = row.get(0)?;
        let username: String = row.get(1)?;
        Ok(Some(PublicUser { id, username }))
    } else {
        Ok(None)
    }
}

async fn create_user(db: &Db, username: &str, password: &str) -> anyhow::Result<PublicUser> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Failed to hash password: {}", e))?
        .to_string();
    let id = Uuid::new_v4().to_string();
    let conn = db.write().await;

    conn.execute(
        "INSERT INTO users (id, name, password_hash) VALUES (?, ?, ?)",
        (id.as_str(), username, hash.as_str()),
    )
    .await?;

    Ok(PublicUser {
        id,
        username: username.to_string(),
    })
}

pub async fn register(
    State(app_state): State<AppState>,
    Json(payload): Json<RegisterPayload>,
) -> Result<(StatusCode, Json<PublicUser>), (StatusCode, String)> {
    // Input validation
    if payload.username.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Username cannot be empty".to_string(),
        ));
    }
    if payload.username.len() < MIN_USERNAME_LENGTH || payload.username.len() > MAX_USERNAME_LENGTH
    {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Username must be between {} and {} characters",
                MIN_USERNAME_LENGTH, MAX_USERNAME_LENGTH
            ),
        ));
    }
    if payload.password.len() < MIN_PASSWORD_LENGTH {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Password must be at least {} characters long",
                MIN_PASSWORD_LENGTH
            ),
        ));
    }
    if !payload
        .username
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Username can only contain alphanumeric characters, underscores, and hyphens"
                .to_string(),
        ));
    }

    let user = create_user(&app_state.main_db, &payload.username, &payload.password)
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                (StatusCode::CONFLICT, "Username already exists".to_string())
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
        })?;

    Ok((StatusCode::CREATED, Json(user)))
}

async fn get_user_by_username(db: &Db, username: &str) -> anyhow::Result<Option<User>> {
    let conn = db.read().await;
    let mut rows = conn
        .query(
            "SELECT id, name, password_hash FROM users WHERE name = ?",
            [username],
        )
        .await?;

    if let Some(row) = rows.next().await? {
        let id: String = row.get(0)?;
        let username: String = row.get(1)?;
        let password_hash: String = row.get(2)?;
        Ok(Some(User {
            id,
            username,
            password_hash,
        }))
    } else {
        Ok(None)
    }
}

fn verify_password(password: &str, hash: &str) -> anyhow::Result<bool> {
    let parsed_hash = PasswordHash::new(hash)
        .map_err(|e| anyhow::anyhow!("Failed to parse password hash: {}", e))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

pub async fn authenticate_user(
    db: &Db,
    username: &str,
    password: &str,
) -> Result<PublicUser, (StatusCode, String)> {
    // Input validation
    if username.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Username cannot be empty".to_string(),
        ));
    }
    if password.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Password cannot be empty".to_string(),
        ));
    }

    let user_data = get_user_by_username(db, username)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let user = match user_data {
        Some(data) => data,
        None => return Err((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string())),
    };

    let is_valid = verify_password(password, &user.password_hash)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !is_valid {
        return Err((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()));
    }

    Ok(PublicUser {
        id: user.id,
        username: user.username,
    })
}

pub async fn login(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<LoginPayload>,
) -> Result<(StatusCode, Json<PublicUser>), (StatusCode, String)> {
    let user = authenticate_user(&app_state.main_db, &payload.username, &payload.password).await?;

    // Set user session
    session
        .insert("user_id", &user.id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    session
        .insert("username", &user.username.clone())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((StatusCode::OK, Json(user)))
}

pub async fn get_current_user(session: &Session) -> Result<PublicUser, (StatusCode, String)> {
    let user_id: Option<String> = session
        .get("user_id")
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let username: Option<String> = session
        .get("username")
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match (user_id, username) {
        (Some(id), Some(name)) => Ok(PublicUser { id, username: name }),
        _ => Err((StatusCode::UNAUTHORIZED, "Not logged in".to_string())),
    }
}

pub async fn me(session: Session) -> Result<(StatusCode, Json<PublicUser>), (StatusCode, String)> {
    let user = get_current_user(&session).await?;
    Ok((StatusCode::OK, Json(user)))
}

pub async fn logout(session: Session) -> Result<StatusCode, (StatusCode, String)> {
    session.clear().await;

    Ok(StatusCode::NO_CONTENT)
}
