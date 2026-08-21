-- Fase 7 Task 4: schema AI (embeddings, tag, assegnazioni).
-- Indice vettoriale HNSW/IVFFlat = Task 11 (non qui).
--
-- Se il pacchetto pgvector non è installato sul server (Postgres esterno
-- senza `postgresql-*-pgvector`), la migrazione è un no-op: Keeppix parte
-- lo stesso e `probe_pgvector` spiega perché le funzioni AI restano spente.
-- Con l'immagine bundled (`Dockerfile.db`) l'estensione è disponibile e
-- viene abilitata qui.

DO $ai$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_available_extensions WHERE name = 'vector'
    ) THEN
        RAISE NOTICE
            'keeppix: pgvector package missing; skipping AI schema';
        RETURN;
    END IF;

    CREATE EXTENSION IF NOT EXISTS vector;

    -- Un embedding per asset. `model_version` è parte dell'identità: vettori
    -- di modelli diversi NON sono confrontabili fra loro.
    CREATE TABLE asset_embeddings (
        asset_id      uuid PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
        embedding     vector(512) NOT NULL,
        model_version text NOT NULL,
        computed_at   timestamptz NOT NULL DEFAULT now()
    );

    CREATE INDEX asset_embeddings_model_idx ON asset_embeddings (model_version);

    -- I tag li crea l'utente. `embedding` è il vettore del *testo* del tag.
    -- `parent_id`: un solo livello di nesting (categoria → tag); vincolo
    -- applicativo, non CHECK con subquery (PG non lo permette).
    CREATE TABLE tags (
        id            uuid PRIMARY KEY,
        name          text NOT NULL,
        kind          text NOT NULL CHECK (kind IN ('tag', 'category')),
        parent_id     uuid REFERENCES tags(id) ON DELETE SET NULL,
        prompt        text,
        embedding     vector(512),
        model_version text,
        color         text,
        -- Soglia per tag: governa le analisi *future*, non rivaluta decisioni.
        threshold     real NOT NULL DEFAULT 0.75,
        created_by    uuid NOT NULL REFERENCES users(id),
        created_at    timestamptz NOT NULL DEFAULT now(),
        UNIQUE (name, kind)
    );

    CREATE INDEX tags_parent_idx ON tags (parent_id) WHERE parent_id IS NOT NULL;

    -- Assegnazioni materializzate (cache del confronto vettoriale).
    CREATE TABLE asset_tags (
        asset_id   uuid NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
        tag_id     uuid NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
        state      text NOT NULL CHECK (state IN ('proposed', 'confirmed', 'rejected')),
        source     text NOT NULL CHECK (source IN ('ai', 'user')),
        score      real,
        decided_by uuid REFERENCES users(id),
        decided_at timestamptz,
        PRIMARY KEY (asset_id, tag_id)
    );

    CREATE INDEX asset_tags_tag_idx ON asset_tags (tag_id);

    -- Coda di revisione: proposte in attesa, raggruppate per tag.
    CREATE INDEX asset_tags_proposed_idx ON asset_tags (tag_id, asset_id)
        WHERE state = 'proposed';
END
$ai$;
