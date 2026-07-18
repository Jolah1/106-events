-- Automated reminders to guests who haven't answered yet.
--
-- A schedule is a rung on a ladder: "14 days before", "3 days before",
-- "the morning of". Offsets are stored rather than absolute timestamps so a
-- rescheduled event re-anchors its whole ladder automatically — event dates
-- move, and a schedule that silently points at the old date is worse than none.

CREATE TABLE reminder_schedules (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id       UUID NOT NULL REFERENCES events (id) ON DELETE CASCADE,
    -- How long before the event's first part this rung fires.
    offset_minutes INT NOT NULL CHECK (offset_minutes > 0),
    enabled        BOOLEAN NOT NULL DEFAULT TRUE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- The same rung twice would just be a double-text.
    UNIQUE (event_id, offset_minutes)
);

CREATE INDEX reminder_schedules_by_event ON reminder_schedules (event_id);

-- One row per (rung, guest) that we have attempted. This table is the
-- idempotency guarantee, not an audit log: the unique constraint is what makes
-- double-texting a guest impossible, even if the worker runs twice, two
-- instances race, or the process restarts mid-batch. The worker claims a row
-- with ON CONFLICT DO NOTHING *before* sending, so whoever wins the insert owns
-- the send.
CREATE TABLE reminder_sends (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    schedule_id UUID NOT NULL REFERENCES reminder_schedules (id) ON DELETE CASCADE,
    guest_id    UUID NOT NULL REFERENCES guests (id) ON DELETE CASCADE,
    channel     TEXT NOT NULL CHECK (channel IN ('whatsapp', 'sms')),
    status      TEXT NOT NULL CHECK (status IN ('sent', 'failed')),
    -- Provider error on failure, for the organizer to see.
    detail      TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (schedule_id, guest_id)
);

CREATE INDEX reminder_sends_by_guest ON reminder_sends (guest_id);
