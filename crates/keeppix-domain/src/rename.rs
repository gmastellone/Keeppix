//! Il motore delle formule di rinomina (Fase 9 Task 6-7, spec UI §62 "Dialog
//! 'Rinomina con formula'"). Puro: nessuna lettura da disco o database —
//! `crates/keeppix-db/src/rename.rs` (Task 8-9) collega questo motore a
//! collisioni vere sul database, ambiti, e allo spostamento fisico.
//! [`render_base`]/[`apply_base_to_filename`] esistono separati da
//! [`render_filename`] proprio per il Task 8: i file affiancati di una
//! pila (RAW + JPEG) prendono la stessa base, ciascuno con la propria
//! estensione.
//!
//! I sei segnaposto e il fallback a schema vuoto seguono esattamente
//! `computeRenamedFilename`/`renameSlug` del prototipo (spec §62.3b). **Tre
//! dei cinque difetti che la spec elenca esplicitamente per il prototipo**
//! (§62.3d, Task 7 del piano) sono chiusi qui, non riprodotti: separatori
//! orfani intorno a un valore mancante, caratteri illegali del filesystem
//! oltre ai tre che il prototipo già sostituiva, e nessun limite di
//! lunghezza. Gli altri due (collisione verificata anche fuori dal gruppo
//! selezionato; `"Applica"` davvero disabilitato) non sono logica pura —
//! il primo è Task 8/9 in `keeppix-db` (serve il database), il secondo è
//! comportamento di interfaccia, Fase 11.

use chrono::NaiveDate;

/// I valori disponibili per una foto, già risolti (non slugificati: la
/// slugificazione è responsabilità di questo modulo, non di chi chiama).
/// `place` è già il risultato della precedenza posizione-foto →
/// posizione-cartella → nome-lotto → niente ([`resolve_place_label`]) — chi
/// costruisce questo valore lo fa con quella funzione, non a mano.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenameValues {
    pub date: Option<NaiveDate>,
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub place: Option<String>,
    pub title: Option<String>,
}

/// Precedenza del "luogo" (Task 6, piano: "Il luogo si risolve con
/// precedenza: posizione della foto → posizione della cartella → nome del
/// lotto → niente"). Pura: le tre candidate arrivano già risolte da chi
/// chiama (Task 7/8, che sa come leggere posizione della foto, posizione
/// della cartella, e nome del lotto dal database) — questa funzione decide
/// solo l'ordine, non come procurarsi i valori.
#[must_use]
pub fn resolve_place_label(
    photo_position: Option<&str>,
    folder_position: Option<&str>,
    lot_name: Option<&str>,
) -> Option<String> {
    photo_position
        .or(folder_position)
        .or(lot_name)
        .map(str::to_owned)
}

/// Limite reale di `NAME_MAX` su ext4 e sulla maggior parte dei filesystem
/// POSIX (byte, non caratteri) — non una cifra scelta a caso: è il vincolo
/// del filesystem stesso, verificato dal Task 7 (piano: "nessun controllo
/// sulla lunghezza massima" era il quarto difetto elencato dalla spec).
const MAX_FILENAME_BYTES: usize = 255;

/// Ricompone il nome file secondo lo schema (spec §62.3b, punti 1-6), poi
/// applica le tre correzioni del Task 7 (§62.3d) che il prototipo non ha:
/// separatori orfani ripuliti, l'intero insieme dei caratteri illegali del
/// filesystem sostituito (non solo `/\:`), lunghezza mai oltre
/// [`MAX_FILENAME_BYTES`]. Nessuna validazione bloccante: qualunque schema
/// produce un risultato, mai un errore — la spec è esplicita ("qualunque
/// testo è accettato").
///
/// `index` è 1-based (spec: "l'indice nell'elenco attivo + 1"): chi chiama
/// passa già `posizione_nell_elenco + 1`, non un indice 0-based.
///
/// Scompone in [`render_base`] + [`apply_base_to_filename`] per il caso
/// del Task 8 (i file affiancati di una pila prendono lo stesso nome): la
/// base va calcolata **una volta per pila**, non una volta per file, poi
/// riattaccata all'estensione — diversa — di ciascun file affiancato.
#[must_use]
pub fn render_filename(
    schema: &str,
    values: &RenameValues,
    index: usize,
    current_filename: &str,
) -> String {
    let base = render_base(schema, values, index);
    apply_base_to_filename(&base, current_filename)
}

/// La parte calcolata dallo schema (spec §62.3b punti 2-5: segnaposto,
/// contatore, pulizia separatori orfani, sanificazione), **senza**
/// estensione — chi ha bisogno di applicare la stessa base a più file con
/// estensioni diverse (Task 8: RAW e JPEG di una pila) chiama questa una
/// volta sola, poi [`apply_base_to_filename`] per ciascun file.
#[must_use]
pub fn render_base(schema: &str, values: &RenameValues, index: usize) -> String {
    let substituted = substitute_placeholders(schema, values, index);
    let collapsed = collapse_orphan_separators(&substituted);
    sanitize(&collapsed)
}

/// Applica una base già calcolata (da [`render_base`]) al nome attuale di
/// **un** file: fallback al nome attuale senza estensione se la base è
/// vuota, taglio a [`MAX_FILENAME_BYTES`], estensione del file — la sua,
/// non quella di un altro membro della stessa pila — riattaccata in
/// maiuscolo.
#[must_use]
pub fn apply_base_to_filename(computed_base: &str, current_filename: &str) -> String {
    let (stem, extension) = split_extension(current_filename);
    let extension = extension.map(str::to_uppercase);
    let base = if computed_base.is_empty() {
        stem.to_owned()
    } else {
        computed_base.to_owned()
    };
    let base = cap_length(&base, extension.as_deref());
    match extension {
        Some(ext) => format!("{base}.{ext}"),
        None => base,
    }
}

/// I caratteri considerati "separatore" ai fini della pulizia orfani —
/// esattamente quelli che uno schema tipico usa fra segnaposto (`_`, `-`,
/// spazio, `.`). Una run di due o più, in qualunque combinazione fra loro,
/// è per costruzione l'effetto di un valore mancante fra due segnaposto
/// adiacenti, mai un'intenzione dell'utente: nessuno schema reale ha
/// bisogno di `__` letterale. — *Costo se sbagliato:* uno schema che
/// contenesse davvero due separatori di fila apposta li vedrebbe
/// compattati a uno; nessun caso reale osservato che lo richieda.
const ORPHAN_SEPARATORS: &[char] = &['_', '-', ' ', '.'];

/// Chiude il difetto 2 della spec (§62.3d): comprime ogni sequenza di due o
/// più caratteri-separatore consecutivi in uno solo, poi rifila
/// separatori orfani ai bordi (un segnaposto vuoto in testa o in coda allo
/// schema lascia un solo separatore isolato, non una run — il rifilo ai
/// bordi lo cattura comunque).
fn collapse_orphan_separators(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if ORPHAN_SEPARATORS.contains(&c) {
            while chars
                .peek()
                .is_some_and(|next| ORPHAN_SEPARATORS.contains(next))
            {
                chars.next();
            }
        }
    }
    out.trim_matches(ORPHAN_SEPARATORS).to_owned()
}

/// Chiude il difetto 4 della spec (§62.3d): tronca `base` così che
/// `base + "." + ESTENSIONE` non superi mai [`MAX_FILENAME_BYTES`] byte —
/// l'estensione non si tronca mai, solo la parte calcolata dallo schema.
/// Rifila di nuovo i separatori orfani dopo il taglio: troncare a metà uno
/// schema può lasciare un separatore esposto in coda (`"nome-"`), lo stesso
/// difetto che [`collapse_orphan_separators`] chiude altrove.
fn cap_length(base: &str, extension: Option<&str>) -> String {
    let ext_len = extension.map_or(0, |ext| 1 + ext.len());
    let budget = MAX_FILENAME_BYTES.saturating_sub(ext_len);
    truncate_to_byte_budget(base, budget)
        .trim_end_matches(ORPHAN_SEPARATORS)
        .to_owned()
}

/// Il prefisso più lungo di `s` che sta in `budget` byte, tagliato su un
/// confine di carattere valido — mai a metà di un carattere UTF-8
/// multi-byte.
fn truncate_to_byte_budget(s: &str, budget: usize) -> &str {
    if s.len() <= budget {
        return s;
    }
    let mut end = budget;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// L'estensione (tutto dopo l'ultimo punto) e lo stem (tutto prima). Nessuna
/// estensione se non c'è alcun punto — non previsto dalla spec (i nomi reali
/// indicizzati da Keeppix ne hanno sempre una), gestito comunque senza
/// panico: lo stem è l'intero nome, nessun punto finale nel risultato.
fn split_extension(filename: &str) -> (&str, Option<&str>) {
    match filename.rsplit_once('.') {
        Some((stem, ext)) => (stem, Some(ext)),
        None => (filename, None),
    }
}

/// Sostituisce i cinque segnaposto testuali e il contatore, lasciando
/// letterale qualunque `{...}` che non corrisponde esattamente a uno dei sei
/// (spec: `{iso}` o `{Data}` maiuscolo "restano nel nome così come sono, non
/// è un errore, non è segnalato"). Scansione lineare invece di un motore di
/// regex: i segnaposto sono un insieme fisso e piccolo, non serve una nuova
/// dipendenza in un crate che oggi non ne ha per il parsing di testo.
fn substitute_placeholders(schema: &str, values: &RenameValues, index: usize) -> String {
    let mut out = String::with_capacity(schema.len());
    let mut rest = schema;
    while let Some(ch) = rest.chars().next() {
        if ch == '{' {
            if let Some((replacement, consumed)) = match_token(rest, values, index) {
                out.push_str(&replacement);
                rest = &rest[consumed..];
                continue;
            }
        }
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    out
}

/// Un solo segnaposto riconosciuto all'inizio di `rest`, con quanti byte
/// consuma — `None` se `rest` inizia per `{` ma non è nessuno dei sei (il
/// chiamante allora copia solo il carattere `{` e prosegue, lasciando il
/// resto letterale carattere per carattere, esattamente come una regex che
/// non trova corrispondenza lascerebbe il testo intatto).
/// Estrae il valore testuale (non ancora slugificato) di un segnaposto da
/// [`RenameValues`] — alias per tenere la tabella di `match_token` sotto il
/// tetto di complessità di tipo di clippy.
type FieldLookup = fn(&RenameValues) -> Option<&str>;

fn match_token(rest: &str, values: &RenameValues, index: usize) -> Option<(String, usize)> {
    const LITERAL: &[(&str, FieldLookup)] = &[
        ("{fotocamera}", |v| v.camera.as_deref()),
        ("{obiettivo}", |v| v.lens.as_deref()),
        ("{luogo}", |v| v.place.as_deref()),
        ("{titolo}", |v| v.title.as_deref()),
    ];
    for (token, field) in LITERAL {
        if rest.starts_with(token) {
            return Some((field(values).map(slug).unwrap_or_default(), token.len()));
        }
    }
    if rest.starts_with("{data}") {
        let date = values
            .date
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        return Some((date, "{data}".len()));
    }
    match_counter(rest, index)
}

/// `\{n(?::(\d+))?\}` — `{n}` senza riempimento, `{n:<cifre>}` con
/// riempimento a zeri. Nessun altro carattere ammesso fra `n` e `}` (una `:`
/// non seguita da cifre, o cifre non seguite da `}`, non è un contatore
/// valido e non consuma nulla: resta letterale, coerente col resto della
/// funzione).
fn match_counter(rest: &str, index: usize) -> Option<(String, usize)> {
    let after_n = rest.strip_prefix("{n")?;
    if after_n.starts_with('}') {
        return Some((index.to_string(), "{n}".len()));
    }
    let digits_and_rest = after_n.strip_prefix(':')?;
    let digit_count = digits_and_rest
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    if digit_count == 0 {
        return None;
    }
    let width: usize = digits_and_rest[..digit_count].parse().ok()?;
    if !digits_and_rest[digit_count..].starts_with('}') {
        return None;
    }
    let consumed = "{n:".len() + digit_count + "}".len();
    Some((format!("{index:0width$}"), consumed))
}

/// `renameSlug` (spec §62.3b): rifila gli spazi ai bordi, elimina `.` e `,`,
/// comprime ogni sequenza di spazi bianchi in un trattino. **Non** applicata
/// a `{data}` — l'ISO `AAAA-MM-GG` non ha nulla da slugificare.
fn slug(value: &str) -> String {
    collapse_whitespace(value.trim().chars().filter(|&c| c != '.' && c != ','), '-')
}

/// Sanificazione finale, applicata all'intera stringa dopo la sostituzione
/// dei segnaposto, non ai singoli valori. Il prototipo (spec §62.3b punto 4)
/// sostituiva solo `/`, `\`, `:`; il Task 7 (spec §62.3d, difetto 3) chiude
/// il resto dell'elenco esplicito della spec — `*`, `?`, `"`, `<`, `>`, `|` —
/// con la stessa regola invece di lasciarli passare. Ogni sequenza di spazi
/// bianchi diventa **un solo spazio** (non un trattino: regola diversa da
/// [`slug`], verificato riga per riga contro la spec); il risultato è
/// rifilato ai bordi.
fn sanitize(value: &str) -> String {
    let replaced = value.chars().map(|c| match c {
        '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
        other => other,
    });
    collapse_whitespace(replaced, ' ').trim().to_owned()
}

/// Comprime ogni sequenza di spazi bianchi consecutivi in un solo
/// `separator` — condivisa da [`slug`] (trattino) e [`sanitize`] (spazio),
/// stessa scansione lineare, unico punto che decide cosa conta come "spazio
/// bianco" per entrambi.
fn collapse_whitespace(chars: impl Iterator<Item = char>, separator: char) -> String {
    let mut out = String::new();
    let mut last_was_space = false;
    for c in chars {
        if c.is_whitespace() {
            if !last_was_space {
                out.push(separator);
                last_was_space = true;
            }
        } else {
            out.push(c);
            last_was_space = false;
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn values() -> RenameValues {
        RenameValues {
            date: Some(NaiveDate::from_ymd_opt(2026, 8, 14).unwrap()),
            camera: Some("Sony A7 IV".to_owned()),
            lens: Some("FE 24-70mm f/2.8".to_owned()),
            place: Some("Val d'Orcia".to_owned()),
            title: Some("Tramonto".to_owned()),
        }
    }

    #[test]
    fn default_schema_matches_the_spec_example() {
        let got = render_filename("{data}_{luogo}_{n:3}", &values(), 1, "DSC08421.arw");
        assert_eq!(got, "2026-08-14_Val-d'Orcia_001.ARW");
    }

    #[test]
    fn extension_is_never_part_of_the_schema_and_is_uppercased() {
        let got = render_filename("{titolo}", &values(), 1, "photo.jpeg");
        assert_eq!(got, "Tramonto.JPEG");
    }

    #[test]
    fn a_malformed_or_unknown_placeholder_stays_literal() {
        let got = render_filename("{iso}-{Data}-{titolo}", &values(), 1, "a.jpg");
        assert_eq!(got, "{iso}-{Data}-Tramonto.JPG");
    }

    #[test]
    fn counter_without_padding_is_the_plain_index() {
        let got = render_filename("{n}", &values(), 7, "a.jpg");
        assert_eq!(got, "7.JPG");
    }

    #[test]
    fn counter_with_padding_zero_fills_to_the_requested_width() {
        let got = render_filename("{n:2}", &values(), 7, "a.jpg");
        assert_eq!(got, "07.JPG");
        let got = render_filename("{n:5}", &values(), 7, "a.jpg");
        assert_eq!(got, "00007.JPG");
    }

    #[test]
    fn counter_can_appear_more_than_once() {
        let got = render_filename("{n:2}-{n:2}", &values(), 3, "a.jpg");
        assert_eq!(got, "03-03.JPG");
    }

    #[test]
    fn missing_value_disappears_without_orphaning_the_separators() {
        // Il prototipo (spec §62.3b) produrrebbe "2026-08-14__001": il
        // Task 7 (spec §62.3d, difetto 2) chiude esattamente questo caso.
        let mut v = values();
        v.place = None;
        let got = render_filename("{data}_{luogo}_{n:3}", &v, 1, "a.jpg");
        assert_eq!(got, "2026-08-14_001.JPG");
    }

    #[test]
    fn a_missing_value_at_the_very_start_leaves_no_leading_separator() {
        let mut v = values();
        v.place = None;
        let got = render_filename("{luogo}_{data}", &v, 1, "a.jpg");
        assert_eq!(got, "2026-08-14.JPG");
    }

    #[test]
    fn mixed_separator_characters_around_a_missing_value_still_collapse() {
        let mut v = values();
        v.place = None;
        let got = render_filename("{data}-_ .{luogo}{n:2}", &v, 5, "a.jpg");
        assert_eq!(got, "2026-08-14-05.JPG");
    }

    #[test]
    fn a_genuine_double_separator_in_the_schema_is_still_collapsed() {
        // Compromesso deliberato (Ruling nel ledger di Fase 9, Task 7): non
        // si distingue un separatore doppio scritto apposta da uno lasciato
        // orfano da un valore mancante — nessuno schema reale ha bisogno di
        // `__` letterale.
        let got = render_filename("IMG__{n:3}", &values(), 1, "a.jpg");
        assert_eq!(got, "IMG_001.JPG");
    }

    #[test]
    fn empty_schema_falls_back_to_the_current_filename_without_extension() {
        let got = render_filename("", &values(), 1, "DSC08421.ARW");
        assert_eq!(got, "DSC08421.ARW");
    }

    #[test]
    fn a_schema_that_sanitizes_to_nothing_also_falls_back() {
        // Solo spazi bianchi: la sanificazione finale li comprime in uno
        // solo e poi rifila i bordi, lasciando la stringa vuota — stesso
        // fallback dello schema vuoto (spec §62.3b punto 5). `/`/`\`/`:`
        // NON producono questo caso: diventano `-`, non spariscono.
        let got = render_filename("   \t  ", &values(), 1, "DSC08421.ARW");
        assert_eq!(got, "DSC08421.ARW");
    }

    #[test]
    fn forbidden_separators_become_a_dash_not_nothing() {
        let got = render_filename("a/b\\c:d", &values(), 1, "DSC08421.ARW");
        assert_eq!(got, "a-b-c-d.ARW");
    }

    #[test]
    fn slug_removes_dots_and_commas_and_collapses_whitespace_to_a_dash() {
        let mut v = values();
        v.place = Some("Toscana,  Val d'Orcia.".to_owned());
        let got = render_filename("{luogo}", &v, 1, "a.jpg");
        assert_eq!(got, "Toscana-Val-d'Orcia.JPG");
    }

    #[test]
    fn date_is_not_slugified() {
        // Se lo fosse, l'ISO AAAA-MM-GG non cambierebbe comunque aspetto:
        // verifica esplicita che il ramo {data} non passi da slug().
        let got = render_filename("{data}", &values(), 1, "a.jpg");
        assert_eq!(got, "2026-08-14.JPG");
    }

    #[test]
    fn forbidden_path_characters_become_a_dash() {
        let mut v = values();
        v.title = Some("A/B\\C:D".to_owned());
        let got = render_filename("{titolo}", &v, 1, "a.jpg");
        assert_eq!(got, "A-B-C-D.JPG");
    }

    #[test]
    fn the_remaining_illegal_filesystem_characters_are_now_sanitized_too() {
        // Spec §62.3d, difetto 3: il prototipo non filtrava *, ?, ", <, >,
        // | oltre a /\: — il Task 7 li chiude con la stessa regola.
        let mut v = values();
        v.title = Some("A*B?C\"D<E>F|G".to_owned());
        let got = render_filename("{titolo}", &v, 1, "a.jpg");
        assert_eq!(got, "A-B-C-D-E-F-G.JPG");
    }

    #[test]
    fn the_result_never_exceeds_the_filesystem_name_limit() {
        let mut v = values();
        v.title = Some("x".repeat(500));
        let got = render_filename("{titolo}", &v, 1, "a.jpg");
        assert!(
            got.len() <= 255,
            "255 byte è NAME_MAX su ext4 e la maggior parte dei filesystem POSIX: {}",
            got.len()
        );
        assert_eq!(
            got.rsplit_once('.').map(|(_, ext)| ext),
            Some("JPG"),
            "l'estensione non si tronca mai: {got}"
        );
    }

    #[test]
    fn truncation_does_not_leave_a_trailing_separator() {
        // Il budget per la base è 251 byte (255 - ".JPG"): un valore che
        // mette un separatore esattamente al byte 251 non deve lasciarlo
        // esposto in coda dopo il taglio.
        let mut v = values();
        v.title = Some(format!("{}_{}", "x".repeat(250), "y".repeat(10)));
        let got = render_filename("{titolo}", &v, 1, "a.jpg");
        assert_eq!(got, format!("{}.JPG", "x".repeat(250)));
    }

    #[test]
    fn a_file_with_no_extension_produces_no_trailing_dot() {
        let got = render_filename("{titolo}", &values(), 1, "README");
        assert_eq!(got, "Tramonto");
    }

    #[test]
    fn render_filename_is_render_base_plus_apply_base_composed() {
        let got_direct = render_filename("{data}_{titolo}_{n:3}", &values(), 4, "DSC08421.arw");
        let base = render_base("{data}_{titolo}_{n:3}", &values(), 4);
        let got_composed = apply_base_to_filename(&base, "DSC08421.arw");
        assert_eq!(got_direct, got_composed);
    }

    #[test]
    fn stack_siblings_share_the_same_base_with_their_own_extension() {
        // Task 8: RAW e JPEG di una stessa pila prendono lo stesso nome —
        // una sola render_base per la pila, un apply_base_to_filename per
        // ciascun file affiancato.
        let base = render_base("{data}_{titolo}_{n:3}", &values(), 1);
        let raw = apply_base_to_filename(&base, "DSC08421.arw");
        let jpeg = apply_base_to_filename(&base, "DSC08421.jpg");
        assert_eq!(raw, "2026-08-14_Tramonto_001.ARW");
        assert_eq!(jpeg, "2026-08-14_Tramonto_001.JPG");
    }

    mod resolve_place_label {
        use super::*;

        #[test]
        fn photo_position_wins_over_everything() {
            let got = resolve_place_label(Some("foto"), Some("cartella"), Some("lotto"));
            assert_eq!(got.as_deref(), Some("foto"));
        }

        #[test]
        fn folder_position_wins_when_the_photo_has_none() {
            let got = resolve_place_label(None, Some("cartella"), Some("lotto"));
            assert_eq!(got.as_deref(), Some("cartella"));
        }

        #[test]
        fn lot_name_is_the_last_resort() {
            let got = resolve_place_label(None, None, Some("lotto"));
            assert_eq!(got.as_deref(), Some("lotto"));
        }

        #[test]
        fn none_of_the_three_present_is_no_place() {
            let got = resolve_place_label(None, None, None);
            assert_eq!(got, None);
        }
    }
}
