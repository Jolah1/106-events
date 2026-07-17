-- Guests belong to an event; invitations attach them to individual parts of it.

CREATE TABLE guests (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id   UUID NOT NULL REFERENCES events (id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    -- E.164, normalized at the API boundary (see domain::phone). The RSVP
    -- phase matches inbound WhatsApp/SMS senders against this column, so the
    -- stored form has to be canonical, not whatever the spreadsheet held.
    phone      TEXT,
    -- Stored lowercased/trimmed, like users.email.
    email      TEXT,
    -- Guests the invitee may bring. A headcount is 1 + plus_ones.
    plus_ones  INT NOT NULL DEFAULT 0,
    dietary    TEXT NOT NULL DEFAULT '',
    notes      TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (length(name) BETWEEN 1 AND 200),
    CHECK (plus_ones BETWEEN 0 AND 20),
    -- Lets guest_invites reference (id, event_id) so an invite can never
    -- cross events. See the composite foreign keys below.
    UNIQUE (id, event_id)
);

-- Within one event a phone number (or email) identifies exactly one guest.
-- These are what CSV re-imports dedupe on, so a second upload of the same
-- spreadsheet updates guests instead of doubling the list.
CREATE UNIQUE INDEX guests_by_event_phone ON guests (event_id, phone) WHERE phone IS NOT NULL;
CREATE UNIQUE INDEX guests_by_event_email ON guests (event_id, email) WHERE email IS NOT NULL;
CREATE INDEX guests_by_event ON guests (event_id, name);

ALTER TABLE sub_events ADD CONSTRAINT sub_events_id_event_id_key UNIQUE (id, event_id);

-- Which parts of the event a guest is invited to: the engagement only, the
-- reception only, or all three.
CREATE TABLE guest_invites (
    guest_id     UUID NOT NULL,
    sub_event_id UUID NOT NULL,
    -- Redundant with guests.event_id, and deliberately so: carrying it here
    -- lets both foreign keys below pin the same event, which makes inviting a
    -- guest to another event's sub-event a constraint violation rather than a
    -- bug we hope the query layer prevents.
    event_id     UUID NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guest_id, sub_event_id),
    FOREIGN KEY (guest_id, event_id) REFERENCES guests (id, event_id) ON DELETE CASCADE,
    FOREIGN KEY (sub_event_id, event_id) REFERENCES sub_events (id, event_id) ON DELETE CASCADE
);

CREATE INDEX guest_invites_by_sub_event ON guest_invites (sub_event_id);
