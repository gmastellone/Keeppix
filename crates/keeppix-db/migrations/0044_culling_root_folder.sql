-- Fase 7 Task 5: colonna usata dall'esclusione IA del sottoalbero Culling.
-- Nullable e inutilizzata finché Fase 9 non imposta la radice: il predicato
-- `NOT (f.path <@ cull.path)` resta inerte quando la JOIN dà NULL.
-- Fase 9 riusa questa colonna (non la ricrea).

ALTER TABLE libraries
    ADD COLUMN culling_root_folder_id uuid REFERENCES folders (id);
