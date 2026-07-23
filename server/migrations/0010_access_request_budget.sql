-- What the prospect expects to spend. Free text, not kobo: budgets arrive as
-- "₦3m", "3-5 million", or "not sure yet", and forcing a number at the first
-- hello would cost real enquiries. It becomes a real figure later, in the
-- vendor sheet, once there's an event to hang it on.
ALTER TABLE access_requests ADD COLUMN budget TEXT NOT NULL DEFAULT '';
