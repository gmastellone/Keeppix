use std::io::Write as _;

use keeppix_server::config::{Config, LogFormat};

/// I test manipolano variabili d'ambiente di processo: vanno eseguiti in serie.
/// `cargo test -- --test-threads=1` è imposto dallo script di verifica.
///
/// L'elenco non è scritto a mano. `Config::load` legge `Env::prefixed("KEEPPIX_")`
/// più `DATABASE_URL`, cioè **qualunque** variabile con quel prefisso: la
/// versione precedente ne azzerava quattro delle sette che la configurazione
/// prevede oggi, e ogni campo aggiunto in Fase 1 avrebbe allargato il buco.
/// Derivarlo dall'ambiente invece che da un elenco lo rende impossibile da
/// lasciare indietro.
fn clear_env() {
    let leaked: Vec<String> = std::env::vars()
        .map(|(key, _)| key)
        // `KEEPPIX_TEST_*` non è configurazione del server ma dell'harness
        // (vedi R9): `Config` la ignora come campo sconosciuto, e cancellarla
        // romperebbe i test di integrazione se un giorno finissero in questo
        // stesso binario.
        .filter(|key| {
            (key.starts_with("KEEPPIX_") && !key.starts_with("KEEPPIX_TEST_"))
                || key == "DATABASE_URL"
        })
        .collect();

    for key in leaked {
        unsafe { std::env::remove_var(key) };
    }
}

#[test]
fn database_url_is_required() {
    clear_env();
    assert!(
        Config::load(None).is_err(),
        "senza DATABASE_URL il caricamento fallisce"
    );
}

#[test]
#[allow(clippy::unwrap_used)]
fn defaults_are_applied() {
    clear_env();
    unsafe { std::env::set_var("DATABASE_URL", "postgres://localhost/keeppix") };

    let cfg = Config::load(None).unwrap();
    assert_eq!(cfg.bind.port(), 5673);
    assert_eq!(cfg.data_dir, std::path::PathBuf::from("/data"));
    assert_eq!(cfg.session_ttl_secs, 60 * 60 * 24 * 30);
    assert!(matches!(cfg.log_format, LogFormat::Json));
    assert_eq!(
        cfg.library_roots,
        vec![std::path::PathBuf::from("/photos")],
        "default KEEPPIX_LIBRARY_ROOTS"
    );
    assert_eq!(
        cfg.watch_poll_secs,
        15 * 60,
        "default KEEPPIX_WATCH_POLL_SECS: 15 min, sostenibile su un Pi"
    );
    assert_eq!(
        cfg.webp_quality,
        keeppix_jobs::DEFAULT_WEBP_QUALITY,
        "default KEEPPIX_WEBP_QUALITY: 82, visibile a schermo e piccolo sul Pi"
    );
}

#[test]
#[allow(clippy::unwrap_used)]
fn environment_overrides_the_file() {
    clear_env();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut file = std::fs::File::create(&path).unwrap();
    writeln!(file, "database_url = \"postgres://from-file/keeppix\"").unwrap();
    writeln!(file, "bind = \"0.0.0.0:1111\"").unwrap();

    unsafe { std::env::set_var("KEEPPIX_BIND", "0.0.0.0:2222") };

    let cfg = Config::load(Some(&path)).unwrap();
    assert_eq!(cfg.bind.port(), 2222, "l'ambiente vince sul file");
    assert_eq!(
        cfg.database_url, "postgres://from-file/keeppix",
        "il file vince sul default"
    );
}

#[test]
#[allow(clippy::unwrap_used)]
fn bare_database_url_is_accepted() {
    clear_env();
    unsafe { std::env::set_var("DATABASE_URL", "postgres://bare/keeppix") };
    assert_eq!(
        Config::load(None).unwrap().database_url,
        "postgres://bare/keeppix"
    );
}

#[test]
#[allow(clippy::unwrap_used)]
fn watch_poll_secs_is_overridable() {
    clear_env();
    unsafe { std::env::set_var("DATABASE_URL", "postgres://localhost/keeppix") };
    unsafe { std::env::set_var("KEEPPIX_WATCH_POLL_SECS", "3600") };
    assert_eq!(Config::load(None).unwrap().watch_poll_secs, 3600);
}

#[test]
#[allow(clippy::unwrap_used)]
fn webp_quality_is_overridable() {
    clear_env();
    unsafe { std::env::set_var("DATABASE_URL", "postgres://localhost/keeppix") };
    unsafe { std::env::set_var("KEEPPIX_WEBP_QUALITY", "70") };
    assert_eq!(Config::load(None).unwrap().webp_quality, 70);
}
