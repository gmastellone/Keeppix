# Task 10 — fix round 2/5

**Finding in ingresso:** N1 (Important) e N2 (Minor) dalla re-review formale —
vedi `task-10-rereview-report.md`
**Commit:** `90f8b82` — *test(api): pinna gli attributi del cookie `__Host-` e
la revoca di famiglia*
**File toccati:** `crates/keeppix-api/tests/auth.rs` soltanto, +108 righe.
Nessuna modifica al codice di produzione: il buco era solo nei test.

## Cosa è stato scritto

### N1 — due test più un helper condiviso

- `logout_clears_the_cookie_with_a_valid_host_prefix` e
  `login_issues_the_cookie_with_a_valid_host_prefix`, entrambi con
  `.header(reqwest::header::HOST, PRODUCTION_HOST)` (`photos.example.com`), così
  `should_be_secure` è `true` e `Secure` diventa osservabile. Contro l'harness
  locale l'header `Host` è `127.0.0.1:<porta>` e il cookie esce senza `Secure`
  in modo del tutto legittimo: senza il `Host` contraffatto il test non
  proverebbe nulla.
- `assert_host_prefix_attributes(&response, expected_max_age)` legge l'header
  `set-cookie` **dalla risposta** e verifica `Secure`, `SameSite=Lax`, `Path=/`,
  `HttpOnly` e il `Max-Age` atteso (`Max-Age=0` per il logout, `Max-Age=3600`
  per il login, TTL dell'harness). Si legge dalla risposta e non dal cookie
  store di `reqwest` perché quest'ultimo scarterebbe un cookie `Secure` arrivato
  su HTTP in chiaro, nascondendo proprio ciò che si vuole osservare. La
  motivazione (RFC 6265bis §4.1.3.2) è scritta una volta sola, sul doc comment
  dell'helper.
- L'helper **non** usa `contains` sull'header intero: splitta su `;`, isola
  `nome=valore` dagli attributi e li confronta per uguaglianza. Il valore del
  token è base64url casuale e potrebbe contenere `Secure` o `Path=/` per caso,
  rendendo un `contains` un falso positivo silenzioso — esattamente il genere di
  test che passa senza provare ciò che afferma.

### N2 — strada forte invece dell'allineamento del commento

Dopo il replay del token consumato, `refresh_rejects_a_reused_token` ripresenta
ora il token *nuovo* — quello uscito dalla prima rotazione, valido finché la
famiglia non viene revocata — a `/api/v1/auth/me` con un client fresco, e
pretende 401. Il commento del test è così diventato vero senza annacquarlo, e la
revoca di famiglia è coperta anche a livello HTTP.

## Mutazioni — output reali

**1. `clearing_cookie` senza `set_secure(secure)`:**

```
thread 'logout_clears_the_cookie_with_a_valid_host_prefix' panicked at crates/keeppix-api/tests/auth.rs:436:9:
manca `Secure` in `__Host-kpx_session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0`
test result: FAILED. 12 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

**2. `clearing_cookie` con `set_secure` ripristinato ma senza `set_same_site(SameSite::Lax)`:**

```
thread 'logout_clears_the_cookie_with_a_valid_host_prefix' panicked at crates/keeppix-api/tests/auth.rs:436:9:
manca `SameSite=Lax` in `__Host-kpx_session=; HttpOnly; Secure; Path=/; Max-Age=0`
test result: FAILED. 12 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

**3. `session_cookie` senza `set_secure(secure)`:**

```
thread 'login_issues_the_cookie_with_a_valid_host_prefix' panicked at crates/keeppix-api/tests/auth.rs:436:9:
manca `Secure` in `__Host-kpx_session=8oUwPwrq38F99GyFDnfmXcgr6mME_bQbJda_vMbT4Bg; HttpOnly; SameSite=Lax; Path=/; Max-Age=3600`
test result: FAILED. 12 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

**4. `session_cookie` senza `set_same_site(SameSite::Lax)`:**

```
thread 'login_issues_the_cookie_with_a_valid_host_prefix' panicked at crates/keeppix-api/tests/auth.rs:436:9:
manca `SameSite=Lax` in `__Host-kpx_session=kXu14Lr0ZUhtWW0ax-AZXzFEh9fSLSimc6Cy1W2EWJQ; HttpOnly; Secure; Path=/; Max-Age=3600`
---- setup_creates_the_first_admin_and_logs_in stdout ----
assertion failed: cookie.contains("SameSite=Lax")
test result: FAILED. 11 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out
```

Qui cade anche il test preesistente, che già asseriva `SameSite` — l'unico
attributo che era osservabile su localhost.

**5. N2 — `SessionRepo::rotate` senza l'`UPDATE ... SET revoked_at` sulla
famiglia, mantenendo `return Err(DbError::Forbidden)`:**

```
thread 'refresh_rejects_a_reused_token' panicked at crates/keeppix-api/tests/auth.rs:304:5:
assertion `left == right` failed: il riuso deve revocare l'intera famiglia, non solo il token ripresentato
  left: 200
 right: 401
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 12 filtered out
```

Ogni mutazione è stata applicata a partire da una copia intatta del file e il
file di produzione ripristinato subito dopo; `git status` finale conferma che
`cookie.rs` e `sessions.rs` sono identici a HEAD.

## Verifiche finali

| Comando | Esito |
|---|---|
| `cargo test --workspace -- --test-threads=1` | verde (`keeppix-api/tests/auth.rs` 13 passed; resto del workspace invariato) |
| `cargo clippy --workspace --all-targets -- -D warnings` | pulito |
| `cargo fmt --check` | pulito |

Riverificato dal controller dopo il commit: 85 esecuzioni di test verdi, clippy
e fmt puliti, `git diff 55de9b9..90f8b82 --stat` conferma un solo file toccato.

I due nuovi test hanno `#[allow(clippy::unwrap_used)]` sulla singola funzione,
l'helper `#[allow(clippy::unwrap_used, clippy::expect_used)]`, secondo la
convenzione del repository.

## Notato ma non corretto

- `setup_creates_the_first_admin_and_logs_in` continua a verificare gli
  attributi con `cookie.contains(...)` sull'header intero e senza contraffare
  `Host`: non può quindi vedere `Secure`, ed è in linea di principio esposto al
  falso positivo del token casuale. Non riscritto per tenere il diff sul
  finding: la proprietà che gli mancava è ora coperta dai due test nuovi, e il
  suo scopo primario è un altro (il setup autentica subito).
- Il `Max-Age=3600` del test di login è accoppiato al TTL dell'harness
  (`AppState::new(db, 3600)`). Se quel valore cambia il test fallisce in modo
  esplicito e leggibile: pin esatto preferito a un'asserzione vaga tipo
  "`Max-Age` presente e non zero", più debole proprio dove serve rigore.
- `should_be_secure(None)` (header `Host` assente ⇒ `Secure`) resta coperto solo
  dagli unit test in `cookie.rs`: a livello HTTP non è riproducibile con
  `reqwest`, che su HTTP/1.1 invia sempre `Host`.
