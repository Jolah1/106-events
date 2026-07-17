# 106 Events

Event planning for the Nigerian market: create an event (with multiple parts —
engagement, ceremony, reception), share a beautiful public page, manage guests
and RSVPs over WhatsApp/SMS, sell tickets in Naira, and check guests in at the
door — even offline.

## Stack

- **Server** — Rust, Axum, PostgreSQL via sqlx (compile-time checked queries,
  `sqlx migrate` migrations). Serves the JSON API, the server-rendered public
  event pages, and the built dashboard from one origin.
- **Dashboard** — React + TypeScript + Vite, TailwindCSS v4, shadcn/ui, Motion,
  TanStack Query, React Router.
- **Auth** — passwordless email magic links; DB-backed sessions in an
  httpOnly cookie. No third-party auth.

## Repository layout

```
server/       Axum API + public pages + migrations + integration tests
  migrations/ sqlx migrations (run automatically at server startup)
  templates/  askama templates for the public, server-rendered pages
  static/     assets embedded into the binary (fonts, fallback OG image)
  .sqlx/      offline query cache — commit after `cargo sqlx prepare`
dashboard/    React SPA for organizers
```

## Public event pages

`GET /e/{slug}` serves a guest-facing page rendered by the server: one request,
inlined CSS, no JavaScript. It carries Open Graph and Twitter card tags, so
pasting the link into WhatsApp, Instagram or iMessage produces a real preview
with the event's title, date and cover image. `PUBLIC_BASE_URL` is what those
absolute URLs are built from — set it correctly in production or previews will
point at the wrong host.

Organizers set a cover image (any public `https://` URL) from the event's edit
dialog; without one, the page and its previews fall back to a branded image.
Every instant is rendered in the *event's* timezone, never the viewer's.

Fonts and the fallback OG image are `include_bytes!`d into the binary and
served under `/static/*-v1.*` with immutable caching: the deploy stays a single
artifact, and a missing asset is a compile error rather than a production 404.
Their filenames carry a version suffix — bump it (in `routes/public.rs` and the
templates) whenever the bytes change, or caches will serve the old ones.

Slugs are transliterated, not stripped: "Ọláṣubọmi & Ṣadé" becomes
`/e/olasubomi-sade`. See `domain::slug` — it folds Yoruba, Igbo and Hausa
letters (and European accents) to ASCII. Titles in scripts with no Latin base
get a random slug instead of a bad romanisation.

## Development setup

### 1. PostgreSQL

Any Postgres ≥ 14 works. Point `DATABASE_URL` at it in `server/.env`
(see `server/.env.example`).

If you can't use a system service, run an unprivileged cluster:

```sh
initdb -D ~/.local/share/106-events/pgdata -A trust -U $USER
setsid nohup /usr/lib/postgresql/18/bin/postgres \
  -D ~/.local/share/106-events/pgdata -p 5433 -k /tmp/claude-1000 \
  > ~/.local/share/106-events/pg.log 2>&1 &
createdb -h localhost -p 5433 events106_dev
```

(`setsid nohup` keeps the cluster alive when the parent shell exits.)

### 2. Server

```sh
cd server
cp .env.example .env   # then edit DATABASE_URL
cargo run              # migrations run at startup; listens on :8080
```

Without `SMTP_URL` set, magic links are logged and returned in the
`/api/auth/request-link` response as `devLink` — the login page shows an
"Open sign-in link (dev)" button, so the whole flow works with zero email
infrastructure.

sqlx macros check queries against a live database at compile time. The
committed `.sqlx/` cache lets you build without one (`SQLX_OFFLINE=true`).
After adding or changing queries run:

```sh
cargo sqlx prepare
```

### 3. Dashboard

```sh
cd dashboard
npm install
npm run dev            # :5173, proxies /api → :8080
```

For a production-style single origin, build and let the server serve it:

```sh
npm run build
DASHBOARD_DIST=$(pwd)/dist cargo run --manifest-path ../server/Cargo.toml
```

## Tests

```sh
cd server && cargo test
```

Integration tests use `#[sqlx::test]` — each test gets its own database with
migrations applied, using the connection from `.env`. They cover the full
magic-link auth flow (single-use tokens, rate limiting, session revocation),
the events API (slug collisions, ownership, validation, sub-event lifecycle
including the PATCH `endsAt` absent/null/value semantics), and the public pages
(link-preview tags, timezone rendering, HTML escaping of organizer input,
cover-image URL rules, 404s).

## Environment variables

See `server/.env.example` for the full list: `DATABASE_URL`, `BIND_ADDR`,
`APP_BASE_URL`, `PUBLIC_BASE_URL`, `SMTP_URL`, `EMAIL_FROM`, `COOKIE_SECURE`,
`DASHBOARD_DIST`.

## Status

- [x] Phase 1 — auth + event/sub-event creation (dashboard + API)
- [x] Phase 2 — public event pages with WhatsApp/Instagram-ready OG tags
- [ ] Phase 3 — guest list management (CSV import, plus-ones)
- [ ] Phase 4 — RSVP capture (link, WhatsApp replies, SMS fallback)
- [ ] Phase 5 — automated reminders
- [ ] Phase 6 — ticketing via Paystack/Flutterwave (Naira, kobo integers)
- [ ] Phase 7 — offline-tolerant QR check-in
- [ ] Phase 8 — organizer dashboard rollups
