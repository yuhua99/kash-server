# AGENTS.md — kash-server

A personal budget tracking server (Rust, Axum, SQLite via libsql).
Single HTTP REST API binary (`src/main.rs`) using one SQLite file (`data/users.db`) through the `kash_server` library crate.

## Architecture contract

Keep the repository organized by ownership. Do not create catch-all modules.

Source layout:

- `src/main.rs` owns HTTP server bootstrap: router assembly, middleware, session layer.
- `src/lib.rs` owns module declarations and the intentional library API surface.
- `src/auth.rs`, `src/categories.rs`, `src/settings.rs`, `src/fx.rs` own their domain: handlers, validation, and SQL for that resource only.
- `src/records/` owns record handlers: `validation.rs`, `handlers.rs` (CRUD), `finalize.rs`, `settlement.rs`.
- `src/splits/` owns split handlers: `create.rs`, `listing.rs`, `idempotency.rs`, `allocation.rs` (share math).
- `src/friends/` owns friendship handlers: `lifecycle.rs`, `listing.rs`, `nicknames.rs`.
- `src/database.rs` owns connection setup, schema init (`init_main_db()`), and `with_transaction`.
- `src/models.rs` owns shared serde DTOs (`Record`, `Category`, `User`, payloads, responses).
- `src/config.rs` owns env-var loading and config shape.
- `src/constants.rs` owns app-wide literals and numeric limits.
- `src/errors.rs` owns shared HTTP error constructors (`db_error`, `db_error_with_context`).
- `src/validation.rs` owns shared request-input validation helpers.
- `tests/` owns behavior: integration tests against a full in-memory `TestApp`.

Boundary rules:

- Handlers must not bypass validation or write SQL without the `owner_user_id` filter.
- `src/database.rs` must not contain domain logic; domain modules must not run schema DDL.
- Models must not perform DB or network calls.
- All new tables go into `init_main_db()` using `CREATE TABLE IF NOT EXISTS`.

## No junk drawers

Do not create generic dumping grounds. Shared code goes in a domain-named module (e.g., `validation.rs`, `errors.rs`, `pagination.rs`, `date_parsing.rs`). Names encode ownership, not implementation history.

Avoid `utils.rs`, `helpers.rs`, `misc.rs`, `shared.rs`, `common.rs` (outside `tests/common/`), `new_*.rs`, `*_v2.rs`.

## File size and split rules

- Source files should target under ~600 LOC. If a file approaches the limit, split by ownership before adding more logic.
- Test files stay cohesive; ~600 LOC target, ~1000 LOC hard ceiling. Do not split tests into artificial buckets.

## Type system and naming rules

- Rust edition 2024 — use its stable features (e.g., `if let` chains).
- Types and enums: `PascalCase`. Functions, variables, modules, fields: `snake_case`. Constants: `SCREAMING_SNAKE_CASE` in `src/constants.rs`.
- Request payloads: `#[derive(Deserialize)]`, named `*Payload`. Responses: `#[derive(Serialize)]`, named `*Response`. Shared domain types: `#[derive(Serialize, Deserialize, Debug, Clone)]`.
- Prefer enums over constrained strings; avoid silent `_ => default` for user input.
- Keep `Option<T>` for real optionality, not unclear state.

## Database rules

- `Db = Arc<RwLock<Connection>>` — `.read().await` for SELECT, `.write().await` for writes.
- Never hold a read lock while acquiring a write lock in the same scope (deadlock).
- Use `with_transaction(db, |conn| Box::pin(async move { ... }))` for multi-statement atomic writes.
- Every `records` and `categories` query **must** filter on `owner_user_id = ?`. This is the multi-tenancy invariant — no exceptions.
- Idempotency (splits): reserve with NULL body → fanout in transaction → commit body. Delete reservation on fanout failure.
- No ORM: raw SQL via the `libsql` async API.

## Error handling rules

- Handler return type is always `Result<(StatusCode, Json<T>), (StatusCode, String)>`.
- Never `.unwrap()` in `src/`. Use `?` or `.map_err(|e| ...)`.
- Use `db_error()` for generic DB failures; `db_error_with_context("what failed")` when context adds value (both in `src/errors.rs`).
- Per-operation error enums (e.g., `FinalizePendingError`) implement `From<TransactionError>` so `?` works inside `with_transaction` closures.
- Status codes: `400` input errors, `401` missing session, `404` missing resource, `409` uniqueness violation.
- Validation helpers return `Result<(), (StatusCode, String)>` and run before any DB write. Reuse `validate_string_length`, `validate_date`, `validate_limit`, `validate_offset`, `validate_category_exists` from `src/validation.rs`.

## Testing contract

Before refactors, keep tests for current behavior. Tests live in `tests/`; there are no unit test modules in `src/`.

- Each test file starts with `mod common;` and uses `common::setup_test_app()` for a fresh temp-dir DB.
- Fixtures via `common::create_test_user()` + `common::login_user()`.
- Requests via `app.router.clone().oneshot(request)` (`tower::util::ServiceExt`).
- Parse with `serde_json::from_slice`; assert on `StatusCode` constants.
- Test names: descriptive `snake_case` mirroring behavior, prefixed with test-set ID where applicable (`a1_`, `b3_`).
- Test users: suffix per file to avoid collisions (`alice_a3`, `bob_a3`).
- In tests prefer `.expect("descriptive message")`; prefix unused fixtures with `_`; annotate partially-used helpers with `#[allow(dead_code)]`.
- Do **not** modify `tests/records_migration_test.rs` — out of scope.

```bash
cargo test --no-fail-fast            # all tests
cargo test <name>                    # substring match
cargo test --test split_create_test  # one file
cargo test <name> -- --nocapture     # debug panics
```

## Toolchain and quality gates

Quality gates must pass before commit:

```bash
cargo fmt
cargo clippy --tests -- -D warnings
cargo test --no-fail-fast
```

Zero warnings policy. Suppress with `#[allow(...)]` only when genuinely necessary, with a comment explaining why.

## Commit message rules

Use `<type>: <imperative summary>` with types `feat`, `fix`, `refactor`, `docs`, `chore`.

```text
feat: add split settlement endpoint
fix: enforce owner filter on category lookup
refactor: extract pagination from records handlers
```

Keep commits focused. Avoid `update`, `cleanup`, `wip`.
