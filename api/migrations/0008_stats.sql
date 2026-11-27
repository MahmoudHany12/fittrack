-- Daily snapshot of game-style character stats (one row per user per day).
CREATE TABLE character_stats (
    user_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    day         date NOT NULL,
    strength    smallint NOT NULL CHECK (strength BETWEEN 0 AND 100),
    cardio      smallint NOT NULL CHECK (cardio BETWEEN 0 AND 100),
    focus       smallint NOT NULL CHECK (focus BETWEEN 0 AND 100),
    computed_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, day)
);
