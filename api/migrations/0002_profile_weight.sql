CREATE TABLE profiles (
    user_id        uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    display_name   text,
    calorie_target integer NOT NULL DEFAULT 2200 CHECK (calorie_target BETWEEN 800 AND 10000),
    weight_goal_kg numeric(5,2) CHECK (weight_goal_kg IS NULL OR weight_goal_kg BETWEEN 20 AND 400),
    units          text NOT NULL DEFAULT 'metric' CHECK (units IN ('metric', 'imperial')),
    updated_at     timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE weight_logs (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    measured_at timestamptz NOT NULL DEFAULT now(),
    kg          numeric(5,2) NOT NULL CHECK (kg BETWEEN 20 AND 400),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    deleted_at  timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX weight_logs_user_measured_idx
    ON weight_logs (user_id, measured_at DESC) WHERE deleted_at IS NULL;
