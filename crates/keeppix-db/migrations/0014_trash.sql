-- Cestino a tre opzioni (spec §6): ogni cancellazione chiede esplicitamente
-- cosa succede al file — nessun comportamento implicito. `moved_to_trash` è
-- l'unica delle tre recuperabile (30 giorni, pulizia notturna non ancora
-- cablata: vedi TrashRepo::cleanup_expired); `kept` e `purged` non lo sono,
-- ma la riga resta per l'audit ("chi ha cancellato cosa e quando").
--
-- `asset_id` non porta una FK verso `assets(id)`: per `kept` e `purged` la
-- riga di `assets` viene cancellata nella stessa operazione che scrive
-- questa riga di trash_entries, e l'audit deve sopravvivere a quella
-- cancellazione — un `ON DELETE CASCADE` la distruggerebbe insieme
-- all'asset, un `ON DELETE SET NULL` renderebbe NULL una colonna che ha
-- senso solo se NOT NULL. L'id resta valido come riferimento storico anche
-- quando l'asset non esiste più.
CREATE TABLE trash_entries (
    id            uuid        PRIMARY KEY,
    asset_id      uuid        NOT NULL,
    deleted_by    uuid        REFERENCES users (id),
    deleted_at    timestamptz NOT NULL DEFAULT now(),

    -- Percorso assoluto al momento dell'azione. Qui non vale l'invariante
    -- "nessun percorso assoluto denormalizzato sugli asset" (spec §4.1),
    -- che riguarda l'identità in `assets` — questa è una riga di
    -- audit/ripristino, non l'identità dell'asset.
    original_path text NOT NULL,
    -- Solo per moved_to_trash: dove il file vive ora, dentro
    -- `.keeppix-trash/` nella stessa libreria. NULL per kept/purged.
    trash_path    text,

    disk_action   text NOT NULL
                       CHECK (disk_action IN ('kept', 'moved_to_trash', 'purged')),
    -- NULL finché non ripristinato. Solo moved_to_trash può averlo non-NULL.
    restored_at   timestamptz
);

CREATE INDEX trash_entries_asset_idx ON trash_entries (asset_id);

-- Il ripristino e la pulizia notturna cercano "il moved_to_trash ancora
-- pendente più vecchio di N giorni", sempre con questo stesso filtro.
CREATE INDEX trash_entries_pending_idx ON trash_entries (deleted_at)
    WHERE disk_action = 'moved_to_trash' AND restored_at IS NULL;
