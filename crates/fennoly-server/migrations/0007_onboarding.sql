-- First steps: the welcome tour each person sees once, and the admin's
-- getting-started checklist.
ALTER TABLE users ADD COLUMN tour_seen_at TIMESTAMPTZ;
ALTER TABLE users ADD COLUMN checklist_hidden_at TIMESTAMPTZ;

-- People who were here before this existed have found their way already.
UPDATE users SET tour_seen_at = now(), checklist_hidden_at = now();
