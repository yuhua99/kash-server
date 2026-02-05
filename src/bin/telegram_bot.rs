use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use teloxide::prelude::*;
use time::{Duration, OffsetDateTime};
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

#[derive(Clone)]
struct BotState {
    main_db: Db,
    db_pool: DbPool,
    http: Client,
    openai_api_key: String,
    openai_model: String,
    timezone: String,
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

            let created =
                get_or_create_category(&state.db_pool, &user_id, fallback_name, ai_record.is_income)
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
