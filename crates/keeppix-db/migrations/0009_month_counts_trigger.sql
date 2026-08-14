-- Contatori per la scrollbar. Solo asset `indexed` con taken_at_utc.
-- INSERT/DELETE/UPDATE (data, cartella, status) tengono la tabella allineata.

CREATE OR REPLACE FUNCTION bump_folder_month_count(
    fid uuid,
    ts timestamptz,
    delta int
) RETURNS void AS $$
DECLARE
    m date;
BEGIN
    IF fid IS NULL OR ts IS NULL OR delta = 0 THEN
        RETURN;
    END IF;
    m := date_trunc('month', ts)::date;
    INSERT INTO folder_month_counts (folder_id, month, asset_count)
    VALUES (fid, m, delta)
    ON CONFLICT (folder_id, month)
    DO UPDATE SET asset_count = folder_month_counts.asset_count + EXCLUDED.asset_count;
    DELETE FROM folder_month_counts
     WHERE folder_id = fid AND month = m AND asset_count <= 0;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION sync_folder_month_counts() RETURNS trigger AS $$
DECLARE
    old_counts boolean;
    new_counts boolean;
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF OLD.status = 'indexed' AND OLD.taken_at_utc IS NOT NULL THEN
            PERFORM bump_folder_month_count(OLD.folder_id, OLD.taken_at_utc, -1);
        END IF;
        RETURN OLD;
    END IF;

    old_counts := TG_OP = 'UPDATE'
        AND OLD.status = 'indexed'
        AND OLD.taken_at_utc IS NOT NULL;
    new_counts := NEW.status = 'indexed' AND NEW.taken_at_utc IS NOT NULL;

    IF old_counts AND new_counts
       AND OLD.folder_id = NEW.folder_id
       AND date_trunc('month', OLD.taken_at_utc) = date_trunc('month', NEW.taken_at_utc) THEN
        RETURN NEW;
    END IF;

    IF old_counts THEN
        PERFORM bump_folder_month_count(OLD.folder_id, OLD.taken_at_utc, -1);
    END IF;
    IF new_counts THEN
        PERFORM bump_folder_month_count(NEW.folder_id, NEW.taken_at_utc, 1);
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER assets_month_counts
    AFTER INSERT OR UPDATE OR DELETE ON assets
    FOR EACH ROW EXECUTE FUNCTION sync_folder_month_counts();
