//! Mission acquisition plans: ESA's KML files for Sentinel-1 and Sentinel-2.
//!
//! A plan lists the segments each satellite will acquire over the coming
//! weeks, with their times and ground polygons. A predicted overflight is
//! "planned" when a recording segment of the same satellite covers the AOI
//! at that time, and "not planned" when the plan covers the period but no
//! segment does. Outside every loaded plan the status is unknown.
//!
//! The terminal downloads the current plans from the Sentinel pages; ESA
//! does not allow cross-origin requests, so on the web the KML files are
//! dropped on the page.

use crate::aoi;
use crate::orbit::Observer;
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use quick_xml::Reader;
use quick_xml::events::Event;

/// Pages listing the plans, by mission.
pub const PAGES: [&str; 2] = [
    "https://sentinels.copernicus.eu/web/sentinel/copernicus/sentinel-2/acquisition-plans",
    "https://sentinels.copernicus.eu/web/sentinel/copernicus/sentinel-1/acquisition-plans",
];
const SITE: &str = "https://sentinels.copernicus.eu";
/// Satellites with plans, as in the catalog's short labels.
pub const SATELLITES: [&str; 6] = ["S1A", "S1C", "S1D", "S2A", "S2B", "S2C"];
/// Slack between a predicted closest approach and a segment's times.
const SLACK_S: i64 = 120;

#[derive(Clone, Debug)]
pub struct Segment {
    pub sat: String,
    pub begin: DateTime<Utc>,
    pub end: DateTime<Utc>,
    /// IW, EW, NOBS…
    pub mode: String,
    /// Sentinel-2 segments styled NOT_RECORDING are observed but not kept.
    pub recording: bool,
    /// Rings as (lat, lon).
    pub rings: Vec<Vec<(f64, f64)>>,
    bbox: [f64; 4],
}

impl Segment {
    fn contains(&self, (lat, lon): (f64, f64)) -> bool {
        let [lat0, lon0, lat1, lon1] = self.bbox;
        lat >= lat0 && lat <= lat1 && lon >= lon0 && lon <= lon1 && aoi::in_rings(&self.rings, lat, lon)
    }
}

#[derive(Clone, Debug)]
pub struct PlanFile {
    pub name: String,
    pub sat: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub segments: Vec<Segment>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Planned(String),
    NotPlanned,
    Unknown,
}

impl Status {
    /// "planned IW", "not planned", or nothing without a plan.
    pub fn label(&self) -> String {
        match self {
            Status::Planned(mode) => format!("planned {mode}").trim_end().to_string(),
            Status::NotPlanned => "not planned".into(),
            Status::Unknown => String::new(),
        }
    }
}

#[derive(Clone, Default)]
pub struct Plans {
    pub files: Vec<PlanFile>,
}

impl Plans {
    /// Adds a plan, replacing one with the same name.
    pub fn add(&mut self, file: PlanFile) {
        self.files.retain(|f| f.name != file.name);
        self.files.push(file);
    }

    /// Whether `sat` plans to acquire any of `points` around `t`, by the
    /// most recent plan covering `t`.
    pub fn status(&self, sat: &str, t: DateTime<Utc>, points: &[(f64, f64)]) -> Status {
        let Some(file) = self
            .files
            .iter()
            .filter(|f| f.sat.eq_ignore_ascii_case(sat) && f.start <= t && t <= f.end)
            .max_by_key(|f| f.start)
        else {
            return Status::Unknown;
        };
        let slack = Duration::seconds(SLACK_S);
        file.segments
            .iter()
            .filter(|s| s.recording && s.begin - slack <= t && t <= s.end + slack)
            .find(|s| points.iter().any(|&p| s.contains(p)))
            .map_or(Status::NotPlanned, |s| Status::Planned(s.mode.clone()))
    }

    /// `status` over the area of interest (its samples, or its point).
    pub fn status_for(&self, sat: &str, t: DateTime<Utc>, obs: &Observer) -> Status {
        match &obs.area {
            Some(area) => self.status(sat, t, &area.samples),
            None => self.status(sat, t, &[(obs.lat, obs.lon)]),
        }
    }

    /// Satellites whose loaded plans cover `[from, to]` entirely.
    pub fn covers(&self, sat: &str, from: DateTime<Utc>, to: DateTime<Utc>) -> bool {
        self.files.iter().any(|f| f.sat.eq_ignore_ascii_case(sat) && f.start <= from && f.end >= to)
    }
}

/// The satellite named in an ESA file name ("s2a_mp_acq__kml_…").
pub fn sat_from_name(name: &str) -> Option<String> {
    let lower = name.rsplit(['/', '\\']).next().unwrap_or(name).to_ascii_lowercase();
    SATELLITES.iter().find(|s| lower.starts_with(&s.to_ascii_lowercase())).map(|s| s.to_string())
}

fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim().trim_end_matches('Z');
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").ok().map(|t| t.and_utc())
}

fn parse_coordinates(text: &str) -> Vec<(f64, f64)> {
    text.split_whitespace()
        .filter_map(|p| {
            let mut it = p.split(',');
            let lon: f64 = it.next()?.parse().ok()?;
            let lat: f64 = it.next()?.parse().ok()?;
            Some((lat, lon))
        })
        .collect()
}

/// Reads an ESA plan. The satellite comes from each placemark (Sentinel-1)
/// or from the file name (Sentinel-2).
pub fn parse_kml(text: &str, file_name: &str) -> Result<PlanFile, String> {
    let file_sat = sat_from_name(file_name);
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);

    #[derive(Default)]
    struct Mark {
        begin: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
        style: String,
        data: Vec<(String, String)>,
        rings: Vec<Vec<(f64, f64)>>,
    }
    let mut segments = Vec::new();
    let mut mark: Option<Mark> = None;
    let mut element = String::new();
    let mut data_name = String::new();
    loop {
        match reader.read_event().map_err(|e| format!("{file_name}: {e}"))? {
            Event::Start(e) => {
                element = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                match element.as_str() {
                    "Placemark" => mark = Some(Mark::default()),
                    "Data" => {
                        data_name = e
                            .try_get_attribute("name")
                            .ok()
                            .flatten()
                            .map(|a| String::from_utf8_lossy(&a.value).into_owned())
                            .unwrap_or_default();
                    }
                    _ => {}
                }
            }
            Event::Text(t) => {
                let Some(m) = mark.as_mut() else { continue };
                let text = t.decode().map_err(|e| e.to_string())?;
                match element.as_str() {
                    "begin" => m.begin = parse_time(&text),
                    "end" => m.end = parse_time(&text),
                    "styleUrl" => m.style = text.trim_start_matches('#').to_string(),
                    "value" => m.data.push((data_name.clone(), text.into_owned())),
                    "coordinates" => m.rings.push(parse_coordinates(&text)),
                    _ => {}
                }
            }
            Event::End(e) => {
                if e.local_name().as_ref() == b"Placemark"
                    && let Some(m) = mark.take()
                {
                    let get = |k: &str| m.data.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
                    let begin = m.begin.or_else(|| get("ObservationTimeStart").and_then(parse_time));
                    let end = m.end.or_else(|| get("ObservationTimeStop").and_then(parse_time));
                    let sat = get("SatelliteId").map(str::to_string).or_else(|| file_sat.clone());
                    let rings: Vec<_> = m.rings.into_iter().filter(|r| r.len() >= 3).collect();
                    if let (Some(begin), Some(end), Some(sat)) = (begin, end, sat)
                        && !rings.is_empty()
                    {
                        let mut bbox = [90.0f64, 180.0f64, -90.0f64, -180.0f64];
                        for &(lat, lon) in rings.iter().flatten() {
                            bbox = [bbox[0].min(lat), bbox[1].min(lon), bbox[2].max(lat), bbox[3].max(lon)];
                        }
                        let mode = get("Mode").filter(|m| !m.is_empty()).unwrap_or(&m.style).to_string();
                        let mode = match get("Polarisation") {
                            Some(p) if p != "NA" => format!("{mode} {p}"),
                            _ => mode,
                        };
                        segments.push(Segment {
                            sat,
                            begin,
                            end,
                            mode,
                            recording: !m.style.contains("NOT_RECORDING"),
                            rings,
                            bbox,
                        });
                    }
                }
                element.clear();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if segments.is_empty() {
        return Err(format!("{file_name}: no acquisition segments (is it an ESA plan?)"));
    }
    let sat = file_sat.unwrap_or_else(|| segments[0].sat.clone());
    let start = segments.iter().map(|s| s.begin).min().unwrap_or_default();
    let end = segments.iter().map(|s| s.end).max().unwrap_or_default();
    Ok(PlanFile { name: file_name.rsplit(['/', '\\']).next().unwrap_or(file_name).to_string(), sat, start, end, segments })
}

/// A plan published on the Sentinel pages.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub url: String,
    pub name: String,
    pub sat: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Plan links on a Sentinel acquisition plans page.
pub fn links(page: &str) -> Vec<Link> {
    let mut out: Vec<Link> = Vec::new();
    for chunk in page.split("href=\"").skip(1) {
        let Some(href) = chunk.split('"').next() else { continue };
        let Some(name) = href.rsplit('/').next() else { continue };
        let Some(sat) = sat_from_name(name) else { continue };
        if !name.contains("_mp_") || !href.contains("/documents/") {
            continue;
        }
        // Two timestamps: ..._20261015t150000_20261102t180000[-uuid]
        let stamps: Vec<DateTime<Utc>> = name
            .split(['_', '-'])
            .filter(|p| p.len() == 15 && p.as_bytes()[8].eq_ignore_ascii_case(&b't'))
            .filter_map(|p| NaiveDateTime::parse_from_str(&p.to_ascii_uppercase(), "%Y%m%dT%H%M%S").ok())
            .map(|t| t.and_utc())
            .collect();
        let [start, end] = stamps[..] else { continue };
        let url = if href.starts_with("http") { href.to_string() } else { format!("{SITE}{href}") };
        if !out.iter().any(|l| l.url == url) {
            out.push(Link { url, name: name.to_string(), sat, start, end });
        }
    }
    out
}

/// For each satellite, the most recent plan in effect at `from`, and the
/// newest one if it starts before `to` (the next fortnight's, or a fresher
/// update).
pub fn pick(links: &[Link], from: DateTime<Utc>, to: DateTime<Utc>, sats: &[&str]) -> Vec<Link> {
    let mut out = Vec::new();
    for sat in sats {
        let mine: Vec<&Link> = links.iter().filter(|l| l.sat == *sat && l.end > from).collect();
        let current = mine.iter().filter(|l| l.start <= from).max_by_key(|l| l.start);
        let newest = mine.iter().filter(|l| l.start <= to).max_by_key(|l| l.start);
        for link in [current, newest].into_iter().flatten() {
            if !out.iter().any(|l: &Link| l.url == link.url) {
                out.push((*link).clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const S1: &str = r#"<?xml version="1.0" encoding="iso-8859-1"?>
<kml xmlns="http://www.opengis.net/kml/2.2"><Document><Folder><Placemark>
  <name>2026-10-20T18:25:12</name>
  <TimeSpan><begin>2026-10-20T18:25:12</begin><end>2026-10-20T18:27:11</end></TimeSpan>
  <styleUrl>IW</styleUrl>
  <ExtendedData>
    <Data name="SatelliteId"><value>S1D</value></Data>
    <Data name="Mode"><value>IW</value></Data>
    <Data name="Polarisation"><value>DV</value></Data>
  </ExtendedData>
  <LinearRing><coordinates>-48,-17,0 -47,-17,0 -47,-15,0 -48,-15,0 -48,-17,0</coordinates></LinearRing>
</Placemark></Folder></Document></kml>"#;

    const S2: &str = r#"<kml><Document><Placemark><name>59096-1</name>
  <TimeSpan><begin>2026-10-15T13:20:00.500</begin><end>2026-10-15T13:24:00.000</end></TimeSpan>
  <styleUrl>NOBS_NOMINAL</styleUrl>
  <ExtendedData><Data name="Mode"><value>NOBS</value></Data></ExtendedData>
  <Polygon><outerBoundaryIs><LinearRing><coordinates>10,0,0 12,0,0 12,2,0 10,2,0 10,0,0</coordinates></LinearRing></outerBoundaryIs></Polygon>
</Placemark></Document></kml>"#;

    #[test]
    fn reads_sentinel_1_and_2_plans() {
        let s1 = parse_kml(S1, "s1d_mp_user_20261020t182319_20261109t200700.kml").unwrap();
        assert_eq!((s1.sat.as_str(), s1.segments[0].mode.as_str()), ("S1D", "IW DV"));
        let s2 = parse_kml(S2, "s2a_mp_acq__kml_20261015t150000_20261102t180000").unwrap();
        assert_eq!(s2.sat, "S2A");
        assert_eq!(s2.segments[0].begin, Utc.with_ymd_and_hms(2026, 10, 15, 13, 20, 0).unwrap() + Duration::milliseconds(500));

        let mut plans = Plans::default();
        plans.add(s1);
        let at = Utc.with_ymd_and_hms(2026, 10, 20, 18, 26, 0).unwrap();
        assert_eq!(plans.status("S1D", at, &[(-16.0, -47.5)]), Status::Planned("IW DV".into()));
        assert_eq!(plans.status("S1D", at, &[(-10.0, -47.5)]), Status::NotPlanned);
        assert_eq!(plans.status("S1D", at + Duration::days(1), &[(-16.0, -47.5)]), Status::Unknown);
        assert_eq!(plans.status("S1A", at, &[(-16.0, -47.5)]), Status::Unknown);
    }

    #[test]
    fn finds_and_picks_links() {
        let page = r#"<a href="/documents/d/sentinel/s2a_mp_acq__kml_20261015t150000_20261102t180000">x</a>
            <a href="/documents/d/sentinel/s2a_mp_acq__kml_20261001t150000_20261019t180000">x</a>
            <a href="/documents/d/sentinel/s2b_mp_acq__kml_20261022t120000_20261109t150000-9cdd218e-a8a5">x</a>
            <a href="https://sentinels.copernicus.eu/documents/d/sentinel/s1d_mp_user_20261019t190903_20261025t210300">x</a>
            <a href="/web/other">x</a>"#;
        let links = links(page);
        assert_eq!(links.len(), 4);
        assert!(links[0].url.starts_with("https://sentinels.copernicus.eu/documents/"));
        let now = Utc.with_ymd_and_hms(2026, 10, 21, 0, 0, 0).unwrap();
        let picked = pick(&links, now, now + Duration::days(3), &["S2A", "S2B", "S1D", "S2C"]);
        let names: Vec<&str> = picked.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "s2a_mp_acq__kml_20261015t150000_20261102t180000",
                "s2b_mp_acq__kml_20261022t120000_20261109t150000-9cdd218e-a8a5",
                "s1d_mp_user_20261019t190903_20261025t210300"
            ]
        );
    }
}
