-- Fase 10 Task 15: il Profilo (§61) mostra "Ultima modifica: 3 mesi fa" per
-- la password. `default now()` copre gli inserimenti futuri (una password
-- appena impostata conta come appena cambiata); il backfill riporta gli
-- account esistenti alla loro creazione, l'unico istante in cui sappiamo con
-- certezza che la password corrente è stata scritta.
ALTER TABLE users
    ADD COLUMN password_changed_at timestamptz NOT NULL DEFAULT now();

UPDATE users SET password_changed_at = created_at;
