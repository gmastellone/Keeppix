-- Task 7 (Fase 10): sessioni attive nel profilo (spec fase-10 §8.1).
--
-- `device_label` è un'etichetta breve ("Chrome on macOS") derivata dallo
-- `User-Agent` **al login**, non lo `User-Agent` completo: conservarlo per
-- intero sarebbe più dato personale del necessario (spec §8.1, ultima riga).
-- La colonna `user_agent` esistente (0002_sessions.sql) resta nello schema
-- per non rompere il checksum di una migrazione già applicata, ma da questo
-- task in avanti non viene più scritta: `SessionRepo::create`/`rotate`
-- scrivono solo `device_label`.
ALTER TABLE sessions ADD COLUMN device_label text;
