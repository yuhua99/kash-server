use crate::models::{BotState, ChatContext, ContextKey};

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
