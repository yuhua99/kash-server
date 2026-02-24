use std::collections::HashMap;
use std::sync::Arc;

use reqwest::Client;
use teloxide::dispatching::UpdateFilterExt;
use tokio::sync::RwLock;

use my_budget_server::constants::DEFAULT_DATA_PATH;
use my_budget_server::database;

mod constants;
mod db;
mod handlers;
mod helpers;
mod models;
mod openai;

use models::{BotError, BotState};

#[tokio::main]
async fn main() -> Result<(), BotError> {
    dotenv::dotenv().ok();

    let bot_token =
        std::env::var("TELEGRAM_BOT_TOKEN").map_err(|_| "TELEGRAM_BOT_TOKEN is required")?;
    let bot = teloxide::Bot::new(bot_token);

    let openai_api_key =
        std::env::var("OPENAI_API_KEY").map_err(|_| "OPENAI_API_KEY is required")?;
    let openai_model = std::env::var("OPENAI_MODEL")
        .unwrap_or_else(|_| constants::DEFAULT_OPENAI_MODEL.to_string());
    let openai_reasoning_effort = std::env::var("OPENAI_REASONING_EFFORT")
        .unwrap_or_else(|_| constants::DEFAULT_REASONING_EFFORT.to_string());
    let timezone =
        std::env::var("BOT_TIMEZONE").unwrap_or_else(|_| constants::DEFAULT_TIMEZONE.to_string());

    let data_path =
        std::env::var("DATABASE_PATH").unwrap_or_else(|_| DEFAULT_DATA_PATH.to_string());
    let main_db = database::init_main_db(&data_path).await?;

    let state = BotState {
        main_db,
        http: Client::new(),
        openai_api_key,
        openai_model,
        openai_reasoning_effort,
        timezone,
        chat_contexts: Arc::new(RwLock::new(HashMap::new())),
    };

    let handler = teloxide::prelude::Update::filter_message().endpoint(handlers::handle_message);
    teloxide::prelude::Dispatcher::builder(bot, handler)
        .dependencies(teloxide::dptree::deps![state])
        .build()
        .dispatch()
        .await;

    Ok(())
}
