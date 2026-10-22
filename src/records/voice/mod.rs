mod extraction;
mod judgments;
mod providers;

pub use providers::VoiceProviders;

use axum::{
    Json,
    extract::{DefaultBodyLimit, Multipart, Query, State},
    http::StatusCode,
    routing::{MethodRouter, post},
};
use tower_sessions::Session;

use crate::auth::get_current_user;
use crate::constants::{MAX_LIMIT, MAX_VOICE_CATEGORIES};
use crate::models::{GetCategoriesQuery, VoiceEntryResponse};
use crate::validation::{validate_currency, validate_date};
use crate::{AppState, categories, friends};

pub const MAX_VOICE_BODY_BYTES: usize = 10 * 1024 * 1024;

pub fn route() -> MethodRouter<AppState> {
    post(create_voice_draft).layer(DefaultBodyLimit::max(MAX_VOICE_BODY_BYTES))
}

#[utoipa::path(
    post,
    path = "/records/voice",
    tag = "records",
    request_body(content = crate::models::VoiceEntryPayload, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "Voice draft; no records are written", body = crate::models::VoiceEntryResponse),
        (status = 400, description = "Invalid or incomplete recording"),
        (status = 401, description = "Unauthorized"),
        (status = 413, description = "Recording exceeds 10 MiB"),
        (status = 502, description = "AI provider unavailable"),
        (status = 503, description = "Voice entry is not configured"),
    ),
)]
pub async fn create_voice_draft(
    State(state): State<AppState>,
    session: Session,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<VoiceEntryResponse>), (StatusCode, String)> {
    get_current_user(&session).await?;
    let providers = state.voice_providers.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Voice entry requires OPENAI_API_KEY and TYPESAFE_API_KEY on the server".to_string(),
        )
    })?;
    let mut audio = None;
    let mut reference_date = None;
    let mut currency = None;
    while let Some(field) = multipart.next_field().await.map_err(multipart_error)? {
        match field.name() {
            Some("audio") if audio.is_none() => {
                let mime = field
                    .content_type()
                    .unwrap_or("")
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .to_string();
                let extension = match mime.as_str() {
                    "audio/webm" => "webm",
                    "audio/mp4" => "mp4",
                    "audio/wav" | "audio/x-wav" => "wav",
                    _ => return Err(bad_input("Use WebM, MP4 or WAV audio")),
                };
                let bytes = field.bytes().await.map_err(multipart_error)?;
                if bytes.is_empty() {
                    return Err(bad_input("Recording cannot be empty"));
                }
                audio = Some((bytes.to_vec(), mime, extension));
            }
            Some("reference_date") if reference_date.is_none() => {
                reference_date = Some(field.text().await.map_err(multipart_error)?);
            }
            Some("currency") if currency.is_none() => {
                currency = Some(field.text().await.map_err(multipart_error)?);
            }
            _ => return Err(bad_input("Unexpected or duplicate voice entry field")),
        }
    }
    let reference_date = reference_date.ok_or_else(|| bad_input("Reference date is required"))?;
    let reference_date = parse_voice_date(&reference_date)?;
    let currency = validate_currency(&currency.ok_or_else(|| bad_input("Currency is required"))?)?;
    let (audio, mime, extension) = audio.ok_or_else(|| bad_input("Recording is required"))?;

    // Reuse domain-owned reads so categories and friends stay scoped to this session.
    let (_, Json(category_response)) = categories::get_categories(
        State(state.clone()),
        session.clone(),
        Query(GetCategoriesQuery {
            limit: Some(MAX_VOICE_CATEGORIES),
            offset: None,
            search: None,
        }),
    )
    .await?;
    if category_response.categories.is_empty() {
        return Err(bad_input("Create a category before using voice entry"));
    }
    if category_response.total_count > MAX_VOICE_CATEGORIES {
        return Err(bad_input("Voice entry supports up to 255 categories"));
    }
    let (_, Json(friend_response)) = friends::list_friends(
        State(state.clone()),
        session,
        Query(friends::ListFriendsQuery {
            pending: Some(false),
            limit: Some(MAX_LIMIT),
            offset: None,
        }),
    )
    .await?;
    if friend_response.total_count > i64::from(MAX_LIMIT) {
        return Err(bad_input("Too many friends for voice entry"));
    }
    let transcript = providers
        .transcribe(&state.http_client, audio, &mime, extension)
        .await?;
    let decision = providers
        .judge(
            &state.http_client,
            &transcript,
            &category_response.categories,
            &friend_response.friends,
        )
        .await?;
    let draft = providers
        .extract(
            &state.http_client,
            &transcript,
            reference_date,
            &currency,
            &decision,
        )
        .await?;
    let response = extraction::validated_response(transcript, draft, decision, reference_date)?;
    Ok((StatusCode::OK, Json(response)))
}

fn parse_voice_date(value: &str) -> Result<time::Date, (StatusCode, String)> {
    validate_date(value)?;
    if value.len() != 10 {
        return Err(bad_input("Date must use YYYY-MM-DD"));
    }
    let format = time::format_description::parse("[year]-[month]-[day]")
        .map_err(|_| bad_input("Invalid date format"))?;
    time::Date::parse(value, &format).map_err(|_| bad_input("Invalid date format"))
}

fn multipart_error(error: axum::extract::multipart::MultipartError) -> (StatusCode, String) {
    (
        error.status(),
        "Invalid or oversized voice recording".to_string(),
    )
}

fn bad_input(message: &str) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, message.to_string())
}
