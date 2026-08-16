-- Stack RAW+JPEG (spec §5): stesso scatto rappresentato da più file nella
-- stessa cartella (`DSC_0042.ARW` + `DSC_0042.JPG`). Il primario è il RAW
-- quando presente, perché porta più informazione. `primary_asset_id` è
-- NOT NULL: uno stack senza primario non deve esistere, e la FK verso
-- `assets` è differita a fine transazione (vedi trigger sotto) proprio per
-- poter riassegnare il primario prima che il vincolo venga controllato.
CREATE TABLE stacks (
    id               uuid PRIMARY KEY,
    primary_asset_id uuid NOT NULL REFERENCES assets (id) DEFERRABLE INITIALLY DEFERRED,
    created_at       timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE assets
    ADD CONSTRAINT assets_stack_id_fkey
    FOREIGN KEY (stack_id) REFERENCES stacks (id) ON DELETE SET NULL;

CREATE INDEX assets_stack_id_idx ON assets (stack_id) WHERE stack_id IS NOT NULL;

-- Promuove un altro membro quando l'asset che lascia lo stack (cancellato,
-- o il cui stack_id viene cambiato da un riraggruppamento) era il
-- primario — mai un primario che punta a un asset sparito. È un trigger,
-- non un metodo di StackRepo: deve reggere qualunque via porti alla
-- rimozione (il cestino di Task 7, un DELETE fatto a mano, un futuro
-- comando batch), e un invariante di schema non si può dimenticare di
-- richiamare mentre un metodo di repository sì.
--
-- AFTER, non BEFORE: se il trigger girasse prima che la riga venga
-- effettivamente cancellata/aggiornata, un `DELETE FROM stacks` per lo
-- stack rimasto senza membri andrebbe a modificare la stessa riga che lo
-- statement esterno sta ancora processando (via l'azione ON DELETE SET
-- NULL della FK assets.stack_id -> stacks.id), e Postgres rifiuta questa
-- auto-modifica ("tuple to be updated was already modified by an
-- operation triggered by the current command"). Con AFTER la riga OLD è
-- già scomparsa (DELETE) o già aggiornata al nuovo stack_id (UPDATE)
-- quando il trigger legge lo stato di `assets`, quindi il conteggio dei
-- membri rimasti è accurato senza toccare di nuovo la riga in corso.
CREATE OR REPLACE FUNCTION promote_stack_primary() RETURNS trigger AS $$
DECLARE
    stack_being_left uuid := OLD.stack_id;
    current_primary  uuid;
    next_primary     uuid;
BEGIN
    IF TG_OP = 'UPDATE' AND NEW.stack_id IS NOT DISTINCT FROM OLD.stack_id THEN
        RETURN NULL;
    END IF;

    IF stack_being_left IS NOT NULL THEN
        SELECT primary_asset_id INTO current_primary
          FROM stacks WHERE id = stack_being_left;

        IF current_primary = OLD.id THEN
            SELECT a.id INTO next_primary
              FROM assets a
             WHERE a.stack_id = stack_being_left
             ORDER BY (a.kind <> 'raw_image'), a.filename
             LIMIT 1;

            IF next_primary IS NULL THEN
                DELETE FROM stacks WHERE id = stack_being_left;
            ELSE
                UPDATE stacks SET primary_asset_id = next_primary
                 WHERE id = stack_being_left;
            END IF;
        END IF;
    END IF;

    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER assets_promote_stack_primary
    AFTER DELETE OR UPDATE OF stack_id ON assets
    FOR EACH ROW EXECUTE FUNCTION promote_stack_primary();
