use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use teloxide::prelude::*;
use time::{Duration, OffsetDateTime};
use tokio::sync::RwLock;
use uuid::Uuid;

use my_budget_server::auth;
use my_budget_server::categories::validate_category_name;
use my_budget_server::constants::DEFAULT_DATA_PATH;
use my_budget_server::database;
use my_budget_server::models::{CreateRecordPayload, Record};
use my_budget_server::records;
use my_budget_server::utils::{get_user_database_from_pool, validate_date};
use my_budget_server::{Db, DbPool};

type BotError = Box<dyn std::error::Error + Send + Sync>;

const DEFAULT_OPENAI_MODEL: &str = "gpt-4o-mini";
const DEFAULT_TIMEZONE: &str = "Asia/Taipei";
const SIMILAR_RECORDS_DAYS: i64 = 180;
const SIMILAR_RECORDS_LIMIT: usize = 5;
const SIMILAR_AMOUNT_RATIO: f64 = 0.2;
const RECORD_CONTEXT_LIMIT: usize = 30;
const PENDING_ACTION_TTL_SECONDS: i64 = 180;

const EDIT_REQUEST_KEYWORDS: [&str; 4] = ["edit", "update", "change", "rename"];
const DELETE_REQUEST_KEYWORDS: [&str; 3] = ["delete", "remove", "erase"];
const CONFIRM_WORDS: [&str; 8] = [
    "yes", "confirm", "ok", "okay", "ok do it", "do it", "apply", "proceed",
];
const CANCEL_WORDS: [&str; 6] = [
    "cancel",
    "stop",
    "never mind",
    "nevermind",
    "don't do it",
    "dont do it",
];

#[derive(Clone)]
struct BotState {
    main_db: Db,
    db_pool: DbPool,
    http: Client,
    openai_api_key: String,
    openai_model: String,
    timezone: String,
    pending_actions: Arc<RwLock<HashMap<i64, Vec<PendingAction>>>>,
}

#[derive(Clone)]
struct CategoryInfo {
    id: String,
    name: String,
    is_income: bool,
}

#[derive(Clone)]
struct SimilarRecord {
    name: String,
    amount: f64,
}

#[derive(Clone, Deserialize)]
struct AiCategoryHint {
    amount: f64,
    category_id: String,
    category_name: String,
    is_income: bool,
}

#[derive(Deserialize)]
struct AiRecordResult {
    name: String,
    amount: f64,
    category_id: String,
    category_name: String,
    date: String,
    is_income: bool,
    needs_clarification: bool,
    clarification: String,
}

#[derive(Clone, Deserialize)]
struct AiEditResult {
    target_type: String,
    target_id: String,
    target_name: String,
    category_id: String,
    category_name: String,
    new_name: Option<String>,
    new_amount: Option<f64>,
    new_category_id: Option<String>,
    new_category_name: Option<String>,
    new_date: Option<String>,
    needs_clarification: bool,
    clarification: String,
}

#[derive(Clone)]
struct PendingAction {
    id: String,
    user_id: String,
    expires_at: i64,
    summary: String,
    action: PendingActionType,
}

#[derive(Clone)]
enum PendingActionType {
    RecordEdit {
        record_id: String,
        patch: PendingRecordPatch,
    },
    CategoryEdit {
        category_id: String,
        new_name: String,
    },
}

#[derive(Clone)]
struct PendingRecordPatch {
    name: Option<String>,
    amount: Option<f64>,
    category_id: Option<String>,
    date: Option<String>,
}

impl PendingRecordPatch {
    fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.amount.is_none()
            && self.category_id.is_none()
            && self.date.is_none()
    }
}

enum Decision {
    Confirm(Option<String>),
    Cancel(Option<String>),
}

enum DecisionSelection {
    Selected(PendingAction),
    Reply(String),
}

#[tokio::main]
async fn main() -> Result<(), BotError> {
    dotenv::dotenv().ok();

    let bot_token =
        std::env::var("TELEGRAM_BOT_TOKEN").map_err(|_| "TELEGRAM_BOT_TOKEN is required")?;
    let bot = Bot::new(bot_token);

    let openai_api_key =
        std::env::var("OPENAI_API_KEY").map_err(|_| "OPENAI_API_KEY is required")?;
    let openai_model =
        std::env::var("OPENAI_MODEL").unwrap_or_else(|_| DEFAULT_OPENAI_MODEL.to_string());
    let timezone = std::env::var("BOT_TIMEZONE").unwrap_or_else(|_| DEFAULT_TIMEZONE.to_string());

    let data_path =
        std::env::var("DATABASE_PATH").unwrap_or_else(|_| DEFAULT_DATA_PATH.to_string());
    let main_db = database::init_main_db(&data_path).await?;
    let db_pool = DbPool::new(data_path);

    let state = BotState {
        main_db,
        db_pool,
        http: Client::new(),
        openai_api_key,
        openai_model,
        timezone,
        pending_actions: Arc::new(RwLock::new(HashMap::new())),
    };

    let handler = Update::filter_message().endpoint(handle_message);
    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![state])
        .build()
        .dispatch()
        .await;

    Ok(())
}

async fn handle_message(bot: Bot, msg: Message, state: BotState) -> Result<(), BotError> {
    cleanup_expired_pending_actions(&state).await;

    let text = match msg.text() {
        Some(text) => text.trim().to_string(),
        None => return Ok(()),
    };

    if text.eq_ignore_ascii_case("/start") {
        return send_help(&bot, msg.chat.id).await;
    }

    if text.starts_with("/link") {
        return handle_link(&bot, msg, &state).await;
    }

    if let Some(decision) = parse_decision(&text) {
        return handle_decision(&bot, msg, &state, decision).await;
    }

    if looks_like_delete_request(&text) {
        bot.send_message(
            msg.chat.id,
            "Delete is manual-only. Please use your app/API delete endpoint.",
        )
        .await?;
        return Ok(());
    }

    if looks_like_edit_request(&text) {
        return handle_edit_message(&bot, msg, &state, &text).await;
    }

    handle_record_message(&bot, msg, &state, &text).await
}

fn telegram_user_id(msg: &Message) -> Result<i64, String> {
    let user = msg
        .from
        .as_ref()
        .ok_or_else(|| "Unable to read Telegram user id.".to_string())?;

    i64::try_from(user.id.0).map_err(|_| "Invalid Telegram user id.".to_string())
}

async fn send_help(bot: &Bot, chat_id: ChatId) -> Result<(), BotError> {
    let message = "Hi! Link your account with /link <username> <password>.\nThen send a message like: lunch 180 or taxi 250.";
    bot.send_message(chat_id, message).await?;
    Ok(())
}

async fn handle_link(bot: &Bot, msg: Message, state: &BotState) -> Result<(), BotError> {
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

    let telegram_user_id = match telegram_user_id(&msg) {
        Ok(value) => value,
        Err(message) => {
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
    };

    let chat_id = msg.chat.id.0;
    if let Err(message) =
        upsert_telegram_link(&state.main_db, telegram_user_id, chat_id, &user.id).await
    {
        bot.send_message(msg.chat.id, message).await?;
        return Ok(());
    }

    bot.send_message(msg.chat.id, "Linked. Send me a record to log.")
        .await?;
    Ok(())
}

async fn handle_record_message(
    bot: &Bot,
    msg: Message,
    state: &BotState,
    text: &str,
) -> Result<(), BotError> {
    let telegram_user_id = match telegram_user_id(&msg) {
        Ok(value) => value,
        Err(message) => {
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
    };

    let user_id = match fetch_linked_user_id(&state.main_db, telegram_user_id).await {
        Ok(Some(user_id)) => user_id,
        Ok(None) => {
            send_help(bot, msg.chat.id).await?;
            return Ok(());
        }
        Err(message) => {
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
    };

    let mut categories = match load_categories(&state.db_pool, &user_id).await {
        Ok(categories) => categories,
        Err(message) => {
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
    };

    let mut category_hint: Option<AiCategoryHint> = None;
    let mut similar_records: Vec<SimilarRecord> = Vec::new();

    if !categories.is_empty()
        && let Ok(hint) = classify_message(
            &state.http,
            &state.openai_api_key,
            &state.openai_model,
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
            && let Ok(records) = load_similar_records(
                &state.db_pool,
                &user_id,
                &category_id,
                hint.amount,
                SIMILAR_RECORDS_DAYS,
                SIMILAR_RECORDS_LIMIT,
            )
            .await
        {
            similar_records = records;
        }

        category_hint = Some(hint);
    }

    let ai_record = match extract_record(
        state,
        text,
        &categories,
        &similar_records,
        category_hint.as_ref(),
    )
    .await
    {
        Ok(record) => record,
        Err(message) => {
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
    };

    if ai_record.needs_clarification || ai_record.amount == 0.0 {
        let clarification = if ai_record.clarification.trim().is_empty() {
            "I need more details (amount, category, or date).".to_string()
        } else {
            ai_record.clarification.clone()
        };
        bot.send_message(msg.chat.id, clarification).await?;
        return Ok(());
    }

    if let Err((_, message)) = validate_date(&ai_record.date) {
        bot.send_message(msg.chat.id, message).await?;
        return Ok(());
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

            let created = get_or_create_category(
                &state.db_pool,
                &user_id,
                fallback_name,
                ai_record.is_income,
            )
            .await;
            match created {
                Ok(category) => {
                    let category_id = category.id.clone();
                    if !categories.iter().any(|c| c.id == category_id) {
                        categories.push(category);
                    }
                    category_id
                }
                Err(message) => {
                    bot.send_message(msg.chat.id, message).await?;
                    return Ok(());
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

    let record = match records::create_record_for_user(&state.db_pool, &user_id, payload).await {
        Ok(record) => record,
        Err((_, message)) => {
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
    };

    let summary = build_record_summary(&record, &categories);
    bot.send_message(msg.chat.id, summary).await?;
    Ok(())
}

fn looks_like_edit_request(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    EDIT_REQUEST_KEYWORDS
        .iter()
        .any(|keyword| lowered.contains(keyword))
}

fn normalize_amount_by_category(amount: f64, is_income: bool) -> f64 {
    if is_income {
        amount.abs()
    } else {
        -amount.abs()
    }
}

fn looks_like_delete_request(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    DELETE_REQUEST_KEYWORDS
        .iter()
        .any(|keyword| lowered.contains(keyword))
}

fn parse_decision(text: &str) -> Option<Decision> {
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

async fn cleanup_expired_pending_actions(state: &BotState) {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let mut pending = state.pending_actions.write().await;
    pending.retain(|_, actions| {
        actions.retain(|action| action.expires_at > now);
        !actions.is_empty()
    });
}

fn format_pending_actions(actions: &[PendingAction]) -> String {
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

fn select_pending_action(
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

async fn handle_decision(
    bot: &Bot,
    msg: Message,
    state: &BotState,
    decision: Decision,
) -> Result<(), BotError> {
    let telegram_user_id = match telegram_user_id(&msg) {
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
        select_pending_action(&mut pending, telegram_user_id, target_action_id)
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

async fn handle_edit_message(
    bot: &Bot,
    msg: Message,
    state: &BotState,
    text: &str,
) -> Result<(), BotError> {
    let telegram_user_id = match telegram_user_id(&msg) {
        Ok(value) => value,
        Err(message) => {
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
    };

    let user_id = match fetch_linked_user_id(&state.main_db, telegram_user_id).await {
        Ok(Some(user_id)) => user_id,
        Ok(None) => {
            send_help(bot, msg.chat.id).await?;
            return Ok(());
        }
        Err(message) => {
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
    };

    let categories = match load_categories(&state.db_pool, &user_id).await {
        Ok(categories) => categories,
        Err(message) => {
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
    };

    let recent_records =
        match load_recent_records(&state.db_pool, &user_id, RECORD_CONTEXT_LIMIT).await {
            Ok(records) => records,
            Err(message) => {
                bot.send_message(msg.chat.id, message).await?;
                return Ok(());
            }
        };

    let edit = match extract_edit(state, text, &categories, &recent_records).await {
        Ok(edit) => edit,
        Err(message) => {
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
    };

    if edit.needs_clarification {
        let clarification = if edit.clarification.trim().is_empty() {
            "Please describe one specific edit target and change."
        } else {
            edit.clarification.trim()
        };
        bot.send_message(msg.chat.id, clarification).await?;
        return Ok(());
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
            bot.send_message(msg.chat.id, message).await?;
            return Ok(());
        }
    };

    let action_id = pending_action.id.clone();
    let expires_in = pending_action.expires_at - OffsetDateTime::now_utc().unix_timestamp();
    let summary = pending_action.summary.clone();
    {
        let mut pending = state.pending_actions.write().await;
        pending
            .entry(telegram_user_id)
            .or_insert_with(Vec::new)
            .push(pending_action);
    }

    let response = format!(
        "Pending edit {} (expires in {}s): {}\nReply with confirm/cancel, or /confirm {} / /cancel {}.",
        action_id,
        expires_in.max(0),
        summary,
        action_id,
        action_id
    );
    bot.send_message(msg.chat.id, response).await?;
    Ok(())
}

async fn build_pending_action_from_edit(
    db_pool: &DbPool,
    user_id: &str,
    edit: &AiEditResult,
    recent_records: &[Record],
    categories: &[CategoryInfo],
) -> Result<PendingAction, String> {
    match edit.target_type.trim().to_ascii_lowercase().as_str() {
        "record" => {
            build_pending_record_edit(db_pool, user_id, edit, recent_records, categories).await
        }
        "category" => build_pending_category_edit(user_id, edit, categories),
        _ => Err("Please specify whether you want to edit one record or one category.".to_string()),
    }
}

async fn upsert_telegram_link(
    db: &Db,
    telegram_user_id: i64,
    chat_id: i64,
    user_id: &str,
) -> Result<(), String> {
    let conn = db.write().await;
    let created_at = OffsetDateTime::now_utc().unix_timestamp();

    conn.execute(
        "INSERT INTO telegram_users (telegram_user_id, user_id, chat_id, created_at) VALUES (?, ?, ?, ?)\
        ON CONFLICT(telegram_user_id) DO UPDATE SET user_id = excluded.user_id, chat_id = excluded.chat_id",
        (
            telegram_user_id.to_string(),
            user_id,
            chat_id.to_string(),
            created_at,
        ),
    )
    .await
    .map_err(|_| "Failed to link Telegram user".to_string())?;

    Ok(())
}

async fn fetch_linked_user_id(db: &Db, telegram_user_id: i64) -> Result<Option<String>, String> {
    let conn = db.read().await;
    let mut rows = conn
        .query(
            "SELECT user_id FROM telegram_users WHERE telegram_user_id = ?",
            [telegram_user_id.to_string()],
        )
        .await
        .map_err(|_| "Failed to lookup Telegram user".to_string())?;

    if let Some(row) = rows
        .next()
        .await
        .map_err(|_| "Failed to lookup Telegram user".to_string())?
    {
        let user_id: String = row
            .get(0)
            .map_err(|_| "Failed to read Telegram user".to_string())?;
        Ok(Some(user_id))
    } else {
        Ok(None)
    }
}

async fn load_categories(db_pool: &DbPool, user_id: &str) -> Result<Vec<CategoryInfo>, String> {
    let user_db = get_user_database_from_pool(db_pool, user_id)
        .await
        .map_err(|(_, message)| message)?;
    let conn = user_db.read().await;
    let mut rows = conn
        .query(
            "SELECT id, name, is_income FROM categories ORDER BY name ASC",
            (),
        )
        .await
        .map_err(|_| "Failed to query categories".to_string())?;

    let mut categories = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| "Failed to query categories".to_string())?
    {
        let id: String = row.get(0).map_err(|_| "Invalid category".to_string())?;
        let name: String = row.get(1).map_err(|_| "Invalid category".to_string())?;
        let is_income: bool = row.get(2).map_err(|_| "Invalid category".to_string())?;
        categories.push(CategoryInfo {
            id,
            name,
            is_income,
        });
    }

    Ok(categories)
}

async fn load_similar_records(
    db_pool: &DbPool,
    user_id: &str,
    category_id: &str,
    amount: f64,
    days: i64,
    limit: usize,
) -> Result<Vec<SimilarRecord>, String> {
    let user_db = get_user_database_from_pool(db_pool, user_id)
        .await
        .map_err(|(_, message)| message)?;
    let conn = user_db.read().await;
    let cutoff = OffsetDateTime::now_utc() - Duration::days(days);
    let cutoff_date = cutoff.date().to_string();
    let delta = amount.abs() * SIMILAR_AMOUNT_RATIO;
    let lower = amount - delta;
    let upper = amount + delta;

    let mut rows = conn
        .query(
            "SELECT name, amount FROM records WHERE category_id = ? AND date >= ? AND amount BETWEEN ? AND ? ORDER BY ABS(amount - ?) ASC, date DESC LIMIT ?",
            (
                category_id,
                cutoff_date.as_str(),
                lower,
                upper,
                amount,
                limit as i64,
            ),
        )
        .await
        .map_err(|_| "Failed to query similar records".to_string())?;

    let mut records = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| "Failed to query similar records".to_string())?
    {
        let name: String = row.get(0).map_err(|_| "Invalid record".to_string())?;
        let amount: f64 = row.get(1).map_err(|_| "Invalid record".to_string())?;
        records.push(SimilarRecord { name, amount });
    }

    Ok(records)
}

async fn load_recent_records(
    db_pool: &DbPool,
    user_id: &str,
    limit: usize,
) -> Result<Vec<Record>, String> {
    let user_db = get_user_database_from_pool(db_pool, user_id)
        .await
        .map_err(|(_, message)| message)?;
    let conn = user_db.read().await;

    let mut rows = conn
        .query(
            "SELECT id, name, amount, category_id, date FROM records ORDER BY date DESC LIMIT ?",
            [limit as i64],
        )
        .await
        .map_err(|_| "Failed to query recent records".to_string())?;

    let mut records = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|_| "Failed to query recent records".to_string())?
    {
        let record =
            records::extract_record_from_row(row).map_err(|(_, message)| message.to_string())?;
        records.push(record);
    }

    Ok(records)
}

fn resolve_record_id_by_name(records: &[Record], target_name: &str) -> Result<String, String> {
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

async fn fetch_record_by_id(
    db_pool: &DbPool,
    user_id: &str,
    record_id: &str,
) -> Result<Record, String> {
    let user_db = get_user_database_from_pool(db_pool, user_id)
        .await
        .map_err(|(_, message)| message)?;
    let conn = user_db.read().await;
    let mut rows = conn
        .query(
            "SELECT id, name, amount, category_id, date FROM records WHERE id = ?",
            [record_id],
        )
        .await
        .map_err(|_| "Failed to query existing record".to_string())?;

    if let Some(row) = rows
        .next()
        .await
        .map_err(|_| "Failed to query existing record".to_string())?
    {
        records::extract_record_from_row(row).map_err(|(_, message)| message)
    } else {
        Err("Record not found.".to_string())
    }
}

async fn build_pending_record_edit(
    db_pool: &DbPool,
    user_id: &str,
    edit: &AiEditResult,
    records: &[Record],
    categories: &[CategoryInfo],
) -> Result<PendingAction, String> {
    let trimmed_id = edit.target_id.trim();
    let existing = if !trimmed_id.is_empty() {
        fetch_record_by_id(db_pool, user_id, trimmed_id).await?
    } else {
        let record_id = resolve_record_id_by_name(records, &edit.target_name)?;
        records
            .iter()
            .find(|record| record.id == record_id)
            .cloned()
            .ok_or_else(|| "Record not found.".to_string())?
    };
    let record_id = existing.id.clone();

    let new_name = edit
        .new_name
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(name) = &new_name {
        records::validate_record_name(name).map_err(|(_, message)| message)?;
    }

    if let Some(amount) = edit.new_amount {
        records::validate_record_amount(amount).map_err(|(_, message)| message)?;
    }

    let new_date = edit
        .new_date
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(date) = &new_date {
        validate_date(date).map_err(|(_, message)| message)?;
    }

    let provided_category_id = edit.new_category_id.as_deref().unwrap_or("");
    let provided_category_name = edit.new_category_name.as_deref().unwrap_or("");
    let new_category_id =
        resolve_category_id(categories, provided_category_id, provided_category_name);
    if (!provided_category_id.trim().is_empty() || !provided_category_name.trim().is_empty())
        && new_category_id.is_none()
    {
        return Err("Category not found for record update.".to_string());
    }

    let patch = PendingRecordPatch {
        name: new_name.clone(),
        amount: edit.new_amount,
        category_id: new_category_id.clone(),
        date: new_date.clone(),
    };

    if patch.is_empty() {
        return Err(
            "No record field changes detected. Please specify at least one field to edit."
                .to_string(),
        );
    }

    let mut parts = Vec::new();
    if let Some(name) = &patch.name {
        parts.push(format!("name: '{}' -> '{}'", existing.name, name));
    }
    if let Some(amount) = patch.amount {
        parts.push(format!("amount: {} -> {}", existing.amount, amount));
    }
    if let Some(category_id) = &patch.category_id {
        let old_name = categories
            .iter()
            .find(|category| category.id == existing.category_id)
            .map(|category| category.name.as_str())
            .unwrap_or("Unknown");
        let new_name = categories
            .iter()
            .find(|category| category.id == *category_id)
            .map(|category| category.name.as_str())
            .unwrap_or("Unknown");
        parts.push(format!("category: '{}' -> '{}'", old_name, new_name));
    }
    if let Some(date) = &patch.date {
        parts.push(format!("date: {} -> {}", existing.date, date));
    }

    let now = OffsetDateTime::now_utc().unix_timestamp();
    Ok(PendingAction {
        id: Uuid::new_v4().to_string(),
        user_id: user_id.to_string(),
        expires_at: now + PENDING_ACTION_TTL_SECONDS,
        summary: format!("record {}: {}", record_id, parts.join(", ")),
        action: PendingActionType::RecordEdit { record_id, patch },
    })
}

fn build_pending_category_edit(
    user_id: &str,
    edit: &AiEditResult,
    categories: &[CategoryInfo],
) -> Result<PendingAction, String> {
    let target_id = if edit.target_id.trim().is_empty() {
        edit.category_id.trim()
    } else {
        edit.target_id.trim()
    };
    let target_name = if edit.target_name.trim().is_empty() {
        edit.category_name.trim()
    } else {
        edit.target_name.trim()
    };

    let category_id = resolve_category_id(categories, target_id, target_name).ok_or_else(|| {
        "Category not found. Please include category id or exact name.".to_string()
    })?;
    let category = categories
        .iter()
        .find(|item| item.id == category_id)
        .ok_or_else(|| "Category not found.".to_string())?;
    let new_name = edit
        .new_name
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Category edit only supports updating name.".to_string())?;

    validate_category_name(&new_name).map_err(|(_, message)| message)?;
    if category.name == new_name {
        return Err("Category name is unchanged.".to_string());
    }
    if categories
        .iter()
        .any(|item| item.id != category.id && item.name.eq_ignore_ascii_case(&new_name))
    {
        return Err("Category name already exists (case-insensitive).".to_string());
    }

    let now = OffsetDateTime::now_utc().unix_timestamp();
    Ok(PendingAction {
        id: Uuid::new_v4().to_string(),
        user_id: user_id.to_string(),
        expires_at: now + PENDING_ACTION_TTL_SECONDS,
        summary: format!(
            "category {}: name '{}' -> '{}'",
            category.id, category.name, new_name
        ),
        action: PendingActionType::CategoryEdit {
            category_id: category.id.clone(),
            new_name,
        },
    })
}

async fn execute_pending_action(
    state: &BotState,
    pending: &PendingAction,
) -> Result<String, String> {
    if pending.expires_at <= OffsetDateTime::now_utc().unix_timestamp() {
        return Err(
            "This confirmation expired after 3 minutes. Please request the edit again.".to_string(),
        );
    }

    match &pending.action {
        PendingActionType::RecordEdit { record_id, patch } => {
            apply_record_edit(&state.db_pool, &pending.user_id, record_id, patch).await?;
            Ok(format!("Confirmed and updated {}.", pending.summary))
        }
        PendingActionType::CategoryEdit {
            category_id,
            new_name,
        } => {
            apply_category_name_edit(&state.db_pool, &pending.user_id, category_id, new_name)
                .await?;
            Ok(format!("Confirmed and updated {}.", pending.summary))
        }
    }
}

async fn apply_record_edit(
    db_pool: &DbPool,
    user_id: &str,
    record_id: &str,
    patch: &PendingRecordPatch,
) -> Result<(), String> {
    if patch.is_empty() {
        return Err("No record fields provided for update.".to_string());
    }

    if let Some(name) = &patch.name {
        records::validate_record_name(name).map_err(|(_, message)| message)?;
    }
    if let Some(amount) = patch.amount {
        records::validate_record_amount(amount).map_err(|(_, message)| message)?;
    }
    if let Some(date) = &patch.date {
        validate_date(date).map_err(|(_, message)| message)?;
    }

    let user_db = get_user_database_from_pool(db_pool, user_id)
        .await
        .map_err(|(_, message)| message)?;
    if let Some(category_id) = &patch.category_id {
        my_budget_server::utils::validate_category_exists(&user_db, category_id)
            .await
            .map_err(|(_, message)| message)?;
    }

    let conn = user_db.write().await;
    let mut existing_rows = conn
        .query(
            "SELECT id, name, amount, category_id, date FROM records WHERE id = ?",
            [record_id],
        )
        .await
        .map_err(|_| "Failed to query existing record".to_string())?;

    let existing = if let Some(row) = existing_rows
        .next()
        .await
        .map_err(|_| "Failed to query existing record".to_string())?
    {
        records::extract_record_from_row(row).map_err(|(_, message)| message)?
    } else {
        return Err("Record not found.".to_string());
    };

    let updated_name = patch.name.as_deref().unwrap_or(&existing.name);
    let updated_category_id = patch
        .category_id
        .as_deref()
        .unwrap_or(&existing.category_id);
    let updated_amount = if let Some(amount) = patch.amount {
        let mut category_rows = conn
            .query(
                "SELECT is_income FROM categories WHERE id = ?",
                [updated_category_id],
            )
            .await
            .map_err(|_| "Failed to query category type".to_string())?;

        let is_income: bool = if let Some(row) = category_rows
            .next()
            .await
            .map_err(|_| "Failed to query category type".to_string())?
        {
            row.get(0)
                .map_err(|_| "Invalid category data".to_string())?
        } else {
            return Err("Category not found.".to_string());
        };

        normalize_amount_by_category(amount, is_income)
    } else {
        existing.amount
    };
    let updated_date = patch.date.as_deref().unwrap_or(&existing.date);

    let affected_rows = conn
        .execute(
            "UPDATE records SET name = ?, amount = ?, category_id = ?, date = ? WHERE id = ?",
            (
                updated_name,
                updated_amount,
                updated_category_id,
                updated_date,
                record_id,
            ),
        )
        .await
        .map_err(|_| "Failed to update record".to_string())?;
    if affected_rows == 0 {
        return Err("Record not found or no changes made.".to_string());
    }

    Ok(())
}

async fn apply_category_name_edit(
    db_pool: &DbPool,
    user_id: &str,
    category_id: &str,
    new_name: &str,
) -> Result<(), String> {
    validate_category_name(new_name).map_err(|(_, message)| message)?;

    let user_db = get_user_database_from_pool(db_pool, user_id)
        .await
        .map_err(|(_, message)| message)?;
    let conn = user_db.write().await;

    let mut existing_rows = conn
        .query(
            "SELECT id, name, is_income FROM categories WHERE id = ?",
            [category_id],
        )
        .await
        .map_err(|_| "Failed to query existing category".to_string())?;
    if existing_rows
        .next()
        .await
        .map_err(|_| "Failed to query existing category".to_string())?
        .is_none()
    {
        return Err("Category not found.".to_string());
    }

    let mut conflict_rows = conn
        .query(
            "SELECT id FROM categories WHERE LOWER(name) = LOWER(?) AND id != ?",
            (new_name, category_id),
        )
        .await
        .map_err(|_| "Failed to check category name conflict".to_string())?;
    if conflict_rows
        .next()
        .await
        .map_err(|_| "Failed to check category name conflict".to_string())?
        .is_some()
    {
        return Err("Category name already exists (case-insensitive).".to_string());
    }

    let affected_rows = conn
        .execute(
            "UPDATE categories SET name = ? WHERE id = ?",
            (new_name, category_id),
        )
        .await
        .map_err(|_| "Failed to update category".to_string())?;
    if affected_rows == 0 {
        return Err("Category not found or no changes made.".to_string());
    }

    Ok(())
}

async fn get_or_create_category(
    db_pool: &DbPool,
    user_id: &str,
    name: &str,
    is_income: bool,
) -> Result<CategoryInfo, String> {
    let trimmed = name.trim();
    let fallback = if trimmed.is_empty() { "Other" } else { trimmed };
    validate_category_name(fallback).map_err(|(_, message)| message)?;

    let user_db = get_user_database_from_pool(db_pool, user_id)
        .await
        .map_err(|(_, message)| message)?;
    let conn = user_db.write().await;

    let mut existing = conn
        .query(
            "SELECT id, name, is_income FROM categories WHERE LOWER(name) = LOWER(?)",
            [fallback],
        )
        .await
        .map_err(|_| "Failed to query categories".to_string())?;

    if let Some(row) = existing
        .next()
        .await
        .map_err(|_| "Failed to query categories".to_string())?
    {
        let id: String = row.get(0).map_err(|_| "Invalid category".to_string())?;
        let name: String = row.get(1).map_err(|_| "Invalid category".to_string())?;
        let is_income: bool = row.get(2).map_err(|_| "Invalid category".to_string())?;
        return Ok(CategoryInfo {
            id,
            name,
            is_income,
        });
    }

    let category_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO categories (id, name, is_income) VALUES (?, ?, ?)",
        (category_id.as_str(), fallback, is_income),
    )
    .await
    .map_err(|_| "Failed to create category".to_string())?;

    Ok(CategoryInfo {
        id: category_id,
        name: fallback.to_string(),
        is_income,
    })
}

fn resolve_category_id(
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

fn build_record_summary(record: &Record, categories: &[CategoryInfo]) -> String {
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

async fn extract_edit(
    state: &BotState,
    message: &str,
    categories: &[CategoryInfo],
    records: &[Record],
) -> Result<AiEditResult, String> {
    let category_list = if categories.is_empty() {
        "(none)".to_string()
    } else {
        categories
            .iter()
            .map(|category| {
                format!(
                    "- {} | {} | is_income={}",
                    category.id, category.name, category.is_income
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let record_list = if records.is_empty() {
        "(none)".to_string()
    } else {
        records
            .iter()
            .map(|record| {
                format!(
                    "- {} | {} | amount={} | category_id={} | date={}",
                    record.id, record.name, record.amount, record.category_id, record.date
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let input = format!(
        "You are a finance assistant. Extract one edit operation from the user's message.\n\nUser message:\n{}\n\nCategories:\n{}\n\nRecent records:\n{}\n\nRules:\n- target_type must be one of: record, category, none.\n- Only one target is allowed. If multiple edits are requested, set needs_clarification=true.\n- Never perform delete operations. If user asks delete, set needs_clarification=true.\n- For category edits, only name change is allowed.\n- For record edits, any of name/amount/category/date may be changed.\n- If unknown target, set needs_clarification=true with a short clarification message.\n",
        message, category_list, record_list
    );

    let schema = json!({
        "type": "object",
        "properties": {
            "target_type": { "type": "string" },
            "target_id": { "type": "string" },
            "target_name": { "type": "string" },
            "category_id": { "type": "string" },
            "category_name": { "type": "string" },
            "new_name": { "type": ["string", "null"] },
            "new_amount": { "type": ["number", "null"] },
            "new_category_id": { "type": ["string", "null"] },
            "new_category_name": { "type": ["string", "null"] },
            "new_date": { "type": ["string", "null"] },
            "needs_clarification": { "type": "boolean" },
            "clarification": { "type": "string" }
        },
        "required": [
            "target_type",
            "target_id",
            "target_name",
            "category_id",
            "category_name",
            "new_name",
            "new_amount",
            "new_category_id",
            "new_category_name",
            "new_date",
            "needs_clarification",
            "clarification"
        ],
        "additionalProperties": false
    });

    call_openai_json(
        &state.http,
        &state.openai_api_key,
        &state.openai_model,
        "edit",
        input,
        schema,
    )
    .await
}

async fn classify_message(
    http: &Client,
    api_key: &str,
    model: &str,
    timezone: &str,
    message: &str,
    categories: &[CategoryInfo],
) -> Result<AiCategoryHint, String> {
    let category_list = categories
        .iter()
        .map(|c| format!("- {} | {} | is_income={}", c.id, c.name, c.is_income))
        .collect::<Vec<_>>()
        .join("\n");

    let input = format!(
        "You are a finance assistant. Extract amount and category for the user's message.\n\nUser message:\n{}\n\nCategories:\n{}\n\nTimezone: {}\n\nRules:\n- Choose the best category_id from the list.\n- If unsure about category, set category_id and category_name to empty strings.\n- If amount is missing, set amount to 0.\n",
        message, category_list, timezone
    );

    let schema = json!({
        "type": "object",
        "properties": {
            "amount": { "type": "number" },
            "category_id": { "type": "string" },
            "category_name": { "type": "string" },
            "is_income": { "type": "boolean" }
        },
        "required": ["amount", "category_id", "category_name", "is_income"],
        "additionalProperties": false
    });

    call_openai_json(http, api_key, model, "category_hint", input, schema).await
}

async fn extract_record(
    state: &BotState,
    message: &str,
    categories: &[CategoryInfo],
    similar_records: &[SimilarRecord],
    hint: Option<&AiCategoryHint>,
) -> Result<AiRecordResult, String> {
    let category_list = if categories.is_empty() {
        "(none)".to_string()
    } else {
        categories
            .iter()
            .map(|c| format!("- {} | {} | is_income={}", c.id, c.name, c.is_income))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let similar_list = if similar_records.is_empty() {
        "(none)".to_string()
    } else {
        similar_records
            .iter()
            .map(|r| format!("- {} ({})", r.name, r.amount))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let now_date = OffsetDateTime::now_utc().date().to_string();
    let hint_text = match hint {
        Some(hint) => format!(
            "Pre-extracted: amount={}, category_id='{}', category_name='{}', is_income={}\n",
            hint.amount, hint.category_id, hint.category_name, hint.is_income
        ),
        None => "Pre-extracted: (none)\n".to_string(),
    };

    let input = format!(
        "You are a finance assistant. Convert the user's message into a record JSON.\n\nUser message:\n{}\n\nCategories:\n{}\n\nSimilar records (same category, similar amount, last {} days):\n{}\n\n{}\nTimezone: {}\nCurrent date (YYYY-MM-DD): {}\n\nRules:\n- Return date in YYYY-MM-DD.\n- If a similar record name matches, reuse its exact name.\n- If category is unclear, leave category_id and category_name empty.\n- If info is missing, set needs_clarification=true and provide a short clarification message.\n",
        message,
        category_list,
        SIMILAR_RECORDS_DAYS,
        similar_list,
        hint_text,
        state.timezone,
        now_date
    );

    let schema = json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "amount": { "type": "number" },
            "category_id": { "type": "string" },
            "category_name": { "type": "string" },
            "date": { "type": "string" },
            "is_income": { "type": "boolean" },
            "needs_clarification": { "type": "boolean" },
            "clarification": { "type": "string" }
        },
        "required": ["name", "amount", "category_id", "category_name", "date", "is_income", "needs_clarification", "clarification"],
        "additionalProperties": false
    });

    call_openai_json(
        &state.http,
        &state.openai_api_key,
        &state.openai_model,
        "record",
        input,
        schema,
    )
    .await
}

async fn call_openai_json<T: for<'de> Deserialize<'de>>(
    http: &Client,
    api_key: &str,
    model: &str,
    schema_name: &str,
    input: String,
    schema: serde_json::Value,
) -> Result<T, String> {
    let body = json!({
        "model": model,
        "input": input,
        "text": {
            "format": {
                "type": "json_schema",
                "name": schema_name,
                "strict": true,
                "schema": schema
            }
        }
    });

    let response = http
        .post("https://api.openai.com/v1/responses")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|_| "Failed to contact OpenAI".to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("OpenAI error: {} {}", status, text));
    }

    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|_| "Failed to parse OpenAI response".to_string())?;

    let payload = extract_output_json(&value)?;
    serde_json::from_value(payload).map_err(|_| "Failed to parse OpenAI output".to_string())
}

fn extract_output_json(value: &serde_json::Value) -> Result<serde_json::Value, String> {
    if let Some(output_text) = value.get("output_text").and_then(|v| v.as_str()) {
        return serde_json::from_str(output_text)
            .map_err(|_| "Failed to parse OpenAI output".to_string());
    }

    let outputs = value
        .get("output")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "OpenAI response missing output".to_string())?;

    for output in outputs {
        if let Some(contents) = output.get("content").and_then(|v| v.as_array()) {
            for content in contents {
                if let Some(kind) = content.get("type").and_then(|v| v.as_str()) {
                    if kind == "output_json"
                        && let Some(json_value) = content.get("json")
                    {
                        return Ok(json_value.clone());
                    }
                    if kind == "output_text"
                        && let Some(text_value) = content.get("text").and_then(|v| v.as_str())
                    {
                        return serde_json::from_str(text_value)
                            .map_err(|_| "Failed to parse OpenAI output".to_string());
                    }
                }
            }
        }
    }

    Err("OpenAI response missing content".to_string())
}
