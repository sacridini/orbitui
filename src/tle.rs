//! Orbital elements: Celestrak queries, local cache and bundled snapshot.
//!
//! Elements travel as OMM records in JSON rather than as two-line sets:
//! Celestrak only serves TLEs for catalog numbers up to 99999, and newer
//! satellites (Tanager-2 is 100886) have six digits.

use crate::platform;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sgp4::Elements;

const SNAPSHOT: &str = include_str!("../assets/elements_snapshot.json");
const GP_URL: &str = "https://celestrak.org/NORAD/elements/gp.php";
/// Celestrak groups holding most of the catalog; the rest is fetched by id.
pub const GROUPS: [&str; 3] = ["resource", "weather", "stations"];
const CACHE_KEY: &str = "cache/elements.json";
/// Celestrak updates elements every few hours and asks clients not to poll
/// more often than that.
pub const MAX_AGE_HOURS: i64 = 12;
pub const MIN_REFRESH_MINUTES: i64 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Celestrak,
    Cache,
    Snapshot,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Celestrak => "celestrak",
            Source::Cache => "cache",
            Source::Snapshot => "snapshot",
        }
    }
}

pub struct TleSet {
    pub elements: Vec<Elements>,
    pub source: Source,
    /// When the set was downloaded, unknown for the snapshot.
    pub fetched: Option<DateTime<Utc>>,
}

impl TleSet {
    pub fn is_stale(&self) -> bool {
        self.fetched.is_none_or(|t| Utc::now() - t > chrono::Duration::hours(MAX_AGE_HOURS))
    }
}

#[derive(Serialize, Deserialize)]
struct Cache {
    fetched: DateTime<Utc>,
    elements: Vec<Elements>,
}

/// Parses a Celestrak JSON (OMM) answer. "No GP data found" and other
/// non-JSON answers give no elements.
pub fn parse(text: &str) -> Vec<Elements> {
    if !text.trim_start().starts_with('[') {
        return Vec::new();
    }
    // One bad record should not cost the others.
    let records: Vec<serde_json::Value> = serde_json::from_str(text).unwrap_or_default();
    records.into_iter().filter_map(|r| serde_json::from_value(r).ok()).collect()
}

pub fn group_url(group: &str) -> String {
    format!("{GP_URL}?GROUP={group}&FORMAT=json")
}

pub fn catnr_url(id: u64) -> String {
    format!("{GP_URL}?CATNR={id}&FORMAT=json")
}

/// The cached set if there is one, else the snapshot shipped in the binary.
pub fn load_local() -> TleSet {
    if let Some(cache) = platform::load(CACHE_KEY).and_then(|t| serde_json::from_str::<Cache>(&t).ok())
        && !cache.elements.is_empty()
    {
        return TleSet { elements: cache.elements, source: Source::Cache, fetched: Some(cache.fetched) };
    }
    TleSet { elements: parse(SNAPSHOT), source: Source::Snapshot, fetched: None }
}

/// Caches a freshly downloaded set.
pub fn store(elements: Vec<Elements>) -> TleSet {
    let cache = Cache { fetched: Utc::now(), elements };
    if let Ok(text) = serde_json::to_string(&cache) {
        platform::save(CACHE_KEY, &text);
    }
    TleSet { elements: cache.elements, source: Source::Celestrak, fetched: Some(cache.fetched) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;

    #[test]
    fn snapshot_covers_catalog() {
        let elements = parse(SNAPSHOT);
        for sat in catalog::builtin() {
            let e = elements.iter().find(|e| e.norad_id == sat.norad).unwrap_or_else(|| panic!("{}", sat.name));
            assert!(sgp4::Constants::from_elements(e).is_ok(), "{}", sat.name);
        }
    }

    #[test]
    fn parses_omm_and_survives_junk() {
        let text = r#"[{"OBJECT_NAME":"TANAGER-2","OBJECT_ID":"2026-001A","EPOCH":"2026-10-20T12:00:00.000000",
            "MEAN_MOTION":15.2,"ECCENTRICITY":0.0005,"INCLINATION":97.4,"RA_OF_ASC_NODE":10.0,"ARG_OF_PERICENTER":90.0,
            "MEAN_ANOMALY":270.0,"EPHEMERIS_TYPE":0,"CLASSIFICATION_TYPE":"U","NORAD_CAT_ID":100886,"ELEMENT_SET_NO":999,
            "REV_AT_EPOCH":100,"BSTAR":0.0001,"MEAN_MOTION_DOT":0.00001,"MEAN_MOTION_DDOT":0},{"broken":true}]"#;
        let elements = parse(text);
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0].norad_id, 100886);
        assert!(parse("No GP data found").is_empty());
        let cache = Cache { fetched: Utc::now(), elements };
        let again: Cache = serde_json::from_str(&serde_json::to_string(&cache).unwrap()).unwrap();
        assert_eq!(again.elements, cache.elements);
    }
}
