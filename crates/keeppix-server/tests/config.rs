use std::io::Write as _;

use keeppix_server::config::{Config, LogFormat};

/// I test manipolano variabili d'ambiente di processo: vanno eseguiti in serie.
/// `cargo test -- --test-threads=1` è imposto dallo script di verifica.
fn clear_env() {
    for key in [
        "DATABASE_URL",
        "KEEPPIX_BIND",
        "KEEPPIX_DATA_DIR",
        "KEEPPIX_LOG_FORMAT",
    ] {
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
