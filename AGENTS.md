# AGENTS.md — kash-server

Personal budget tracker. Rust (Axum + libsql + tower-sessions) API in `src/` and a SvelteKit SPA in `web/`, served by one binary on one port. Single SQLite file at `data/users.db`. Entry: `src/main.rs`.

## Invariants

- Every `records`, `categories`, `splits`, `friends` SQL query must filter on `owner_user_id = ?` (or the equivalent ownership column). Multi-tenancy has no exceptions.
- `Db = Arc<RwLock<Connection>>`: `.read().await` for SELECT, `.write().await` for writes. Never hold a read lock while acquiring a write lock in the same scope (deadlock).
- Multi-statement writes go through `with_transaction(db, |conn| Box::pin(async move { ... }))`.
- All new tables are created in `init_main_db()` (`src/database.rs`) with `CREATE TABLE IF NOT EXISTS`. Domain modules never run DDL.
- Split idempotency: reserve with NULL body → fan out inside the transaction → commit body. Delete the reservation if fanout fails.
- No `.unwrap()` in `src/`. Use `?` or `.map_err(db_error)` / `db_error_with_context("...")` from `src/errors.rs`.
- Handlers return `Result<(StatusCode, Json<T>), (StatusCode, String)>`. Status codes: 400 input, 401 missing session, 404 missing resource, 409 uniqueness violation.
- Validation runs before any DB write. Reuse helpers in `src/validation.rs` (`validate_string_length`, `validate_date`, `validate_limit`, `validate_offset`, `validate_category_exists`).
- Raw SQL only — no ORM. Use the `libsql` async API.
- Per-operation error enums (e.g. `SettleShareError`, `SplitRecordError`) implement `From<TransactionError>` so `?` works inside `with_transaction` closures.
- Do not modify `tests/records_migration_test.rs`.

## Architecture contract

Rust (`src/`):

- `main.rs` — HTTP bootstrap: router, middleware, session layer, static-file serving for the built SPA.
- `lib.rs` — module declarations and `AppState`.
- `auth.rs`, `categories.rs`, `settings.rs`, `fx.rs`, `money.rs` — one domain each (handlers + validation + SQL for that resource only).
- `records/` — `handlers.rs` (CRUD) and `validation.rs`. Record finalize/settlement live under `splits/`, not here.
- `splits/` — `create.rs`, `listing.rs`, `idempotency.rs`, `finalize.rs`, `settlement.rs`, `allocation.rs` (share math).
- `friends/` — `lifecycle.rs`, `listing.rs`, `nicknames.rs`.
- `database.rs` — connection, `init_main_db()`, `with_transaction`. No domain logic.
- `models.rs` — shared serde DTOs (`*Payload` for requests, `*Response` for responses). No DB or network calls.
- `errors.rs`, `validation.rs`, `constants.rs`, `config.rs`, `openapi.rs` — names say what they own.
- `bin/gen_openapi.rs` — prints the OpenAPI doc; pipe to `openapi.json`.

Web (`web/`, SvelteKit SPA via `@sveltejs/adapter-static`, talks to `/api` same-origin):

- `src/routes/<feature>/+page.svelte` — page shells. `src/routes/+layout.ts` handles auth gating (`PROTECTED` vs `AUTH_ROUTES`).
- `src/lib/api/` — `client.ts` (the only `fetch` wrapper), `errors.ts`, `schema.d.ts` (generated — do not edit by hand).
- `src/lib/features/<domain>/` — domain UI + `api.ts` (+ optional `cache.ts`). Domains: `auth`, `categories`, `friends`, `inbox`, `money`, `periods`, `records`, `settings`, `shell`, `splits`, `stats`.
- `src/lib/ui/` — generic primitives only (`Button`, `Dialog`, `SelectField`, ...). No domain knowledge.
- `src/lib/*.ts` (`cache.ts`, `config.ts`, `date.ts`, `validation.ts`) — cross-feature helpers reused by multiple domains. Domain-scoped caches go under `features/<domain>/cache.ts`.

Tests (`tests/`, top-level only — no unit tests under `src/`):

- Each file starts with `mod common;` and uses `common::setup_test_app()` + `common::create_test_user()` + `common::login_user()`.
- Requests via `app.router.clone().oneshot(request)` (`tower::util::ServiceExt`). Parse with `serde_json::from_slice`.
- Suffix test users per file to avoid collisions (`alice_a3`, `bob_a3`). Prefer `.expect("...")` over `.unwrap()`.

Boundary rules:

- Handlers must not bypass `src/validation.rs` helpers or write SQL without the ownership filter.
- `src/lib/features/*/api.ts` is the only place that calls the API client; pages and UI primitives never `fetch` directly.
- No catch-all modules. Shared code lives in a domain-named file. Avoid `utils.rs`, `helpers.rs`, `misc.rs`, `shared.rs`, `common.rs` (outside `tests/common/`).
- Target ~600 LOC per source file; tests cohesive at ~600 LOC with ~1000 LOC ceiling. Split by ownership before adding more logic.

## Quality gates (run before handoff)

```bash
cargo fmt
cargo clippy --tests -- -D warnings
cargo test --no-fail-fast

cd web && npm run fmt:check && npm run lint && npm run check && npm test
```

Zero warnings policy on the Rust side. Suppress with `#[allow(...)]` only when genuinely necessary, with a comment explaining why.

When response shapes or routes change, regenerate the typed client:

```bash
cargo run --bin gen_openapi > openapi.json
cd web && npm run gen:api
```

## Commit format

`<type>: <imperative summary>` — types: `feat`, `fix`, `refactor`, `docs`, `chore`.

```
feat: add split settlement endpoint
fix: enforce owner filter on category lookup
refactor: extract pagination from records handlers
```

Avoid `update`, `cleanup`, `wip`.
