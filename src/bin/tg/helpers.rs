use teloxide::prelude::*;
use time::OffsetDateTime;

use crate::constants::{
    CANCEL_WORDS, CONFIRM_WORDS, DELETE_REQUEST_KEYWORDS, EDIT_REQUEST_KEYWORDS,
};
use crate::models::{
    BotState, CategoryInfo, ChatContext, ContextKey, Decision, DecisionSelection, PendingAction,
};

use std::collections::HashMap;

use my_budget_server::models::Record;

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
// Keyword detection
// ---------------------------------------------------------------------------

pub fn looks_like_edit_request(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    EDIT_REQUEST_KEYWORDS
        .iter()
        .any(|keyword| lowered.contains(keyword))
}

pub fn looks_like_delete_request(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    DELETE_REQUEST_KEYWORDS
        .iter()
        .any(|keyword| lowered.contains(keyword))
}

// ---------------------------------------------------------------------------
// Decision parsing (confirm / cancel)
// ---------------------------------------------------------------------------

pub fn parse_decision(text: &str) -> Option<Decision> {
    let trimmed = text.trim();
    let lowered = trimmed.to_ascii_lowercase();

    let mut parts = trimmed.split_whitespace();
    let command = parts.next().unwrap_or_default().to_ascii_lowercase();

    if command == "/confirm" || command.starts_with("/confirm@") {
        let action_id = parts.next().map(str::to_string);
        return Some(Decision::Confirm(action_id));
    }

    if command == "/cancel" || command.starts_with("/cancel@") {
        let action_id = parts.next().map(str::to_string);
        return Some(Decision::Cancel(action_id));
    }

    if CONFIRM_WORDS.iter().any(|word| lowered == *word) {
        return Some(Decision::Confirm(None));
    }

    if CANCEL_WORDS.iter().any(|word| lowered == *word) {
        return Some(Decision::Cancel(None));
    }

    None
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
// Record summary formatting
// ---------------------------------------------------------------------------

pub fn build_record_summary(record: &Record, categories: &[CategoryInfo]) -> String {
    let category_name = categories
        .iter()
        .find(|c| c.id == record.category_id)
        .map(|c| c.name.as_str())
        .unwrap_or("Unknown");

    format!(
        "Recorded: {} / {} / {} / {} (id: {})",
        record.name, record.amount, category_name, record.date, record.id
    )
}

// ---------------------------------------------------------------------------
// Record name resolution
// ---------------------------------------------------------------------------

pub fn resolve_record_id_by_name(records: &[Record], target_name: &str) -> Result<String, String> {
    let trimmed_name = target_name.trim();
    if trimmed_name.is_empty() {
        return Err("Please specify which record to edit.".to_string());
    }

    let matches: Vec<&Record> = records
        .iter()
        .filter(|record| record.name.eq_ignore_ascii_case(trimmed_name))
        .collect();
    match matches.len() {
        0 => Err("Record not found. Please include record id.".to_string()),
        1 => Ok(matches[0].id.clone()),
        _ => Err(
            "Multiple records match that name. Please resend the edit request with a specific record id."
                .to_string(),
        ),
    }
}

// ---------------------------------------------------------------------------
// Pending action formatting & selection
// ---------------------------------------------------------------------------

pub fn format_pending_actions(actions: &[PendingAction]) -> String {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let mut lines = vec![
        "Multiple pending edits found. Please specify one with /confirm <id> or /cancel <id>:"
            .to_string(),
    ];

    for action in actions {
        let remain = (action.expires_at - now).max(0);
        lines.push(format!(
            "- {} (expires in {}s): {}",
            action.id, remain, action.summary
        ));
    }

    lines.join("\n")
}

pub fn select_pending_action(
    pending: &mut HashMap<i64, Vec<PendingAction>>,
    telegram_user_id: i64,
    target_action_id: Option<String>,
) -> DecisionSelection {
    let Some(actions) = pending.get_mut(&telegram_user_id) else {
        return DecisionSelection::Reply("No pending edits to confirm or cancel.".to_string());
    };

    let index = if let Some(action_id) = target_action_id {
        actions.iter().position(|action| action.id == action_id)
    } else if actions.len() == 1 {
        Some(0)
    } else {
        None
    };

    let Some(index) = index else {
        return DecisionSelection::Reply(format_pending_actions(actions));
    };

    let action = actions.swap_remove(index);
    if actions.is_empty() {
        pending.remove(&telegram_user_id);
    }
    DecisionSelection::Selected(action)
}

// ---------------------------------------------------------------------------
// Conversation context management
// ---------------------------------------------------------------------------

pub async fn cleanup_expired_pending_actions(state: &BotState) {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let mut pending = state.pending_actions.write().await;
    pending.retain(|_, actions| {
        actions.retain(|action| action.expires_at > now);
        !actions.is_empty()
    });
}

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
