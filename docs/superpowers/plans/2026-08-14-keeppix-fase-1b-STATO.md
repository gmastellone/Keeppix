# Fase 1b — stato di avanzamento e consegna

**Aggiornato:** 2026-08-14, chiusura della Fase 1b sul branch `fase-1`
**Piano:** [`2026-08-14-keeppix-fase-1b.md`](2026-08-14-keeppix-fase-1b.md)
**Spec:** [`../specs/fase-1b-ingestione.md`](../specs/fase-1b-ingestione.md)
**Design:** [`../specs/2026-08-13-keeppix-design.md`](../specs/2026-08-13-keeppix-design.md)
**Roadmap:** [`2026-08-13-keeppix-roadmap.md`](2026-08-13-keeppix-roadmap.md)
**Stato:** **chiusa sul branch `fase-1`**. Non mergiata su `main`. La Fase 1c
è il prossimo lavoro; 1a+1b+1c si allineano a `main` con una sola PR.

Questo documento è la **consegna della Fase 1b**: qui c'è ciò che serve a
riprendere il lavoro senza rileggere il ledger. Il ledger cronologico vive in
`.superpowers/sdd/2026-08-14-keeppix-fase-1b/progress.md`.

## Metodo di esecuzione

TDD in-line sul branch `fase-1` (stesso precedente della 1a: non un worktree
separato). Spec vince sul piano; le divergenze sono nei ruling sotto.

## Avanzamento

**Fase 1b completa.** I 12 task del piano sono chiusi.

| # | Task | Stato | Commit |
|---|---|---|---|
| 1 | Harness container Postgres condiviso | ✅ | `de75da7` |
| 2 | Migrazione `jobs` + tipi | ✅ | `8662b33` |
| 3 | `JobRepo` SKIP LOCKED | ✅ | `c939aef` |
| 4 | Magic-number kind | ✅ | `db25913` |
| 5 | Worker pool + profili energetici | ✅ | `9257bba` |
| 6 | Discovery walker | ✅ | `e4e69b6` |
| 7 | EXIF 128 KB | ✅ | `a84cd10` |
| 8 | Hash blake3 | ✅ | `1cba2ed` |
| 9 | Thumb + thumbhash | ✅ | `34d8dd3` |
| 10 | Sandbox ffmpeg + poster | ✅ | `d50d158` |
| 11 | Watcher, move, probe | ✅ | `6ec3037` |
| 12 | Fixture + STATO | ✅ | `bd915e0` |

## Numeri del fixture — non del TB

Misurati da `ingest_fixture_indexes_three_jpegs` su questa macchina
(macOS, Apple Silicon, Postgres in Docker). Tre JPEG: due 64×64 (~248 B) e
uno «WhatsApp-size» 1600×1200 generato con ffmpeg (~11 KiB). Una sottocartella,
un `.DS_Store` ignorato.

| Cosa | Valore | Nota |
|---|---|---|
| Wall totale (discover→meta→hash→derive, 3 file) | **818 ms** | include claim/coda, non solo decode |
| Wall / file | **~272 ms** | coda + DB, non throughput TB |
| `read_exif` su 64×64 | **< 1 ms** | `as_millis` tronca a 0 |
| `derive_jpeg` su 64×64 (thumb, preview saltata) | **5 ms** | lato ≤1600 e file ≤400 KB → no preview |
| Asset `indexed` | 3 | dopo i metadati, non dopo i derivati |
| `thumbhash` valorizzato | 3 | |
| Originali | mtime invariato | |
| `.DS_Store` | assente dagli asset | |

Questi numeri **non ricalibrano** le stime TB della spec §10. Servono a
chi riprende: la pipeline gira; il throughput sul TB si misura in 1c/2
contro una cartella reale.

`keeppix-db` dopo il Task 1: wall `cargo test -p keeppix-db -- --test-threads=1`
~**176 s** (era ~6–7 min in 1a). Isolamento: `two_test_databases_are_isolated`
verde.

## Ruling

1. Si resta sul branch `fase-1`, in-place. Stesso precedente R1 / 1a.
2. `system_capabilities` «già in Fase 0» non esiste. Probe →
   `system_settings` chiave `capabilities` (jsonb).
3. `AssetStatus::Indexed` dopo i **metadati**, non dopo i derivati. Vince
   spec 1b §1: la timeline 1c deve riempirsi in minuti.
4. `TestServer::start_stoppable()` per il test 503: il container condiviso
   non può essere spento.
5. `promote` usa `LEAST(priority, $n)`: un job interactive (0) non si
   declassa.
6. TIFF `DateTime` (0x0132) è fallback dopo DateTimeOriginal /
   DateTimeDigitized. kamadak-exif `display_value` è `YYYY-MM-DD`.
7. Encode WebP via **`image-webp` lossless**. Il crate non espone q78.
   Recordato: le thumb non sono q78.
8. Eccezioni `AuthContext` dello scanner, ciascuna col motivo nel doc
   comment: `LibraryRepo::{load_for_scan,set_status_for_scan,list_for_scan}`,
   `AssetRepo::{get_for_scan,count_in_library,insert_exif,set_thumbhash_for_hash,ids_with_hash,copy_exif}`,
   `FolderRepo::absolute_path_for_scan`. `JobRepo` non prende `AuthContext`
   (è il worker).
9. Sparizione di massa: `seen * 5 < existing * 4` → `JobError::MassDisappearance`,
   libreria resta Active. Root assente/vuoto con asset esistenti → libreria
   **Offline**, zero `mark_offline` di massa.
10. Discover scrive `AssetKind::Unknown` (nessun open). Il kind arriva dai
    magic bytes quando si legge l'header.
11. Terza copia dell'harness `TestDb`: `crates/keeppix-jobs/tests/harness/mod.rs`.
12. Sandbox: `rlimit` su Unix, **niente seccomp in 1b** (niente dipendenza C
    extra). Upgrade: `libseccomp` nel figlio. Interfaccia
    `sandbox::run(program, args, memory_bytes, cpu_secs)`.
13. Spostamento: stesso `(content_hash, size)` **e** il file vecchio non è
    più sul disco. Due copie presenti restano due asset `indexed` (dedup
    è presentazione, 1c). File: `moves.rs`, non `r#move.rs`.
14. NFS/SMB: su Linux si legge `/proc/mounts`. Su macOS il probe FS è
    Native (FSEvents); niente `statfs` in `keeppix-jobs`.
15. Watcher: debounce 2 s in produzione, iniettabile nei test. Librerie
    create dopo il boot restano scoperte fino al riavvio (ponytail: 1c).
16. `RamGate` è per istanza di `WorkerPool`, non condiviso fra gli N
    worker. ActivityTracker e flag paused sì. Upgrade se l'RSS sale.

## Cosa non è in 1b (di proposito)

- HTTP/WebSocket/TimelineRepo/frontend della 1c.
- Transcodifica all'ingest. Poster video al 10% della durata, basta.
- Rating/album sul move (non esistono ancora).
- Pagina Duplicati.
- Seccomp Linux.
- q78 lossy WebP.

## Come riprendere — Fase 1c

1. Leggere questa consegna e lo spec [`fase-1c-timeline.md`](../specs/fase-1c-timeline.md).
2. Branch: `fase-1` (già su `origin`). Non aprire la PR su `main` prima
   della 1c.
3. Il server avvia già `WorkerPool` e i watcher in `keeppix-server` `serve()`.
4. `ActivityTracker::notify_authenticated_request` è il gancio che l'API
   1c deve toccare sulle richieste autenticate.

## Differiti

- Encode WebP lossless invece di q78 (ruling 7).
- Seccomp assente (ruling 12).
- Watcher non vede le librerie create a runtime (ruling 15).
- `RamGate` non condiviso (ruling 16).
- Flake testcontainers `PortNotExposed`: ritentare il binario, non
  «aggiustare» la produzione.
- Spec 1a ancora marcata «in esecuzione» prima di questo close: corretta
  in chiusura 1b.
