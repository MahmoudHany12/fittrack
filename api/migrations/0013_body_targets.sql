-- Extended profile (all optional) and body composition (InBody or manual).
ALTER TABLE profiles
    ADD COLUMN height_cm double precision CHECK (height_cm IS NULL OR height_cm BETWEEN 100 AND 250),
    ADD COLUMN birth_date date,
    ADD COLUMN sex text CHECK (sex IS NULL OR sex IN ('male', 'female')),
    ADD COLUMN activity_level text CHECK (activity_level IS NULL OR activity_level IN
        ('sedentary', 'light', 'moderate', 'active', 'very_active')),
    ADD COLUMN goal text CHECK (goal IS NULL OR goal IN ('lose', 'maintain', 'gain')),
    ADD COLUMN goal_rate_kg_per_week double precision
        CHECK (goal_rate_kg_per_week IS NULL OR goal_rate_kg_per_week BETWEEN 0 AND 1),
    ADD COLUMN calorie_target_mode text NOT NULL DEFAULT 'auto'
        CHECK (calorie_target_mode IN ('auto', 'manual'));

CREATE TABLE body_compositions (
    id                 uuid PRIMARY KEY DEFAULT gen_random_uuid(),   -- client-generated
    user_id            uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    measured_at        timestamptz NOT NULL DEFAULT now(),
    weight_kg          double precision NOT NULL CHECK (weight_kg BETWEEN 20 AND 400),
    body_fat_pct       double precision CHECK (body_fat_pct IS NULL OR body_fat_pct BETWEEN 2 AND 75),
    skeletal_muscle_kg double precision CHECK (skeletal_muscle_kg IS NULL OR skeletal_muscle_kg BETWEEN 5 AND 100),
    fat_mass_kg        double precision CHECK (fat_mass_kg IS NULL OR fat_mass_kg BETWEEN 0 AND 400),
    visceral_fat_level smallint CHECK (visceral_fat_level IS NULL OR visceral_fat_level BETWEEN 1 AND 30),
    bmr_kcal           integer CHECK (bmr_kcal IS NULL OR bmr_kcal BETWEEN 600 AND 5000),
    body_water_pct     double precision CHECK (body_water_pct IS NULL OR body_water_pct BETWEEN 20 AND 80),
    source             text NOT NULL DEFAULT 'manual' CHECK (source IN ('inbody', 'manual')),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    deleted_at         timestamptz,
    created_at         timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX body_compositions_user_measured_idx
    ON body_compositions (user_id, measured_at DESC) WHERE deleted_at IS NULL;
