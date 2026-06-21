# Kash

Personal budget tracker. Rust (Axum) API + SvelteKit web UI in one repo, served
as a single binary backed by one SQLite file.

## Layout

- `src/` — Rust API server (Axum, libsql, tower-sessions). Serves `/api` and the
  built web UI on one port.
- `web/` — SvelteKit SPA (adapter-static), talks to `/api` same-origin.
- `data/` — SQLite database (`users.db`).

## Develop

```bash
cp .env.example .env                  # set SESSION_SECRET (openssl rand -hex 64)

cargo run                             # API → http://localhost:3000
cd web && npm install && npm run dev  # UI dev server (proxies to the API)
```

## Deploy (Docker)

One image builds the SPA and the binary; the binary serves both.

```bash
cp .env.example .env                  # set SESSION_SECRET
docker compose up -d --build          # runs on 127.0.0.1:3000
docker compose logs -f                # follow logs
```

State persists in the `kash-data` volume. `PRODUCTION=true` (set in
`docker-compose.yml`) enables secure cookies, so login requires HTTPS — run it
behind a reverse proxy. Sessions are in-memory: restarting logs everyone out.

## Configure

| Variable | Required | Default |
|---|---|---|
| `SESSION_SECRET` | ✅ | — (min 64 chars) |
| `SERVER_HOST` | | `0.0.0.0` |
| `SERVER_PORT` | | `3000` |
| `DATABASE_PATH` | | `./data` |
| `STATIC_DIR` | | `web/build` |
| `PRODUCTION` | | `false` |

## Quality gates

```bash
cargo fmt && cargo clippy --tests -- -D warnings && cargo test --no-fail-fast
cd web && npm run check
```
