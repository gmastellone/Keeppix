-- Fase 8 Task 3: schema volti (rilevamento, persone, gruppi, separazioni).
--
-- Stesso contratto no-op di 0043/0045 se pgvector non è installato: Keeppix
-- parte comunque, il riconoscimento resta spento.
--
-- Ordine di creazione: `persons.cover_face_id` e `faces.person_id` sono un
-- riferimento incrociato, quindi `persons` nasce senza il vincolo di FK su
-- `cover_face_id` (colonna nuda) e lo riceve con un ALTER dopo che `faces`
-- esiste — stesso trucco usato per ogni coppia di tabelle mutuamente
-- referenziate in Postgres.
--
-- Indice vettoriale: IVFFlat, non HNSW (a differenza di quanto scritto nella
-- prima stesura della spec fase-8-volti.md §3) — stessa ragione di
-- 0045_asset_embeddings_ivfflat.sql, build e RAM più leggeri sul Pi 8 GB
-- bersaglio. Vedi Ruling nel ledger di fase.
--
-- `libraries.faces_enabled` **non** è dentro il blocco gated da pgvector qui
-- sotto, a differenza di tutto il resto: `LibraryRepo` (core, non IA) legge
-- e scrive questa colonna incondizionatamente in ogni INSERT/SELECT su
-- `libraries` (stesso `COLUMNS` usato da ogni libreria, non solo quelle con
-- riconoscimento facciale). Lasciarla nel blocco `DO $faces$` — che va in
-- no-op quando pgvector manca — romperebbe `LibraryRepo::create` ovunque,
-- non solo quando serve il resto dello schema volti: esattamente il difetto
-- che la CI reale ha trovato (server Postgres di test intenzionalmente
-- senza `vector` per i crate che non ne hanno bisogno — `.github/workflows/ci.yml`
-- — con `refresh_returns_added_ids_as_succeeded_bulk_outcome` di
-- `keeppix-api/tests/albums.rs` che falliva su "column faces_enabled does
-- not exist" creando una libreria). Interruttore per libreria (Task 10):
-- spento non rileva nulla, non "rileva ma non mostra". Stesso pattern di
-- `scan_enabled` (0004_libraries_folders.sql), non una voce di preferenze
-- utente — e per lo stesso motivo va aggiunta come `scan_enabled`, sempre,
-- non condizionata a IA disponibile.
ALTER TABLE libraries
    ADD COLUMN IF NOT EXISTS faces_enabled boolean NOT NULL DEFAULT true;

DO $faces$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_available_extensions WHERE name = 'vector'
    ) THEN
        RAISE NOTICE
            'keeppix: pgvector package missing; skipping faces schema';
        RETURN;
    END IF;

    CREATE EXTENSION IF NOT EXISTS vector;

    -- Una persona. Il nome è opzionale: «Persona 4» con 37 foto è già utile.
    CREATE TABLE IF NOT EXISTS persons (
        id            uuid PRIMARY KEY,
        name          text,
        cover_face_id uuid,
        -- Centroide degli embedding dei volti confermati: evita di
        -- ricalcolarlo a ogni confronto. Si aggiorna quando il gruppo cambia.
        centroid      vector(512),
        hidden_at     timestamptz,
        created_at    timestamptz NOT NULL DEFAULT now(),
        UNIQUE (name)
    );

    -- Un volto rilevato. Vive anche senza persona: prima si rileva, poi si
    -- raggruppa.
    CREATE TABLE IF NOT EXISTS faces (
        id           uuid PRIMARY KEY,
        asset_id     uuid NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
        -- Riquadro in coordinate relative (0..1): sopravvive a ritagli e
        -- derivati di dimensione diversa, che coi pixel assoluti sarebbe da
        -- ricalcolare.
        bbox_x       real NOT NULL,
        bbox_y       real NOT NULL,
        bbox_w       real NOT NULL,
        bbox_h       real NOT NULL,
        landmarks    jsonb,
        embedding    vector(512),
        detect_score real NOT NULL,
        -- Qualità (nitidezza, dimensione, posa): un volto sfocato di 20px
        -- non deve decidere l'identità di un cluster.
        quality      real,
        person_id    uuid REFERENCES persons(id) ON DELETE SET NULL,
        -- Decisione umana su QUESTO volto. NULL = l'automatismo può ancora
        -- agire.
        assigned_by  uuid REFERENCES users(id),
        assigned_at  timestamptz,
        -- Falso positivo dichiarato: un disegno, una texture, un volto in un
        -- poster. Permanente: non viene mai riproposto.
        rejected_at  timestamptz,
        -- Candidato del raggruppamento incrementale quando la distanza dal
        -- centroide più vicino è "dubbia" (spec §4.1): non assegnato
        -- (`person_id` resta NULL), ma proposto in coda di revisione (Task
        -- 8). Non è nella prima stesura della spec (§3 non la elenca): senza
        -- un candidato da mostrare, "Questi volti sembrano Giovanni" (spec
        -- §5) non avrebbe un nome da proporre. Ruling nel ledger di fase.
        proposed_person_id uuid REFERENCES persons(id) ON DELETE SET NULL,
        proposed_score      real,
        model_version text NOT NULL,
        created_at   timestamptz NOT NULL DEFAULT now()
    );

    ALTER TABLE persons
        ADD CONSTRAINT persons_cover_face_id_fkey
        FOREIGN KEY (cover_face_id) REFERENCES faces(id) ON DELETE SET NULL;

    CREATE INDEX IF NOT EXISTS faces_asset_idx ON faces (asset_id);
    CREATE INDEX IF NOT EXISTS faces_person_idx ON faces (person_id)
        WHERE rejected_at IS NULL;
    CREATE INDEX IF NOT EXISTS faces_proposed_idx ON faces (proposed_person_id)
        WHERE proposed_person_id IS NOT NULL AND person_id IS NULL;
    -- lists ≈ N/1000 per ~200k impronte (stessa regola pgvector di
    -- asset_embeddings). I NULL (volto senza impronta calcolabile) restano
    -- fuori dall'indice, non generano errore.
    CREATE INDEX IF NOT EXISTS faces_embedding_ivfflat_idx
        ON faces USING ivfflat (embedding vector_cosine_ops) WITH (lists = 200);

    -- Gruppi di PERSONE FOTOGRAFATE. Da non confondere con i `groups` della
    -- Fase 3, che sono gruppi di *utenti* per i permessi: nomi simili,
    -- concetti distinti, tabelle separate di proposito.
    CREATE TABLE IF NOT EXISTS person_groups (
        id         uuid PRIMARY KEY,
        name       text NOT NULL UNIQUE,
        created_by uuid NOT NULL REFERENCES users(id),
        created_at timestamptz NOT NULL DEFAULT now()
    );

    CREATE TABLE IF NOT EXISTS person_group_members (
        group_id  uuid NOT NULL REFERENCES person_groups(id) ON DELETE CASCADE,
        person_id uuid NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
        PRIMARY KEY (group_id, person_id)
    );

    -- La chiave primaria copre "chi è nel gruppo X" (group_id in testa), non
    -- "in quali gruppi sta la persona Y" — query altrettanto naturale (il
    -- dettaglio di una persona che mostra i suoi gruppi) e senza questo
    -- indice farebbe una scansione della tabella.
    CREATE INDEX IF NOT EXISTS person_group_members_person_idx
        ON person_group_members (person_id);

    -- La tabella che fa la differenza (spec §4.3): due persone che l'utente
    -- ha separato non devono mai essere riunite dall'automatismo.
    CREATE TABLE IF NOT EXISTS person_separations (
        person_a   uuid NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
        person_b   uuid NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
        created_by uuid NOT NULL REFERENCES users(id),
        created_at timestamptz NOT NULL DEFAULT now(),
        -- Coppia non ordinata: si memorizza sempre con a < b.
        PRIMARY KEY (person_a, person_b),
        CHECK (person_a < person_b)
    );
END
$faces$;
