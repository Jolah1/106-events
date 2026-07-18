-- RSVPs live on guest_invites: an invitation and its response are one row, not
-- two tables to keep in sync. A guest invited to three parts can confirm the
-- reception and decline the engagement independently, which is why the state
-- is per (guest, sub_event) rather than per guest.

ALTER TABLE guest_invites
    ADD COLUMN rsvp_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (rsvp_status IN ('pending', 'confirmed', 'declined')),
    -- How many are attending this part (the guest plus any of their allowance
    -- they're bringing). 0 unless confirmed. The real ceiling is 1 + the
    -- guest's plus_ones, which is cross-table and so enforced in the API; this
    -- CHECK is just a floor and a sane cap (1 + the 20 plus-ones maximum).
    ADD COLUMN party_size INT NOT NULL DEFAULT 0 CHECK (party_size BETWEEN 0 AND 21),
    ADD COLUMN responded_at TIMESTAMPTZ,
    -- Which channel the response came through, for the dashboard and for audit.
    ADD COLUMN responded_via TEXT
        CHECK (responded_via IN ('link', 'whatsapp', 'sms', 'dashboard')),
    -- A confirmed part has people attending; a declined part has none; a
    -- pending part has not answered. Keeps the three columns from disagreeing.
    ADD CONSTRAINT guest_invites_response_coherent CHECK (
        (rsvp_status = 'confirmed' AND party_size >= 1)
        OR (rsvp_status IN ('pending', 'declined') AND party_size = 0)
    );

-- The public RSVP link identifies a guest with no login. The token is
-- unguessable (a random UUID, 122 bits) and per-guest, so one link lets a
-- guest respond to every part they're invited to.
ALTER TABLE guests ADD COLUMN rsvp_token UUID NOT NULL DEFAULT gen_random_uuid();
CREATE UNIQUE INDEX guests_by_rsvp_token ON guests (rsvp_token);

-- Inbound WhatsApp/SMS replies arrive as a phone number and a short message
-- ("1", "yes", "2"). We log every one, matched to a guest or not, so a reply
-- that couldn't be placed is visible to the organizer rather than lost, and so
-- a provider retrying a webhook can be recognised as a duplicate.
CREATE TABLE inbound_messages (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    channel      TEXT NOT NULL CHECK (channel IN ('whatsapp', 'sms')),
    -- The provider's own id for this message, when it gives one. Deduplicates
    -- webhook retries.
    provider_ref TEXT,
    from_phone   TEXT NOT NULL,
    body         TEXT NOT NULL,
    -- The guest we matched the sender to, if any. NULL means "from a number we
    -- don't recognise" — surfaced to the organizer, not silently dropped.
    guest_id     UUID REFERENCES guests (id) ON DELETE SET NULL,
    -- How we interpreted the message: confirmed, declined, or couldn't tell.
    parsed_as    TEXT NOT NULL CHECK (parsed_as IN ('confirmed', 'declined', 'unclear', 'unknown_sender')),
    received_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX inbound_messages_provider_ref
    ON inbound_messages (channel, provider_ref) WHERE provider_ref IS NOT NULL;
CREATE INDEX inbound_messages_by_guest ON inbound_messages (guest_id, received_at);
