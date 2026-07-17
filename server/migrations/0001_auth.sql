CREATE TABLE users (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Stored lowercased/trimmed; normalization happens at the API boundary.
    email      TEXT UNIQUE,
    -- E.164, e.g. +2348066882563. Login by phone arrives with the SMS provider phase.
    phone      TEXT UNIQUE,
    name       TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (email IS NOT NULL OR phone IS NOT NULL)
);

-- Magic-link logins. Only the SHA-256 hash of the token is stored.
CREATE TABLE login_tokens (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    identifier  TEXT NOT NULL,
    token_hash  TEXT NOT NULL UNIQUE,
    expires_at  TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX login_tokens_by_identifier ON login_tokens (identifier, created_at);

-- DB-backed sessions (revocable). Opaque token lives in an httpOnly cookie;
-- only its hash is stored here.
CREATE TABLE sessions (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash   TEXT NOT NULL UNIQUE,
    expires_at   TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX sessions_by_user ON sessions (user_id);
