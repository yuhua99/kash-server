# Kash Server

A personal expense tracking backend built with Rust and Axum. It exposes REST APIs for budget management and includes an optional AI-powered Telegram assistant.

## Features

- Session-based authentication with secure password hashing (Argon2)
- Per-user database isolation (libsql/Turso-backed local files)
- Record and category management APIs
- Strict record validation (including non-zero amount validation)
- AI-assisted parsing for records from text, voice, and receipt photos
- Telegram edit workflow with confirm/cancel before applying updates

## Tech Stack

- Rust (edition 2024)
- Axum + tower-sessions
- libsql
- teloxide
- OpenAI Responses API + Whisper transcription

## Project Structure

```
kash-server/
├── Cargo.toml
├── .env.example
├── data/                        # Per-user databases
└── src/
    ├── main.rs                  # HTTP server entrypoint
    ├── auth.rs                  # Register/login/logout/me handlers
    ├── records.rs               # Record CRUD + validation
    ├── categories.rs            # Category CRUD
    ├── db_pool.rs               # User DB pooling
    ├── database.rs              # Schema and DB initialization
    ├── config.rs                # Environment config loading
    ├── constants.rs             # App constants and limits
    ├── utils.rs                 # Shared helpers
    ├── models.rs                # Shared API/domain models
    └── bin/tg/
        ├── main.rs              # Telegram bot entrypoint
        ├── handlers.rs          # Text/voice/photo and edit handlers
        ├── openai.rs            # OpenAI extraction/transcription
        └── ...                  # Bot support modules
```

## Configuration

Copy `.env.example` to `.env` and fill required secrets:

```env
SERVER_HOST=0.0.0.0
SERVER_PORT=3000
DATABASE_PATH=./data
SESSION_SECRET=GENERATE_YOURS_USING_OPENSSL_RAND_HEX_64
PRODUCTION=false

TELEGRAM_BOT_TOKEN=
OPENAI_API_KEY=
OPENAI_MODEL=gpt-4o-mini
OPENAI_REASONING_EFFORT=low
BOT_TIMEZONE=Asia/Taipei
```

## Run

Run the API server:

```bash
cargo run
```

Run the Telegram bot:

```bash
cargo run --bin tg
```

By default, the server listens on `http://localhost:3000`.

## Telegram Usage

Link your Telegram account:

```text
/link <username> <password>
```

Then send any of the following:

- text like `lunch 180`
- a voice message (transcribed via Whisper)
- a photo of a receipt (image understanding)
- an edit request like `change lunch to dinner` and confirm/cancel when prompted
