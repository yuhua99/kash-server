# codemap for src/bin/tg

## Responsibility
- The Telegram bot binary exposes a budget-assistant API over Telegram: it links users (via `/link <username> <password>`) to Okta-style auth from `kash_server::auth`, listens for text/voice/photo requests, routes them through OpenAI tools, and persists category/record data in the shared `Db` so budget data stays synchronized with the main application.

## Design
- Teloxide is the runtime: `main.rs` builds a `teloxide::Bot`, wraps the `handlers::handle_message` endpoint in a dispatcher (`teloxide::prelude::Dispatcher::builder`) and injects shared dependencies (`state`) via `teloxide::dptree::deps!`.
- `models::BotState` centralizes resources: `Db` from `kash_server`, `reqwest::Client`, OpenAI config strings, timezone, and an `Arc<RwLock<HashMap<ContextKey, ChatContext>>>` for context TTL/replay logic (see `helpers.rs`).
- Handler dispatch: `handlers::handle_message` filters updates to messages, delegates to `handle_text_message`, `handle_voice_message`, or `handle_photo_message`, enforces `/start` and `/link` flows, calls `handle_ai_turn`, and maintains typing indicators via `send_chat_action`.
- OpenAI integration sits in `openai.rs`: `respond_with_tools` builds a system prompt referencing categories, iterates up to `TOOL_MAX_ROUNDS`, inspects `responses` output for tool calls, and pushes results back into OpenAI before returning formatted replies. `transcribe_voice` calls OpenAI Whisper/Transcriptions API with `DEFAULT_WHISPER_MODEL`.
- DB access pattern in `db.rs`: all queries use `owner_user_id` filters (`WHERE owner_user_id = ?`), categories scoped per user via `load_categories`, `get_or_create_category`, `fetch_record_by_id`/`fetch_record_by_exact_name`, and `records::create_record_for_user`/`records::extract_record_from_row`. `execute_tool_call` routes `create_record`, `edit_record`, and `list_records` through helpers that respect owner scoping, category validation, amount normalization, and explicit error handling.

## Flow
1. Telegram sends `Update`; Teloxide dispatcher (`main.rs`) filters to `Update::filter_message()` and invokes `handlers::handle_message` while sharing `state`.
2. `handle_message` routes by content: text commands go to `/start`, `/link`, then `handle_ai_turn`; voice/photo paths transcribe/download media, generate context text (`[voice]`, `[photo]`), and call `handle_ai_turn`.
3. `handle_ai_turn` ensures user linkage (`db::fetch_linked_user_id`), loads scoped categories (`db::load_categories`), gathers context (`helpers::get_context_messages`), calls `openai::respond_with_tools`, and records the last turn (`helpers::push_context_turn`).
4. `respond_with_tools` loops with OpenAI Responses: builds prompt, appends chat history, inspects tool call outputs, invokes `db::execute_tool_call` (which delegates to `create_record_tool`, `edit_record_tool`, `list_records_tool`), and returns either tool-provided text or error.
5. Tools hit the shared `Db` with owner scoping: create/edit/list validate categories, normalize amounts by income/expense (`helpers::normalize_amount_by_category`), update/insert records, then dispatcher sends final reply via `bot.send_message`.

## Integration
- Uses `kash_server::constants::DEFAULT_DATA_PATH` and `kash_server::database::init_main_db` to bootstrap `Db` in `main.rs`.
- Brings in `kash_server::auth::authenticate_user` (handlers) and `kash_server::models::{CreateRecordPayload, Record}` plus `records` helpers/validators used by `db.rs` for record queries.
- Imports validation utilities from `kash_server::utils` (e.g., `validate_date`, `validate_offset`, `validate_records_limit`) and categorization helpers (`categories::validate_category_name`).
- Context storage is strictly local (BotState) but uses OpenAI tool schema (`openai.rs`) to talk to `respond_with_tools`/`transcribe_voice` with `Reqwest::Client` and config constants from `constants.rs`.
