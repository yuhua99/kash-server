use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use reqwest::Client;
use serde_json::json;
use time::OffsetDateTime;
use tokio::sync::RwLock;

use my_budget_server::{Db, DbPool};

use crate::constants::{CONTEXT_MAX_TURNS, CONTEXT_TTL_SECONDS};

// ---------------------------------------------------------------------------
// Bot state
// ---------------------------------------------------------------------------

pub type BotError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone)]
pub struct BotState {
    pub main_db: Db,
    pub db_pool: DbPool,
    pub http: Client,
    pub openai_api_key: String,
    pub openai_model: String,
    pub openai_reasoning_effort: String,
    pub timezone: String,
    pub chat_contexts: Arc<RwLock<HashMap<ContextKey, ChatContext>>>,
}

// ---------------------------------------------------------------------------
// Category helpers
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct CategoryInfo {
    pub id: String,
    pub name: String,
    pub is_income: bool,
}

// ---------------------------------------------------------------------------
// Conversation context
// ---------------------------------------------------------------------------

pub type ContextKey = (i64, i64);

#[derive(Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub timestamp: i64,
}

pub struct ChatContext {
    pub messages: VecDeque<ChatMessage>,
}

impl ChatContext {
    pub fn new() -> Self {
        Self {
            messages: VecDeque::new(),
        }
    }

    pub fn push_turn(&mut self, user_msg: &str, assistant_msg: &str) {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        self.messages.push_back(ChatMessage {
            role: "user".to_string(),
            content: user_msg.to_string(),
            timestamp: now,
        });
        self.messages.push_back(ChatMessage {
            role: "assistant".to_string(),
            content: assistant_msg.to_string(),
            timestamp: now,
        });
        // Each turn is 2 messages; keep at most CONTEXT_MAX_TURNS turns
        while self.messages.len() > CONTEXT_MAX_TURNS * 2 {
            self.messages.pop_front();
            self.messages.pop_front();
        }
    }

    pub fn to_openai_messages(&self) -> Vec<serde_json::Value> {
        self.messages
            .iter()
            .map(|m| {
                json!({
                    "role": m.role,
                    "content": m.content
                })
            })
            .collect()
    }

    pub fn is_expired(&self) -> bool {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        match self.messages.back() {
            Some(last) => now - last.timestamp > CONTEXT_TTL_SECONDS,
            None => true,
        }
    }
}
