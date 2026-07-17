-- 106 Events is one planning company, not a marketplace of unrelated
-- organizers. Events belong to the company; the people who sign in are its
-- staff and all of them can work all of it. A coordinator has to be able to
-- run the door at a wedding the founder booked.
--
-- So the user on an event stops being an owner and becomes an author.

ALTER TABLE events RENAME COLUMN user_id TO created_by;
ALTER INDEX events_by_user RENAME TO events_by_creator;

-- Attribution must never be a delete path. As an ownership column, cascading
-- was right; as a byline it would mean removing a staff member silently takes
-- every event they ever booked with them — and guests, RSVPs, tickets and
-- check-ins cascade behind those. The event outlives the employment.
ALTER TABLE events DROP CONSTRAINT events_user_id_fkey;
ALTER TABLE events ALTER COLUMN created_by DROP NOT NULL;
ALTER TABLE events ADD CONSTRAINT events_created_by_fkey
    FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE SET NULL;

-- Staff are invited, never self-served: a users row *is* the invitation, which
-- is why sign-in no longer creates one. Admins manage that list. Door-only
-- roles arrive with the check-in phase, when there is a check-in screen to
-- restrict; until then 'staff' can do everything except manage the team.
ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'staff'
    CHECK (role IN ('admin', 'staff'));

ALTER TABLE users ADD COLUMN invited_by UUID REFERENCES users (id) ON DELETE SET NULL;
