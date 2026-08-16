# Prova sul campo

Script: [`scripts/field-test.sh`](../scripts/field-test.sh).

Misura l’ingestione su un **archivio reale** (o una copia di prova), usando solo
gli endpoint HTTP: setup → libreria → scansione. Nessun `INSERT` SQL, nessun
riavvio del container dopo la create.

## Prerequisiti

- Docker e `docker compose`
- Un archivio di foto sul host, montato in sola lettura su `/photos`
- `KEEPPIX_LIBRARY_ROOTS` di default include `/photos` (vedi `Config`)

## Esecuzione

```bash
PHOTOS_PATH="/percorso/all/archivio" ./scripts/field-test.sh
```

Lo script:

1. ricostruisce lo stack `bundled` e verifica che `/photos` sia **read-only**
   (via `docker inspect`, non con `sh` — l’immagine è distroless);
2. crea l’admin con `POST /api/v1/setup`;
3. crea la libreria con `POST /api/v1/libraries` (`root_path: /photos`);
4. avvia la scansione con `POST /api/v1/libraries/{id}/scan`;
5. misura discovery / EXIF / hash / totale finché la coda job è vuota;
6. confronta la discovery con il budget del Task 8 (~30 ms/file, minimo 30 s)
   e **esce ≠ 0** se lo sfora;
7. verifica che l’impronta dell’archivio non sia cambiata;
8. scrive il report in `.superpowers/field-test-YYYYMMDD-HHMM.md`.

## Codici di uscita

| Codice | Significato |
|---|---|
| 0 | ok, entro budget, archivio intatto |
| 2 | timeout a 2 ore |
| 3 | discovery fuori budget |
| 4 | l’archivio è stato modificato |

## Cosa NON fa

Non è un sostituto dei test di viaggio (`journeys.rs`): quelli girano in CI su
fixture. Questa prova è per l’archivio dell’operatore (es. 1 558 ARW Sony) e
per i numeri da scrivere nel ledger di fase.
