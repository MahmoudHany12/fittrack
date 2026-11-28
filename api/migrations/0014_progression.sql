-- Progression: a `focus` muscle group (activities that train Focus) and the
-- kettlebell swing used by the starter program. Progression state itself is
-- derived from history, so no new tables. Forward-only.

ALTER TABLE exercises DROP CONSTRAINT exercises_muscle_group_check;
ALTER TABLE exercises ADD CONSTRAINT exercises_muscle_group_check
    CHECK (muscle_group IN ('chest','back','legs','shoulders','arms','core','cardio','focus'));

INSERT INTO exercises (name, muscle_group) VALUES
    ('Meditation', 'focus'),
    ('Yoga', 'focus'),
    ('Stretching', 'focus'),
    ('Mobility Flow', 'focus'),
    ('Tai Chi', 'focus'),
    ('Breathwork', 'focus'),
    ('Pilates', 'focus'),
    ('Basketball', 'focus'),
    ('Golf', 'focus'),
    ('Swim Technique Drills', 'focus'),
    ('Kettlebell Swing', 'legs')
ON CONFLICT DO NOTHING;
