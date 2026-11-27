-- Friends (v2, flag `friends`). Friendship is stored once as a normalised pair.
CREATE TABLE friendships (
    user_a     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_b     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_a, user_b),
    CONSTRAINT friendships_ordered CHECK (user_a < user_b)
);
CREATE INDEX friendships_user_b_idx ON friendships (user_b);

CREATE TABLE invites (
    code       text PRIMARY KEY CHECK (char_length(code) = 10),
    owner      uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at timestamptz NOT NULL,
    used_by    uuid REFERENCES users(id) ON DELETE SET NULL,
    used_at    timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX invites_owner_idx ON invites (owner);

-- Owner consents to share their WEEKLY SUMMARY ONLY with `viewer`. Default: no row = no access.
CREATE TABLE share_grants (
    owner      uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    viewer     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    granted_at timestamptz NOT NULL DEFAULT now(),
    revoked_at timestamptz,
    PRIMARY KEY (owner, viewer),
    CONSTRAINT share_grants_not_self CHECK (owner <> viewer)
);
CREATE INDEX share_grants_viewer_idx ON share_grants (viewer);
