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

## Deploy (Docker + Cloudflare)

One image serves the API and the built SPA on a single port.

```bash
cp .env.example .env                 # set SESSION_SECRET: openssl rand -hex 64
docker compose up -d --build         # builds web + binary, runs on 127.0.0.1:3000
```

State lives in the `kash-data` volume (`/app/data/users.db`). The compose file
sets `PRODUCTION=true` (secure cookies), which assumes the browser reaches the
app over HTTPS — so put it behind a Cloudflare Tunnel:

```bash
cloudflared tunnel --url http://127.0.0.1:3000   # quick test
# or a named tunnel routed to your domain in the Cloudflare dashboard
```

Run `cloudflared` on the host (it reaches the localhost-bound port). For local
HTTP testing without Cloudflare, set `PRODUCTION=false`, otherwise the login
cookie won't be sent over plain HTTP.

## Notes

- Fresh `data/` dir required — no migration from legacy per-user DB files.
