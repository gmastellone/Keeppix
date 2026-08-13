-- Impostazioni di sistema e segreti generati al primo avvio.
-- `value` è jsonb per non dover migrare lo schema a ogni nuova chiave.
CREATE TABLE system_settings (
    key        text        PRIMARY KEY,
    value      jsonb       NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);
