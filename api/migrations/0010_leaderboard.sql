-- Leaderboard (v2, flag `leaderboard`). Strictly opt-in: no row = not opted in.
CREATE TABLE leaderboard_optin (
    user_id    uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    opted_in   boolean NOT NULL DEFAULT false,
    updated_at timestamptz NOT NULL DEFAULT now()
);
