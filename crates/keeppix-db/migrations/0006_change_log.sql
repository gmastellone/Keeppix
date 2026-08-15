-- Alimentato da trigger su `assets`. Va attivato da subito: accenderlo dopo
-- significherebbe che tutto ciò che è successo prima è invisibile al client
-- mobile (Fase 6). `library_id` non è nello sketch dello spec §2.6: senza, un
-- `delete` non potrebbe più applicare la visibilità perché la riga di `assets`
-- è già sparita.
CREATE TABLE change_log (
    seq        bigserial   PRIMARY KEY,
    entity     text        NOT NULL CHECK (entity IN ('asset', 'folder', 'album', 'library')),
    entity_id  uuid        NOT NULL,
    op         text        NOT NULL CHECK (op IN ('upsert', 'delete')),
    library_id uuid        NOT NULL,
    at         timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX change_log_seq_idx ON change_log (seq);
CREATE INDEX change_log_entity_idx ON change_log (entity, entity_id);

CREATE OR REPLACE FUNCTION log_asset_change() RETURNS trigger AS $$
DECLARE
    lib uuid;
BEGIN
    IF (TG_OP = 'DELETE') THEN
        -- ON DELETE CASCADE dalla cartella cancella il genitore prima che
        -- questo trigger veda `folders`. Il log dell'upsert ha già library_id.
        SELECT library_id INTO lib FROM folders WHERE id = OLD.folder_id;
        IF lib IS NULL THEN
            SELECT c.library_id INTO lib FROM change_log c
             WHERE c.entity = 'asset' AND c.entity_id = OLD.id
             ORDER BY c.seq DESC LIMIT 1;
        END IF;
        INSERT INTO change_log (entity, entity_id, op, library_id)
        VALUES ('asset', OLD.id, 'delete', lib);
        RETURN OLD;
    END IF;
    SELECT library_id INTO lib FROM folders WHERE id = NEW.folder_id;
    INSERT INTO change_log (entity, entity_id, op, library_id)
    VALUES ('asset', NEW.id, 'upsert', lib);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER assets_change_log
    AFTER INSERT OR UPDATE OR DELETE ON assets
    FOR EACH ROW EXECUTE FUNCTION log_asset_change();
