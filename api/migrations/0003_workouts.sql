-- Workouts: exercise catalog, workouts and their sets. Forward-only.

CREATE TABLE exercises (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name         text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 80),
    muscle_group text NOT NULL
        CHECK (muscle_group IN ('chest','back','legs','shoulders','arms','core','cardio')),
    owner_id     uuid REFERENCES users(id) ON DELETE CASCADE,  -- NULL = shared catalog
    created_at   timestamptz NOT NULL DEFAULT now()
);
-- Catalog names are unique; each user's custom names are unique for that user.
CREATE UNIQUE INDEX exercises_catalog_name_idx ON exercises (lower(name)) WHERE owner_id IS NULL;
CREATE UNIQUE INDEX exercises_owner_name_idx ON exercises (owner_id, lower(name)) WHERE owner_id IS NOT NULL;

CREATE TABLE workouts (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    started_at  timestamptz NOT NULL DEFAULT now(),
    duration_s  integer NOT NULL CHECK (duration_s BETWEEN 1 AND 86400),
    note        text CHECK (note IS NULL OR char_length(note) <= 500),
    source      text NOT NULL DEFAULT 'manual' CHECK (source IN ('manual','health')),
    external_id text,
    updated_at  timestamptz NOT NULL DEFAULT now(),
    deleted_at  timestamptz
);
CREATE UNIQUE INDEX workouts_external_idx ON workouts (user_id, source, external_id)
    WHERE external_id IS NOT NULL;
CREATE INDEX workouts_user_started_idx ON workouts (user_id, started_at DESC);

CREATE TABLE workout_sets (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workout_id  uuid NOT NULL REFERENCES workouts(id) ON DELETE CASCADE,
    exercise_id uuid NOT NULL REFERENCES exercises(id),
    reps        integer NOT NULL CHECK (reps BETWEEN 1 AND 1000),
    weight_kg   double precision NOT NULL CHECK (weight_kg >= 0 AND weight_kg <= 1000),
    position    integer NOT NULL
);
CREATE INDEX workout_sets_workout_idx ON workout_sets (workout_id, position);

INSERT INTO exercises (name, muscle_group) VALUES
    ('Bench Press', 'chest'),
    ('Incline Bench Press', 'chest'),
    ('Decline Bench Press', 'chest'),
    ('Dumbbell Bench Press', 'chest'),
    ('Dumbbell Fly', 'chest'),
    ('Push-Up', 'chest'),
    ('Chest Dip', 'chest'),
    ('Cable Crossover', 'chest'),
    ('Machine Chest Press', 'chest'),
    ('Deadlift', 'back'),
    ('Pull-Up', 'back'),
    ('Chin-Up', 'back'),
    ('Lat Pulldown', 'back'),
    ('Barbell Row', 'back'),
    ('Dumbbell Row', 'back'),
    ('Seated Cable Row', 'back'),
    ('T-Bar Row', 'back'),
    ('Face Pull', 'back'),
    ('Back Extension', 'back'),
    ('Squat', 'legs'),
    ('Front Squat', 'legs'),
    ('Leg Press', 'legs'),
    ('Lunge', 'legs'),
    ('Bulgarian Split Squat', 'legs'),
    ('Romanian Deadlift', 'legs'),
    ('Leg Extension', 'legs'),
    ('Leg Curl', 'legs'),
    ('Hip Thrust', 'legs'),
    ('Standing Calf Raise', 'legs'),
    ('Goblet Squat', 'legs'),
    ('Step-Up', 'legs'),
    ('Overhead Press', 'shoulders'),
    ('Dumbbell Shoulder Press', 'shoulders'),
    ('Lateral Raise', 'shoulders'),
    ('Front Raise', 'shoulders'),
    ('Rear Delt Fly', 'shoulders'),
    ('Arnold Press', 'shoulders'),
    ('Upright Row', 'shoulders'),
    ('Barbell Shrug', 'shoulders'),
    ('Barbell Curl', 'arms'),
    ('Dumbbell Curl', 'arms'),
    ('Hammer Curl', 'arms'),
    ('Preacher Curl', 'arms'),
    ('Triceps Pushdown', 'arms'),
    ('Skull Crusher', 'arms'),
    ('Overhead Triceps Extension', 'arms'),
    ('Close-Grip Bench Press', 'arms'),
    ('Triceps Dip', 'arms'),
    ('Plank', 'core'),
    ('Crunch', 'core'),
    ('Sit-Up', 'core'),
    ('Hanging Leg Raise', 'core'),
    ('Russian Twist', 'core'),
    ('Ab Wheel Rollout', 'core'),
    ('Cable Crunch', 'core'),
    ('Side Plank', 'core'),
    ('Running', 'cardio'),
    ('Walking', 'cardio'),
    ('Cycling', 'cardio'),
    ('Swimming', 'cardio'),
    ('Rowing Machine', 'cardio'),
    ('Elliptical', 'cardio'),
    ('Jump Rope', 'cardio'),
    ('Stair Climber', 'cardio'),
    ('Hiking', 'cardio'),
    ('Burpees', 'cardio');
