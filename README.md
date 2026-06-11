# Kash Server

Personal expense tracking backend in Rust (edition 2024) — Axum HTTP API.

## Stack

- **Axum 0.8** — HTTP framework
- **libsql** — Single shared SQLite DB (`data/users.db`)
- **tower-sessions** — Session-based auth (Argon2)

## Run

```bash
cp .env.example .env   # fill SESSION_SECRET at minimum
cargo run              # API server → http://localhost:3000
```

## Configuration

| Variable | Required | Default |
|---|---|---|
| `SESSION_SECRET` | ✅ (API) | — min 64 chars |
| `DATABASE_PATH` | | `./data` |

## Dev

```bash
cargo check
cargo fmt
cargo clippy --tests -- -D warnings
cargo test --no-fail-fast
```

## Notes

- Fresh `data/` dir required — no migration from legacy per-user DB files.
