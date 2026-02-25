# Repository Atlas: kash-server

## Project Responsibility
A personal budget tracking server with two delivery surfaces:
1. **HTTP REST API** (`src/main.rs`) — Axum-based web server for a frontend SPA; handles auth, records, categories, friends, and expense splits
2. **Telegram Bot** (`src/bin/tg/main.rs`) — AI-powered assistant that lets users manage budget records via natural language (text, voice, or photos) using OpenAI tool calls

Both surfaces share a single SQLite database (`data/users.db`) via the `kash_server` library crate.

## System Entry Points
- `src/main.rs` — HTTP server binary: wires `Config`, `AppState`, session layer, CORS, and Axum router
- `src/bin/tg/main.rs` — Telegram bot binary: wires `BotState`, Teloxide dispatcher, OpenAI config
- `src/lib.rs` — Library crate root: re-exports `Db`, `init_main_db`, `AppState`, `with_transaction`
- `src/database.rs` — Schema definition and DB initializer (`init_main_db`)
- `Cargo.toml` — Defines `kash-server` lib + two binaries; key deps: `axum`, `libsql`, `teloxide`, `tower-sessions`, `argon2`, `reqwest`, `serde_json`, `time`, `uuid`

## Key Architectural Decisions
- **Single shared DB** (`Arc<RwLock<Connection>>`): all users, records, categories, and friendships in one file — multi-tenancy enforced by `owner_user_id` column, not separate DB files
- **No ORM**: raw SQL via `libsql` async API; all queries inline in handler modules
- **Idempotent splits**: reserve-then-commit pattern with tombstone cleanup on failure
- **Session auth**: `tower-sessions` `MemoryStore` with signed cookies (not JWT)

## Directory Map

| Directory | Responsibility | Detailed Map |
|-----------|---------------|--------------|
| `src/` | Service layer + DAOs for HTTP API; shared `AppState`, transaction helper, all business logic modules | [View Map](src/codemap.md) |
| `src/bin/` | Container for standalone binary crates | [View Map](src/bin/codemap.md) |
| `src/bin/tg/` | Telegram bot: Teloxide dispatcher, OpenAI tool-call loop, DB helpers, conversation context management | [View Map](src/bin/tg/codemap.md) |

## Module Summary

| Module | Role |
|--------|------|
| `src/database.rs` | Schema DDL + `init_main_db()` |
| `src/auth.rs` | Register, login, logout, `get_current_user`, Argon2 hashing |
| `src/records.rs` | CRUD for expense/income records, settle, finalize-pending |
| `src/categories.rs` | CRUD for user-owned categories |
| `src/splits.rs` | Expense split fanout with idempotency |
| `src/friends.rs` | Friend request, accept, block, unfriend, nickname, search |
| `src/models.rs` | Shared request/response types (serde structs) |
| `src/utils.rs` | Validation helpers, split math, DB error constructors |
| `src/config.rs` | `Config::from_env()` — reads env vars with validation |
| `src/constants.rs` | App-wide string/numeric constants |
| `src/bin/tg/handlers.rs` | Telegram message dispatcher (text/voice/photo → AI turn) |
| `src/bin/tg/openai.rs` | OpenAI Responses API loop + Whisper transcription |
| `src/bin/tg/db.rs` | Bot-side DB helpers: link user, CRUD records/categories via `owner_user_id` |
| `src/bin/tg/models.rs` | `BotState`, `ChatContext`, `CategoryInfo`, conversation context types |
| `src/bin/tg/helpers.rs` | Context lifecycle (TTL, push/get turns), amount normalization |
| `src/bin/tg/constants.rs` | Bot-specific constants (model names, limits, TTLs) |
