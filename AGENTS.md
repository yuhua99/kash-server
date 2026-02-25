# AGENTS.md — kash-server

A personal budget tracking server (Rust, Axum, SQLite via libsql).
Two binaries: HTTP REST API (`src/main.rs`) and Telegram bot (`src/bin/tg/main.rs`).
Both share one SQLite file (`data/users.db`) through the `kash_server` library crate.

---

## Build & Run

```bash
# Build everything
cargo build

# Build only the HTTP server binary
cargo build --bin kash-server

# Build only the Telegram bot binary
cargo build --bin tg

# Run the HTTP server (requires .env with SESSION_SECRET, etc.)
cargo run --bin kash-server

# Run the Telegram bot
cargo run --bin tg
```

---

## Lint

```bash
# Zero warnings policy — always run with -D warnings
cargo clippy --tests -- -D warnings
```

Fix all clippy warnings before committing. The CI gate is `clippy --tests -- -D warnings`.

---

## Tests

```bash
# Run all tests
cargo test --no-fail-fast

# Run a single test by name (substring match)
cargo test <test_name>

# Examples:
cargo test a1_single_db_init_creates_all_required_tables
cargo test split_create_happy_path_fans_out_records
cargo test records_isolation

# Run all tests in a specific test file
cargo test --test schema_test
cargo test --test split_create_test
cargo test --test records_filters_test

# Run with output visible (useful for debugging panics)
cargo test <test_name> -- --nocapture
```

Tests live in `tests/`. Each file is an integration test that spins up a full
in-memory `TestApp` with a temp-dir SQLite DB via `tests/common/mod.rs`.
There are no unit test modules inside `src/`.

---

## Environment Variables

Required for the HTTP server (`cargo run`):
```
SESSION_SECRET=<at least 64 chars>
DATABASE_PATH=data          # optional, default: "data"
SERVER_HOST=0.0.0.0         # optional, default: "0.0.0.0"
SERVER_PORT=3000            # optional, default: "3000"
FRONTEND_ORIGIN=http://localhost:8080   # optional
PRODUCTION=false            # optional; true enables secure cookies
```

Required for the Telegram bot:
```
TELEGRAM_BOT_TOKEN=<token>
OPENAI_API_KEY=<key>
OPENAI_MODEL=o4-mini         # optional
OPENAI_REASONING_EFFORT=low  # optional
BOT_TIMEZONE=UTC             # optional
DATABASE_PATH=data           # shares the same DB as the HTTP server
```

---

## Code Style

### General
- **Rust edition 2024**. Use all stable features available in it (e.g., `if let` chains).
- `rustfmt` defaults — run `cargo fmt` before committing.
- All clippy warnings must be resolved. Suppress with `#[allow(...)]` only when genuinely necessary and add a comment explaining why.

### Imports
- Group: std → external crates → internal `crate::` — separated by blank lines, each group sorted alphabetically.
- Axum extractors are imported explicitly: `use axum::{Json, extract::{Path, State}, http::StatusCode}`.
- `use crate::constants::*` is acceptable inside handler modules.
- Prefer `use crate::X` over `kash_server::X` inside `src/`; use `kash_server::X` inside `src/bin/tg/`.

### Naming
- Types and enums: `PascalCase`.
- Functions, variables, modules, fields: `snake_case`.
- Constants: `SCREAMING_SNAKE_CASE` in `src/constants.rs` — add new app-wide literals there.
- Test functions: descriptive `snake_case` mirroring the behaviour under test (e.g., `split_create_happy_path_fans_out_records`). Prefix with test-set ID where applicable (`a1_`, `b3_`).
- Test users: use suffixed names per test file to avoid cross-test collisions (`alice_a3`, `bob_a3`).

### Error Handling
- Handler return type is always `Result<(StatusCode, Json<T>), (StatusCode, String)>`.
- Never use `.unwrap()` in production code (`src/`). Use `?` or `.map_err(|e| ...)`.
- Use `db_error()` for generic DB failures; `db_error_with_context("what failed")` when context adds value. Both live in `src/utils.rs`.
- Per-operation error enums (e.g., `FinalizePendingError`, `SplitRecordError`) implement `From<TransactionError>` so `?` works inside `with_transaction` closures.
- In tests, `.expect("descriptive message")` is preferred over `.unwrap()`.

### Database Access
- `Db = Arc<RwLock<Connection>>` — always acquire `.read().await` for SELECT, `.write().await` for INSERT/UPDATE/DELETE.
- Never hold a read lock and then try to acquire a write lock in the same scope (deadlock).
- Use `with_transaction(db, |conn| Box::pin(async move { ... }))` for any multi-statement atomic write.
- Every `records` and `categories` query **must** include an `owner_user_id = ?` filter. This is the multi-tenancy invariant.
- All new tables go into `init_main_db()` in `src/database.rs` using `CREATE TABLE IF NOT EXISTS`.

### Models
- Request payloads: `#[derive(Deserialize)]`, named `*Payload` (e.g., `CreateRecordPayload`).
- Response types: `#[derive(Serialize)]`, named `*Response` (e.g., `GetRecordsResponse`).
- Shared domain types (`Record`, `Category`, `User`): `#[derive(Serialize, Deserialize, Debug, Clone)]`.
- All shared types live in `src/models.rs`. Bot-local types go in `src/bin/tg/models.rs`.

### Validation
- All validation helpers return `Result<(), (StatusCode, String)>`.
- Reuse `validate_string_length`, `validate_date`, `validate_limit`, `validate_offset` from `src/utils.rs`.
- For ownership checks, call `validate_category_exists(db, user_id, category_id)` (also in `src/utils.rs`).
- Validate before any DB write. Return `400 BAD_REQUEST` for input errors, `409 CONFLICT` for uniqueness violations, `404 NOT_FOUND` for missing resources, `401 UNAUTHORIZED` for missing session.

### Tests
- Each integration test file starts with `mod common;`.
- Use `common::setup_test_app()` for a fresh isolated DB per test (temp dir).
- Use `common::create_test_user()` + `common::login_user()` to set up fixtures.
- Send requests via `app.router.clone().oneshot(request)` from `tower::util::ServiceExt`.
- Parse responses with `serde_json::from_slice` or `serde_json::from_str`; assert on `StatusCode` constants.
- Prefix test-local helper variables that are unused with `_` (e.g., `let _alice_id = ...`) to suppress warnings, or explicitly consume them with `let _ = (alice_id, bob_id);`.
- Annotate dead-code helpers used only in some tests with `#[allow(dead_code)]`.
- Do **not** modify `tests/records_migration_test.rs` — it is out of scope.

### Constants
- All magic strings and numeric limits go in `src/constants.rs` or `src/bin/tg/constants.rs`.
- Status strings like `"pending"`, `"accepted"` must use the `FRIEND_STATUS_*` constants, not inline literals.

---

## Architecture Notes (quick reference)

- **Single DB**: one `data/users.db` for everything. Multi-tenancy via `owner_user_id`.
- **No ORM**: raw SQL via `libsql` async API.
- **Auth**: session cookie (tower-sessions `MemoryStore` + signed key). No JWTs.
- **Idempotency** (splits): reserve with NULL body → fanout in transaction → commit body. Delete reservation on fanout failure.
- **Telegram bot**: OpenAI Responses API tool-call loop (`create_record`, `edit_record`, `list_records`). Context stored in `BotState.chat_contexts` (in-memory, TTL-expiring).
- See `codemap.md` (root) and `src/codemap.md` for the full architectural map.
