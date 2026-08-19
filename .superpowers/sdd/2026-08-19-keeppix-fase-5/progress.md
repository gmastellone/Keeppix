# Fase 5 — WebDAV e upload riprendibili — Ledger

Branch: fase-5
Base commit (main before Fase 5): 7751eed
Plan: docs/superpowers/plans/2026-08-19-keeppix-fase-5.md
Spec: docs/superpowers/specs/fase-5-webdav-upload.md

## Decisioni e Ruling

Ruling: il campo di risposta di `POST /api/v1/upload/check` si chiama
`unknown_hashes`, non `known_hashes` come nell'illustrazione della spec — la
spec stessa (§1.2) e il caso di test pinnato descrivono il comportamento come
«47 hash di cui 12 sconosciuti → la risposta elenca esattamente quei 12»,
cioè gli sconosciuti, non i noti. L'illustrazione `{ known_hashes: [...] }`
è quindi un'etichetta fuorviante sullo stesso comportamento. Dato che è
un'API nuova (Task 1 di Fase 5, non ancora rilasciata), rinominare il campo
non rompe `/api/v1` congelato. Costo se sbagliato: un rename del campo JSON,
nessuna migrazione dati coinvolta.

Ruling: `POST /api/v1/upload/check`, `POST /api/v1/upload`, `HEAD` e `PATCH
/api/v1/upload/{id}` restano dietro il middleware CSRF standard di
`/api/v1` in questo task, nonostante un commento ormai obsoleto in
`csrf.rs` che ipotizzava un'esenzione per le rotte tus/WebDAV vivessero fuori
da `/api/v1`. Il piano di fase (Task 5) mette esplicitamente queste rotte
sotto `/api/v1/upload/*` e prevede l'esenzione CSRF come lavoro a parte più
avanti nella fase. Costo se sbagliato: un client tus reale senza
`x-keeppix-client` riceverebbe 403 finché il Task 5 non aggiunge l'esenzione.

Ruling: l'estrazione dei metadati (`JobKind::ExtractMetadata`) non viene
accodata alla finalizzazione dell'upload in questo task. L'asset creato da
`UploadSessionRepo::finalize` resta con lo stato di default
(`AssetStatus::Discovered`), come un asset scoperto dal walker prima
dell'indicizzazione. Il piano assegna esplicitamente l'enqueue con
`JobPriority::High` al Task 2. Costo se sbagliato: un asset caricato via tus
resta "discovered" finché non arriva la prossima scansione o il Task 2,
invece di essere indicizzato subito.

## Task Log

Task 1 (Sessioni di upload tus — schema e protocollo): complete
(commit ea660f9, test verdi: 14 in `keeppix-db`, 9 in `keeppix-api`,
`cargo fmt --check` e `cargo clippy --workspace --all-targets -- -D
warnings` puliti).

