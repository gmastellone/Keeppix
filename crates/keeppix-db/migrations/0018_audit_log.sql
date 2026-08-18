-- Log di audit append-only. `bigserial` per ordine d'inserimento garantito.
-- Non c'è FK su `actor_id`: i log restano anche dopo la cancellazione di un
-- utente (informazione forense più utile che un NULL silenzioso).
CREATE TABLE audit_log (
    id          bigserial PRIMARY KEY,
    actor_id    uuid,
    actor_kind  text NOT NULL CHECK (actor_kind IN ('user','share_link','system')),
    action      text NOT NULL,
    object_type text,
    object_id   uuid,
    detail      jsonb,
    ip          inet,
    at          timestamptz NOT NULL DEFAULT now()
);

-- Consultazione admin per attore (paginazione cronologica).
CREATE INDEX audit_log_actor_idx ON audit_log (actor_id, at DESC);

-- Consultazione admin per oggetto ("chi ha toccato questo asset?").
CREATE INDEX audit_log_object_idx ON audit_log (object_type, object_id, at DESC);
