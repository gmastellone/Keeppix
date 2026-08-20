-- «Aggiorna album» al posto degli album dinamici (spec fase-10 §5.2): un
-- album ricorda il filtro (SearchNode) con cui è nato e lo rilancia solo
-- quando l'utente lo chiede via POST /albums/{id}/refresh, non a ogni
-- apertura della griglia (vedi Ruling nel piano Task 5).
ALTER TABLE albums
    ADD COLUMN rule        jsonb,
    ADD COLUMN rule_run_at timestamptz,
    ADD COLUMN is_shared   boolean NOT NULL DEFAULT false,
    ADD COLUMN cover_tint  text,
    ADD COLUMN monochrome  boolean NOT NULL DEFAULT false;
