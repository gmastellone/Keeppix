# Fase 3 — Multiutente, condivisione e link pubblici

**Stato:** specifica di progetto, non ancora pianificata in task
**Dipende da:** Fase 1 (visibilità già predisposta), Fase 2 (album e flag)
**Chiusa quando:** una cartella condivisa a un utente esterno mostra esattamente
quel sottoalbero, e un link pubblico con password e scadenza funziona da fuori
casa

Questa è la fase con la maggiore superficie di sicurezza. Ogni decisione qui
dentro è stata presa per rendere **impossibile** una certa classe di errore,
non per rendere il codice elegante.

---

## 1. Ruoli — due livelli tenuti separati

### 1.1 Ruolo di sistema

`admin` o `user`. L'admin crea utenti e gruppi, definisce le librerie e i loro
path, vede lo stato del sistema, e **ha accesso completo in lettura e scrittura
su tutto**.

Ogni accesso dell'admin a contenuti non suoi finisce nell'**audit log**. Non
per sfiducia: perché su un sistema multiutente è la cosa che salva quando
qualcuno chiede «chi ha cancellato quella cartella».

### 1.2 Ruolo sull'oggetto

`owner`, `editor`, `viewer`.

| Azione | viewer | editor | owner | admin |
|---|:-:|:-:|:-:|:-:|
| Vedere, scaricare preview | ✅ | ✅ | ✅ | ✅ |
| Scaricare l'originale | opz. | ✅ | ✅ | ✅ |
| Rating e pick (propri) | ✅ | ✅ | ✅ | ✅ |
| Modificare metadati, tag, descrizione | ❌ | ✅ | ✅ | ✅ |
| Aggiungere/togliere da album | ❌ | ✅ | ✅ | ✅ |
| Caricare file nella cartella | ❌ | ✅ | ✅ | ✅ |
| Cestinare in Keeppix | ❌ | ✅ | ✅ | ✅ |
| **Cancellare dal disco** | ❌ | ❌ | ✅ | ✅ |
| **Ri-condividere ad altri** | ❌ | ❌ | ✅ | ✅ |

Le due righe in grassetto sono la protezione che conta: **un editor non può
distruggere i tuoi file** — può solo cestinarli in Keeppix, da dove li recuperi
— e **non può allargare la condivisione** a sua discrezione.

---

## 2. Gruppi

```sql
groups (id, name, created_by, created_at)          -- già creata in Fase 0
group_members (group_id, user_id, added_at)        -- già creata in Fase 0
```

Un gruppo compare ovunque compaia un utente. «Famiglia» con dentro 4 persone:
condividi la cartella con il gruppo, e chi entra nel gruppo dopo **eredita
l'accesso automaticamente**. Nessuna ri-condivisione manuale.

**I gruppi non si trasportano nell'`AuthContext`.** Si derivano da `user_id`
con un join su `group_members` dentro la risoluzione dello scope. Un elenco di
gruppi trasportato nel token è un elenco che può essere stantio: rimuovere
qualcuno da un gruppo deve avere effetto immediato.

---

## 3. Permessi

```sql
permissions (
    id           uuid PRIMARY KEY,
    subject_type text NOT NULL CHECK (subject_type IN ('user','group')),
    subject_id   uuid NOT NULL,
    object_type  text NOT NULL CHECK (object_type IN ('folder','album','asset')),
    object_id    uuid NOT NULL,
    role         text NOT NULL CHECK (role IN ('viewer','editor')),
    inherit      boolean NOT NULL DEFAULT true,
    granted_by   uuid REFERENCES users(id),
    created_at   timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX permissions_subject_idx ON permissions (subject_type, subject_id);
CREATE INDEX permissions_object_idx  ON permissions (object_type, object_id);
```

### 3.1 Solo-allow, nessun deny

**Vince il permesso più alto** fra tutti quelli che si applicano (proprio, dei
gruppi, ereditati dalle cartelle superiori). Nessun «deny» esplicito.

È una scelta deliberata. I sistemi con deny (NTFS, S3) sono potenti e sono
anche la fonte numero uno di «perché non riesco a vedere questa foto?». Con
solo-allow, l'interfaccia può sempre rispondere alla domanda **«perché ho
accesso a questa foto?»** con una catena leggibile:

> Hai accesso perché → il gruppo **Famiglia** ha ruolo *viewer* su
> **/Foto/Vacanze**, ereditato in **/2024/Grecia**.

Per togliere l'accesso a una sottocartella si interrompe l'ereditarietà su quel
nodo (`inherit = false`), che è esplicito e visibile in interfaccia.

### 3.2 La query di visibilità

Estende quella della Fase 1a **senza cambiare i chiamanti** — è il motivo per
cui `VisibilityScope` espone una clausola SQL con i suoi parametri e non un
elenco grezzo di id.

```sql
WITH my_groups AS (
    SELECT group_id FROM group_members WHERE user_id = $me
),
allowed AS (
    -- Le librerie che possiedo
    SELECT f.path FROM folders f
      JOIN libraries l ON l.id = f.library_id
     WHERE l.owner_id = $me
    UNION
    -- I sottoalberi condivisi con me o con i miei gruppi
    SELECT f.path FROM permissions p
      JOIN folders f ON f.id = p.object_id
     WHERE p.object_type = 'folder'
       AND (
            (p.subject_type = 'user'  AND p.subject_id = $me)
         OR (p.subject_type = 'group' AND p.subject_id IN (SELECT group_id FROM my_groups))
       )
)
SELECT a.* FROM assets a
  JOIN folders f ON f.id = a.folder_id
 WHERE f.path <@ ANY(SELECT path FROM allowed)
   AND a.status = 'indexed'
 ORDER BY a.taken_at_utc DESC, a.id DESC
 LIMIT 200;
```

I prefissi autorizzati sono tipicamente **1-10** nei casi reali. **Nessuna
tabella di visibilità materializzata**: cambiare un permesso è un `INSERT` con
effetto immediato.

Da misurare durante l'esecuzione: il tempo di questa query con 50 permessi e
200.000 asset. Se degrada, la mitigazione è cachare i prefissi risolti per
utente (invalidati su `permissions.changed`), **non** materializzare la
visibilità.

---

## 4. I tre oggetti condivisibili

| Oggetto | Cosa vede il destinatario |
|---|---|
| **Foto singola** | Solo quella. Non risale alla cartella, non vede i vicini. |
| **Cartella** | Il sottoalbero navigabile, incluse le sottocartelle (salvo interruzione). I file aggiunti dopo sono visibili subito. |
| **Album** | L'insieme curato, senza esporre dove i file stanno sul disco. |

**Chi riceve una cartella condivisa non vede mai il percorso reale sul
filesystem.** Vede `Vacanze / 2024 / Grecia`, non `/mnt/nas/foto/…`. Il path
assoluto è informazione del proprietario.

---

## 5. Album

```sql
albums (id, name, description, owner_id, cover_asset_id, created_at);
album_assets (album_id, asset_id, position, added_by, added_at,
              PRIMARY KEY (album_id, asset_id));
```

**Virtuali**: nessuno storage. Una foto può stare in 10 album pesando una volta
sola. Ordinamento manuale possibile (`position`).

Gli album condivisi mostrano gli avatar di chi ha accesso. In un album
condiviso si può attivare la **modalità selezione collaborativa**: i pick di
tutti vengono uniti e mostrati con l'autore — è il caso «culling a quattro mani
con il cliente» della Fase 2.

---

## 6. Link pubblici

```sql
share_links (
    id              uuid PRIMARY KEY,
    token_hash      bytea NOT NULL,      -- SHA-256 del token, MAI il token
    object_type     text NOT NULL CHECK (object_type IN ('asset','folder','album')),
    object_id       uuid NOT NULL,
    created_by      uuid NOT NULL REFERENCES users(id),
    password_hash   text,                -- argon2id, opzionale
    expires_at      timestamptz,
    max_views       int,
    view_count      int NOT NULL DEFAULT 0,
    allow_download  boolean NOT NULL DEFAULT true,
    allow_original  boolean NOT NULL DEFAULT false,
    allow_upload    boolean NOT NULL DEFAULT false,
    allow_cdn_cache boolean NOT NULL DEFAULT false,
    hide_metadata   boolean NOT NULL DEFAULT true,
    upload_quota_bytes bigint,
    revoked_at      timestamptz,
    last_accessed_at timestamptz,
    created_at      timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX share_links_token_hash_key ON share_links (token_hash);
```

### 6.1 Sicurezza — non opzionale

- **Token da 32 byte casuali** in base64url. In database **solo l'hash**: un
  dump del database **non apre i tuoi link**.
- **`X-Robots-Tag: noindex, nofollow`** e **`Referrer-Policy: no-referrer`** su
  tutte le pagine pubbliche. Il secondo impedisce che il token trapeli
  nell'header `Referer` quando l'ospite clicca un link.
- **Rate limiting** per token e per IP sui tentativi di password.
- **Lookup a tempo costante**, nessuna enumerazione possibile.
- **`hide_metadata` attivo di default** sui link senza password: quando mandi
  la foto di casa a un conoscente, non gli mandi anche le coordinate GPS di
  casa tua. Preview servite senza EXIF, mappa nascosta, coordinate assenti
  anche dall'API.

### 6.2 Nascondere le posizioni sensibili

Impostazione **«nascondi le posizioni entro N metri da un punto»**: si definisce
casa propria, e nei contenuti condivisi le foto scattate lì appaiono senza
coordinate. Il dato resta intatto nel database.

### 6.3 Upload da ospite

Il caso «il cliente mi manda le sue foto». Il link con `allow_upload` mostra
un'area di caricamento.

I file arrivano nella cartella di destinazione con flag `uploaded_by_guest` e
finiscono in una **coda di revisione** che il proprietario approva o scarta.
**Nessuno riempie il disco a tua insaputa**: il link ha `upload_quota_bytes`.

### 6.4 CDN

`allow_cdn_cache` per-link, **spento di default**. I contenuti autenticati
portano `Cache-Control: private` sempre. Un album di matrimonio mandato a 200
invitati è però pubblico per definizione: quel caso può essere servito con
`Cache-Control: public` da un URL separato, cacheabile da un CDN.

---

## 7. Il punto in cui i permessi vengono applicati

Questo è l'aspetto architetturale della fase, e conta più delle tabelle.

**Esiste una sola funzione che costruisce il filtro di visibilità**, in
`keeppix-db`. Ogni repository che legge asset **richiede un `AuthContext` come
parametro**: non è possibile scrivere una query sugli asset senza passare da
lì, perché non esiste un metodo che non lo prenda.

Ne conseguono due cose:

- **REST, WebDAV, WebSocket e link pubblici condividono lo stesso identico
  controllo.** Un buco nei permessi non può esistere in un solo canale.
- **Un link pubblico è un `AuthContext::ShareLink { scope, allow_download, … }`.**
  Non è una strada parallela con regole sue: è lo stesso motore con un contesto
  diverso. È il tipo di errore in cui cadono molti progetti, ed è escluso per
  costruzione.

La variante `Actor::ShareLink` è **prevista dalla Fase 0** ma non implementata:
va aggiunta qui.

---

## 8. Pannello permessi

Su **qualsiasi** oggetto — foto, cartella, album, e su selezioni multiple — lo
stesso pannello:

```
Condivisione — /Foto/2024/Matrimonio Rossi
─────────────────────────────────────────────────────
 Accesso diretto
   👤 Giovanni          proprietario
   👥 Famiglia          viewer          [▾] [×]
   👤 mario@studio.it   editor          [▾] [×]

 Ereditato da /Foto                      [interrompi ereditarietà]
   👥 Casa              viewer          [▾ sovrascrivi] [× escludi qui]

 ＋ Aggiungi utente o gruppo…

 Link pubblici (2 attivi)
   🔗 …/s/x7Kp9  🔒 password · scade 12/09 · 47 visite   [copia] [revoca]
   🔗 …/s/m2Qw4  scade mai · download ON                  [copia] [revoca]
─────────────────────────────────────────────────────
```

Quello che lo rende utile davvero:

- **diretti ed ereditati sono visivamente distinti**, e su un ereditato si può
  sovrascrivere il ruolo o escluderlo su quel nodo senza toccare il livello
  superiore;
- cliccando un utente si vede **la catena del perché** ha accesso;
- su una selezione multipla lo stesso pannello applica il permesso in blocco;
- pagina globale **«Condivisioni»** con tutto ciò che esce di casa: chi vede
  cosa, tutti i link attivi con ultimo accesso, revoca di massa.

---

## 9. Audit log

```sql
audit_log (
    id          bigserial PRIMARY KEY,
    actor_id    uuid REFERENCES users(id),
    actor_kind  text NOT NULL,     -- 'user' | 'share_link' | 'system'
    action      text NOT NULL,
    object_type text,
    object_id   uuid,
    detail      jsonb,
    ip          inet,
    at          timestamptz NOT NULL DEFAULT now()
);
```

Registra: creazione e revoca di condivisioni e link, **accessi ai link
pubblici**, cancellazioni dal disco, cambi di ruolo, **accessi dell'admin a
contenuti altrui**, login falliti.

Consultabile dall'admin, non modificabile da nessuno.

---

## 10. Debiti della Fase 0 da saldare qui

Erano stati differiti a questa fase con una ragione precisa. Vanno chiusi
adesso, non rimandati ancora:

| Voce | Perché ora |
|---|---|
| **`refresh`/`rotate` non ricontrollano `users.disabled_at`** | Ora esiste un percorso di disabilitazione da interfaccia. Va anche scritto il test «disabilitare un utente termina le sue sessioni» |
| **Nessun rate limiting su `/auth/login` e `/setup`** | È **lo stesso middleware** dei link pubblici: farlo prima significava scriverlo due volte |
| **`logout` risponde 204 anche se `revoke` fallisce** | Si chiude con la pagina «Dispositivi» (`/auth/devices`), che nasce qui |
| **`sessions.ip` mai popolata** | Serve all'audit log, e richiede la configurazione «proxy fidati» per leggere `X-Forwarded-For`. Popolarla con l'IP del proxy sarebbe peggio che lasciarla vuota |
| **`map_unique_violation` scarta l'errore sottostante** | Serve dove «username preso» ed «email presa» sono messaggi distinti: la gestione utenti, che nasce qui |

---

## 11. Cosa NON è in Fase 3

Mappa: Fase 4. WebDAV: Fase 5. 2FA e backup: Fase 6. Federazione fra istanze:
mai, è fuori dagli obiettivi del progetto.
