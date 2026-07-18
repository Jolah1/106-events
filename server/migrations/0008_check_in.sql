-- Free attendance, by QR code.
--
-- Nobody buys anything: the guest list is the only thing that grants entry.
-- Every head gets its own code, so a party can arrive separately and each
-- person scans for themselves.

CREATE TABLE attendees (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    guest_id   UUID NOT NULL REFERENCES guests (id) ON DELETE CASCADE,
    event_id   UUID NOT NULL,
    -- 0 is the guest themselves; 1..n are their plus-ones. Plus-ones have no
    -- names of their own, so this index is what distinguishes them and what
    -- makes their label ("Aunt Ngozi +1") reproducible.
    head_index INT NOT NULL CHECK (head_index >= 0),
    -- What the QR encodes and what staff can read aloud when a phone is dead.
    -- Short, uppercase, no ambiguous characters (see domain::code).
    code       TEXT NOT NULL,
    -- Heads created at the door beyond the guest's allowance. Kept as data
    -- rather than inferred, because the allowance can change afterwards and
    -- the record of what the door decided should not move with it.
    is_extra   BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (guest_id, head_index),
    -- Pins the attendee to the same event as its guest, so a check-in can
    -- never cross events. Mirrors the guest_invites composite key.
    FOREIGN KEY (guest_id, event_id) REFERENCES guests (id, event_id) ON DELETE CASCADE,
    UNIQUE (id, event_id)
);

-- Codes are looked up on every scan and must be globally unique: the scanner
-- knows a code, not which event it belongs to.
CREATE UNIQUE INDEX attendees_by_code ON attendees (code);
CREATE INDEX attendees_by_guest ON attendees (guest_id, head_index);

-- One row per head per part. The unique constraint is what makes a scan
-- idempotent: a double-tap, a retried offline sync, or two doors scanning the
-- same code all converge on one check-in rather than inflating the headcount.
CREATE TABLE check_ins (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    attendee_id   UUID NOT NULL,
    sub_event_id  UUID NOT NULL,
    event_id      UUID NOT NULL,
    checked_in_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Who was on the door. Nullable and ON DELETE SET NULL: a staff member
    -- leaving must not delete the record that someone attended.
    checked_in_by UUID REFERENCES users (id) ON DELETE SET NULL,
    -- True when staff admitted this head past the guest's confirmed allowance.
    -- Surfaced to the organizer rather than silently absorbed.
    over_allowance BOOLEAN NOT NULL DEFAULT FALSE,
    -- Set when the scan happened offline, so the organizer can tell a live
    -- door count from one that synced later.
    synced_offline BOOLEAN NOT NULL DEFAULT FALSE,
    UNIQUE (attendee_id, sub_event_id),
    FOREIGN KEY (attendee_id, event_id) REFERENCES attendees (id, event_id) ON DELETE CASCADE,
    FOREIGN KEY (sub_event_id, event_id) REFERENCES sub_events (id, event_id) ON DELETE CASCADE
);

CREATE INDEX check_ins_by_sub_event ON check_ins (sub_event_id, checked_in_at);
