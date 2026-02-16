use crate::models::{BotState, CategoryInfo, ChatContext, ContextKey};
use teloxide::prelude::*;

// ---------------------------------------------------------------------------
// Telegram user helpers
// ---------------------------------------------------------------------------

pub fn telegram_user_id(msg: &Message) -> Result<i64, String> {
    let user = msg
        .from
        .as_ref()
        .ok_or_else(|| "Unable to read Telegram user id.".to_string())?;

    i64::try_from(user.id.0).map_err(|_| "Invalid Telegram user id.".to_string())
}

// ---------------------------------------------------------------------------
// Amount normalization
// ---------------------------------------------------------------------------

pub fn normalize_amount_by_category(amount: f64, is_income: bool) -> f64 {
    if is_income {
        amount.abs()
    } else {
        -amount.abs()
    }
}

// ---------------------------------------------------------------------------
// Category resolution
// ---------------------------------------------------------------------------

pub fn resolve_category_id(
    categories: &[CategoryInfo],
    category_id: &str,
    category_name: &str,
) -> Option<String> {
    if !category_id.trim().is_empty() && categories.iter().any(|c| c.id == category_id) {
        return Some(category_id.to_string());
    }

    if !category_name.trim().is_empty()
        && let Some(category) = categories
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(category_name.trim()))
    {
        return Some(category.id.clone());
    }

    None
}

// ---------------------------------------------------------------------------
// Conversation context management
// ---------------------------------------------------------------------------

pub async fn cleanup_expired_contexts(state: &BotState) {
    let mut contexts = state.chat_contexts.write().await;
    contexts.retain(|_, ctx| !ctx.is_expired());
}

pub async fn get_context_messages(state: &BotState, key: ContextKey) -> Vec<serde_json::Value> {
    let contexts = state.chat_contexts.read().await;
    match contexts.get(&key) {
        Some(ctx) if !ctx.is_expired() => ctx.to_openai_messages(),
        _ => Vec::new(),
    }
}

pub async fn push_context_turn(state: &BotState, key: ContextKey, user_msg: &str, bot_msg: &str) {
    let mut contexts = state.chat_contexts.write().await;
    let ctx = contexts.entry(key).or_insert_with(ChatContext::new);
    ctx.push_turn(user_msg, bot_msg);
}
