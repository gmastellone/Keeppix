#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Verifica end-to-end della pipeline "DB prima, file poi" (spec §3.3): un
//! `apply` su `OverrideRepo` deve tradursi, dopo un giro del job
//! `WriteSidecar`, in un `.xmp` reale accanto al file — con il rating del
//! *proprietario* della libreria, non generato da zero se un sidecar
//! (magari di Lightroom) esiste già.

mod harness;

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{
    FlagRepo, JobRepo, LibraryRepo, OverrideRepo, ProblemLanguage, ProblemSeverity, ProblemsRepo,
};
use keeppix_domain::{
    AssetFlags, AssetId, AuthContext, GeoPoint, JobKind, JobPriority, NewLibrary, OverridePatch,
    Pick, Rating, SystemRole,
};
use keeppix_jobs::{discover, xmp as xmp_job};

/// Sidecar prodotto da Lightroom, con un campo di sviluppo (`crs:Exposure2012`)
/// che Keeppix non conosce e non deve mai cancellare rigenerando il file da
/// zero.
const LIGHTROOM_SIDECAR: &str = r#"<?xpacket begin="﻿" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="Adobe XMP Core 6.0-c002">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
   xmp:Rating="2"
   crs:Exposure2012="+0.35">
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>
"#;

struct Seeded {
    root: std::path::PathBuf,
    sidecar: std::path::PathBuf,
    asset_id: AssetId,
}

/// Scrive un file "immagine" (il contenuto non conta: il job non lo legge),
/// lo fa scoprire da `discover::run` così l'asset esiste con un `folder_id`
/// e un `filename` reali, e restituisce dove il job dovrebbe scrivere il
/// sidecar (spec §3.4: accanto al file, `<nome>.xmp`).
async fn seed_asset(test: &TestDb, admin_ctx: &AuthContext, filename: &str) -> Seeded {
    let root = std::env::temp_dir().join(format!("kpx-xmp-job-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).unwrap();
    let path = root.join(filename);
    fs::write(
        &path,
        b"not a real image, content is irrelevant to this job",
    )
    .unwrap();

    let library = LibraryRepo::new(test.db())
        .create(
            admin_ctx,
            NewLibrary {
                name: "Xmp".to_owned(),
                owner_id: admin_ctx.user_id().unwrap(),
                root_path: root.clone(),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();

    let asset_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM assets")
        .fetch_one(test.db().pool())
        .await
        .unwrap();

    let mut sidecar = path.clone().into_os_string();
    sidecar.push(".xmp");

    Seeded {
        root,
        sidecar: sidecar.into(),
        asset_id: AssetId::from_uuid(asset_id),
    }
}

#[tokio::test]
async fn applying_an_override_writes_the_owners_rating_and_pick_to_a_new_sidecar() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let seeded = seed_asset(&test, &ctx, "sunset.jpg").await;

    FlagRepo::new(test.db())
        .set(
            &ctx,
            seeded.asset_id,
            &AssetFlags {
                rating: Some(Rating::parse(4).unwrap()),
                pick: Pick::Pick,
                color_label: None,
                favorite: false,
            },
        )
        .await
        .unwrap();

    let taken_at = Utc.with_ymd_and_hms(2024, 6, 1, 10, 20, 30).unwrap();
    OverrideRepo::new(test.db())
        .apply(
            &ctx,
            seeded.asset_id,
            &OverridePatch {
                title: Some(Some("Tramonto sul lago".to_owned())),
                description: Some(Some("Scattata da Punta San Vigilio".to_owned())),
                taken_at: Some(Some(taken_at)),
                location: Some(Some(GeoPoint {
                    lat: 45.502,
                    lon: 9.259,
                })),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    xmp_job::run(test.db()).await.unwrap();

    assert!(
        seeded.sidecar.is_file(),
        "il job deve creare il sidecar se non esisteva"
    );
    let written = fs::read_to_string(&seeded.sidecar).unwrap();
    assert!(written.contains(r#"xmp:Rating="4""#), "{written}");
    assert!(written.contains(r#"xmp:Label="pick""#), "{written}");
    assert!(written.contains("Tramonto sul lago"), "{written}");
    assert!(
        written.contains("Scattata da Punta San Vigilio"),
        "{written}"
    );
    assert!(written.contains("exif:GPSLatitude"), "{written}");
    assert!(written.contains("exif:DateTimeOriginal"), "{written}");

    let still_pending = OverrideRepo::new(test.db())
        .pending_sidecars(10)
        .await
        .unwrap();
    assert!(
        !still_pending.contains(&seeded.asset_id),
        "dopo la scrittura verificata l'asset non deve più risultare pendente"
    );

    let _ = fs::remove_dir_all(&seeded.root);
}

/// Task 13 (Fase 10): il fallimento deve portare un marcatore stabile
/// (`permission-denied:`) e non un testo libero — è da lì che
/// `ProblemsRepo::composed` riconosce la natura "sidecar XMP non
/// scrivibile" senza dover interpretare il messaggio di `io::Error`.
#[tokio::test]
async fn a_read_only_folder_fails_with_a_stable_permission_denied_marker() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let seeded = seed_asset(&test, &ctx, "blocked.jpg").await;

    OverrideRepo::new(test.db())
        .apply(
            &ctx,
            seeded.asset_id,
            &OverridePatch {
                description: Some(Some("Non dovrebbe mai arrivare al disco".to_owned())),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let mut perms = fs::metadata(&seeded.root).unwrap().permissions();
    perms.set_mode(0o555);
    fs::set_permissions(&seeded.root, perms).expect("chmod sola lettura");

    let result = xmp_job::run(test.db()).await;

    let mut restored = fs::metadata(&seeded.root).unwrap().permissions();
    restored.set_mode(0o755);
    fs::set_permissions(&seeded.root, restored).expect("ripristino permessi");

    let error = result.expect_err("una cartella di sola lettura deve far fallire il job");
    let message = error.to_string();
    assert!(
        message.contains(&seeded.asset_id.to_string()) && message.contains("permission-denied:"),
        "il messaggio deve portare l'id e il marcatore stabile: {message}"
    );

    let _ = fs::remove_dir_all(&seeded.root);
}

/// Verifica richiesta dal brief del Task 13: una cartella resa non
/// scrivibile con `chmod` deve produrre, in `/problems`, un problema
/// composto con la gravità, il testo e l'azione corretti — non solo un
/// `jobs.last_error` grezzo.
#[tokio::test]
async fn a_readonly_folder_surfaces_as_a_composed_sidecar_problem() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let root = std::env::temp_dir().join(format!("kpx-xmp-problem-{}", uuid::Uuid::now_v7()));
    let subdir = root.join("Chioggia");
    fs::create_dir_all(&subdir).unwrap();
    fs::write(subdir.join("blocked.jpg"), b"not a real image").unwrap();

    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Rete".to_owned(),
                owner_id: admin,
                root_path: root.clone(),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();

    let asset_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM assets")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    let asset_id = AssetId::from_uuid(asset_id);

    OverrideRepo::new(test.db())
        .apply(
            &ctx,
            asset_id,
            &OverridePatch {
                description: Some(Some("Non deve mai arrivare al disco".to_owned())),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let jobs = JobRepo::new(test.db());
    let job = jobs
        .enqueue(
            JobKind::WriteSidecar,
            serde_json::json!({}),
            JobPriority::Background,
            None,
        )
        .await
        .unwrap();

    let mut perms = fs::metadata(&subdir).unwrap().permissions();
    perms.set_mode(0o555);
    fs::set_permissions(&subdir, perms).expect("chmod sola lettura");

    let result = xmp_job::run(test.db()).await;

    let mut restored = fs::metadata(&subdir).unwrap().permissions();
    restored.set_mode(0o755);
    fs::set_permissions(&subdir, restored).expect("ripristino permessi");

    let error = result.expect_err("una cartella di sola lettura deve far fallire il job");
    sqlx::query("UPDATE jobs SET status = 'failed', last_error = $2 WHERE id = $1")
        .bind(job.id)
        .bind(error.to_string())
        .execute(test.db().pool())
        .await
        .unwrap();

    let problems = ProblemsRepo::new(test.db())
        .composed(&ctx, ProblemLanguage::It)
        .await
        .unwrap();

    let problem = problems
        .iter()
        .find(|p| p.title.to_lowercase().contains("sidecar"))
        .expect("il fallimento di permessi deve comparire come problema composto");
    assert_eq!(problem.severity, ProblemSeverity::Warning);
    assert!(
        problem.description.contains("Chioggia"),
        "{}",
        problem.description
    );
    assert!(
        problem
            .description
            .to_lowercase()
            .contains("permessi di scrittura"),
        "{}",
        problem.description
    );
    assert!(
        !problem.actions.is_empty(),
        "deve proporre almeno un'azione (\"Vedi i file\"/\"Ignora\")"
    );

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn the_sweep_never_regenerates_an_existing_sidecar_from_scratch() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let seeded = seed_asset(&test, &ctx, "IMG_0001.ARW").await;
    fs::write(&seeded.sidecar, LIGHTROOM_SIDECAR).unwrap();

    // Il proprietario non ha ancora votato: nessun rating da propagare.
    // Solo la descrizione cambia, ma tocca comunque tutto l'asset in
    // `pending_sidecars`.
    OverrideRepo::new(test.db())
        .apply(
            &ctx,
            seeded.asset_id,
            &OverridePatch {
                description: Some(Some("Sviluppata a mano in Lightroom".to_owned())),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    xmp_job::run(test.db()).await.unwrap();

    let written = fs::read_to_string(&seeded.sidecar).unwrap();
    assert!(
        written.contains("crs:Exposure2012=\"+0.35\""),
        "un campo che Keeppix non gestisce deve sopravvivere: {written}"
    );
    assert!(
        written.contains("Sviluppata a mano in Lightroom"),
        "{written}"
    );

    let _ = fs::remove_dir_all(&seeded.root);
}
