# Task 13 — note di pre-volo del controller

Da leggere insieme a `task-13-brief.md`. **Dove queste note contraddicono il
brief, vincono le note**, e la contraddizione va segnalata nel report.

## P1 — Il brief reintroduce il bug del ruling R5, e lo estende al fallback SPA

Questo è il punto critico del task.

Nel Task 9 fu trovato e corretto un difetto del piano: `.fallback(not_found)`
veniva chiamato **dopo** la catena `.layer(...)`. In axum 0.8 `Router::fallback`
sostituisce il fallback esistente, e `.layer()` avvolge soltanto il fallback
presente **al momento della chiamata**: con quell'ordine ogni 404 usciva senza
CSP, `x-content-type-options`, `referrer-policy` e `permissions-policy`. La
correzione — `.fallback()` **prima** dei `.layer()` — è nel codice attuale di
`crates/keeppix-api/src/lib.rs` con un commento esplicito che dice di non
riordinare.

Lo step 5 del brief ristruttura `common_layers` e rimette l'ordine sbagliato:

```rust
fn common_layers<S>(router: Router<S>) -> Router<S> { /* solo i .layer(...) */ }

pub fn router(state: AppState) -> Router {
    router_parts().fallback(not_found).with_state(state)   // fallback DOPO i layer
}

fn base_router_stateless() -> Router {
    common_layers( ... ).fallback(not_found)               // idem
}
```

E lo stesso vale per il fallback SPA dello step 4:

```rust
pub fn mount(router: Router<AppState>) -> Router<AppState> {
    router.fallback(get(serve))    // applicato a un router già "layerizzato"
}
```

La conseguenza qui è peggiore che nel Task 9: il fallback SPA serve
**`index.html`**, cioè proprio il documento che carica l'applicazione. Servirlo
senza `Content-Security-Policy` significa che la pagina principale di Keeppix
gira senza CSP in produzione, mentre `/health` ce l'ha.

**Invariante da rispettare, comunque tu scelga di ottenerla:** ogni risposta
che esce dal binario — rotte API, 404 `problem+json`, `index.html` del fallback
SPA e gli asset statici — deve portare i quattro header di sicurezza. Non ti
prescrivo la forma: puoi far sì che i layer si applichino per ultimi, o che
`mount` inserisca il fallback prima della layerizzazione, o altro. Scegli e
motiva nel report.

**Come dimostrare che ci sei riuscito:**

1. Esiste già `assert_security_headers` in `crates/keeppix-api/tests/health.rs`,
   usato sia su `/health` sia sul 404. Quei test devono restare verdi: se
   diventano rossi mentre lavori, **non è il test a essere sbagliato** — è la
   ristrutturazione. Non indebolirli.
2. Aggiungi l'asserzione equivalente sulla risposta del **fallback SPA**
   (`GET /` e un percorso client-side qualsiasi, per esempio `/albums/42`), che
   oggi non esiste perché il fallback SPA non esiste.
3. Provalo per mutazione: rimetti deliberatamente `.fallback()` dopo i
   `.layer()`, verifica che il nuovo test diventi rosso, ripristina, e riporta
   nel report l'output reale di entrambi i passaggi.

## P2 — I percorsi `/api` non devono mai cadere nel fallback SPA

Il brief lo dice in testa («I percorsi sotto `/api` non ricadono mai nel
fallback: devono restituire `404 problem+json`») ma non prescrive un test che lo
inchiodi. Un client che riceve `index.html` con status 200 al posto di un 404
JSON è un bug silenzioso e sgradevole da diagnosticare: il codice ramifica sul
tipo di contenuto e trova HTML.

Aggiungi un test che chieda `/api/v1/does-not-exist` e pretenda **404** con
`content-type: application/problem+json`, non HTML. Verifica che sia vivo.

## P3 — Il test salta silenziosamente se `frontend/dist` non esiste

Lo step 2 introduce `frontend_built()` e fa uscire i test senza fallire quando
la build del frontend manca. È una scelta ragionevole — il backend deve poter
essere testato da solo — ma ha un costo: in un ambiente dove `frontend/dist` non
viene mai costruito, questi test non provano **nulla** e nessuno se ne accorge,
perché passano.

Quando esegui la suite, **costruisci prima il frontend** (`cd frontend && npm
run build`, il Task 12 lo ha già reso possibile) così i test girano davvero, e
riporta nel report l'output che dimostra che non sono stati saltati. Se un test
è saltato, dillo esplicitamente invece di contare un verde che non c'è.

## P4 — `interpolate-folder-path` e il percorso di `frontend/dist`

`rust-embed` con `interpolate-folder-path` risolve il percorso a tempo di
compilazione. Il crate è `keeppix-server`, quindi il percorso relativo parte da
`crates/keeppix-server/`. Verifica che il binario compilato **contenga davvero**
i file (non un embed vuoto che degrada a 404 su tutto): un modo è controllare
che `GET /` restituisca l'`index.html` reale, con dentro il tag `<script>`
generato da Vite, non una stringa vuota con status 200.

## P5 — Confini

- Non toccare `crates/keeppix-api/src/routes/`, `cookie.rs`, `extract.rs`,
  `problem.rs`, `openapi.rs`: il task ristruttura solo il montaggio del router.
- Se la ristrutturazione cambia la firma di `router()` usata dagli harness di
  test (`crates/keeppix-api/tests/harness/mod.rs` chiama
  `keeppix_api::router(state)`), aggiorna gli harness di conseguenza — ma
  **non** cambiare ciò che i test asseriscono.
- La rotta `/api/openapi.json` del Task 11 deve restare montata e coperta dagli
  header di sicurezza: c'è un test che lo verifica, e vale la stessa regola del
  punto P1.

## P6 — Ambiente

Docker non è disponibile (policy di egress). Per la suite Rust:

```bash
export KEEPPIX_TEST_DATABASE_URL="postgres://keeppix:keeppix@127.0.0.1:5432/postgres"
cargo test --workspace -- --test-threads=1
```

Se Postgres non risponde: `pg_ctlcluster 16 main start`.
