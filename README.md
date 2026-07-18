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
  httpOnly cookie. No third-party auth. Sign-in is invite-only: staff are added
  by an admin, not self-served.

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

## One workspace, invited staff

106 Events is a single event-planning company, not a marketplace of unrelated
organizers. Every signed-in staff member sees and works **every** event — a
coordinator has to be able to run the door at a wedding the founder booked.
Events carry a `created_by` for attribution, but it never scopes access, and
removing a staff member sets it to `NULL` rather than cascading their events
away (see migration `0004`): the event outlives the employment.

Because "anyone with an email" is no longer the right answer to "who works
here", **sign-in is invite-only**. `POST /api/auth/verify` only signs in an
email that already has a `users` row; `request-link` mints and rate-limits a
token for any address but only *emails* it to a member, so the endpoint can't
be used to discover who is on the team.

Admins manage the roster under `/api/team` (`role` is `admin` or `staff`).
The last admin can't be demoted or removed, and nobody can remove themselves —
otherwise the team could lock itself out. Door-only roles will arrive with the
check-in phase, when there is a check-in screen to restrict.

Somebody has to be able to sign in before anyone can be invited, so
`ADMIN_EMAILS` (comma-separated) seeds admins on startup — idempotently, and
only ever promoting, never demoting. **Set it, or nobody can log in.**

## Guest lists

Guests belong to an event and are invited to individual parts of it, so one
person can be on the engagement list, the reception list, or both. A headcount
is always `1 + plus_ones`. `guest_invites` carries a redundant `event_id` on
purpose: both its foreign keys pin the same event, which makes inviting a guest
to *another* event's sub-event a constraint violation rather than a bug.

Phone numbers are normalized to E.164 on the way in (`domain::phone`), because
the RSVP phase has to match an inbound WhatsApp sender against a row typed into
a spreadsheet months earlier. `0806 688 2563`, `+234 (0) 806 688 2563` and
`8066882563` all store as `+2348066882563`. Numbers written with a `+` are kept
as dialled, so foreign guests import cleanly. Within one event a phone number
(or email) identifies exactly one guest — that uniqueness is what CSV
re-imports dedupe on.

### CSV import

`POST /api/events/{id}/guests/import` takes the spreadsheet the organizer
already has. Columns are matched by meaning, not by template: "Full Name",
"Mobile Number" and "WhatsApp Number" all land where you'd expect, and
unrecognised columns are reported rather than silently dropped. Pass
`dryRun: true` for a preview — the dashboard always does this first, so the
organizer sees the bad rows before committing. It runs the real import inside a
transaction and rolls it back, so the counts it reports are what will actually
happen.

Two rules exist because re-importing is normal, not exceptional:

- **A column the file doesn't have never clears the field.** Uploading a plain
  name-and-phone list must not reset plus-ones and dietary needs typed into the
  dashboard. An empty plus-ones cell is "unknown", not zero.
- **Importing adds invitations and never removes them.** Organizers import one
  list per part, and the second upload must not undo the first.

Bad rows fail alone, each reported with the line number to look at in Excel.
Note that `domain::csv_import` normalizes CRLF up front: the csv crate's line
counter is off by one on CRLF files, which is exactly what every real export
is, and an error pointing at the wrong row is worse than no error at all.

## RSVPs

RSVP state lives on `guest_invites`, one answer per part, not one per guest:
Aunt Ngozi can come to the reception and skip the engagement, and the headcount
for each part has to reflect that. Each row carries `rsvp_status`
(`pending` / `confirmed` / `declined`), `party_size`, `responded_at` and
`responded_via`. A database CHECK keeps the pair coherent — a confirmation is
at least one head, and anything else is zero — so a "declined, 4 attending" row
cannot exist even if a future code path is wrong. Party size is clamped to the
guest's allowance (`1 + plus_ones`) on the way in.

Three channels write to those same rows.

**The public link.** Every guest row has a `rsvp_token` (UUID, unique index),
and `/r/{token}` is their page — no login, no app. It is server-rendered with
no JavaScript at all: checkboxes for the parts, a party-size `<select>`, one
submit, then a POST/redirect/GET to `?saved=true`. Submitting is a full
statement of intent, so a part left unticked is recorded as a decline rather
than left pending. It is served `Cache-Control: no-store`, since the page
contains the guest's name and their current answers.

**WhatsApp and SMS replies.** `POST /api/webhooks/inbound` takes a normalized
`{channel, fromPhone, body, providerRef}`. The sender's E.164 number is matched
against the guest list — the reason phone numbers are normalized at import. Two
deliberate rules:

- **A coarse channel confirms at the full allowance.** "1" or "yes" doesn't say
  how many are coming, and a guest with three plus-ones who replies "yes" most
  likely means the family. Over-counting is recoverable at the door;
  under-counting means turning people away. The public link and the dashboard
  both set an exact number.
- **If the number matches guests in more than one event, the soonest upcoming
  event wins.** Someone replying today is answering the invitation they just
  received, not last year's wedding.

`domain::rsvp::interpret` never guesses. Numeric answers are checked first
("reply 1 to confirm, 2 to decline"), then specific phrases, then whole-word
yes/no matching including Pidgin ("I go come", "no fit"). Anything it can't
read is `Unclear`: the message is logged for the organizer and the RSVP is left
untouched. "Can't wait!" is a confirmation — a naive negation rule gets that
one backwards, so it's tested.

Every inbound message is stored in `inbound_messages`, including replies from
numbers nobody recognises, which surface to the organizer instead of vanishing.
A unique index on `(channel, provider_ref)` makes provider retries idempotent,
and the endpoint always answers 2xx — a non-2xx just makes a provider retry
harder. Set `WEBHOOK_SECRET` to require an `X-Webhook-Secret` header.

The endpoint is deliberately provider-agnostic: mapping a WhatsApp Cloud API or
Termii payload onto that body, and verifying the provider's own signature, is
the only piece that needs live credentials. Everything behind it is testable
without them, which is how the RSVP state machine is covered.

## Reminders

Guests who haven't answered get chased automatically. An event carries a
*ladder* of rungs — "2 weeks before", "3 days before", "the morning of" — and
each rung, when it comes due, messages every guest with at least one part still
`pending`.

Rungs are stored as **offsets, not timestamps**. Event dates move, and a
schedule pinned to absolute datetimes is silently wrong the moment they do;
an offset re-anchors itself. The anchor is the start of the event's first part.

**A guest is never texted twice for the same rung.** That guarantee is a unique
constraint on `reminder_sends (schedule_id, guest_id)`, not a check in
application code. The worker *claims* a row with `ON CONFLICT DO NOTHING`
before sending, so two instances, an overlapping deploy, a retry, or a restart
mid-batch cannot each decide to send. The cost of claiming first is that a
crash between claim and send drops one reminder — much the better failure. A
test races two workers at one rung and asserts the phone buzzes once.

Three rules that are less obvious:

- **Nobody is texted at 3am.** Sends are held outside 08:00–21:00 in the
  *event's own* timezone. Held, not skipped: nothing is claimed, so the rung
  goes out at 08:00.
- **A late reminder still tells the truth.** The wording is composed from the
  real remaining time at send, not from the rung's label. If the process was
  down, a "2 weeks before" rung that fires two days out says "in 2 days".
- **A failed send is not retried.** A provider rejection is usually a bad
  number, and blindly retrying a timeout that actually delivered would
  double-text. The failure is recorded with its reason for the organizer.

Guests with no phone number aren't failures, they're simply not in the set —
there's nothing to attempt. Reminders stop once the event has started, and
answering through any channel removes a guest from the target set immediately.

The worker is an in-process tokio task polling once a minute (`reminders.rs`);
rungs are days apart and the ledger makes a missed or repeated tick harmless.
`run_due` takes `now` as an argument rather than reading the clock, which is
what lets the tests walk an event down its whole ladder without sleeping.

Outbound sending sits behind the same kind of port as `Mailer`: `Messenger` in
`messenger.rs`. With no provider configured it logs what would have gone out,
so the scheduler, targeting, quiet hours and idempotency all run for real
without an account. Wiring Termii, Africa's Talking or the WhatsApp Cloud API
is one more variant and one more `send` arm — nothing above it changes.

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
cargo sqlx prepare -- --all-targets
```

`--all-targets` matters: the integration tests use the query macros too, and a
plain `cargo sqlx prepare` drops their entries from the cache, which breaks
`SQLX_OFFLINE=true cargo test` for everyone else.

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
including the PATCH `endsAt` absent/null/value semantics), the public pages
(link-preview tags, timezone rendering, HTML escaping of organizer input,
cover-image URL rules, 404s), and guest lists (phone normalization, CSV import
including dry runs, re-import dedupe, per-row errors, and the cross-event
invitation constraint).

RSVPs get their own coverage of the state transitions, per the brief: confirming
one part while declining another, party-size clamping, changing your mind after
answering, unticked-means-declined, coarse WhatsApp/SMS confirms and declines,
unclear replies leaving state untouched, unknown senders being kept, provider
retries deduping, and the webhook secret being enforced. `domain::rsvp` adds
unit tests for the reply parser itself.

Reminders cover the ladder end to end: a rung firing only once due, each rung
being its own nudge, answering removing a guest from the set, a partial answer
still owing one, quiet hours holding rather than dropping a send, unreachable
guests not counting as failures, failures not being retried, reminders stopping
once the event starts, late sends re-wording themselves, and two workers racing
one rung sending exactly one message. `domain::reminder` unit-tests the quiet
hours arithmetic and the wording.

## Environment variables

See `server/.env.example` for the full list: `DATABASE_URL`, `ADMIN_EMAILS`,
`BIND_ADDR`, `APP_BASE_URL`, `PUBLIC_BASE_URL`, `SMTP_URL`, `EMAIL_FROM`,
`COOKIE_SECURE`, `WEBHOOK_SECRET`, `DASHBOARD_DIST`.

## Status

- [x] Phase 1 — auth + event/sub-event creation (dashboard + API)
- [x] Phase 2 — public event pages with WhatsApp/Instagram-ready OG tags
- [x] Phase 3 — guest list management (CSV import, plus-ones)
- [x] Phase 4 — RSVP capture (link, WhatsApp replies, SMS fallback)
- [x] Phase 5 — automated reminders to non-responders
- [ ] Phase 6 — ticketing via Paystack/Flutterwave (Naira, kobo integers)
- [ ] Phase 7 — offline-tolerant QR check-in
- [ ] Phase 8 — organizer dashboard rollups
