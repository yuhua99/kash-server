# AGENTS.md — Kash Server

Personal expense tracking backend in Rust (edition 2024) using Axum, libsql, and teloxide.

## Build / Run / Test Commands

```bash
# Build
cargo build
cargo build --release

# Run API server (requires .env with SESSION_SECRET)
cargo run

# Run Telegram bot binary
cargo run --bin tg

# Check (type-check without codegen — fastest feedback)
cargo check

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt
cargo fmt -- --check          # CI check only

# Test (no tests exist yet — the framework is wired up)
cargo test
cargo test -- --nocapture     # show println output
cargo test <test_name>        # run a single test by name
cargo test <module>::         # run all tests in a module
```

## Architecture Overview

Two binaries sharing one library crate (`my_budget_server`):

| Binary | Entrypoint | Purpose |
|---|---|---|
| `my-budget-server` | `src/main.rs` | Axum HTTP API (auth, records, categories) |
| `tg` | `src/bin/tg/main.rs` | Telegram bot with OpenAI tool-calling |

Library crate (`src/lib.rs`) exports: `AppState`, `Db`, `DbPool`, `TransactionError`, `with_transaction`.

### Module layout

```
src/
  lib.rs          # Crate root — re-exports, AppState struct
  main.rs         # HTTP server setup, routing, CORS, sessions
  auth.rs         # Register/login/logout, password hashing (Argon2)
  records.rs      # Record CRUD handlers + validation
  categories.rs   # Category CRUD handlers + transaction usage
  database.rs     # Schema init (main DB + per-user DBs), migrations
  db_pool.rs      # LRU connection pool, transaction helpers
  config.rs       # Config struct + ConfigError enum from env vars
  constants.rs    # All magic numbers and string constants
  models.rs       # Serde structs for API payloads and domain objects
  utils.rs        # Shared validators and DB helpers
  bin/tg/
    main.rs       # Bot entrypoint, dispatcher setup
    handlers.rs   # Message routing (text/voice/photo)
    openai.rs     # Responses API + Whisper transcription
    db.rs         # Tool execution, record/category DB ops
    models.rs     # BotState, ChatContext, CategoryInfo
    helpers.rs    # Context management, category resolution
    constants.rs  # Bot-specific limits and defaults
```

## Key Dependencies

- **axum 0.8** — HTTP framework. Route params use `{id}` syntax (not `:id`).
- **libsql** — SQLite-compatible async DB (Turso). No ORM; raw SQL everywhere.
- **tower-sessions 0.14** — Session middleware with in-memory store and signed cookies.
- **teloxide 0.17** — Telegram bot framework.
- **serde / serde_json** — All API types derive `Serialize`/`Deserialize`.
- **anyhow** — Used in DB/infra layers, NOT in handler return types.
- **argon2** — Password hashing.
- **uuid** — V4 UUIDs for all entity IDs (stored as TEXT in SQLite).

## Code Style & Conventions

### Error handling

- **Handlers** return `Result<(StatusCode, Json<T>), (StatusCode, String)>`. This is the universal pattern — follow it exactly.
- **Internal/DB functions** use `anyhow::Result` (see `database.rs`, `db_pool.rs`).
- **Telegram bot** uses `Result<T, String>` for tool/helper functions, mapping to user-facing error messages.
- Map errors with `.map_err(|_| ...)` or `.map_err(|e| ...)`. Never use `.unwrap()` in handlers.
- Error constants live in `constants.rs` (`ERR_DATABASE_ACCESS`, etc.). Use them.

### Naming

- **snake_case** for functions, variables, modules.
- **PascalCase** for types, enums, structs.
- **SCREAMING_SNAKE_CASE** for constants.
- Payload structs: `CreateXPayload`, `UpdateXPayload`, `GetXQuery`, `GetXResponse`.
- Validation functions: `validate_x_name()`, `validate_x_amount()`.
- Extraction functions: `extract_x_from_row()`.

### Imports

- Group: `std` first, then external crates, then `crate::` imports.
- Use `crate::` paths for library imports within the library crate.
- Binary (`src/bin/tg/`) imports library as `my_budget_server::` and local modules as `crate::`.
- Prefer specific imports over glob (`use crate::constants::*` is the one exception).

### Types & Database

- All entity IDs are `String` (UUID v4), stored as `TEXT` in SQLite.
- Dates are `String` in `YYYY-MM-DD` format, validated with `time::Date::parse`.
- Amounts are `f64` / `REAL`. Expenses are negative, income is positive (normalized by category).
- `Db` = `Arc<RwLock<Connection>>`. Use `.read().await` for queries, `.write().await` for mutations.
- Use `DbPool::get_user_db()` for per-user database access (LRU-cached).

### Validation

- Validate all inputs at handler entry. Check emptiness, length, format.
- String lengths: constants in `constants.rs` (`MAX_RECORD_NAME_LENGTH`, etc.).
- Pagination: validate limit (1..MAX_LIMIT) and offset (0..MAX_OFFSET).
- Category existence: always verify before creating/updating records.
- Return `(StatusCode::BAD_REQUEST, "message".to_string())` for validation failures.

### Handler pattern

```rust
pub async fn handler_name(
    State(app_state): State<AppState>,
    session: Session,
    Json(payload): Json<PayloadType>,    // or Path, Query
) -> Result<(StatusCode, Json<ResponseType>), (StatusCode, String)> {
    let user = get_current_user(&session).await?;
    // validate inputs
    // get user DB from pool
    // execute query
    // return Ok((StatusCode::OK, Json(response)))
}
```

### Formatting

- No `rustfmt.toml` — uses default `cargo fmt` settings.
- No `clippy.toml` — uses default clippy lints.
- Edition 2024 — `let-else`, `let chains` (`if let ... && let ...`) are used throughout.

### Things to avoid

- Do not use `as any` or suppress warnings.
- Do not add `#[allow(...)]` without justification (one existing `#[allow(dead_code)]` on `DbPool::clear`).
- Do not use `.unwrap()` in handler code paths.
- Do not introduce new dependencies without good reason.
- Do not use `println!` for logging (the project does not use a logging framework yet; `println!` appears only in startup).

## CI Pipeline

GitHub Actions (`.github/workflows/build.yml`):
- Runs `cargo test --verbose` on every push/PR to `main`.
- Builds release binary on Linux x86_64.
- Publishes to GitHub Releases on push to `main` (tag `latest`).

## Environment

Requires `.env` file (see `.env.example`). Key vars:
- `SESSION_SECRET` — required, min 64 chars.
- `DATABASE_PATH` — defaults to `./data`.
- `TELEGRAM_BOT_TOKEN`, `OPENAI_API_KEY` — required only for the `tg` binary.
