CREATE TABLE sleep_logs (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),   -- client may supply
    user_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    bedtime     timestamptz NOT NULL,
    wake_time   timestamptz NOT NULL,
    quality     smallint CHECK (quality BETWEEN 1 AND 5),
    source      text NOT NULL DEFAULT 'manual',
    external_id text,
    updated_at  timestamptz NOT NULL DEFAULT now(),
    deleted_at  timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT sleep_wake_after_bed CHECK (wake_time > bedtime),
    CONSTRAINT sleep_max_24h CHECK (wake_time - bedtime <= interval '24 hours')
);

CREATE UNIQUE INDEX sleep_logs_external_uq
    ON sleep_logs (user_id, source, external_id)
    WHERE external_id IS NOT NULL;

CREATE INDEX sleep_logs_user_wake_idx
    ON sleep_logs (user_id, wake_time DESC)
    WHERE deleted_at IS NULL;
