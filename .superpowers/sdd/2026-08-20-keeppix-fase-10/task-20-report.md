# Task 20 report — La pausa automatica dell'analisi è un comportamento del server

Branch: `fase-10` (nessun nuovo branch, come richiesto).

## Cosa esiste ora

`keeppix-jobs::profile` guadagna, senza toccare `EnergyProfile`/`current_profile`
esistenti:

- `ActivityTracker::notify_viewport_activity()` / `notify_viewport_activity_at`
  — un secondo segnale (`last_viewport_unix_ms`, risoluzione ms) indipendente
  da `last_auth_unix` (risoluzione s, 5 minuti).
- `ActivityTracker::analysis_should_run(now, idle_threshold_ms) -> bool` —
  falso se l'ultimo cambio di vista è più recente della soglia, vero
  altrimenti (incluso "nessun cambio di vista mai visto"). La soglia è un
  parametro, non una costante cablata; `DEFAULT_ANALYSIS_IDLE_MS = 4000` è il
  punto di partenza documentato dal prototipo.
- `AnalysisLevel::{Full, Reduced}::ms_per_photo() -> u64` (42 / 260) — tipo
  puro e testato per i due livelli di velocità.

`POST /viewport` (`keeppix-api`) chiama un nuovo gancio `AppState::
on_viewport_activity` **sempre**, anche con `hashes: []` — una navigazione è
una navigazione anche senza nulla da promuovere. `main.rs` lo collega allo
stesso `ActivityTracker` già usato per `on_authenticated`.

## Cosa NON esiste ancora (per scelta, non per omissione)

Fase 7 (l'analisi IA vera) non esiste in questo branch. Non ho quindi:

- aggiunto `analysis_should_run` dentro `WorkerPool::step()` — è condiviso da
  *tutti* i job `Background` (backup, cleanup, retry, hash, regions, watch,
  xmp): cablarlo lì avrebbe imposto una pausa di 4s anche a lavoro che non è
  analisi IA, mai richiesto dal brief;
- aggiunto un campo di configurazione runtime (`KEEPPIX_ANALYSIS_IDLE_MS`) —
  nessun consumatore lo leggerebbe oggi;
- collegato `AnalysisLevel` a un throughput reale — non c'è un job da
  misurare.

Le tre scelte sono nel ledger come Ruling, con il costo se sbagliate.

## Verifica (TDD, mutazioni osservate rosse)

- `keeppix-jobs` `profile.rs`: 5 test nuovi, orologio controllato (non
  `sleep`), inclusa la ripresa esatta al millisecondo della soglia
  (3999 ms ancora in pausa, 4000 ms ripresa).
- `keeppix-api` `viewport.rs`: 1 test nuovo end-to-end via HTTP reale
  (Postgres testcontainer), osservato **rosso** disabilitando la chiamata al
  gancio (`if false && ...`) prima di ripristinarla.
- `cargo fmt --check` e `cargo clippy --workspace --all-targets -- -D
  warnings`: verdi. `cargo build --workspace --all-targets`: verde.
- Non-regressione: `keeppix-api` auth.rs (28/28), openapi.rs (7/7, nessuna
  modifica di superficie); `keeppix-server` config.rs (8/8), embed.rs (5/5).
- `./scripts/test.sh` completo non eseguito (stesso motivo dei task
  precedenti: costerebbe l'intera suite per un task che tocca due file di
  libreria); eseguiti tutti i moduli toccati più le suite di
  non-regressione sopra.

## Commit

- `82b2f66` `feat(jobs): viewport-driven idle gate for the analysis pause (Task 20)`
- `f5316fb` `feat(api): wire POST /viewport to the analysis pause gate (Task 20)`

Nessun push, nessun Task 21+. Ledger aggiornato in
`.superpowers/sdd/2026-08-20-keeppix-fase-10/progress.md`.
