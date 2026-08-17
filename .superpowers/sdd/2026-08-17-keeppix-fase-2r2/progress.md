# SDD ledger — plan: docs/superpowers/plans/2026-08-17-keeppix-fase-2r2-difetti-di-campo.md

Branch: `fase-2r2`
Base: `fase-2r` @ `708f792` (docs del piano 2R2; il piano cita HEAD `716e253`,
che è il parent — si parte dal tip di `fase-2r` così il piano è nel branch)
Spec di riferimento: docs/superpowers/specs/fase-1b-ingestione.md

Ruling: lavoro in-place sul branch `fase-2r2`, non in un worktree. Il
checkout cloud è già isolato (`GIT_DIR == GIT_COMMON`), l'utente ha
chiesto il branch, non un worktree. Costo se sbagliato: spostare il
branch in un worktree, un comando.

Ruling: base `708f792` e non `716e253` — il commit docs che aggiunge il
piano è l'unica differenza, e senza di esso il branch non conterrebbe il
documento che stiamo eseguendo. Costo se sbagliato: un commit docs in
più rispetto al SHA citato dal piano.

Nessun push/PR/merge. Field test sull'archivio reale: lo esegue
l'operatore.

## Scansione pre-volo

Task 1–3 condividono `upsert_discovered` e `kind`: D2 senza D1 lascia
`detect_kind` morta; D1 senza D2 fa `SET kind = EXCLUDED.kind` (sempre
`Unknown`) a ogni riscansione. Un solo ciclo di verifica.

Task 4 dipende da D1 (unknown non genera derive) ma è indipendente sul
walker. Task 5 è indipendente.

---

**RED osservato (D2, test 1):**
`a_second_discover_on_unchanged_files_does_not_enqueue_metadata`
dopo aver marcato i job `done`: `left: 4 right: 2`. Il raddoppio è il
difetto. I job sono marcati `done` di proposito: senza, `dedup_key`
maschererebbe D2 e il test tornerebbe verde per caso.

**RED osservato (D2, test 2):** un mtime toccato → `left: 4 right: 3`
(riaccoda anche il file fermo).

**RED osservato (D1+D2):** `rescan_of_unchanged_file_does_not_reset_kind`
→ `left: "unknown" right: "raw_image"`.

**RED osservato (D1):** `metadata_classifies_sony_tiff` → `kind: Unknown`;
`metadata_leaves_unknown_files_unhashed` → `hash_asset` count 1;
`detect_kind_is_wired_into_the_metadata_job` → `detect_kind` assente da
`metadata.rs`.

Ruling (Task 1–3): `upsert_discovered -> Result<Option<Asset>, DbError>`
con `WHERE mtime IS DISTINCT FROM … OR size_bytes IS DISTINCT FROM …`.
L'`UPDATE` (e il reset di `kind`) scatta solo se il file è cambiato.
Costo se sbagliato: un cambio di solo inode non riaccoda — allineato al
piano; il rilevamento spostamenti resta sul job hash.

Ruling (Task 1–3): classificare in `metadata::run` (4 KB + `detect_kind`),
non in discover. Discover continua a inserire `kind = Unknown`. Se
`Unknown`, metadata non accoda `hash_asset` e non chiama `set_indexed`.
Due `open` (4 KB + i 128 KB di `read_exif`) solo su file nuovi/cambiati.
Costo se sbagliato: un syscall extra per file nuovo, non per riscansione.

Ruling (Task 1–3): il ritentativo dei derive falliti non passa dalla
riscansione. Voce differita, come chiede il piano.

Task 1–3: complete (commit `879e81b`, test D1/D2 verdi sul ciclo mirato)

---

**RED osservato (D3 walker):** `walker_skips_dxo_dop_sidecars` vedeva
`foto.ARW`, `edit.pp3`, `foto.ARW.dop`.

**RED osservato (D3 timeline):** `timeline_page_omits_unknown_assets`
includeva l'asset `unknown` indicizzato a mano.

Ruling (Task 4): denylist di estensioni sidecar (`xmp dop pp3 arp thm
aae`), non allowlist — il tipo resta dai magic number. Costo se sbagliato:
un formato valido con quelle estensioni non viene indicizzato; nessuno
di quei suffissi è un'immagine.

Ruling (Task 4): `TimelineRepo::page` aggiunge `kind <> 'unknown'` anche
se metadata non indicizza più gli unknown — difesa se qualcuno li marca
`indexed` a mano. I bucket `folder_month_counts` restano agganciati a
`status = indexed`; senza `set_indexed` gli unknown non ci finiscono.

Task 4: complete (commit `879e81b`, test walker + timeline verdi)

---

**RED osservato (Task 5):** `Config.watch_poll_secs` non esisteva
(E0609 nei test di config).

Ruling (Task 5): la cadenza 4–5 min del field test non è `DEFAULT_POLL`
(già 15 min). Ipotesi: eventi notify spurii sul bind mount Docker
Desktop in modo Native. D2 rende quella riscansione economica. Non si
aggiunge `virtiofs` alla lista dei FS di rete senza evidenza da
`/proc/mounts` sul campo. Costo se sbagliato: su un mount fuse non
rilevato si resta in Native; D2 copre il costo.

Ruling (Task 5): `KEEPPIX_WATCH_POLL_SECS` (default 900) entra in
`Config` e in `LibraryWatchers::with_poll`. `LibraryWatchers::new` per
i test resta sul default. Costo se sbagliato: un operatore che mette `0`
fa busy-loop in polling — non clampato (YAGNI).

Task 5: complete (commit `879e81b`, test config + `DEFAULT_POLL == 15 min` verdi)

---

Differito (piano): field test sull'archivio 779 ARW — lo esegue
l'operatore. Criteri di chiusura del piano non verificabili qui.

Differito: ritentativo automatico dei job `derive_*` falliti.

Verifica (branch): `npm ci && npm run build` (entry ~82 KB gzip),
`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`./scripts/test.sh` — tutti verdi. D2 test 1
(`a_second_discover_on_unchanged_files_does_not_enqueue_metadata`) ok
nella suite completa.

Ruling (verifica): `scripts/test.sh` faceva `docker ps` se il binario
`docker` esiste, anche senza daemon. Con `set -o pipefail` la suite
usciva 1 dopo il primo crate, sul percorso `KEEPPIX_TEST_DATABASE_URL`.
Ora `cleanup_containers` chiama `docker info` prima. Costo se sbagliato:
in CI con daemon giù i container orfani non vengono rimossi — già il
caso precedente, solo che ora non maschera i test.
