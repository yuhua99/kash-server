# AGENTS.md - Coding Agent Guidelines

This document provides guidelines for AI coding agents working in the my-budget-server codebase.

## Project Overview

A personal expense tracking backend built with Rust and Axum. Features session-based authentication, per-user SQLite databases (via libsql/Turso), and RESTful API endpoints for budget management.

## Build, Test, and Lint Commands

```bash
# Build
cargo build                    # Debug build
cargo build --release          # Release build

# Test
cargo test                     # Run all tests
cargo test --verbose           # Run all tests with output
cargo test <test_name>         # Run a single test by name
cargo test <module>::          # Run all tests in a module

# Lint and Format
cargo fmt                      # Format all code
cargo fmt --check              # Check formatting without changes
cargo clippy                   # Run linter
cargo clippy -- -D warnings    # Treat warnings as errors

# Run
cargo run                      # Run the server (requires .env)
```

## Project Structure

```
src/
├── main.rs        # Application entry, routing, server setup
├── lib.rs         # Library exports, AppState definition
├── auth.rs        # Authentication handlers (register, login, logout, me)
├── records.rs     # Expense record CRUD operations
├── categories.rs  # Category management
├── database.rs    # Database initialization and schema
├── db_pool.rs     # Connection pooling with LRU eviction
├── models.rs      # Data structures (User, Record, Category, payloads)
├── config.rs      # Environment configuration
├── constants.rs   # Application constants and limits
└── utils.rs       # Shared validation and helper functions
```

## Code Style Guidelines

### Imports

Group imports in this order, separated by blank lines:
1. External crates (axum, serde, tokio, etc.)
2. Standard library (std::)
3. Local crate imports (crate::, my_budget_server::)

```rust
use axum::{Json, extract::State, http::StatusCode};
use tower_sessions::Session;
use uuid::Uuid;

use crate::AppState;
use crate::constants::*;
use crate::models::{CreateRecordPayload, Record};
```

### Naming Conventions

- **Files**: `snake_case.rs`
- **Functions**: `snake_case` (e.g., `validate_record_name`, `get_current_user`)
- **Structs/Enums**: `PascalCase` (e.g., `CreateRecordPayload`, `ConfigError`)
- **Constants**: `SCREAMING_SNAKE_CASE` (e.g., `MAX_RECORD_NAME_LENGTH`)
- **Variables**: `snake_case`
- **Type aliases**: `PascalCase` (e.g., `type Db = Arc<RwLock<Connection>>`)

### Type Patterns

- Use dedicated payload structs for request/response bodies
- Use `Option<T>` for optional/partial update fields
- Derive `Serialize`/`Deserialize` from serde as needed
- Use `#[serde(skip_serializing)]` for sensitive fields like `password_hash`

### Error Handling

Return `Result<T, (StatusCode, String)>` from HTTP handlers:

```rust
pub async fn handler() -> Result<(StatusCode, Json<T>), (StatusCode, String)> {
    if invalid {
        return Err((StatusCode::BAD_REQUEST, "Error message".to_string()));
    }
    
    let result = db_operation()
        .await
        .map_err(|_| db_error_with_context("operation failed"))?;
    
    Ok((StatusCode::OK, Json(result)))
}
```

Use helper functions from `utils.rs`:
- `db_error()` - Generic database error
- `db_error_with_context(msg)` - Database error with context
- `validate_string_length(value, field_name, max)` - String validation

### Validation Pattern

Validate all inputs at the start of handlers before any database operations. Use validation helpers from `utils.rs` and `records.rs` (e.g., `validate_record_name`, `validate_record_amount`).

### Database Access

- Use `db.read().await` for SELECT queries
- Use `db.write().await` for INSERT/UPDATE/DELETE
- Always use parameterized queries (never string interpolation)
- Trim string inputs before storing: `payload.name.trim()`

```rust
let conn = user_db.write().await;
conn.execute(
    "INSERT INTO records (id, name, amount) VALUES (?, ?, ?)",
    (id.as_str(), name.trim(), amount),
).await.map_err(|_| db_error_with_context("insert failed"))?;
```

### Handler Signatures

Follow this pattern for Axum handlers:

```rust
pub async fn handler_name(
    State(app_state): State<AppState>,
    session: Session,
    Path(id): Path<String>,           // For path parameters
    Query(query): Query<QueryStruct>, // For query parameters
    Json(payload): Json<PayloadStruct>, // For request body
) -> Result<(StatusCode, Json<ResponseType>), (StatusCode, String)> {
    // Implementation
}
```

### Constants

Define limits and configuration in `constants.rs`:

## Environment Variables

Required:
- `SESSION_SECRET` - At least 64 characters (use `openssl rand -hex 64`)

Optional:
- `SERVER_HOST` - Default: `0.0.0.0`
- `SERVER_PORT` - Default: `3000`
- `DATABASE_PATH` - Default: `data`
- `FRONTEND_ORIGIN` - Default: `http://localhost:5173`
- `PRODUCTION` - Set to `true` for secure cookies
