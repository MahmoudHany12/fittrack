CREATE TABLE foods (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name        text NOT NULL,
    brand       text,
    -- nutrient values are per serving (serving_g grams)
    kcal        double precision NOT NULL CHECK (kcal >= 0),
    protein_g   double precision NOT NULL CHECK (protein_g >= 0),
    carbs_g     double precision NOT NULL CHECK (carbs_g >= 0),
    fat_g       double precision NOT NULL CHECK (fat_g >= 0),
    serving_g   double precision NOT NULL CHECK (serving_g > 0),
    source      text NOT NULL CHECK (source IN ('usda', 'off', 'custom')),
    external_id text,
    owner_id    uuid REFERENCES users(id) ON DELETE CASCADE,
    created_at  timestamptz NOT NULL DEFAULT now(),
    -- custom foods belong to a user; cached external foods are shared
    CHECK ((source = 'custom') = (owner_id IS NOT NULL))
);
CREATE UNIQUE INDEX foods_source_external_idx ON foods(source, external_id) WHERE external_id IS NOT NULL;
CREATE INDEX foods_owner_idx ON foods(owner_id) WHERE owner_id IS NOT NULL;
CREATE INDEX foods_name_lower_idx ON foods(lower(name));

CREATE TABLE food_logs (
    id         uuid PRIMARY KEY,                 -- client-generated
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    food_id    uuid NOT NULL REFERENCES foods(id),
    logged_at  timestamptz NOT NULL DEFAULT now(),
    servings   double precision NOT NULL CHECK (servings > 0),
    -- totals snapshotted at write time (food value * servings)
    kcal       double precision NOT NULL,
    protein_g  double precision NOT NULL,
    carbs_g    double precision NOT NULL,
    fat_g      double precision NOT NULL,
    meal       text CHECK (meal IN ('breakfast', 'lunch', 'dinner', 'snack')),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);
CREATE INDEX food_logs_user_day_idx ON food_logs(user_id, logged_at) WHERE deleted_at IS NULL;

CREATE TABLE favorite_foods (
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    food_id    uuid NOT NULL REFERENCES foods(id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, food_id)
);
