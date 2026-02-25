# src/bin/

## Responsibility
Container directory for standalone binary crates compiled from the `kash_server` library. Currently holds one binary: `tg/` — the Telegram bot. Each binary re-uses the library's `Db`, `init_main_db`, models, and auth without duplicating logic.

## Design
- Cargo convention: each subdirectory under `src/bin/` with a `main.rs` compiles as a separate binary
- `tg/` is self-contained with its own modules (`models`, `db`, `handlers`, `helpers`, `openai`, `constants`) but links to the parent library via `use kash_server::...`

## Flow
`cargo build --bin tg` → compiles `src/bin/tg/main.rs` + its local modules + `kash_server` lib

## Integration
- Depends on: `src/` library crate (`kash_server`)
- See [src/bin/tg/codemap.md](tg/codemap.md) for the full Telegram bot map
