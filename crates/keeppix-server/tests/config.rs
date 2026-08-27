use std::io::Write as _;

use keeppix_server::config::{Config, LogFormat};
use serial_test::serial;

/// The tests manipulate process environment variables: they must run
/// serially with each other, not with the whole suite — every test below
/// has `#[serial]`, so `cargo test -- --test-threads=1` on the whole
/// workspace is no longer needed.
///
/// The list isn't written by hand. `Config::load` reads
/// `Env::prefixed("KEEPPIX_")` plus `DATABASE_URL`, i.e. **any** variable
/// with that prefix: a previous version cleared only four of the seven
/// fields the configuration has today, and every field added later would
/// have widened the hole. Deriving the list from the environment instead of
/// hardcoding it makes it impossible to leave one behind.
fn clear_env() {
    let leaked: Vec<String> = std::env::vars()
        .map(|(key, _)| key)
        // `KEEPPIX_TEST_*` isn't server configuration but harness
        // configuration (see R9): `Config` ignores it as an unknown field,
        // and clearing it would break integration tests if they ever ended
        // up in this same binary.
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
#[serial]
fn database_url_is_required() {
    clear_env();
    assert!(
        Config::load(None).is_err(),
        "loading fails without DATABASE_URL"
    );
}

#[test]
#[serial]
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
        "default KEEPPIX_WATCH_POLL_SECS: 15 min, sustainable on a Pi"
    );
    assert_eq!(
        cfg.webp_quality,
        keeppix_jobs::DEFAULT_WEBP_QUALITY,
        "default KEEPPIX_WEBP_QUALITY: 82, visually acceptable and small on a Pi"
    );
    assert_eq!(
        cfg.webp_method,
        keeppix_jobs::DEFAULT_WEBP_METHOD,
        "default KEEPPIX_WEBP_METHOD: 2, ~2x faster than 4 with nearly the same size"
    );
    assert_eq!(
        cfg.full_cache_bytes,
        keeppix_jobs::DEFAULT_FULL_CACHE_BYTES,
        "default KEEPPIX_FULL_CACHE_BYTES: 512 MiB, one culling session"
    );
    assert_eq!(
        cfg.trash_retention_days,
        keeppix_db::TRASH_RETENTION_DAYS,
        "default KEEPPIX_TRASH_RETENTION_DAYS: 30, the window declared to the user"
    );
    assert_eq!(
        cfg.server_name, "Keeppix",
        "default KEEPPIX_SERVER_NAME: the brand name, unmodified"
    );
}

#[test]
#[serial]
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
    assert_eq!(cfg.bind.port(), 2222, "the environment wins over the file");
    assert_eq!(
        cfg.database_url, "postgres://from-file/keeppix",
        "the file wins over the default"
    );
}

#[test]
#[serial]
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
#[serial]
#[allow(clippy::unwrap_used)]
fn watch_poll_secs_is_overridable() {
    clear_env();
    unsafe { std::env::set_var("DATABASE_URL", "postgres://localhost/keeppix") };
    unsafe { std::env::set_var("KEEPPIX_WATCH_POLL_SECS", "3600") };
    assert_eq!(Config::load(None).unwrap().watch_poll_secs, 3600);
}

#[test]
#[serial]
#[allow(clippy::unwrap_used)]
fn webp_quality_is_overridable() {
    clear_env();
    unsafe { std::env::set_var("DATABASE_URL", "postgres://localhost/keeppix") };
    unsafe { std::env::set_var("KEEPPIX_WEBP_QUALITY", "70") };
    assert_eq!(Config::load(None).unwrap().webp_quality, 70);
}

#[test]
#[serial]
#[allow(clippy::unwrap_used)]
fn webp_method_is_overridable() {
    clear_env();
    unsafe { std::env::set_var("DATABASE_URL", "postgres://localhost/keeppix") };
    unsafe { std::env::set_var("KEEPPIX_WEBP_METHOD", "2") };
    assert_eq!(Config::load(None).unwrap().webp_method, 2);
}

#[test]
#[serial]
#[allow(clippy::unwrap_used)]
fn server_name_is_overridable() {
    clear_env();
    unsafe { std::env::set_var("DATABASE_URL", "postgres://localhost/keeppix") };
    unsafe { std::env::set_var("KEEPPIX_SERVER_NAME", "Casa Mastellone") };
    assert_eq!(Config::load(None).unwrap().server_name, "Casa Mastellone");
}
