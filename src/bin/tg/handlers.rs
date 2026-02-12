use base64::Engine as _;
use teloxide::prelude::*;
use teloxide::types::ChatAction;
use time::OffsetDateTime;

use my_budget_server::auth;
use my_budget_server::models::CreateRecordPayload;
use my_budget_server::records;
use my_budget_server::utils::validate_date;

use crate::constants::RECORD_CONTEXT_LIMIT;
use crate::constants::{MAX_PHOTO_FILE_SIZE, MAX_VOICE_FILE_SIZE};
use crate::db::{
    fetch_linked_user_id, get_or_create_category, load_categories, load_recent_records,
    load_similar_records, upsert_telegram_link,
};
use crate::helpers::{
    build_record_summary, cleanup_expired_contexts, cleanup_expired_pending_actions,
    get_context_messages, looks_like_delete_request, looks_like_edit_request, parse_decision,
    push_context_turn, resolve_category_id, select_pending_action, telegram_user_id,
};
use crate::models::{BotError, BotState, ContextKey, Decision, DecisionSelection};
use crate::openai::{classify_message, extract_edit, extract_records, transcribe_voice};
use crate::pending::{build_pending_action_from_edit, execute_pending_action};

// ---------------------------------------------------------------------------
// Top-level message dispatcher
// ---------------------------------------------------------------------------

pub async fn handle_message(bot: Bot, msg: Message, state: BotState) -> Result<(), BotError> {
    cleanup_expired_pending_actions(&state).await;
    cleanup_expired_contexts(&state).await;

    if let Some(text) = msg.text() {
        return handle_text_message(&bot, &msg, &state, text.trim().to_string()).await;
    }

    if msg.voice().is_some() {
        return handle_voice_message(&bot, &msg, &state).await;
    }

    if msg.photo().is_some() {
        return handle_photo_message(&bot, &msg, &state).await;
    }

    Ok(())
}

async fn handle_text_message(
    bot: &Bot,
    msg: &Message,
    state: &BotState,
    text: String,
) -> Result<(), BotError> {
    if text.is_empty() {
        return Ok(());
    }

    if text.eq_ignore_ascii_case("/start") {
        return send_help(bot, msg.chat.id).await;
    }

    if text.starts_with("/link") {
        return handle_link(bot, msg, state).await;
    }

    if let Some(decision) = parse_decision(&text) {
        return handle_decision(bot, msg, state, decision).await;
    }

    if looks_like_delete_request(&text) {
        bot.send_message(
            msg.chat.id,
            "Delete is manual-only. Please use your app/API delete endpoint.",
        )
        .await?;
        return Ok(());
    }

    // Build context key for conversation history
    let tg_user_id = match telegram_user_id(&msg) {
        Ok(value) => value,
        Err(message) => {
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
    };
    let context_key: ContextKey = (msg.chat.id.0, tg_user_id);
    let history = get_context_messages(&state, context_key).await;

    let response = if looks_like_edit_request(&text) {
        send_typing(bot, msg.chat.id).await;
        handle_edit_message(bot, msg.chat.id, state, &text, tg_user_id, &history).await?
    } else {
        send_typing(bot, msg.chat.id).await;
        handle_record_message(bot, msg.chat.id, state, &text, None, tg_user_id, &history).await?
    };

    // Store conversation turn in context
    push_context_turn(&state, context_key, &text, &response).await;

    Ok(())
}

async fn handle_voice_message(bot: &Bot, msg: &Message, state: &BotState) -> Result<(), BotError> {
    let voice = match msg.voice() {
        Some(voice) => voice,
        None => return Ok(()),
    };

    let tg_user_id = match telegram_user_id(msg) {
        Ok(value) => value,
        Err(message) => {
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
    };
    if !ensure_user_is_linked(bot, msg.chat.id, state, tg_user_id).await? {
        return Ok(());
    }

    if voice.file.size as usize > MAX_VOICE_FILE_SIZE {
        bot.send_message(msg.chat.id, "Voice message is too large (max 3MB).")
            .await?;
        return Ok(());
    }

    let (audio_bytes, _) =
        match download_telegram_file_bytes(bot, &state.http, &voice.file.id).await {
            Ok(result) => result,
            Err(message) => {
                bot.send_message(msg.chat.id, message).await?;
                return Ok(());
            }
        };

    if audio_bytes.len() > MAX_VOICE_FILE_SIZE {
        bot.send_message(msg.chat.id, "Voice message is too large (max 3MB).")
            .await?;
        return Ok(());
    }

    send_typing(bot, msg.chat.id).await;

    let transcript = match transcribe_voice(
        &state.http,
        &state.openai_api_key,
        audio_bytes,
        "voice.oga",
    )
    .await
    {
        Ok(text) if !text.trim().is_empty() => text,
        Ok(_) => {
            bot.send_message(
                msg.chat.id,
                "I couldn't transcribe that voice message. Please try again.",
            )
            .await?;
            return Ok(());
        }
        Err(message) => {
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
    };

    handle_text_message(bot, msg, state, transcript).await
}

async fn handle_photo_message(bot: &Bot, msg: &Message, state: &BotState) -> Result<(), BotError> {
    let photos = match msg.photo() {
        Some(photos) if !photos.is_empty() => photos,
        _ => return Ok(()),
    };

    let tg_user_id = match telegram_user_id(msg) {
        Ok(value) => value,
        Err(message) => {
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
    };
    if !ensure_user_is_linked(bot, msg.chat.id, state, tg_user_id).await? {
        return Ok(());
    }

    let context_key: ContextKey = (msg.chat.id.0, tg_user_id);
    let history = get_context_messages(state, context_key).await;

    let largest = match photos.last() {
        Some(photo) => photo,
        None => return Ok(()),
    };

    if largest.file.size as usize > MAX_PHOTO_FILE_SIZE {
        bot.send_message(msg.chat.id, "Image is too large (max 10MB).")
            .await?;
        return Ok(());
    }

    let (photo_bytes, file_path) =
        match download_telegram_file_bytes(bot, &state.http, &largest.file.id).await {
            Ok(result) => result,
            Err(message) => {
                bot.send_message(msg.chat.id, message).await?;
                return Ok(());
            }
        };

    if photo_bytes.len() > MAX_PHOTO_FILE_SIZE {
        bot.send_message(msg.chat.id, "Image is too large (max 10MB).")
            .await?;
        return Ok(());
    }

    let media_type = infer_image_mime_type(&file_path);
    let encoded = base64::engine::general_purpose::STANDARD.encode(&photo_bytes);
    let image_data_url = format!("data:{};base64,{}", media_type, encoded);
    let caption = msg.caption().map(str::trim).unwrap_or("").to_string();
    let content_text = if caption.is_empty() {
        "Extract records from this image.".to_string()
    } else {
        caption.clone()
    };

    send_typing(bot, msg.chat.id).await;
    let response = handle_record_message(
        bot,
        msg.chat.id,
        state,
        &content_text,
        Some(&image_data_url),
        tg_user_id,
        &history,
    )
    .await?;

    let context_input = if caption.is_empty() {
        "[photo]".to_string()
    } else {
        format!("[photo] {}", caption)
    };
    push_context_turn(state, context_key, &context_input, &response).await;

    Ok(())
}

async fn send_typing(bot: &Bot, chat_id: ChatId) {
    let _ = bot.send_chat_action(chat_id, ChatAction::Typing).await;
}

// ---------------------------------------------------------------------------
// /start help
// ---------------------------------------------------------------------------

async fn send_help(bot: &Bot, chat_id: ChatId) -> Result<(), BotError> {
    let message = "Hi! Link your account with /link <username> <password>.\n\
                   Then send a message like: lunch 180 or taxi 250.";
    bot.send_message(chat_id, message).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// /link
// ---------------------------------------------------------------------------

async fn handle_link(bot: &Bot, msg: &Message, state: &BotState) -> Result<(), BotError> {
    let text = msg.text().unwrap_or("");
    let mut parts = text.split_whitespace();
    let _ = parts.next();
    let (Some(username), Some(password)) = (parts.next(), parts.next()) else {
        bot.send_message(msg.chat.id, "Usage: /link <username> <password>.")
            .await?;
        return Ok(());
    };

    let user = match auth::authenticate_user(&state.main_db, username, password).await {
        Ok(user) => user,
        Err((_, message)) => {
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
    };

    let tg_user_id = match telegram_user_id(&msg) {
        Ok(value) => value,
        Err(message) => {
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
    };

    let chat_id = msg.chat.id.0;
    if let Err(message) = upsert_telegram_link(&state.main_db, tg_user_id, chat_id, &user.id).await
    {
        bot.send_message(msg.chat.id, message).await?;
        return Ok(());
    }

    bot.send_message(msg.chat.id, "Linked. Send me a record to log.")
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Record creation flow
// ---------------------------------------------------------------------------

async fn handle_record_message(
    bot: &Bot,
    chat_id: ChatId,
    state: &BotState,
    text: &str,
    image_data_url: Option<&str>,
    tg_user_id: i64,
    history: &[serde_json::Value],
) -> Result<String, BotError> {
    let user_id = match fetch_linked_user_id(&state.main_db, tg_user_id).await {
        Ok(Some(user_id)) => user_id,
        Ok(None) => {
            send_help(bot, chat_id).await?;
            return Ok(String::new());
        }
        Err(message) => {
            bot.send_message(chat_id, &message).await?;
            return Ok(message);
        }
    };

    let mut categories = match load_categories(&state.db_pool, &user_id).await {
        Ok(categories) => categories,
        Err(message) => {
            bot.send_message(chat_id, &message).await?;
            return Ok(message);
        }
    };

    let mut category_hint = None;
    let mut similar_records = Vec::new();

    if image_data_url.is_none()
        && !categories.is_empty()
        && let Ok(hint) = classify_message(
            &state.http,
            &state.openai_api_key,
            &state.openai_model,
            &state.openai_reasoning_effort,
            &state.timezone,
            text,
            &categories,
        )
        .await
    {
        if hint.amount != 0.0
            && let Some(category_id) = resolve_category_id(
                &categories,
                hint.category_id.trim(),
                hint.category_name.trim(),
            )
            && let Ok(records) =
                load_similar_records(&state.db_pool, &user_id, &category_id, hint.amount).await
        {
            similar_records = records;
        }

        category_hint = Some(hint);
    }

    send_typing(bot, chat_id).await;

    let batch = match extract_records(
        state,
        text,
        image_data_url,
        &categories,
        &similar_records,
        category_hint.as_ref(),
        history,
    )
    .await
    {
        Ok(batch) => batch,
        Err(message) => {
            bot.send_message(chat_id, &message).await?;
            return Ok(message);
        }
    };

    if batch.needs_clarification || batch.records.is_empty() {
        let clarification = if batch.clarification.trim().is_empty() {
            "I need more details (amount, category, or date).".to_string()
        } else {
            batch.clarification.clone()
        };
        bot.send_message(chat_id, &clarification).await?;
        return Ok(clarification);
    }

    let mut summaries: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for ai_record in &batch.records {
        if ai_record.amount == 0.0 {
            errors.push(format!("{}: amount is missing", ai_record.name));
            continue;
        }

        if let Err((_, message)) = validate_date(&ai_record.date) {
            errors.push(format!("{}: {}", ai_record.name, message));
            continue;
        }

        let category_id = match resolve_category_id(
            &categories,
            &ai_record.category_id,
            &ai_record.category_name,
        ) {
            Some(category_id) => category_id,
            None => {
                let fallback_name = if categories.is_empty() {
                    ai_record.category_name.as_str()
                } else if ai_record.is_income {
                    "Other Income"
                } else {
                    "Other"
                };

                match get_or_create_category(
                    &state.db_pool,
                    &user_id,
                    fallback_name,
                    ai_record.is_income,
                )
                .await
                {
                    Ok(category) => {
                        let category_id = category.id.clone();
                        if !categories.iter().any(|c| c.id == category_id) {
                            categories.push(category);
                        }
                        category_id
                    }
                    Err(message) => {
                        errors.push(format!("{}: {}", ai_record.name, message));
                        continue;
                    }
                }
            }
        };

        let payload = CreateRecordPayload {
            name: ai_record.name.trim().to_string(),
            amount: ai_record.amount,
            category_id,
            date: ai_record.date.trim().to_string(),
        };

        match records::create_record_for_user(&state.db_pool, &user_id, payload).await {
            Ok(record) => {
                summaries.push(build_record_summary(&record, &categories));
            }
            Err((_, message)) => {
                errors.push(format!("{}: {}", ai_record.name, message));
            }
        }
    }

    let mut response_parts: Vec<String> = Vec::new();
    if !summaries.is_empty() {
        response_parts.extend(summaries);
    }
    if !errors.is_empty() {
        for error in &errors {
            response_parts.push(format!("Failed: {}", error));
        }
    }

    let response = if response_parts.is_empty() {
        "No records could be created.".to_string()
    } else {
        response_parts.join("\n")
    };

    bot.send_message(chat_id, &response).await?;
    Ok(response)
}

// ---------------------------------------------------------------------------
// Edit flow
// ---------------------------------------------------------------------------

async fn handle_edit_message(
    bot: &Bot,
    chat_id: ChatId,
    state: &BotState,
    text: &str,
    tg_user_id: i64,
    history: &[serde_json::Value],
) -> Result<String, BotError> {
    let user_id = match fetch_linked_user_id(&state.main_db, tg_user_id).await {
        Ok(Some(user_id)) => user_id,
        Ok(None) => {
            send_help(bot, chat_id).await?;
            return Ok(String::new());
        }
        Err(message) => {
            bot.send_message(chat_id, &message).await?;
            return Ok(message);
        }
    };

    let categories = match load_categories(&state.db_pool, &user_id).await {
        Ok(categories) => categories,
        Err(message) => {
            bot.send_message(chat_id, &message).await?;
            return Ok(message);
        }
    };

    let recent_records =
        match load_recent_records(&state.db_pool, &user_id, RECORD_CONTEXT_LIMIT).await {
            Ok(records) => records,
            Err(message) => {
                bot.send_message(chat_id, &message).await?;
                return Ok(message);
            }
        };

    send_typing(bot, chat_id).await;

    let edit = match extract_edit(state, text, &categories, &recent_records, history).await {
        Ok(edit) => edit,
        Err(message) => {
            bot.send_message(chat_id, &message).await?;
            return Ok(message);
        }
    };

    if edit.needs_clarification {
        let clarification = if edit.clarification.trim().is_empty() {
            "Please describe one specific edit target and change.".to_string()
        } else {
            edit.clarification.trim().to_string()
        };
        bot.send_message(chat_id, &clarification).await?;
        return Ok(clarification);
    }

    let pending_action = match build_pending_action_from_edit(
        &state.db_pool,
        &user_id,
        &edit,
        &recent_records,
        &categories,
    )
    .await
    {
        Ok(action) => action,
        Err(message) => {
            bot.send_message(chat_id, &message).await?;
            return Ok(message);
        }
    };

    let action_id = pending_action.id.clone();
    let expires_in = pending_action.expires_at - OffsetDateTime::now_utc().unix_timestamp();
    let summary = pending_action.summary.clone();
    {
        let mut pending = state.pending_actions.write().await;
        pending
            .entry(tg_user_id)
            .or_insert_with(Vec::new)
            .push(pending_action);
    }

    let response = format!(
        "Pending edit {} (expires in {}s): {}\n\
         Reply with confirm/cancel, or /confirm {} / /cancel {}.",
        action_id,
        expires_in.max(0),
        summary,
        action_id,
        action_id
    );
    bot.send_message(chat_id, &response).await?;
    Ok(response)
}

// ---------------------------------------------------------------------------
// Confirm / Cancel decision
// ---------------------------------------------------------------------------

async fn handle_decision(
    bot: &Bot,
    msg: &Message,
    state: &BotState,
    decision: Decision,
) -> Result<(), BotError> {
    let tg_user_id = match telegram_user_id(&msg) {
        Ok(value) => value,
        Err(message) => {
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
    };

    let target_action_id = match &decision {
        Decision::Confirm(action_id) | Decision::Cancel(action_id) => action_id.clone(),
    };

    let selection = {
        let mut pending = state.pending_actions.write().await;
        select_pending_action(&mut pending, tg_user_id, target_action_id)
    };

    let selected = match selection {
        DecisionSelection::Reply(message) => {
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
        DecisionSelection::Selected(action) => action,
    };

    match decision {
        Decision::Cancel(_) => {
            bot.send_message(
                msg.chat.id,
                format!("Cancelled pending edit {}.", selected.id),
            )
            .await?;
        }
        Decision::Confirm(_) => {
            let message = match execute_pending_action(state, &selected).await {
                Ok(message) | Err(message) => message,
            };
            bot.send_message(msg.chat.id, message).await?;
        }
    }

    Ok(())
}

async fn download_telegram_file_bytes(
    bot: &Bot,
    http: &reqwest::Client,
    file_id: &teloxide::types::FileId,
) -> Result<(Vec<u8>, String), String> {
    let file = bot
        .get_file(file_id.clone())
        .await
        .map_err(|_| "Failed to fetch Telegram file metadata".to_string())?;

    let file_path = file.path;
    let file_url = format!(
        "https://api.telegram.org/file/bot{}/{}",
        bot.token(),
        file_path
    );
    let response = http
        .get(file_url)
        .send()
        .await
        .map_err(|_| "Failed to download Telegram file".to_string())?;

    if !response.status().is_success() {
        return Err("Failed to download Telegram file".to_string());
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|_| "Failed to read Telegram file bytes".to_string())?;

    Ok((bytes.to_vec(), file_path))
}

fn infer_image_mime_type(file_path: &str) -> &'static str {
    let lower = file_path.to_ascii_lowercase();
    if lower.ends_with(".png") {
        return "image/png";
    }
    if lower.ends_with(".webp") {
        return "image/webp";
    }
    "image/jpeg"
}

async fn ensure_user_is_linked(
    bot: &Bot,
    chat_id: ChatId,
    state: &BotState,
    tg_user_id: i64,
) -> Result<bool, BotError> {
    match fetch_linked_user_id(&state.main_db, tg_user_id).await {
        Ok(Some(_)) => Ok(true),
        Ok(None) => {
            send_help(bot, chat_id).await?;
            Ok(false)
        }
        Err(message) => {
            bot.send_message(chat_id, message).await?;
            Ok(false)
        }
    }
}
