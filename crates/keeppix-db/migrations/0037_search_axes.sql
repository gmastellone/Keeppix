-- Nuovi assi di SearchNode (spec fase-10 §6): indici richiesti dal §6.1,
-- tutti parziali dove ha senso — i valori che contano sono una minoranza.

-- `SearchNode::Rating` (per-utente, spec §4.1): l'indice non porta
-- `user_id` perché così lo vuole la spec §6.1 letteralmente; il filtro reale
-- aggiunge sempre `user_id = $x` in AND, quindi resta selettivo comunque.
CREATE INDEX assets_rating_idx ON asset_flags (rating) WHERE rating > 0;

-- `SearchNode::Favorite` — il concetto "preferito" arriva per intero al
-- Task 10 (scrittura, AssetView, dominio); qui basta la colonna e il suo
-- indice parziale perché l'asse di ricerca del Task 6 non menta all'EXPLAIN
-- richiesto dal piano. Nome e forma dell'indice (`user_id, asset_id`)
-- riprendono esattamente quanto la spec del Task 10 già dichiara, così
-- quel task troverà entrambi pronti invece di riscontrare un conflitto.
ALTER TABLE asset_flags ADD COLUMN favorite boolean NOT NULL DEFAULT false;
CREATE INDEX asset_flags_favorite_idx ON asset_flags (user_id, asset_id) WHERE favorite;

-- `SearchNode::Day`/`Month`/`DateRange`: tutti filtrano su `taken_at_utc`
-- fuori dal cestino, come da spec §6.1.
CREATE INDEX assets_taken_day_idx ON assets (taken_at_utc) WHERE status <> 'trashed';
