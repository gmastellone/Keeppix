-- Permessi solo-allow su cartella, album o asset. `inherit = false` ferma
-- l'ereditarietà su quel nodo: il sottoalbero sotto di esso non riceve
-- questo permesso. Nessun deny esplicito.
CREATE TABLE permissions (
    id           uuid PRIMARY KEY,
    subject_type text NOT NULL CHECK (subject_type IN ('user','group')),
    subject_id   uuid NOT NULL,
    object_type  text NOT NULL CHECK (object_type IN ('folder','album','asset')),
    object_id    uuid NOT NULL,
    role         text NOT NULL CHECK (role IN ('viewer','editor')),
    inherit      boolean NOT NULL DEFAULT true,
    granted_by   uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now()
);

-- Un soggetto non può avere due permessi sullo stesso oggetto: si aggiorna
-- il ruolo, non se ne accumulano due.
CREATE UNIQUE INDEX permissions_unique_grant
    ON permissions (subject_type, subject_id, object_type, object_id);

-- Il verso caldo: "cosa posso vedere io" — risolto a ogni richiesta.
CREATE INDEX permissions_subject_idx ON permissions (subject_type, subject_id);

-- Il verso freddo: "chi vede questo" — pannello di condivisione.
CREATE INDEX permissions_object_idx ON permissions (object_type, object_id);
