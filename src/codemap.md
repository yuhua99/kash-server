# src/

## Responsibility
Core library crate for the kash-server HTTP API. Implements the Service Layer (request handlers), Data Access Object layer (inline SQL via libsql), and shared infrastructure (`AppState`, transaction management, validation utilities). Compiled as both a binary (`main.rs`) and a library crate consumed by `src/bin/tg/`.

## Design

**Application State — Singleton via Axum Extension:**
- `AppState { main_db: Db }` defined in `lib.rs`; `Db = Arc<RwLock<Connection>>` from `database.rs`
- Injected into handlers via `State<AppState>` extractor; cloned cheaply (Arc)
- Single shared SQLite file (`data/users.db`) holds all tables

**Schema — Single DB, Multi-tenant by `owner_user_id`:**
All tables created by `init_main_db(data_dir)` in `database.rs` using `CREATE TABLE IF NOT EXISTS`:
- `users`, `telegram_users`, `records`, `categories`, `friendship_relations`, `idempotency_keys`
- `records` and `categories` scoped per user via `owner_user_id TEXT NOT NULL`
- Indices: `idx_records_date`, `idx_records_owner`, `idx_categories_owner`, `idx_friendship_from`, `idx_friendship_to`, `idx_idempotency_user`

**Transaction Helper — Higher-Order Function (lib.rs):**
- `with_transaction(db, async_closure)`: acquires write lock, executes `BEGIN TRANSACTION`, runs the closure, then `COMMIT` or `ROLLBACK`
- `TransactionError { Begin, Commit }` — per-handler error enums implement `From<TransactionError>`

**Session Authentication — tower-sessions:**
- `MemoryStore` + signed `SessionManagerLayer` (cookie key from `SESSION_SECRET` env var)
- `auth::get_current_user(&session)` → extracts `user_id`/`username`, used as auth guard in all protected handlers
- `auth::authenticate_user(db, username, password)` → Argon2 password verification

**Idempotency — Reserve/Commit/Delete Pattern (splits.rs):**
1. `reserve_idempotency_entry` — INSERT with `response_body = NULL` (marks in-flight)
2. `create_split_records` — atomic record fanout via `with_transaction`
3. `commit_idempotency_entry` — UPDATE with serialized `CreateSplitResponse` + status code
4. `delete_idempotency_reservation` — DELETE on fanout failure, enabling clean client retry
5. Stale NULL reservations (server crash) cleaned up on next lookup

**Validation Utilities (utils.rs):**
- `validate_string_length`, `validate_date`, `validate_limit`, `validate_offset` — uniform `Result<_, (StatusCode, String)>` error type
- `validate_category_exists(db, user_id, category_id)` — DB-backed ownership guard
- `validate_split_participants` + `calculate_split_amounts` — pure business logic; remainder assigned to initiator

## Flow

```
main.rs
  ├── Config::from_env()           → SERVER_HOST, SERVER_PORT, DATABASE_PATH, SESSION_SECRET
  ├── database::init_main_db()     → opens data/users.db, creates all tables
  ├── AppState { main_db }         → injected via .with_state()
  └── axum::serve(TcpListener, Router)

HTTP Request
  → CorsLayer → SessionManagerLayer
  → Handler(State<AppState>, Session, Json<Payload>)
      1. auth::get_current_user(&session)   → user_id or 401
      2. validate_* helpers (utils.rs)
      3. main_db.read().await / .write().await  → SQL query/execute
      4. return (StatusCode, Json<T>) | (StatusCode, String)
```

**Route table (main.rs):**
| Method | Path | Handler |
|--------|------|---------|
| POST/GET | `/records` | `records::create_record` / `get_records` |
| PUT/DELETE | `/records/{id}` | `records::update_record` / `delete_record` |
| PUT | `/records/{id}/settle` | `records::update_settle` |
| POST | `/records/finalize-pending` | `records::finalize_pending_record` |
| POST/GET | `/categories` | `categories::create_category` / `get_categories` |
| PUT/DELETE | `/categories/{id}` | `categories::update_category` / `delete_category` |
| POST | `/auth/register` | `auth::register` |
| POST/GET | `/auth/login` / `/auth/me` | `auth::login` / `auth::me` |
| POST | `/auth/logout` | `auth::logout` |
| POST/GET | `/friends/*` | `friends::*` |
| POST | `/splits/create` | `splits::create_split` |
| GET | `/splits/pending` | `splits::list_pending_splits` |
| GET | `/splits/unsettled` | `splits::list_unsettled_splits_with_friend` |

## Integration
Exported to `src/bin/tg/` as the `kash_server` library crate:
- `pub use crate::database::{Db, init_main_db}` — bot reuses same DB type and initializer
- `kash_server::auth::authenticate_user` — used by `/link` command
- `kash_server::records::{create_record_for_user, validate_record_name, validate_record_amount, extract_record_from_row}`
- `kash_server::categories::validate_category_name`
- `kash_server::models::{CreateRecordPayload, Record}`
- `kash_server::utils::{validate_date, validate_offset, validate_records_limit}`
- `kash_server::constants::DEFAULT_DATA_PATH`
