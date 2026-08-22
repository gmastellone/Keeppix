//! Il motore delle formule di rinomina (Fase 9 Task 6, spec UI §62 "Dialog
//! 'Rinomina con formula'"). Puro: nessuna lettura da disco o database — le
//! quattro fasi successive del Gruppo C (Task 7-9) collegano questo motore a
//! collisioni vere, ambiti, e allo spostamento fisico.
//!
//! La logica segue esattamente `computeRenamedFilename`/`renameSlug` del
//! prototipo (spec §62.3b), non un'interpretazione: i sei segnaposto, la
//! sanificazione, e il fallback a schema vuoto sono lo stesso comportamento,
//! bug del prototipo compresi (i separatori orfani intorno a un valore
//! mancante non vengono ripuliti — è il difetto 2 che il Task 7 chiude).

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

/// Ricompone il nome file secondo lo schema (spec §62.3b, punti 1-6).
/// Nessuna validazione bloccante: qualunque schema produce un risultato,
/// mai un errore — la spec è esplicita ("qualunque testo è accettato").
///
/// `index` è 1-based (spec: "l'indice nell'elenco attivo + 1"): chi chiama
/// passa già `posizione_nell_elenco + 1`, non un indice 0-based.
#[must_use]
pub fn render_filename(
    schema: &str,
    values: &RenameValues,
    index: usize,
    current_filename: &str,
) -> String {
    let (stem, extension) = split_extension(current_filename);
    let substituted = substitute_placeholders(schema, values, index);
    let sanitized = sanitize(&substituted);
    let base = if sanitized.is_empty() {
        stem.to_owned()
    } else {
        sanitized
    };
    match extension {
        Some(ext) => format!("{base}.{}", ext.to_uppercase()),
        None => base,
    }
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

/// Sanificazione finale (spec §62.3b punto 4), applicata all'intera stringa
/// dopo la sostituzione dei segnaposto, non ai singoli valori: `/`, `\`, `:`
/// diventano `-`; ogni sequenza di spazi bianchi diventa **un solo spazio**
/// (non un trattino: regola diversa da [`slug`], verificato riga per riga
/// contro la spec); il risultato è rifilato ai bordi. Deliberatamente **non**
/// filtra `*`, `?`, `"`, `<`, `>`, `|` — la spec lo dichiara esplicitamente
/// un limite del prototipo, e chiuderlo è il Task 7, non questo.
fn sanitize(value: &str) -> String {
    let replaced = value.chars().map(|c| match c {
        '/' | '\\' | ':' => '-',
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
    fn missing_value_disappears_but_orphans_the_separators() {
        let mut v = values();
        v.place = None;
        let got = render_filename("{data}_{luogo}_{n:3}", &v, 1, "a.jpg");
        assert_eq!(
            got, "2026-08-14__001.JPG",
            "difetto noto del prototipo (spec §62.3b): due trattini bassi di fila, \
             il Task 7 lo chiude"
        );
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
    fn other_illegal_filesystem_characters_are_deliberately_left_alone() {
        // Spec §62.3d: "non sono filtrati altri caratteri problematici (*,
        // ?, ", <, >, |)" — limite noto del prototipo, chiuso dal Task 7.
        let mut v = values();
        v.title = Some("A*B?C\"D<E>F|G".to_owned());
        let got = render_filename("{titolo}", &v, 1, "a.jpg");
        assert_eq!(got, "A*B?C\"D<E>F|G.JPG");
    }

    #[test]
    fn a_file_with_no_extension_produces_no_trailing_dot() {
        let got = render_filename("{titolo}", &values(), 1, "README");
        assert_eq!(got, "Tramonto");
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
