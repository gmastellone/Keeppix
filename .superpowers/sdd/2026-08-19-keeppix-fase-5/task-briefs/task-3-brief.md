# Task 3: Frontend — pannello di upload persistente

## Contesto
Fase 5 (WebDAV e upload riprendibili), Task 3 su 10. Task 1 (sessioni tus backend) e Task 2 (indicizzazione alta priorità + cleanup) sono già completi e in prod su `fase-5`. Questo task è puro frontend.

## File da creare/modificare
- CREATE: `frontend/src/stores/upload.ts`
- CREATE: `frontend/src/components/UploadPanel.vue`
- CREATE: `frontend/src/components/UploadPanel.spec.ts` (Vitest)
- CREATE: `frontend/src/api/upload.ts` (client HTTP per le rotte tus)
- MODIFY: `frontend/src/i18n/en.json` e `frontend/src/i18n/it.json` — aggiungere chiave `"upload"` con tutte le stringhe UI
- NO nuova rotta nel router: il pannello è un overlay globale, non una vista

## API backend già disponibili
```
POST   /api/v1/upload/check          body: { hashes: string[] }
                                     resp: { unknown_hashes: string[] }
POST   /api/v1/upload                body: { target_folder_id, filename, expected_size, expected_hash?, client_mtime? }
                                     resp: 201 { id: string }  Location: /api/v1/upload/{id}
HEAD   /api/v1/upload/{id}           resp: header Upload-Offset: <i64>
PATCH  /api/v1/upload/{id}           headers: Upload-Offset, Upload-Checksum: "blake3 <hex>"
                                     body: raw bytes
                                     resp: 204 (chunk ok) | 201 (finalizzato) { asset_id, filename, collision, existing_asset_id? }
```
`collision` può essere `"created"`, `"skipped_duplicate"`, `"renamed"`.

NOTA: le rotte `/api/v1/upload/*` NON mandano `x-keeppix-client` — di default `apiFetch` in `api/client.ts` lo aggiunge, ma queste rotte tus inviano `Content-Type: application/offset+octet-stream` e il body è bytes grezzi, non JSON. Serve una fetch separata (o un parametro skip) per i PATCH chunk. Per le richieste JSON (check, create) si può usare `apiFetch` normale.

## Struttura store `frontend/src/stores/upload.ts`
- Lista di sessioni attive in memoria + copia in localStorage per sopravvivere al refresh
- Una sessione ha: id, filename, target_folder_id, expected_size, received_bytes, status ('queued'|'uploading'|'paused'|'done'|'error'|'skipped'), collision?, error?
- `initFromStorage()`: al mount dell'app legge localStorage, fa HEAD su ciascun id, riprende le vive (status uploading/paused → riprende), segna le scadute (HEAD→410 = "gone")
- Massimo 3 upload concorrenti (già in esecuzione contemporaneamente, non chunk in parallelo: chunk sequenziali per file)
- Chunk adattivi: parte da 8 MB, scende a 1 MB se un chunk fallisce, ritorna su gradualmente
- `addFiles(files: File[], folder_id: string)`: lancia pre-check, salta i duplicati, accoda i nuovi
- `pause(id)` / `resume(id)` / `removeCompleted()`

## UploadPanel.vue (overlay globale)
- Mostrare solo se ci sono sessioni attive o completate non ancora rimosse
- Minimizzabile: bottone per collassare a una barra piccola in basso
- Barra di progresso per-file
- Icona stato (orologio = in coda, freccia = in corso, ✓ = fatto, ✕ = errore)
- Collisione segnalata: "già presente" (skipped_duplicate), "salvato come X" (renamed)
- "Riprova" per i file in errore
- Il pannello non viene montato nel router — va montato in `App.vue` o in un layout wrapper

## Test Vitest (UploadPanel.spec.ts)
TDD: scrivere prima i test che falliscono, poi l'implementazione. Test richiesti:
1. `pre_check_skips_files_already_in_library`: mock `POST /upload/check` che restituisce 1 hash noto su 2 → solo 1 file entra nello store come "queued"
2. `resumes_session_from_localstorage_on_init`: store inizializzato con un id in localStorage, mock `HEAD` che restituisce offset 1024 → sessione ripresa con received_bytes=1024
3. `marks_session_gone_when_head_returns_410`: id in localStorage, mock HEAD→410 → sessione segnata "error" (scaduta)
4. `two_uploads_run_concurrently_up_to_three`: 4 file accodati → dopo start, max 3 in stato "uploading" simultaneamente

## Vincoli
- Nessuna stringa utente hard-coded nei componenti: tutto via `t('upload.*')`
- Chiavi i18n uguali in `en.json` e `it.json` (c'è un test CI che lo verifica)
- `npx vue-tsc --noEmit` pulito
- `npm run test` (vitest) verde
- NO nuove dipendenze npm senza necessità reale
- NO nuovo endpoint backend: le rotte sono già cablate su `fase-5`

## Verifica prima di dichiarare done
```bash
cd frontend && npm run test
npx vue-tsc --noEmit
```
Clippy/cargo non toccati: questo è solo frontend.

## Commit
```
git commit -m "feat(frontend): persistent resumable upload panel"
```

## Report
Scrivi il report su:
`/workspace/.superpowers/sdd/2026-08-19-keeppix-fase-5/task-briefs/task-3-report.md`

Il report deve contenere:
- commit sha(s)
- output `npm run test` (quanti test, quanti passati)
- output `npx vue-tsc --noEmit` (pulito o errori)
- eventuali ruling/decisioni prese
- self-review: hai visto fallire ogni test prima di implementare?
