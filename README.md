# Kash Server

Personal expense tracking backend in Rust (edition 2024) — Axum HTTP API + optional Telegram bot.

## Stack

- **Axum 0.8** — HTTP framework
- **libsql** — Single shared SQLite DB (`data/users.db`)
- **tower-sessions** — Session-based auth (Argon2)
- **teloxide** — Telegram bot
- **OpenAI** — Text/voice/photo parsing via Responses API + Whisper

## Run

```bash
cp .env.example .env   # fill SESSION_SECRET at minimum
cargo run              # API server → http://localhost:3000
cargo run --bin tg     # Telegram bot (requires TELEGRAM_BOT_TOKEN + OPENAI_API_KEY)
```

## Configuration

| Variable | Required | Default |
|---|---|---|
| `SESSION_SECRET` | ✅ (API) | — min 64 chars |
| `DATABASE_PATH` | | `./data` |
| `TELEGRAM_BOT_TOKEN` | ✅ (bot) | — |
| `OPENAI_API_KEY` | ✅ (bot) | — |
| `OPENAI_MODEL` | | `gpt-4o-mini` |
| `OPENAI_REASONING_EFFORT` | | `low` |
| `BOT_TIMEZONE` | | `Asia/Taipei` |

## Dev

```bash
cargo check
cargo fmt
cargo clippy --tests -- -D warnings
cargo test --no-fail-fast
```

## Notes

- Fresh `data/` dir required — no migration from legacy per-user DB files.
- Telegram: send `/link <username> <password>` to link your account, then send text, voice, or receipt photos.
