use base64::Engine as _;
use teloxide::prelude::*;
use teloxide::types::ChatAction;

use my_budget_server::auth;

use crate::constants::{MAX_PHOTO_FILE_SIZE, MAX_VOICE_FILE_SIZE};
use crate::db::{fetch_linked_user_id, load_categories, upsert_telegram_link};
use crate::helpers::{
    cleanup_expired_contexts, get_context_messages, push_context_turn, telegram_user_id,
};
use crate::models::{BotError, BotState, ContextKey};
use crate::openai::{respond_with_tools, transcribe_voice};

// ---------------------------------------------------------------------------
// Top-level message dispatcher
// ---------------------------------------------------------------------------

pub async fn handle_message(bot: Bot, msg: Message, state: BotState) -> Result<(), BotError> {
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

    let tg_user_id = match telegram_user_id(msg) {
        Ok(value) => value,
        Err(message) => {
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
    };

    handle_ai_turn(bot, msg.chat.id, state, tg_user_id, &text, None, &text).await
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

    let context_input = format!("[voice] {}", transcript.trim());
    handle_ai_turn(
        bot,
        msg.chat.id,
        state,
        tg_user_id,
        &transcript,
        None,
        &context_input,
    )
    .await
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
        "Please process this image for my records.".to_string()
    } else {
        caption.clone()
    };
    let context_input = if caption.is_empty() {
        "[photo]".to_string()
    } else {
        format!("[photo] {}", caption)
    };

    handle_ai_turn(
        bot,
        msg.chat.id,
        state,
        tg_user_id,
        &content_text,
        Some(&image_data_url),
        &context_input,
    )
    .await
}

async fn handle_ai_turn(
    bot: &Bot,
    chat_id: ChatId,
    state: &BotState,
    tg_user_id: i64,
    text: &str,
    image_data_url: Option<&str>,
    context_input: &str,
) -> Result<(), BotError> {
    let user_id = match fetch_linked_user_id(&state.main_db, tg_user_id).await {
        Ok(Some(user_id)) => user_id,
        Ok(None) => {
            send_help(bot, chat_id).await?;
            return Ok(());
        }
        Err(message) => {
            bot.send_message(chat_id, message).await?;
            return Ok(());
        }
    };

    let categories = match load_categories(&state.main_db, &user_id).await {
        Ok(categories) => categories,
        Err(message) => {
            bot.send_message(chat_id, message).await?;
            return Ok(());
        }
    };

    let context_key: ContextKey = (chat_id.0, tg_user_id);
    let history = get_context_messages(state, context_key).await;

    send_typing(bot, chat_id).await;
    let response = match respond_with_tools(
        state,
        &user_id,
        text,
        image_data_url,
        &categories,
        &history,
    )
    .await
    {
        Ok(message) if !message.trim().is_empty() => message,
        Ok(_) => "Done.".to_string(),
        Err(message) => message,
    };

    bot.send_message(chat_id, &response).await?;
    push_context_turn(state, context_key, context_input, &response).await;

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
                   Then ask naturally, for example:\n\
                   - create: lunch 180 today\n\
                   - edit: change taxi amount to 220\n\
                   - list: show my records from this week";
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

    let tg_user_id = match telegram_user_id(msg) {
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

    bot.send_message(msg.chat.id, "Linked. Send me your request.")
        .await?;
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
