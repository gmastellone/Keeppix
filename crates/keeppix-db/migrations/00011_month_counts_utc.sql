-- I bucket della scrollbar sono mesi UTC, come TimelineRepo::page.
-- date_trunc/::date su timestamptz seguono TimeZone di sessione: una foto
-- del 31 luglio 23:30Z finiva in agosto se il server non era UTC.

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
    m := date_trunc('month', ts AT TIME ZONE 'UTC')::date;
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
       AND date_trunc('month', OLD.taken_at_utc AT TIME ZONE 'UTC')
         = date_trunc('month', NEW.taken_at_utc AT TIME ZONE 'UTC') THEN
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
