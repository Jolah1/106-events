-- People asking to be let in.
--
-- Signing in is invite-only: staff exist because an admin created them. That
-- leaves a stranger who wants to use 106 Events with nowhere to knock, so the
-- landing page takes a request and queues it here for an admin to act on.

CREATE TABLE access_requests (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL,
    email      TEXT NOT NULL,
    -- Optional: this market reaches for WhatsApp before email, so a number is
    -- often the faster way back to someone.
    phone      TEXT,
    -- What they're planning, in their own words ("wedding in November").
    about      TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Set when an admin has dealt with the request, either by inviting them or
    -- by deciding not to. Kept rather than deleted so the same person asking
    -- again is visibly a repeat, not a new name.
    handled_at TIMESTAMPTZ,
    handled_by UUID REFERENCES users (id) ON DELETE SET NULL
);

-- One row per person. Asking twice should reopen the same request, not give an
-- admin two of the same name to work through.
CREATE UNIQUE INDEX access_requests_by_email ON access_requests (lower(email));

-- The queue is read open-first, newest first.
CREATE INDEX access_requests_open ON access_requests (created_at DESC)
    WHERE handled_at IS NULL;
