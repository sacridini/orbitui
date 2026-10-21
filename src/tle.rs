//! Orbital elements: Celestrak queries, local cache and bundled snapshot.

use crate::platform;
use chrono::{DateTime, Utc};
use sgp4::Elements;

const SNAPSHOT: &str = include_str!("../assets/tle_snapshot.txt");
const GP_URL: &str = "https://celestrak.org/NORAD/elements/gp.php";
/// Celestrak groups holding most of the catalog; the rest is fetched by id.
pub const GROUPS: [&str; 3] = ["resource", "weather", "stations"];
const CACHE_KEY: &str = "cache.tle";
const FETCHED_PREFIX: &str = "# fetched ";
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

#[derive(Clone, Debug)]
pub struct Raw {
    pub name: String,
    pub line1: String,
    pub line2: String,
}

impl Raw {
    pub fn norad(&self) -> Option<u64> {
        self.line1.get(2..7)?.trim().parse().ok()
    }

    pub fn elements(&self) -> Option<Elements> {
        Elements::from_tle(Some(self.name.clone()), self.line1.as_bytes(), self.line2.as_bytes()).ok()
    }
}

pub struct TleSet {
    pub raw: Vec<Raw>,
    pub source: Source,
    /// When the set was downloaded, unknown for the snapshot.
    pub fetched: Option<DateTime<Utc>>,
}

impl TleSet {
    pub fn is_stale(&self) -> bool {
        self.fetched.is_none_or(|t| Utc::now() - t > chrono::Duration::hours(MAX_AGE_HOURS))
    }
}

/// Parses TLE text with or without name lines, skipping malformed entries.
pub fn parse(text: &str) -> Vec<Raw> {
    let lines: Vec<&str> = text.lines().map(str::trim_end).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < lines.len() {
        if lines[i].starts_with("1 ") && lines[i + 1].starts_with("2 ") && lines[i].len() >= 7 {
            let name = match i.checked_sub(1).map(|j| lines[j]) {
                Some(prev) if !prev.starts_with("2 ") => prev.trim().to_string(),
                _ => lines[i][2..7].trim().to_string(),
            };
            out.push(Raw { name, line1: lines[i].to_string(), line2: lines[i + 1].to_string() });
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

pub fn group_url(group: &str) -> String {
    format!("{GP_URL}?GROUP={group}&FORMAT=tle")
}

pub fn catnr_url(id: u64) -> String {
    format!("{GP_URL}?CATNR={id}&FORMAT=tle")
}

/// The cached set if there is one, else the snapshot shipped in the binary.
pub fn load_local() -> TleSet {
    if let Some(text) = platform::load(CACHE_KEY) {
        let raw = parse(&text);
        if !raw.is_empty() {
            let fetched = text
                .lines()
                .next()
                .and_then(|l| l.strip_prefix(FETCHED_PREFIX))
                .and_then(|t| DateTime::parse_from_rfc3339(t.trim()).ok())
                .map(|t| t.with_timezone(&Utc));
            return TleSet { raw, source: Source::Cache, fetched };
        }
    }
    TleSet { raw: parse(SNAPSHOT), source: Source::Snapshot, fetched: None }
}

/// Caches a freshly downloaded set.
pub fn store(raw: Vec<Raw>) -> TleSet {
    let now = Utc::now();
    let mut text = format!("{FETCHED_PREFIX}{}\n", now.to_rfc3339());
    for r in &raw {
        text.push_str(&format!("{}\n{}\n{}\n", r.name, r.line1, r.line2));
    }
    platform::save(CACHE_KEY, &text);
    TleSet { raw, source: Source::Celestrak, fetched: Some(now) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;

    #[test]
    fn snapshot_covers_catalog() {
        let raw = parse(SNAPSHOT);
        for sat in catalog::builtin() {
            let r = raw.iter().find(|r| r.norad() == Some(sat.norad)).unwrap_or_else(|| panic!("{}", sat.name));
            assert!(r.elements().is_some(), "{}", sat.name);
        }
    }

    #[test]
    fn parses_two_and_three_line_sets() {
        let l1 = "1 25544U 98067A   26294.12345678  .00010000  00000+0  18000-3 0  9990";
        let l2 = "2 25544  51.6400 100.0000 0005000  90.0000 270.0000 15.50000000400000";
        assert_eq!(parse(&format!("# fetched x\nISS (ZARYA)\n{l1}\n{l2}\n"))[0].name, "ISS (ZARYA)");
        assert_eq!(parse(&format!("{l1}\r\n{l2}\r\n"))[0].name, "25544");
    }
}
