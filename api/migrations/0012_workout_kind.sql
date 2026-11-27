-- Workout kind (e.g. 'running') for health imports and manual workouts. Forward-only.
ALTER TABLE workouts ADD COLUMN kind text CHECK (kind IS NULL OR char_length(kind) <= 40);
