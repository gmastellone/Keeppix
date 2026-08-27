//! XMP sidecar: read and write without ever touching the original file.
//!
//! The rule that decides everything: **read, modify the fields Keeppix
//! manages, rewrite** — never generate a sidecar from scratch if one
//! already exists. A file produced by Lightroom or darktable carries
//! fields Keeppix doesn't know about (`crs:Exposure2012`,
//! `darktable:history`, ...): losing them would be worse than not writing
//! at all.
//!
//! Implementation: the whole document is read as a sequence of quick-xml
//! events, the first `rdf:Description` is located (the convention used by
//! Lightroom, darktable, and Bridge: a single block with simple values as
//! attributes and structured properties — `dc:title`, `dc:description`,
//! `dc:subject` — as child elements), and **only** the attributes/elements
//! that appear in the field map below are replaced, leaving everything
//! else — including namespaces that aren't ours — byte-for-byte unchanged.
//! Documents with multiple sibling `rdf:Description` blocks (rare) aren't
//! supported: only the first one found is edited.
//!
//! Field mapping:
//!
//! | Keeppix                   | XMP                                       |
//! |----------------------------|-------------------------------------------|
//! | rating (owner's)           | `xmp:Rating`                               |
//! | description                | `dc:description`                           |
//! | title                      | `dc:title`                                 |
//! | tag                        | `dc:subject`                               |
//! | GPS                        | `exif:GPSLatitude` / `exif:GPSLongitude`   |
//! | taken_at                   | `exif:DateTimeOriginal`                    |
//! | pick/reject                | `xmp:Label` (darktable convention)         |

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write as _;
use std::path::Path;

use chrono::{DateTime, SecondsFormat, Utc};
use keeppix_domain::GeoPoint;
use quick_xml::events::attributes::Attribute;
use quick_xml::events::{BytesEnd, BytesPI, BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum XmpError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed xmp sidecar: {0}")]
    Malformed(String),
    #[error("xmp sidecar verification failed after write: {0}")]
    VerificationFailed(String),
}

/// State that Keeppix syncs onto a `.xmp` sidecar. Every `None` field (or
/// empty for `tags`) means "no value": on write, the corresponding XMP
/// attribute or element is **removed**, not left with a stale value —
/// `SidecarData` represents the full current state, not a patch. Fields
/// Keeppix doesn't know about at all (other namespaces) never pass through
/// here: they survive because [`write_sidecar`] doesn't touch them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SidecarData {
    pub rating: Option<u8>,
    pub description: Option<String>,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub gps: Option<GeoPoint>,
    pub taken_at: Option<DateTime<Utc>>,
    pub label: Option<String>,
}

const NS_X: (&str, &str) = ("xmlns:x", "adobe:ns:meta/");
const NS_RDF: (&str, &str) = ("xmlns:rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#");
const NS_XMP: (&str, &str) = ("xmlns:xmp", "http://ns.adobe.com/xap/1.0/");
const NS_DC: (&str, &str) = ("xmlns:dc", "http://purl.org/dc/elements/1.1/");
const NS_EXIF: (&str, &str) = ("xmlns:exif", "http://ns.adobe.com/exif/1.0/");

const DESCRIPTION_TAG: &[u8] = b"rdf:Description";
const TAG_TITLE: &[u8] = b"dc:title";
const TAG_DESCRIPTION: &[u8] = b"dc:description";
const TAG_SUBJECT: &[u8] = b"dc:subject";
const TAG_ALT: &str = "rdf:Alt";
const TAG_BAG: &str = "rdf:Bag";
const TAG_LI: &str = "rdf:li";

const ATTR_RATING: &str = "xmp:Rating";
const ATTR_LABEL: &str = "xmp:Label";
const ATTR_GPS_LAT: &str = "exif:GPSLatitude";
const ATTR_GPS_LON: &str = "exif:GPSLongitude";
const ATTR_TAKEN_AT: &str = "exif:DateTimeOriginal";

/// Attributes Keeppix fully rewrites on every write. Any other attribute
/// on the same tag — including `xmlns:*` declarations that aren't ours —
/// passes through unchanged.
const MANAGED_ATTRS: [&[u8]; 5] = [
    ATTR_RATING.as_bytes(),
    ATTR_LABEL.as_bytes(),
    ATTR_GPS_LAT.as_bytes(),
    ATTR_GPS_LON.as_bytes(),
    ATTR_TAKEN_AT.as_bytes(),
];

/// Reads an existing `.xmp` sidecar. `Ok(None)` if the file doesn't
/// exist — not having a sidecar is the norm, not an error.
///
/// # Errors
/// I/O (other than "file missing"), or unparsable/non-UTF-8 XML. Never a
/// panic on a corrupt file.
pub fn read_sidecar(path: &Path) -> Result<Option<SidecarData>, XmpError> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let events = parse_events(&bytes)?;
    Ok(Some(extract(&events)))
}

/// Writes `data` into the sidecar at `path`: reads the existing file if
/// there is one (to preserve unmanaged fields), otherwise starts from a
/// minimal skeleton. The write is atomic: `.xmp.tmp` in the same folder,
/// `fsync`, re-read and verify, `rename()`. If any step fails (including a
/// read-only folder), the original file is never touched.
///
/// # Errors
/// `Malformed` if the existing sidecar isn't valid XML or has no
/// recognizable `rdf:Description` — this avoids blindly overwriting a file
/// we can't interpret. `Io` for filesystem failures (permissions, disk
/// full, ...). `VerificationFailed` if re-reading the temp file doesn't
/// match what was written.
pub fn write_sidecar(path: &Path, data: &SidecarData) -> Result<(), XmpError> {
    let mut events = match fs::read(path) {
        Ok(bytes) => parse_events(&bytes)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => default_skeleton(),
        Err(e) => return Err(e.into()),
    };

    apply(&mut events, data)?;

    let mut out = Vec::new();
    {
        let mut writer = Writer::new(&mut out);
        for ev in &events {
            writer.write_event(ev.borrow())?;
        }
    }

    atomic_write(path, &out)
}

// ---------------------------------------------------------------------
// Low-level parsing / serialization
// ---------------------------------------------------------------------

fn parse_events(bytes: &[u8]) -> Result<Vec<Event<'static>>, XmpError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| XmpError::Malformed(format!("not valid utf-8: {e}")))?;
    let mut reader = Reader::from_str(text);
    let mut events = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(ev) => events.push(ev.into_owned()),
            Err(e) => return Err(XmpError::Malformed(e.to_string())),
        }
    }
    Ok(events)
}

fn default_skeleton() -> Vec<Event<'static>> {
    vec![
        Event::PI(BytesPI::new(
            "xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"",
        )),
        Event::Text(BytesText::from_escaped("\n")),
        Event::Start(BytesStart::new("x:xmpmeta").with_attributes([NS_X])),
        Event::Start(BytesStart::new("rdf:RDF").with_attributes([NS_RDF])),
        Event::Empty(BytesStart::new("rdf:Description").with_attributes([("rdf:about", "")])),
        Event::End(BytesEnd::new("rdf:RDF")),
        Event::End(BytesEnd::new("x:xmpmeta")),
        Event::Text(BytesText::from_escaped("\n")),
        Event::PI(BytesPI::new("xpacket end=\"w\"")),
    ]
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), XmpError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("sidecar");
    let tmp = dir.join(format!("{file_name}.{}.tmp", std::process::id()));

    let result = write_and_verify(&tmp, path, bytes);
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// The body of [`atomic_write`], isolated to make it obvious that every
/// failing exit path leaves `path` untouched: no branch touches `path`
/// before the final `rename()`.
fn write_and_verify(tmp: &Path, path: &Path, bytes: &[u8]) -> Result<(), XmpError> {
    {
        let mut file = File::create(tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }

    let verify = fs::read(tmp)?;
    if verify != bytes {
        return Err(XmpError::VerificationFailed(
            "re-read bytes differ from what was written".to_owned(),
        ));
    }
    // Not just the same bytes: they also need to be XMP we can still
    // parse, otherwise the next read would find a file that only looks
    // valid at a glance.
    parse_events(&verify)?;

    fs::rename(tmp, path)?;
    Ok(())
}

// ---------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------

fn extract(events: &[Event<'static>]) -> SidecarData {
    let mut data = SidecarData::default();
    let mut lat: Option<f64> = None;
    let mut lon: Option<f64> = None;

    for ev in events {
        let (Event::Start(e) | Event::Empty(e)) = ev else {
            continue;
        };
        for attr in e.attributes().flatten() {
            let Some(value) = attr_str(&attr) else {
                continue;
            };
            match attr.key.as_ref() {
                b"xmp:Rating" => data.rating = value.trim().parse::<u8>().ok().filter(|r| *r <= 5),
                b"xmp:Label" => {
                    let trimmed = value.trim();
                    if !trimmed.is_empty() {
                        data.label = Some(trimmed.to_owned());
                    }
                }
                b"exif:GPSLatitude" => lat = gps_from_xmp(value.trim(), 'N', 'S'),
                b"exif:GPSLongitude" => lon = gps_from_xmp(value.trim(), 'E', 'W'),
                b"exif:DateTimeOriginal" => data.taken_at = parse_xmp_datetime(value.trim()),
                _ => {}
            }
        }
    }
    if let (Some(lat), Some(lon)) = (lat, lon) {
        data.gps = Some(GeoPoint { lat, lon });
    }

    data.title = find_child_text(events, TAG_TITLE);
    data.description = find_child_text(events, TAG_DESCRIPTION);
    data.tags = find_child_list(events, TAG_SUBJECT);

    data
}

fn attr_str(attr: &Attribute<'_>) -> Option<String> {
    attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)
        .ok()
        .map(std::borrow::Cow::into_owned)
}

/// Index of the event (`End` for a `Start`, itself for an `Empty`) that
/// closes the subtree started at `start`, tracking depth so it doesn't get
/// confused by children with the same name as the parent.
fn subtree_end(events: &[Event<'static>], start: usize) -> usize {
    if !matches!(events[start], Event::Start(_)) {
        return start;
    }
    let mut depth = 1i32;
    let mut i = start + 1;
    while i < events.len() {
        match &events[i] {
            Event::Start(_) => depth += 1,
            Event::End(_) => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    events.len().saturating_sub(1)
}

fn is_tag(ev: &Event<'static>, name: &[u8]) -> bool {
    matches!(ev, Event::Start(s) | Event::Empty(s) if s.name().as_ref() == name)
}

fn find_child_text(events: &[Event<'static>], tag: &[u8]) -> Option<String> {
    let start = events.iter().position(|e| is_tag(e, tag))?;
    let end = subtree_end(events, start);
    text_of_range(&events[start..=end])
}

/// The first non-empty text node in the subtree, whether wrapped in
/// `rdf:Alt`/`rdf:li` (Lightroom, darktable) or written as the element's
/// direct text: Keeppix only needs the value, not the exact structure.
fn text_of_range(events: &[Event<'static>]) -> Option<String> {
    for ev in events {
        if let Event::Text(t) = ev
            && let Ok(s) = t.decode()
        {
            let s = s.trim();
            if !s.is_empty() {
                return Some(s.to_owned());
            }
        }
    }
    None
}

fn find_child_list(events: &[Event<'static>], tag: &[u8]) -> Vec<String> {
    let Some(start) = events.iter().position(|e| is_tag(e, tag)) else {
        return Vec::new();
    };
    let end = subtree_end(events, start);
    let mut out = Vec::new();
    let mut i = start + 1;
    while i < end {
        if is_tag(&events[i], TAG_LI.as_bytes()) {
            let li_end = subtree_end(events, i);
            if let Some(text) = text_of_range(&events[i..=li_end]) {
                out.push(text);
            }
            i = li_end;
        }
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------
// GPS and EXIF datetime — XMP format (DDD,MM.mmmmmm{N|S|E|W})
// ---------------------------------------------------------------------

fn gps_to_xmp(value: f64, positive: char, negative: char) -> String {
    let hemisphere = if value >= 0.0 { positive } else { negative };
    let abs = value.abs();
    let degrees = abs.trunc();
    let minutes = (abs - degrees) * 60.0;
    format!("{degrees:.0},{minutes:.6}{hemisphere}")
}

fn gps_from_xmp(raw: &str, positive: char, negative: char) -> Option<f64> {
    let (num_part, hemisphere) = raw.split_at(raw.len().checked_sub(1)?);
    let hemisphere = hemisphere.chars().next()?;
    let sign = if hemisphere == positive {
        1.0
    } else if hemisphere == negative {
        -1.0
    } else {
        return None;
    };
    let (deg_str, min_str) = num_part.split_once(',')?;
    let degrees: f64 = deg_str.trim().parse().ok()?;
    let minutes: f64 = min_str.trim().parse().ok()?;
    Some(sign * (degrees + minutes / 60.0))
}

fn parse_xmp_datetime(raw: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Some(dt.with_timezone(&Utc));
    }
    chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S")
        .ok()
        .map(|naive| naive.and_utc())
}

// ---------------------------------------------------------------------
// Writing: read, modify the managed fields, rewrite
// ---------------------------------------------------------------------

fn declared_namespaces(events: &[Event<'static>]) -> HashSet<Vec<u8>> {
    let mut set = HashSet::new();
    for ev in events {
        let (Event::Start(e) | Event::Empty(e)) = ev else {
            continue;
        };
        for attr in e.attributes().flatten() {
            if attr.key.as_ref().starts_with(b"xmlns:") {
                set.insert(attr.key.as_ref().to_vec());
            }
        }
    }
    set
}

fn collect_attrs(s: &BytesStart<'static>) -> Vec<(Vec<u8>, Vec<u8>)> {
    s.attributes()
        .flatten()
        .map(|a| (a.key.as_ref().to_vec(), a.value.into_owned()))
        .collect()
}

fn apply(events: &mut Vec<Event<'static>>, data: &SidecarData) -> Result<(), XmpError> {
    let desc_idx = events
        .iter()
        .position(|e| is_tag(e, DESCRIPTION_TAG))
        .ok_or_else(|| XmpError::Malformed("no rdf:Description element found".to_owned()))?;

    let declared = declared_namespaces(events);
    let (was_empty, old_attrs) = match &events[desc_idx] {
        Event::Empty(s) => (true, collect_attrs(s)),
        Event::Start(s) => (false, collect_attrs(s)),
        _ => unreachable!("is_tag only matches Start/Empty"),
    };

    let mut new_start = BytesStart::new("rdf:Description");
    for (key, value) in &old_attrs {
        if MANAGED_ATTRS.contains(&key.as_slice()) {
            continue;
        }
        new_start.push_attribute((key.as_slice(), value.as_slice()));
    }

    let mut needs_xmp_ns = false;
    let mut needs_exif_ns = false;

    if let Some(rating) = data.rating {
        new_start.push_attribute((ATTR_RATING, rating.to_string().as_str()));
        needs_xmp_ns = true;
    }
    if let Some(label) = data.label.as_deref().filter(|s| !s.is_empty()) {
        new_start.push_attribute((ATTR_LABEL, label));
        needs_xmp_ns = true;
    }
    if let Some(gps) = data.gps {
        new_start.push_attribute((ATTR_GPS_LAT, gps_to_xmp(gps.lat, 'N', 'S').as_str()));
        new_start.push_attribute((ATTR_GPS_LON, gps_to_xmp(gps.lon, 'E', 'W').as_str()));
        needs_exif_ns = true;
    }
    if let Some(taken_at) = data.taken_at {
        new_start.push_attribute((
            ATTR_TAKEN_AT,
            taken_at.to_rfc3339_opts(SecondsFormat::Secs, true).as_str(),
        ));
        needs_exif_ns = true;
    }

    if needs_xmp_ns && !declared.contains(NS_XMP.0.as_bytes()) {
        new_start.push_attribute(NS_XMP);
    }
    if needs_exif_ns && !declared.contains(NS_EXIF.0.as_bytes()) {
        new_start.push_attribute(NS_EXIF);
    }
    let needs_dc = data.title.is_some() || data.description.is_some() || !data.tags.is_empty();
    if needs_dc && !declared.contains(NS_DC.0.as_bytes()) {
        new_start.push_attribute(NS_DC);
    }

    let mut desc_end = if was_empty {
        if needs_dc {
            events.splice(
                desc_idx..=desc_idx,
                [
                    Event::Start(new_start),
                    Event::End(BytesEnd::new("rdf:Description")),
                ],
            );
            desc_idx + 1
        } else {
            events[desc_idx] = Event::Empty(new_start);
            desc_idx
        }
    } else {
        let end = subtree_end(events, desc_idx);
        events[desc_idx] = Event::Start(new_start);
        end
    };

    let title = data.title.as_deref().filter(|s| !s.is_empty());
    let description = data.description.as_deref().filter(|s| !s.is_empty());
    set_child(
        events,
        desc_idx,
        &mut desc_end,
        TAG_TITLE,
        title.map(|t| text_element("dc:title", t)),
    );
    set_child(
        events,
        desc_idx,
        &mut desc_end,
        TAG_DESCRIPTION,
        description.map(|d| text_element("dc:description", d)),
    );
    let tags = (!data.tags.is_empty()).then(|| list_element("dc:subject", &data.tags));
    set_child(events, desc_idx, &mut desc_end, TAG_SUBJECT, tags);

    Ok(())
}

/// Removes any existing occurrence of `tag` between `desc_idx` and
/// `desc_end`, then inserts `new_subtree` (if present) right before the
/// closing of `rdf:Description`. `desc_end` is updated to reflect the
/// shifts, so subsequent calls see the correct index.
fn set_child(
    events: &mut Vec<Event<'static>>,
    desc_idx: usize,
    desc_end: &mut usize,
    tag: &[u8],
    new_subtree: Option<Vec<Event<'static>>>,
) {
    if let Some(pos) = (desc_idx + 1..*desc_end).find(|&i| is_tag(&events[i], tag)) {
        let end = subtree_end(events, pos);
        let removed = end - pos + 1;
        events.drain(pos..=end);
        *desc_end -= removed;
    }
    if let Some(subtree) = new_subtree {
        let inserted = subtree.len();
        events.splice(*desc_end..*desc_end, subtree);
        *desc_end += inserted;
    }
}

fn text_element(tag: &'static str, text: &str) -> Vec<Event<'static>> {
    vec![
        Event::Start(BytesStart::new(tag)),
        Event::Start(BytesStart::new(TAG_ALT)),
        Event::Start(BytesStart::new(TAG_LI).with_attributes([("xml:lang", "x-default")])),
        Event::Text(BytesText::new(text).into_owned()),
        Event::End(BytesEnd::new(TAG_LI)),
        Event::End(BytesEnd::new(TAG_ALT)),
        Event::End(BytesEnd::new(tag)),
    ]
}

fn list_element(tag: &'static str, items: &[String]) -> Vec<Event<'static>> {
    let mut events = vec![
        Event::Start(BytesStart::new(tag)),
        Event::Start(BytesStart::new(TAG_BAG)),
    ];
    for item in items {
        events.push(Event::Start(BytesStart::new(TAG_LI)));
        events.push(Event::Text(BytesText::new(item.as_str()).into_owned()));
        events.push(Event::End(BytesEnd::new(TAG_LI)));
    }
    events.push(Event::End(BytesEnd::new(TAG_BAG)));
    events.push(Event::End(BytesEnd::new(tag)));
    events
}
