//! The rename formula engine. Pure: no reads from disk or database —
//! `crates/keeppix-db/src/rename.rs` connects this engine to real
//! collisions on the database, scopes, and the physical move.
//! [`render_base`]/[`apply_base_to_filename`] exist separately from
//! [`render_filename`] precisely so that side-by-side files in a stack
//! (RAW + JPEG) take the same base, each with its own extension.
//!
//! The six placeholders and the empty-schema fallback follow exactly the
//! `computeRenamedFilename`/`renameSlug` logic of an earlier prototype.
//! **Three of the five defects that prototype had** are closed here, not
//! reproduced: orphan separators around a missing value, illegal filesystem
//! characters beyond the three the prototype already replaced, and no
//! length limit. The other two (collision checked even outside the
//! selected group; the "Apply" button actually disabled) aren't pure
//! logic — the first needs the database (handled in `keeppix-db`), the
//! second is UI behavior.

use chrono::NaiveDate;

/// The values available for a photo, already resolved (not slugified:
/// slugification is this module's responsibility, not the caller's).
/// `place` is already the result of the precedence photo-position →
/// folder-position → lot-name → nothing ([`resolve_place_label`]) —
/// whoever builds this value does it with that function, not by hand.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenameValues {
    pub date: Option<NaiveDate>,
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub place: Option<String>,
    pub title: Option<String>,
}

/// Precedence of the "place" value: it resolves as photo position →
/// folder position → lot name → nothing. Pure: the three candidates
/// already arrive resolved from the caller (which knows how to read photo
/// position, folder position, and lot name from the database) — this
/// function only decides the order, not how to obtain the values.
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

/// The real `NAME_MAX` limit on ext4 and most POSIX filesystems (bytes, not
/// characters) — not an arbitrarily chosen number: it's the filesystem's
/// own constraint. An earlier prototype had no length check at all.
const MAX_FILENAME_BYTES: usize = 255;

/// Rebuilds the filename according to the schema, then applies three fixes
/// an earlier prototype lacked: orphan separators cleaned up, the full set
/// of illegal filesystem characters replaced (not just `/\:`), length
/// never exceeding [`MAX_FILENAME_BYTES`]. No blocking validation: any
/// schema produces a result, never an error — any text is accepted.
///
/// `index` is 1-based: the caller already passes `position_in_list + 1`,
/// not a 0-based index.
///
/// Split into [`render_base`] + [`apply_base_to_filename`] for the case of
/// side-by-side files in a stack taking the same name: the base must be
/// computed **once per stack**, not once per file, then reattached to each
/// side-by-side file's own — different — extension.
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

/// The part computed from the schema (placeholders, counter, orphan
/// separator cleanup, sanitization), **without** an extension — whoever
/// needs to apply the same base to multiple files with different
/// extensions (RAW and JPEG of a stack) calls this once, then
/// [`apply_base_to_filename`] for each file.
#[must_use]
pub fn render_base(schema: &str, values: &RenameValues, index: usize) -> String {
    let substituted = substitute_placeholders(schema, values, index);
    let collapsed = collapse_orphan_separators(&substituted);
    sanitize(&collapsed)
}

/// Applies an already-computed base (from [`render_base`]) to the current
/// name of **one** file: falls back to the current name without extension
/// if the base is empty, truncated to [`MAX_FILENAME_BYTES`], the file's
/// own extension — not another stack member's — reattached in uppercase.
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

/// The characters considered a "separator" for orphan-cleanup purposes —
/// exactly the ones a typical schema uses between placeholders (`_`, `-`,
/// space, `.`). A run of two or more, in any combination, is by
/// construction the effect of a missing value between two adjacent
/// placeholders, never the user's intent: no real schema needs a literal
/// `__`. — *Cost if wrong:* a schema that deliberately contained two
/// separators in a row would see them collapsed to one; no real case
/// observed that needs this.
const ORPHAN_SEPARATORS: &[char] = &['_', '-', ' ', '.'];

/// Collapses every run of two or more consecutive separator characters
/// into one, then trims orphan separators at the edges (an empty
/// placeholder at the start or end of the schema leaves a single isolated
/// separator, not a run — trimming the edges catches it anyway).
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

/// Truncates `base` so that `base + "." + EXTENSION` never exceeds
/// [`MAX_FILENAME_BYTES`] bytes — the extension is never truncated, only
/// the schema-computed part. Trims orphan separators again after
/// truncation: cutting a schema mid-way can leave a trailing separator
/// exposed (`"name-"`), the same defect [`collapse_orphan_separators`]
/// closes elsewhere.
fn cap_length(base: &str, extension: Option<&str>) -> String {
    let ext_len = extension.map_or(0, |ext| 1 + ext.len());
    let budget = MAX_FILENAME_BYTES.saturating_sub(ext_len);
    truncate_to_byte_budget(base, budget)
        .trim_end_matches(ORPHAN_SEPARATORS)
        .to_owned()
}

/// The longest prefix of `s` that fits within `budget` bytes, cut on a
/// valid character boundary — never in the middle of a multi-byte UTF-8
/// character.
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

/// The extension (everything after the last dot) and the stem (everything
/// before). No extension if there's no dot at all — real filenames indexed
/// by Keeppix always have one, but this is handled without panicking
/// anyway: the stem is the whole name, no trailing dot in the result.
fn split_extension(filename: &str) -> (&str, Option<&str>) {
    match filename.rsplit_once('.') {
        Some((stem, ext)) => (stem, Some(ext)),
        None => (filename, None),
    }
}

/// Substitutes the five text placeholders and the counter, leaving any
/// `{...}` that doesn't exactly match one of the six as a literal (e.g.
/// `{iso}` or capitalized `{Data}` stay in the name exactly as written —
/// not an error, not flagged). Linear scan instead of a regex engine: the
/// placeholders are a small, fixed set, no need for a new dependency in a
/// crate that doesn't otherwise parse text today.
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

/// A single placeholder recognized at the start of `rest`, along with how
/// many bytes it consumes — `None` if `rest` starts with `{` but isn't one
/// of the six (the caller then copies just the `{` character and moves on,
/// leaving the rest of the text literal character by character, exactly
/// like a regex that finds no match would leave the text untouched).
/// Extracts the (not yet slugified) text value of a placeholder from
/// [`RenameValues`] — an alias to keep `match_token`'s table under
/// clippy's type-complexity ceiling.
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

/// `\{n(?::(\d+))?\}` — `{n}` with no padding, `{n:<digits>}` with
/// zero-padding. No other character allowed between `n` and `}` (a `:` not
/// followed by digits, or digits not followed by `}`, is not a valid
/// counter and consumes nothing: it stays literal, consistent with the
/// rest of the function).
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

/// `renameSlug`: trims whitespace at the edges, removes `.` and `,`,
/// collapses every run of whitespace into a dash. **Not** applied to
/// `{data}` — the ISO `YYYY-MM-DD` has nothing to slugify.
fn slug(value: &str) -> String {
    collapse_whitespace(value.trim().chars().filter(|&c| c != '.' && c != ','), '-')
}

/// Final sanitization, applied to the whole string after placeholder
/// substitution, not to individual values. An earlier prototype only
/// replaced `/`, `\`, `:`; this closes the rest of the explicit list of
/// illegal characters — `*`, `?`, `"`, `<`, `>`, `|` — with the same rule
/// instead of letting them through. Every run of whitespace becomes **a
/// single space** (not a dash: a different rule from [`slug`]); the result
/// is trimmed at the edges.
fn sanitize(value: &str) -> String {
    let replaced = value.chars().map(|c| match c {
        '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
        other => other,
    });
    collapse_whitespace(replaced, ' ').trim().to_owned()
}

/// Collapses every run of consecutive whitespace into a single
/// `separator` — shared by [`slug`] (dash) and [`sanitize`] (space), the
/// same linear scan, the single place that decides what counts as
/// "whitespace" for both.
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
        // An earlier prototype would produce "2026-08-14__001": this
        // closes exactly this case.
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
        // Deliberate trade-off: a double separator written on purpose
        // isn't distinguished from one left orphaned by a missing value —
        // no real schema needs a literal `__`.
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
        // Only whitespace: the final sanitization collapses it into one
        // and then trims the edges, leaving an empty string — same
        // fallback as an empty schema. `/`/`\`/`:` do NOT produce this
        // case: they become `-`, they don't disappear.
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
        // Even if it were, the ISO YYYY-MM-DD wouldn't change appearance
        // anyway: this explicitly verifies that the {data} branch doesn't
        // go through slug().
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
        // An earlier prototype didn't filter *, ?, ", <, >, | beyond /\: —
        // this closes them with the same rule.
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
            "255 bytes is NAME_MAX on ext4 and most POSIX filesystems: {}",
            got.len()
        );
        assert_eq!(
            got.rsplit_once('.').map(|(_, ext)| ext),
            Some("JPG"),
            "the extension is never truncated: {got}"
        );
    }

    #[test]
    fn truncation_does_not_leave_a_trailing_separator() {
        // The budget for the base is 251 bytes (255 - ".JPG"): a value
        // that places a separator exactly at byte 251 must not leave it
        // exposed at the end after truncation.
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
        // RAW and JPEG of the same stack take the same name — one
        // render_base call for the stack, one apply_base_to_filename call
        // for each side-by-side file.
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
