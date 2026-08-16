use chrono::{TimeZone, Utc};
use keeppix_domain::GeoPoint;
use keeppix_media::xmp::{SidecarData, read_sidecar, write_sidecar};

/// Sidecar come lo esporta Lightroom: valori di sviluppo (`crs:*`) che
/// Keeppix non conosce affatto, più un rating che Keeppix *deve* poter
/// aggiornare senza portarsi via il resto.
const LIGHTROOM_SIDECAR: &str = r#"<?xpacket begin="﻿" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="Adobe XMP Core 6.0-c002">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
   xmp:Rating="2"
   crs:Version="15.0"
   crs:ProcessVersion="15.4"
   crs:Exposure2012="+0.35"
   crs:Contrast2012="+12"
   crs:Temperature="5500"
   crs:Tint="+8">
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>
"#;

/// Sidecar come lo scriverebbe darktable: copre tutta la mappatura dei
/// campi (spec §3.4) più un elemento che Keeppix non gestisce affatto
/// (`darktable:xmp_version`, sia come attributo sia come elemento figlio).
const DARKTABLE_SIDECAR: &str = r#"<?xpacket begin="﻿" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="darktable 4.6.1">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:exif="http://ns.adobe.com/exif/1.0/"
    xmlns:darktable="http://darktable.sf.net/"
   xmp:Rating="5"
   xmp:Label="pick"
   exif:GPSLatitude="45,30.1234N"
   exif:GPSLongitude="9,15.5678E"
   exif:DateTimeOriginal="2024-06-01T10:20:30Z"
   darktable:xmp_version="4">
   <dc:title>
    <rdf:Alt>
     <rdf:li xml:lang="x-default">Tramonto sul lago</rdf:li>
    </rdf:Alt>
   </dc:title>
   <dc:description>
    <rdf:Alt>
     <rdf:li xml:lang="x-default">Scattata da Punta San Vigilio</rdf:li>
    </rdf:Alt>
   </dc:description>
   <dc:subject>
    <rdf:Bag>
     <rdf:li>tramonto</rdf:li>
     <rdf:li>lago di garda</rdf:li>
    </rdf:Bag>
   </dc:subject>
   <darktable:history_end>3</darktable:history_end>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>
"#;

#[test]
#[allow(clippy::unwrap_used)]
fn writing_preserves_fields_we_do_not_manage() {
    // Un sidecar prodotto da Lightroom contiene campi che Keeppix non
    // conosce (crs:Exposure2012, crs:Temperature, …). Riscriverlo da zero
    // li cancellerebbe: perdere il lavoro di sviluppo altrui è peggio che
    // non scrivere affatto.
    let dir = tempfile::tempdir().unwrap();
    let sidecar = dir.path().join("IMG_0001.ARW.xmp");
    std::fs::write(&sidecar, LIGHTROOM_SIDECAR).unwrap();

    write_sidecar(
        &sidecar,
        &SidecarData {
            rating: Some(4),
            ..Default::default()
        },
    )
    .unwrap();

    let after = std::fs::read_to_string(&sidecar).unwrap();
    assert!(
        after.contains("crs:Exposure2012"),
        "i campi altrui sopravvivono"
    );
    assert!(
        after.contains("crs:Exposure2012=\"+0.35\""),
        "non solo il nome: anche il valore altrui è intatto"
    );
    assert!(after.contains("crs:Temperature=\"5500\""));
    assert!(
        after.contains("xmp:Rating=\"4\""),
        "il nostro campo è aggiornato"
    );
    assert!(
        !after.contains("xmp:Rating=\"2\""),
        "il vecchio rating non deve restare da nessuna parte"
    );
}

#[test]
#[allow(clippy::unwrap_used)]
fn writing_preserves_child_elements_we_do_not_manage() {
    // Non solo attributi: anche un elemento figlio di un namespace altrui
    // (qui darktable:history_end) deve sopravvivere quando riscriviamo
    // dc:title accanto ad esso.
    let dir = tempfile::tempdir().unwrap();
    let sidecar = dir.path().join("IMG_0002.NEF.xmp");
    std::fs::write(&sidecar, DARKTABLE_SIDECAR).unwrap();

    write_sidecar(
        &sidecar,
        &SidecarData {
            title: Some("Nuovo titolo".to_owned()),
            ..Default::default()
        },
    )
    .unwrap();

    let after = std::fs::read_to_string(&sidecar).unwrap();
    assert!(after.contains("darktable:history_end"));
    assert!(after.contains("3</darktable:history_end>"));
    assert!(after.contains("Nuovo titolo"));
    // Il vecchio titolo non deve restare accanto a quello nuovo.
    assert!(!after.contains("Tramonto sul lago"));
}

#[test]
#[allow(clippy::unwrap_used)]
fn reading_a_real_darktable_sidecar() {
    let dir = tempfile::tempdir().unwrap();
    let sidecar = dir.path().join("sample.ARW.xmp");
    std::fs::write(&sidecar, DARKTABLE_SIDECAR).unwrap();

    let data = read_sidecar(&sidecar).unwrap().unwrap();

    assert_eq!(data.rating, Some(5));
    assert_eq!(data.label.as_deref(), Some("pick"));
    assert_eq!(data.title.as_deref(), Some("Tramonto sul lago"));
    assert_eq!(
        data.description.as_deref(),
        Some("Scattata da Punta San Vigilio")
    );
    assert_eq!(
        data.tags,
        vec!["tramonto".to_owned(), "lago di garda".to_owned()]
    );

    let taken_at = data.taken_at.unwrap();
    assert_eq!(
        taken_at,
        Utc.with_ymd_and_hms(2024, 6, 1, 10, 20, 30).unwrap()
    );

    let gps = data.gps.unwrap();
    assert!(
        (gps.lat - 45.502_056_666_7).abs() < 1e-6,
        "lat = {}",
        gps.lat
    );
    assert!(
        (gps.lon - 9.259_463_333_3).abs() < 1e-6,
        "lon = {}",
        gps.lon
    );
}

#[test]
#[allow(clippy::unwrap_used)]
fn reading_a_missing_sidecar_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope.xmp");
    assert!(read_sidecar(&missing).unwrap().is_none());
}

#[test]
#[allow(clippy::unwrap_used)]
fn a_malformed_sidecar_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let sidecar = dir.path().join("broken.xmp");
    std::fs::write(&sidecar, b"<rdf:Description not even closed").unwrap();

    assert!(read_sidecar(&sidecar).is_err());
}

#[test]
#[allow(clippy::unwrap_used)]
fn invalid_utf8_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let sidecar = dir.path().join("binary.xmp");
    std::fs::write(&sidecar, [0xFF, 0xFE, 0x00, 0x01]).unwrap();

    assert!(read_sidecar(&sidecar).is_err());
}

#[test]
#[allow(clippy::unwrap_used)]
fn writing_to_a_malformed_existing_sidecar_leaves_it_untouched() {
    // Non si rischia di sovrascrivere alla cieca un file che non si sa
    // interpretare: meglio fallire che perdere dati che potrebbero essere
    // recuperabili da un altro strumento.
    let dir = tempfile::tempdir().unwrap();
    let sidecar = dir.path().join("broken.xmp");
    let garbage: &[u8] = b"<rdf:Description not even closed";
    std::fs::write(&sidecar, garbage).unwrap();

    let result = write_sidecar(
        &sidecar,
        &SidecarData {
            rating: Some(3),
            ..Default::default()
        },
    );

    assert!(result.is_err());
    assert_eq!(std::fs::read(&sidecar).unwrap(), garbage);
    assert!(
        std::fs::read_dir(dir.path()).unwrap().count() == 1,
        "nessun file temporaneo deve restare in giro"
    );
}

#[test]
#[allow(clippy::unwrap_used)]
fn write_sidecar_creates_a_new_one_when_none_exists() {
    let dir = tempfile::tempdir().unwrap();
    let sidecar = dir.path().join("fresh.xmp");
    assert!(!sidecar.exists());

    write_sidecar(
        &sidecar,
        &SidecarData {
            rating: Some(3),
            title: Some("Prima foto".to_owned()),
            ..Default::default()
        },
    )
    .unwrap();

    assert!(sidecar.is_file());
    let data = read_sidecar(&sidecar).unwrap().unwrap();
    assert_eq!(data.rating, Some(3));
    assert_eq!(data.title.as_deref(), Some("Prima foto"));
}

#[test]
#[allow(clippy::unwrap_used)]
fn round_trip_preserves_every_field() {
    let dir = tempfile::tempdir().unwrap();
    let sidecar = dir.path().join("round_trip.xmp");

    let taken_at = Utc.with_ymd_and_hms(2024, 3, 15, 9, 30, 0).unwrap();
    let written = SidecarData {
        rating: Some(4),
        description: Some("Una bella descrizione".to_owned()),
        title: Some("Un bel titolo".to_owned()),
        tags: vec![
            "mare".to_owned(),
            "estate".to_owned(),
            "famiglia".to_owned(),
        ],
        gps: Some(GeoPoint {
            lat: 45.5,
            lon: 9.25,
        }),
        taken_at: Some(taken_at),
        label: Some("pick".to_owned()),
    };

    write_sidecar(&sidecar, &written).unwrap();
    let read_back = read_sidecar(&sidecar).unwrap().unwrap();

    assert_eq!(read_back.rating, written.rating);
    assert_eq!(read_back.description, written.description);
    assert_eq!(read_back.title, written.title);
    assert_eq!(read_back.tags, written.tags);
    assert_eq!(read_back.taken_at, written.taken_at);
    assert_eq!(read_back.label, written.label);

    let gps = read_back.gps.unwrap();
    assert!((gps.lat - 45.5).abs() < 1e-9);
    assert!((gps.lon - 9.25).abs() < 1e-9);
}

#[test]
#[allow(clippy::unwrap_used)]
fn writing_default_data_clears_every_managed_field() {
    // `SidecarData` rappresenta lo stato corrente completo, non una patch:
    // un campo `None` deve sparire dal file, non restare con il valore
    // precedente.
    let dir = tempfile::tempdir().unwrap();
    let sidecar = dir.path().join("clear.xmp");
    std::fs::write(&sidecar, DARKTABLE_SIDECAR).unwrap();

    write_sidecar(&sidecar, &SidecarData::default()).unwrap();

    let data = read_sidecar(&sidecar).unwrap().unwrap();
    assert_eq!(data.rating, None);
    assert_eq!(data.label, None);
    assert_eq!(data.gps, None);
    assert_eq!(data.taken_at, None);
    assert_eq!(data.title, None);
    assert_eq!(data.description, None);
    assert!(data.tags.is_empty());

    // Ma i campi altrui restano, esattamente come nel test di scrittura.
    let after = std::fs::read_to_string(&sidecar).unwrap();
    assert!(after.contains("darktable:xmp_version=\"4\""));
    assert!(after.contains("darktable:history_end"));
}

#[test]
#[allow(clippy::unwrap_used)]
fn no_tmp_file_survives_a_successful_write() {
    let dir = tempfile::tempdir().unwrap();
    let sidecar = dir.path().join("clean.xmp");

    write_sidecar(
        &sidecar,
        &SidecarData {
            rating: Some(1),
            ..Default::default()
        },
    )
    .unwrap();

    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(entries, vec![std::ffi::OsString::from("clean.xmp")]);
}

#[cfg(unix)]
#[test]
#[allow(clippy::unwrap_used)]
fn writing_to_a_read_only_directory_fails_without_corrupting_anything() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir().unwrap();
    let sidecar = dir.path().join("readonly.xmp");
    std::fs::write(&sidecar, LIGHTROOM_SIDECAR).unwrap();

    let mut perms = std::fs::metadata(dir.path()).unwrap().permissions();
    perms.set_mode(0o555);
    std::fs::set_permissions(dir.path(), perms).unwrap();

    let result = write_sidecar(
        &sidecar,
        &SidecarData {
            rating: Some(5),
            ..Default::default()
        },
    );

    // Ripristina i permessi prima di ogni assert/panic, altrimenti tempfile
    // non riesce a pulire la cartella alla fine del test.
    let mut restored = std::fs::metadata(dir.path()).unwrap().permissions();
    restored.set_mode(0o755);
    std::fs::set_permissions(dir.path(), restored).unwrap();

    assert!(
        result.is_err(),
        "una cartella in sola lettura è un errore, non un panico"
    );
    assert_eq!(
        std::fs::read_to_string(&sidecar).unwrap(),
        LIGHTROOM_SIDECAR
    );
}

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn blake3_file(path: &std::path::Path) -> blake3::Hash {
    #[allow(clippy::expect_used)]
    {
        blake3::hash(&std::fs::read(path).expect("read"))
    }
}

/// Criterio Fase 2: dopo un ciclo di editing via sidecar, i RAW sono bit-identici.
#[test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
fn writing_sidecars_never_rewrites_real_raw_files() {
    let dir = tempfile::tempdir().unwrap();
    let names = [
        "sample.arw",
        "sample.nef",
        "sample.cr2",
        "sample.cr3",
        "sample.dng",
    ];
    let mut before = Vec::new();
    for name in names {
        let dest = dir.path().join(name);
        std::fs::copy(fixture(name), &dest).unwrap();
        before.push((dest.clone(), blake3_file(&dest)));
    }

    for (path, _) in &before {
        let sidecar = {
            let mut name = path.file_name().unwrap().to_string_lossy().into_owned();
            name.push_str(".xmp");
            path.with_file_name(name)
        };
        write_sidecar(
            &sidecar,
            &SidecarData {
                rating: Some(5),
                title: Some("culling".to_owned()),
                label: Some("pick".to_owned()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(sidecar.is_file());
    }

    for (path, hash) in &before {
        assert_eq!(
            &blake3_file(path),
            hash,
            "RAW riscritto: {}",
            path.display()
        );
    }
}

/// Sessione su ≥1000 file reali in cartella: 995 JPEG (fixture) + 5 RAW.
/// Verifica che i sidecar non tocchino i RAW e che la cartella sia navigabile.
#[test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
fn one_thousand_real_files_survive_sidecar_editing_without_raw_rewrite() {
    const JPEG_COUNT: usize = 995;
    let dir = tempfile::tempdir().unwrap();
    let tiny = std::fs::read(fixture("tiny.jpg")).unwrap();
    for i in 0..JPEG_COUNT {
        std::fs::write(dir.path().join(format!("IMG_{i:05}.jpg")), &tiny).unwrap();
    }
    let raw_names = [
        "sample.arw",
        "sample.nef",
        "sample.cr2",
        "sample.cr3",
        "sample.dng",
    ];
    let mut raw_hashes = Vec::new();
    for name in raw_names {
        let dest = dir.path().join(name);
        std::fs::copy(fixture(name), &dest).unwrap();
        raw_hashes.push((dest.clone(), blake3_file(&dest)));
    }

    let total = std::fs::read_dir(dir.path()).unwrap().count();
    assert_eq!(total, JPEG_COUNT + raw_names.len());

    let started = std::time::Instant::now();
    for i in 0..JPEG_COUNT {
        let sidecar = dir.path().join(format!("IMG_{i:05}.jpg.xmp"));
        write_sidecar(
            &sidecar,
            &SidecarData {
                rating: Some(if i % 2 == 0 { 4 } else { 1 }),
                label: Some(if i % 3 == 0 {
                    "pick".to_owned()
                } else {
                    "reject".to_owned()
                }),
                ..Default::default()
            },
        )
        .unwrap();
    }
    for (path, _) in &raw_hashes {
        let mut name = path.file_name().unwrap().to_string_lossy().into_owned();
        name.push_str(".xmp");
        write_sidecar(
            &path.with_file_name(name),
            &SidecarData {
                rating: Some(5),
                ..Default::default()
            },
        )
        .unwrap();
    }
    let elapsed = started.elapsed();
    eprintln!(
        "MEASUREMENT sidecar write su {} file: {elapsed:?}",
        JPEG_COUNT + raw_names.len()
    );

    for (path, hash) in &raw_hashes {
        assert_eq!(
            &blake3_file(path),
            hash,
            "RAW riscritto: {}",
            path.display()
        );
    }
    assert!(
        elapsed.as_secs() < 30,
        "1000 sidecar devono restare snelli, impiegati {elapsed:?}"
    );
}
