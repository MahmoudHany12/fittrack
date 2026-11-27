-- Health import: per-category consent and imported step samples. Forward-only.
-- Workouts and sleep imports reuse `workouts` / `sleep_logs` (source = 'health').

CREATE TABLE health_consents (
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    category   text NOT NULL CHECK (category IN ('steps', 'workouts', 'sleep')),
    granted    boolean NOT NULL DEFAULT false,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, category)
);

CREATE TABLE health_samples (
    user_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    category    text NOT NULL CHECK (category IN ('steps')),
    external_id text NOT NULL CHECK (char_length(external_id) BETWEEN 1 AND 200),
    date        date NOT NULL,
    value       numeric NOT NULL CHECK (value >= 0),
    created_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, category, external_id)
);
CREATE INDEX health_samples_user_date_idx ON health_samples (user_id, category, date DESC);
