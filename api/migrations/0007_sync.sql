-- Server-side change cursor for offline sync. `synced_at` is set by the database on every
-- insert/update (REST or sync), so a client pulls "everything that changed since X" even when
-- the row's client-supplied updated_at is older than X (e.g. an offline edit uploaded later).
CREATE OR REPLACE FUNCTION set_synced_at() RETURNS trigger AS $$
BEGIN
    NEW.synced_at := clock_timestamp();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

ALTER TABLE workouts    ADD COLUMN synced_at timestamptz NOT NULL DEFAULT clock_timestamp();
ALTER TABLE sleep_logs  ADD COLUMN synced_at timestamptz NOT NULL DEFAULT clock_timestamp();
ALTER TABLE weight_logs ADD COLUMN synced_at timestamptz NOT NULL DEFAULT clock_timestamp();
ALTER TABLE food_logs   ADD COLUMN synced_at timestamptz NOT NULL DEFAULT clock_timestamp();

CREATE TRIGGER workouts_synced    BEFORE INSERT OR UPDATE ON workouts    FOR EACH ROW EXECUTE FUNCTION set_synced_at();
CREATE TRIGGER sleep_logs_synced  BEFORE INSERT OR UPDATE ON sleep_logs  FOR EACH ROW EXECUTE FUNCTION set_synced_at();
CREATE TRIGGER weight_logs_synced BEFORE INSERT OR UPDATE ON weight_logs FOR EACH ROW EXECUTE FUNCTION set_synced_at();
CREATE TRIGGER food_logs_synced   BEFORE INSERT OR UPDATE ON food_logs   FOR EACH ROW EXECUTE FUNCTION set_synced_at();

CREATE INDEX workouts_user_synced_idx    ON workouts    (user_id, synced_at);
CREATE INDEX sleep_logs_user_synced_idx  ON sleep_logs  (user_id, synced_at);
CREATE INDEX weight_logs_user_synced_idx ON weight_logs (user_id, synced_at);
CREATE INDEX food_logs_user_synced_idx   ON food_logs   (user_id, synced_at);
