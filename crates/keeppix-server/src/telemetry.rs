use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;

use crate::config::LogFormat;

/// Inizializza il logging. `RUST_LOG` sovrascrive il livello predefinito.
pub fn init(format: LogFormat) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tower_http=info"));

    match format {
        LogFormat::Json => fmt().json().with_env_filter(filter).init(),
        LogFormat::Pretty => fmt().pretty().with_env_filter(filter).init(),
    }
}
