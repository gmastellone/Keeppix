//! `PROPFIND` (RFC 4918 §9.1): risponde **dal database**, mai da `stat()`
//! sul filesystem — la stessa tabella `folders`/`assets` che alimenta la
//! timeline ha già nome, dimensione, mtime, hash. Per una libreria con
//! meno di 10.000 file l'intero corpo `multistatus` in un `Vec<u8>` sta
//! comodamente in RAM (qualche MB); la vera streaming a blocchi è
//! un'ottimizzazione per scale molto più grandi, differita finché non
//! serve davvero — vedi ledger del Task 6.

use std::io;

use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use keeppix_db::{AssetRepo, FolderRepo};
use keeppix_domain::{Asset, AssetId, AuthContext, Folder, FolderId};
use quick_xml::events::{BytesDecl, BytesText, Event};
use quick_xml::writer::Writer;

use crate::problem::Problem;
use crate::routes::media::mime_for_name;
use crate::routes::timeline::hex_hash;
use crate::state::AppState;

/// `Depth` richiesto dal client. `infinity` è rifiutato prima di leggere
/// qualunque riga — vedi [`parse_depth`].
#[derive(Debug)]
pub enum Depth {
    Zero,
    One,
}

/// Header `Depth` assente: trattato come `1`, il caso comune di un client
/// che lista una cartella (non è il default RFC 4918, che sarebbe
/// `infinity` — ma rifiutare `infinity` senza offrire un'alternativa
/// sensata all'assenza dell'header renderebbe PROPFIND senza header
/// inutile). Un valore diverso da `0`/`1`/`infinity` è trattato con la
/// stessa tolleranza (`1`): un client `WebDAV` bacato non deve ricevere un
/// errore invece di un elenco.
///
/// # Errors
/// `403 Forbidden` per `Depth: infinity` — costruire l'XML dell'intera
/// libreria in un `Vec<u8>` esploderebbe la RAM su una libreria grande;
/// niente in questo task fa streaming a blocchi (vedi doc del modulo).
pub fn parse_depth(raw: Option<&str>) -> Result<Depth, Problem> {
    match raw.map(str::trim) {
        None | Some("1") => Ok(Depth::One),
        Some("0") => Ok(Depth::Zero),
        Some(value) if value.eq_ignore_ascii_case("infinity") => Err(Problem::new(
            StatusCode::FORBIDDEN,
            "webdav-depth-infinity-unsupported",
            "Depth: infinity is not supported; retry with Depth: 0 or Depth: 1",
        )),
        Some(_) => Ok(Depth::One),
    }
}

struct Entry {
    href: String,
    display_name: String,
    is_collection: bool,
    size: Option<i64>,
    last_modified: String,
    etag: Option<String>,
    content_type: Option<&'static str>,
}

fn folder_entry(folder: &Folder) -> Entry {
    Entry {
        href: format!("/dav/folder/{}/", folder.id),
        display_name: folder.name.clone(),
        is_collection: true,
        size: None,
        // `folders` non porta un mtime nel modello di dominio (solo
        // `created_at` in tabella, non caricato da `FolderRepo`): un client
        // WebDAV usa `getlastmodified` per decidere se un file è cambiato,
        // non una cartella — l'ETag degli asset è la chiave di sync reale
        // (spec §2.3). "adesso" è un valore innocuo: nessun client ne
        // dipende per le cartelle. Vedi ledger Task 6.
        last_modified: http_date(Utc::now()),
        etag: None,
        content_type: None,
    }
}

fn asset_entry(asset: &Asset) -> Entry {
    Entry {
        href: format!("/dav/asset/{}", asset.id),
        display_name: asset.filename.as_str().to_owned(),
        is_collection: false,
        size: Some(asset.size_bytes),
        last_modified: http_date(asset.mtime),
        etag: asset
            .content_hash
            .as_ref()
            .map(|hash| format!("\"{}\"", hex_hash(hash))),
        content_type: Some(mime_for_name(asset.filename.as_str())),
    }
}

fn http_date(at: DateTime<Utc>) -> String {
    at.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

/// # Errors
/// `Forbidden` se il chiamante non vede la cartella — mai una lista vuota:
/// altrimenti l'endpoint diventerebbe un oracolo di esistenza. `Internal`
/// se la codifica XML fallisce (praticamente irraggiungibile scrivendo su
/// un `Vec<u8>`, ma propagato invece di un `unwrap()`).
pub async fn folder(
    state: &AppState,
    ctx: &AuthContext,
    folder_id: FolderId,
    depth: &Depth,
) -> Result<Response, Problem> {
    let folder_repo = FolderRepo::new(&state.db);
    let folder = folder_repo.find_by_id(ctx, folder_id).await?;

    let mut entries = vec![folder_entry(&folder)];
    if matches!(depth, Depth::One) {
        let children = folder_repo.children(ctx, folder_id).await?;
        entries.extend(children.iter().map(folder_entry));

        let assets = AssetRepo::new(&state.db)
            .find_by_folder(ctx, folder_id)
            .await?;
        entries.extend(assets.iter().map(asset_entry));
    }

    multistatus_response(&entries)
}

/// PROPFIND su un singolo asset (un client `WebDAV` che sonda un file prima
/// di un `GET`): un solo `D:response`, indipendente da `Depth` — un file
/// non ha figli.
///
/// # Errors
/// Come [`folder`], sull'asset.
pub async fn asset(
    state: &AppState,
    ctx: &AuthContext,
    asset_id: AssetId,
) -> Result<Response, Problem> {
    let asset = AssetRepo::new(&state.db).find_by_id(ctx, asset_id).await?;
    multistatus_response(&[asset_entry(&asset)])
}

fn multistatus_response(entries: &[Entry]) -> Result<Response, Problem> {
    let body = write_multistatus(entries).map_err(|_| Problem::internal())?;
    let mut response = Body::from(body).into_response();
    *response.status_mut() = StatusCode::MULTI_STATUS;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/xml; charset=\"utf-8\""),
    );
    Ok(response)
}

fn write_multistatus(entries: &[Entry]) -> io::Result<Vec<u8>> {
    let mut writer = Writer::new(Vec::new());
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)))?;
    writer
        .create_element("D:multistatus")
        .with_attribute(("xmlns:D", "DAV:"))
        .write_inner_content(|writer| {
            for entry in entries {
                write_response(writer, entry)?;
            }
            Ok(())
        })?;
    Ok(writer.into_inner())
}

fn write_response(writer: &mut Writer<Vec<u8>>, entry: &Entry) -> io::Result<()> {
    writer
        .create_element("D:response")
        .write_inner_content(|writer| {
            writer
                .create_element("D:href")
                .write_text_content(BytesText::new(&entry.href))?;
            writer
                .create_element("D:propstat")
                .write_inner_content(|writer| {
                    writer
                        .create_element("D:prop")
                        .write_inner_content(|writer| write_prop(writer, entry))?;
                    writer
                        .create_element("D:status")
                        .write_text_content(BytesText::new("HTTP/1.1 200 OK"))?;
                    Ok(())
                })?;
            Ok(())
        })?;
    Ok(())
}

fn write_prop(writer: &mut Writer<Vec<u8>>, entry: &Entry) -> io::Result<()> {
    writer
        .create_element("D:displayname")
        .write_text_content(BytesText::new(&entry.display_name))?;
    if entry.is_collection {
        writer
            .create_element("D:resourcetype")
            .write_inner_content(|writer| {
                writer.create_element("D:collection").write_empty()?;
                Ok(())
            })?;
    } else {
        writer.create_element("D:resourcetype").write_empty()?;
    }
    if let Some(size) = entry.size {
        writer
            .create_element("D:getcontentlength")
            .write_text_content(BytesText::new(&size.to_string()))?;
    }
    writer
        .create_element("D:getlastmodified")
        .write_text_content(BytesText::new(&entry.last_modified))?;
    if let Some(etag) = &entry.etag {
        writer
            .create_element("D:getetag")
            .write_text_content(BytesText::new(etag))?;
    }
    if let Some(content_type) = entry.content_type {
        writer
            .create_element("D:getcontenttype")
            .write_text_content(BytesText::new(content_type))?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn depth_zero_and_one_parse() {
        assert!(matches!(parse_depth(Some("0")), Ok(Depth::Zero)));
        assert!(matches!(parse_depth(Some("1")), Ok(Depth::One)));
    }

    #[test]
    fn missing_depth_defaults_to_one() {
        assert!(matches!(parse_depth(None), Ok(Depth::One)));
    }

    #[test]
    fn depth_infinity_is_rejected() {
        let err = parse_depth(Some("infinity")).unwrap_err();
        assert_eq!(err.status, 403);
    }

    #[test]
    fn depth_infinity_is_case_insensitive() {
        let err = parse_depth(Some("Infinity")).unwrap_err();
        assert_eq!(err.status, 403);
    }

    #[test]
    fn an_unrecognized_depth_value_falls_back_to_one_instead_of_erroring() {
        assert!(matches!(parse_depth(Some("2")), Ok(Depth::One)));
    }

    #[test]
    fn multistatus_body_is_well_formed_and_contains_the_entry() {
        let entries = [Entry {
            href: "/dav/asset/abc".to_owned(),
            display_name: "photo & friends.jpg".to_owned(),
            is_collection: false,
            size: Some(42),
            last_modified: "Mon, 01 Jan 2024 00:00:00 GMT".to_owned(),
            etag: Some("\"deadbeef\"".to_owned()),
            content_type: Some("image/jpeg"),
        }];
        let body = write_multistatus(&entries).expect("xml encoding never fails on a Vec<u8>");
        let text = String::from_utf8(body).expect("valid utf-8");
        assert!(text.starts_with("<?xml version=\"1.0\" encoding=\"utf-8\"?>"));
        assert!(text.contains("<D:multistatus xmlns:D=\"DAV:\">"));
        assert!(text.contains("<D:href>/dav/asset/abc</D:href>"));
        // `&` deve uscire come entità: un client XML non tollera un `&`
        // letterale nel testo di un elemento.
        assert!(text.contains("photo &amp; friends.jpg"));
        // Le virgolette dell'ETag sono escaped come `&quot;` nel testo
        // dell'elemento — XML valido (RFC 4918 §15.6): un client li
        // decodifica di nuovo a `"deadbeef"`, il valore letterale
        // dell'ETag comprese le virgolette.
        assert!(text.contains("<D:getetag>&quot;deadbeef&quot;</D:getetag>"));
        assert!(text.contains("<D:status>HTTP/1.1 200 OK</D:status>"));
    }
}
