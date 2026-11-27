-- Chat between friends (v2, flag `chat`). Access is gated by friendship at read time;
-- history is kept when users unfriend but is not reachable while they are not friends.
CREATE TABLE messages (
    id        uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    from_user uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    to_user   uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    body      text NOT NULL CHECK (char_length(body) BETWEEN 1 AND 2000),
    sent_at   timestamptz NOT NULL DEFAULT now(),
    read_at   timestamptz,
    CONSTRAINT messages_not_self CHECK (from_user <> to_user)
);
CREATE INDEX messages_pair_idx ON messages (from_user, to_user, sent_at);
CREATE INDEX messages_unread_idx ON messages (to_user, read_at);
