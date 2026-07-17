CREATE TABLE events (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title           TEXT NOT NULL,
    -- Public URL: /e/{slug}. Immutable after creation so shared links never break.
    slug            TEXT NOT NULL UNIQUE,
    description     TEXT NOT NULL DEFAULT '',
    cover_image_url TEXT,
    timezone        TEXT NOT NULL DEFAULT 'Africa/Lagos',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX events_by_user ON events (user_id);

-- Every event has at least one sub-event. A "simple" event gets a single
-- is_default row so guests/RSVPs/tickets/check-ins always hang off sub_events
-- with no nullable-FK special cases downstream.
CREATE TABLE sub_events (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id      UUID NOT NULL REFERENCES events (id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    slug          TEXT NOT NULL,
    description   TEXT NOT NULL DEFAULT '',
    starts_at     TIMESTAMPTZ NOT NULL,
    ends_at       TIMESTAMPTZ,
    venue_name    TEXT NOT NULL DEFAULT '',
    venue_address TEXT NOT NULL DEFAULT '',
    is_default    BOOLEAN NOT NULL DEFAULT false,
    position      INT NOT NULL DEFAULT 0,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (event_id, slug),
    CHECK (ends_at IS NULL OR ends_at > starts_at)
);

CREATE INDEX sub_events_by_event ON sub_events (event_id, position);
