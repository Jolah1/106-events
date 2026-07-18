-- Per-event vendor sheet: the caterer, the DJ, the venue, what they cost and
-- what they've been paid.
--
-- This is an internal tracker, not a marketplace and not vendor logins. The
-- money here goes *out* to suppliers and is recorded as a ledger note — no
-- payment processing, which is Phase 6's job and points the other way.

CREATE TABLE vendors (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id        UUID NOT NULL REFERENCES events (id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    -- Free text with suggested chips in the UI, deliberately not an enum: the
    -- next event will need a category nobody thought of ("aso-ebi coordinator",
    -- "small chops"), and a migration is a silly price for that.
    category        TEXT NOT NULL DEFAULT '',
    -- E.164, normalized at the API boundary like guests.phone.
    phone           TEXT,
    email           TEXT,
    -- What they're actually doing for this event.
    service         TEXT NOT NULL DEFAULT '',
    -- Kobo, never naira floats: money is integer arithmetic. BIGINT because a
    -- Lagos wedding venue in kobo comfortably exceeds a 32-bit int.
    cost_kobo       BIGINT NOT NULL DEFAULT 0 CHECK (cost_kobo >= 0),
    amount_paid_kobo BIGINT NOT NULL DEFAULT 0 CHECK (amount_paid_kobo >= 0),
    notes           TEXT NOT NULL DEFAULT '',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (length(name) BETWEEN 1 AND 200)
);

-- Paid status (unpaid / part-paid / paid) is deliberately NOT a column. It is
-- derived from the two amounts wherever it's shown, so it cannot drift out of
-- agreement with the money it describes.

CREATE INDEX vendors_by_event ON vendors (event_id, name);
