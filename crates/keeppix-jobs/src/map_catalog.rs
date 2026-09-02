//! The 35-country catalog behind the map region search box
//! (`docs/ui/documento-funzionale-ui.md`, "B — Ricerca di regione"): a
//! deliberately wide, wired-in list — "at world scale a chip list becomes
//! unmanageable, better a search field (like cities/countries in Immich
//! or Google Maps)", per that document's own quote of the prototype's
//! reasoning. Bounding boxes are approximate mainland extents (Natural
//! Earth-class accuracy, not surveyed borders) — `pmtiles extract`'s
//! `--bbox` only needs "roughly this rectangle", not a precise polygon;
//! a few km of slack at a coastline costs a handful of extra tiles, not
//! correctness. Sizes are rough estimates for the search result list,
//! before a real size is known — overwritten with the real,
//! post-extraction number once a download actually completes
//! ([`crate::map_extract`]).

/// `(min_lon, min_lat, max_lon, max_lat)`, WGS84 degrees — exactly
/// `pmtiles extract --bbox`'s own argument order.
pub type BBox = (f64, f64, f64, f64);

pub struct RegionCatalogEntry {
    /// Stable, URL/id-safe slug — becomes `map_regions.id`.
    pub id: &'static str,
    /// English name; the frontend's search is a plain substring match
    /// today (`ListPersonsQuery`-style, no i18n on the catalog itself —
    /// same simplification already accepted for tag/album pickers).
    pub label: &'static str,
    pub bbox: BBox,
    /// Rough pre-download estimate shown in the search results list
    /// ("Francia — 480 MB"), replaced by the real size once downloaded.
    pub approx_size_bytes: i64,
}

const MB: i64 = 1_000_000;

/// The three regions `docs/ui/documento-funzionale-ui.md` calls out as
/// pre-seeded/undeletable ("Italia, Europa (resto), Resto del mondo") are
/// **not** modeled here: nothing in this codebase ever seeded them (no
/// migration, no fixture), and inventing bounding boxes for "the rest of
/// Europe" / "the rest of the world" as leftover-after-Italy polygons is
/// a real cartographic problem, not a config value — out of scope for
/// this catalog. Every entry here behaves like the document's own
/// "non-default" regions: added by search, removable freely.
pub const CATALOG: &[RegionCatalogEntry] = &[
    RegionCatalogEntry { id: "france", label: "Francia", bbox: (-5.5, 41.0, 9.7, 51.5), approx_size_bytes: 480 * MB },
    RegionCatalogEntry { id: "germany", label: "Germania", bbox: (5.5, 47.2, 15.5, 55.1), approx_size_bytes: 520 * MB },
    RegionCatalogEntry { id: "spain", label: "Spagna", bbox: (-9.5, 35.9, 4.5, 43.9), approx_size_bytes: 420 * MB },
    RegionCatalogEntry { id: "united-kingdom", label: "Regno Unito", bbox: (-8.7, 49.8, 2.0, 61.0), approx_size_bytes: 380 * MB },
    RegionCatalogEntry { id: "portugal", label: "Portogallo", bbox: (-9.6, 36.8, -6.1, 42.2), approx_size_bytes: 130 * MB },
    RegionCatalogEntry { id: "netherlands", label: "Paesi Bassi", bbox: (3.3, 50.7, 7.3, 53.6), approx_size_bytes: 160 * MB },
    RegionCatalogEntry { id: "belgium", label: "Belgio", bbox: (2.5, 49.4, 6.5, 51.6), approx_size_bytes: 110 * MB },
    RegionCatalogEntry { id: "switzerland", label: "Svizzera", bbox: (5.9, 45.8, 10.6, 47.9), approx_size_bytes: 140 * MB },
    RegionCatalogEntry { id: "austria", label: "Austria", bbox: (9.4, 46.3, 17.3, 49.1), approx_size_bytes: 150 * MB },
    RegionCatalogEntry { id: "greece", label: "Grecia", bbox: (19.3, 34.7, 29.7, 41.8), approx_size_bytes: 220 * MB },
    RegionCatalogEntry { id: "ireland", label: "Irlanda", bbox: (-10.7, 51.3, -5.9, 55.5), approx_size_bytes: 90 * MB },
    RegionCatalogEntry { id: "poland", label: "Polonia", bbox: (14.0, 48.9, 24.3, 55.0), approx_size_bytes: 350 * MB },
    RegionCatalogEntry { id: "sweden", label: "Svezia", bbox: (10.9, 55.2, 24.3, 69.1), approx_size_bytes: 320 * MB },
    RegionCatalogEntry { id: "norway", label: "Norvegia", bbox: (4.5, 57.9, 31.3, 71.3), approx_size_bytes: 340 * MB },
    RegionCatalogEntry { id: "denmark", label: "Danimarca", bbox: (8.0, 54.5, 15.3, 57.9), approx_size_bytes: 90 * MB },
    RegionCatalogEntry { id: "croatia", label: "Croazia", bbox: (13.4, 42.3, 19.5, 46.6), approx_size_bytes: 140 * MB },
    RegionCatalogEntry { id: "united-states", label: "Stati Uniti", bbox: (-125.0, 24.4, -66.8, 49.5), approx_size_bytes: 2_600 * MB },
    RegionCatalogEntry { id: "canada", label: "Canada", bbox: (-141.0, 41.6, -52.6, 83.2), approx_size_bytes: 1_400 * MB },
    RegionCatalogEntry { id: "mexico", label: "Messico", bbox: (-118.5, 14.3, -86.5, 32.8), approx_size_bytes: 500 * MB },
    RegionCatalogEntry { id: "brazil", label: "Brasile", bbox: (-74.0, -33.9, -34.7, 5.3), approx_size_bytes: 900 * MB },
    RegionCatalogEntry { id: "argentina", label: "Argentina", bbox: (-73.6, -55.1, -53.6, -21.7), approx_size_bytes: 420 * MB },
    RegionCatalogEntry { id: "chile", label: "Cile", bbox: (-75.8, -56.0, -66.4, -17.5), approx_size_bytes: 220 * MB },
    RegionCatalogEntry { id: "japan", label: "Giappone", bbox: (122.9, 24.0, 154.0, 45.6), approx_size_bytes: 480 * MB },
    RegionCatalogEntry { id: "china", label: "Cina", bbox: (73.5, 18.1, 135.1, 53.6), approx_size_bytes: 1_800 * MB },
    RegionCatalogEntry { id: "south-korea", label: "Corea del Sud", bbox: (125.0, 33.0, 131.9, 38.7), approx_size_bytes: 160 * MB },
    RegionCatalogEntry { id: "thailand", label: "Thailandia", bbox: (97.3, 5.6, 105.7, 20.5), approx_size_bytes: 260 * MB },
    RegionCatalogEntry { id: "india", label: "India", bbox: (68.1, 6.5, 97.4, 35.7), approx_size_bytes: 950 * MB },
    RegionCatalogEntry { id: "vietnam", label: "Vietnam", bbox: (102.1, 8.2, 109.5, 23.4), approx_size_bytes: 210 * MB },
    RegionCatalogEntry { id: "indonesia", label: "Indonesia", bbox: (94.9, -11.1, 141.0, 6.1), approx_size_bytes: 650 * MB },
    RegionCatalogEntry { id: "australia", label: "Australia", bbox: (112.9, -43.7, 153.7, -10.0), approx_size_bytes: 700 * MB },
    RegionCatalogEntry { id: "new-zealand", label: "Nuova Zelanda", bbox: (166.3, -47.4, 178.6, -34.4), approx_size_bytes: 130 * MB },
    RegionCatalogEntry { id: "morocco", label: "Marocco", bbox: (-13.2, 27.6, -1.0, 35.9), approx_size_bytes: 200 * MB },
    RegionCatalogEntry { id: "egypt", label: "Egitto", bbox: (24.6, 21.9, 36.9, 31.7), approx_size_bytes: 240 * MB },
    RegionCatalogEntry { id: "south-africa", label: "Sudafrica", bbox: (16.3, -34.9, 32.9, -22.1), approx_size_bytes: 380 * MB },
    RegionCatalogEntry { id: "kenya", label: "Kenya", bbox: (33.9, -4.7, 41.9, 5.5), approx_size_bytes: 190 * MB },
];

#[must_use]
pub fn find(id: &str) -> Option<&'static RegionCatalogEntry> {
    CATALOG.iter().find(|entry| entry.id == id)
}

#[cfg(test)]
mod tests {
    use super::CATALOG;

    #[test]
    fn has_exactly_the_documented_thirty_five_countries() {
        assert_eq!(CATALOG.len(), 35);
    }

    #[test]
    fn every_id_is_unique_and_url_safe() {
        let mut ids: Vec<&str> = CATALOG.iter().map(|entry| entry.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len(), "duplicate catalog id");
        for entry in CATALOG {
            assert!(
                entry
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'-'),
                "{} is not a valid map_regions.id slug",
                entry.id
            );
        }
    }

    #[test]
    fn every_bbox_is_a_real_non_empty_rectangle_within_world_bounds() {
        for entry in CATALOG {
            let (min_lon, min_lat, max_lon, max_lat) = entry.bbox;
            assert!(min_lon < max_lon, "{}: min_lon >= max_lon", entry.label);
            assert!(min_lat < max_lat, "{}: min_lat >= max_lat", entry.label);
            assert!((-180.0..=180.0).contains(&min_lon), "{}: min_lon out of range", entry.label);
            assert!((-180.0..=180.0).contains(&max_lon), "{}: max_lon out of range", entry.label);
            assert!((-90.0..=90.0).contains(&min_lat), "{}: min_lat out of range", entry.label);
            assert!((-90.0..=90.0).contains(&max_lat), "{}: max_lat out of range", entry.label);
        }
    }

    #[test]
    fn find_looks_up_by_id_and_is_none_for_an_unknown_one() {
        assert_eq!(super::find("france").map(|e| e.label), Some("Francia"));
        assert!(super::find("atlantis").is_none());
    }
}
