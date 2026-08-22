-- Fase 9 Task 2: cartella radice e ruoli del culling a cartelle.
--
-- `libraries.culling_root_folder_id` esiste già dalla migrazione 0044 (Fase
-- 7 Task 5, riusata qui senza ricrearla — vedi il commento in quella
-- migrazione stessa).
--
-- Il ruolo è una colonna, non il nome della cartella (spec §2.2, Ruling nel
-- ledger di fase): riconoscere `_taken`/`_skipped` dal nome significherebbe
-- che una cartella chiamata così per caso diventa magica, e rinominarla
-- romperebbe il comportamento. NULL per ogni cartella normale, incluse le
-- radici dei lotti (`Vacanze 2026-07/` stessa) — marca solo le due
-- sottocartelle che il culling crea e gestisce da sé.
ALTER TABLE folders
    ADD COLUMN culling_role text CHECK (culling_role IN ('taken', 'skipped'));

CREATE INDEX folders_culling_role_idx ON folders (culling_role)
    WHERE culling_role IS NOT NULL;
