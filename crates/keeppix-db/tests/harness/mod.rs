//! Postgres reale per i test di integrazione.
//! Ogni `TestDb` è isolato: database vergine, migrazioni applicate.

use keeppix_db::Db;
use sqlx::{Connection as _, PgConnection};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};

pub struct TestDb {
    // Tenuto vivo: alla deallocazione il container viene fermato. `None`
    // quando i test girano contro un server già esistente.
    _container: Option<ContainerAsync<Postgres>>,
    db: Db,
}

impl TestDb {
    /// # Panics
    /// Se il database non è raggiungibile o le migrazioni falliscono: in un
    /// test è il comportamento voluto.
    #[allow(clippy::expect_used)]
    pub async fn start() -> Self {
        let (container, url) = provision().await;

        let db = Db::connect(&url, 5).await.expect("connessione");
        db.migrate().await.expect("migrazioni");

        Self {
            _container: container,
            db,
        }
    }

    #[must_use]
    pub const fn db(&self) -> &Db {
        &self.db
    }
}

/// Procura un database vergine e restituisce l'eventuale container che lo
/// ospita, da tenere vivo per la durata del test.
///
/// Percorso predefinito: un container usa-e-getta `postgis/postgis:17-3.5`.
/// È ciò che gira in CI e su una macchina di sviluppo normale, e la ragione
/// per cui l'immagine è `PostGIS` è che la Fase 4 aggiungerà colonne
/// geografiche senza dover cambiare immagine.
///
/// Percorso alternativo, attivo **solo** se `KEEPPIX_TEST_DATABASE_URL` è
/// impostata: si usa il server già in ascolto a quell'indirizzo, creando un
/// database vergine per ogni test. Serve negli ambienti dove il registry
/// delle immagini non è raggiungibile — senza questa via d'uscita la suite di
/// integrazione lì non è eseguibile affatto. L'isolamento fra test è lo
/// stesso nei due casi: nessun test vede i dati di un altro.
///
/// I database creati per il secondo percorso **non** vengono eliminati:
/// l'indirizzo deve puntare a un'istanza di scarto. Si ripuliscono con un
/// `DROP DATABASE` sui nomi che iniziano per `keeppix_test_`.
///
/// La stessa logica esiste in `crates/keeppix-api/tests/harness/mod.rs`: i
/// due crate non condividono codice di test, quindi le due copie vanno
/// tenute allineate a mano.
#[allow(clippy::expect_used)]
async fn provision() -> (Option<ContainerAsync<Postgres>>, String) {
    let Ok(server_url) = std::env::var("KEEPPIX_TEST_DATABASE_URL") else {
        let container = Postgres::default()
            .with_tag("17-3.5")
            .with_name("postgis/postgis")
            .start()
            .await
            .expect("avvio del container Postgres");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("porta mappata");
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

        return (Some(container), url);
    };

    // Il nome deriva da un UUID, quindi contiene solo cifre esadecimali e
    // underscore: nessun input esterno finisce nell'identificatore, che non
    // sarebbe parametrizzabile in un `CREATE DATABASE`.
    let name = format!("keeppix_test_{}", uuid::Uuid::now_v7().simple());

    let mut admin = PgConnection::connect(&server_url)
        .await
        .expect("connessione al server Postgres esistente");
    sqlx::query(&format!("CREATE DATABASE \"{name}\""))
        .execute(&mut admin)
        .await
        .expect("creazione del database di test");
    admin.close().await.ok();

    (None, with_database(&server_url, &name))
}

/// Sostituisce il nome del database in un URL di connessione, conservando
/// credenziali, host, porta e parametri di query.
fn with_database(server_url: &str, name: &str) -> String {
    let (base, query) = match server_url.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (server_url, None),
    };

    // Il primo `/` dopo `scheme://` apre il nome del database; senza schema
    // non c'è authority da saltare.
    let authority_start = base.find("://").map_or(0, |i| i + 3);
    let without_db = base[authority_start..]
        .find('/')
        .map_or(base, |i| &base[..authority_start + i]);

    match query {
        Some(query) => format!("{without_db}/{name}?{query}"),
        None => format!("{without_db}/{name}"),
    }
}

/// Questi test girano in ogni binario di integrazione di questo crate: il
/// modulo `harness` è incluso da ciascuno. È chirurgia su stringhe, cioè
/// esattamente il posto dove un refuso passa inosservato finché non fa
/// puntare i test al database sbagliato.
#[cfg(test)]
mod tests {
    use super::with_database;

    #[test]
    fn replaces_an_existing_database_name() {
        assert_eq!(
            with_database("postgres://u:p@127.0.0.1:5432/postgres", "kpx"),
            "postgres://u:p@127.0.0.1:5432/kpx"
        );
    }

    #[test]
    fn appends_when_the_url_has_no_database() {
        assert_eq!(
            with_database("postgres://u:p@127.0.0.1:5432", "kpx"),
            "postgres://u:p@127.0.0.1:5432/kpx"
        );
    }

    #[test]
    fn preserves_the_query_string() {
        assert_eq!(
            with_database(
                "postgres://u:p@127.0.0.1:5432/postgres?sslmode=disable",
                "kpx"
            ),
            "postgres://u:p@127.0.0.1:5432/kpx?sslmode=disable"
        );
    }
}
