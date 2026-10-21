//! The agenda as CSV (for spreadsheets and scripts) and iCalendar (for
//! calendar apps).

use crate::app::App;
use crate::predict::Acquisition;
use chrono::{DateTime, Duration, Utc};

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

fn opt(v: Option<f64>, digits: usize) -> String {
    v.map_or(String::new(), |v| format!("{v:.digits$}"))
}

/// One line per imaging opportunity, times in UTC.
pub fn csv(app: &App, acquisitions: &[&Acquisition]) -> String {
    let mut out = String::from(
        "satellite,norad,sensor,time_utc,direction,cross_track_km,off_nadir_deg,elevation_deg,azimuth_deg,\
         sun_elevation_deg,aoi_covered_pct,cloud_cover_pct,plan,aoi,aoi_lat,aoi_lon\n",
    );
    let obs = &app.observer;
    for a in acquisitions {
        let info = &app.sats[a.sat].info;
        let row = [
            csv_field(info.name),
            info.norad.to_string(),
            csv_field(info.sensor),
            a.time.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            if a.ascending { "ascending" } else { "descending" }.to_string(),
            format!("{:.1}", a.cross_km),
            format!("{:.2}", a.off_nadir),
            format!("{:.2}", a.elevation),
            format!("{:.1}", a.azimuth),
            format!("{:.1}", a.sun_el),
            format!("{:.0}", a.coverage * 100.0),
            opt(app.cloud_at(a.time), 0),
            csv_field(&app.plan_label(a)),
            csv_field(&obs.name),
            format!("{:.5}", obs.lat),
            format!("{:.5}", obs.lon),
        ];
        out.push_str(&row.join(","));
        out.push('\n');
    }
    out
}

fn ics_text(s: &str) -> String {
    s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace('\n', "\\n")
}

/// Folds a content line at 75 octets, as RFC 5545 requires.
fn fold(line: &str) -> String {
    let mut out = String::new();
    let mut width = 0;
    for c in line.chars() {
        if width + c.len_utf8() > 75 {
            out.push_str("\r\n ");
            width = 1;
        }
        out.push(c);
        width += c.len_utf8();
    }
    out.push_str("\r\n");
    out
}

fn stamp(t: DateTime<Utc>) -> String {
    t.format("%Y%m%dT%H%M%SZ").to_string()
}

/// One one-minute event per imaging opportunity.
pub fn ics(app: &App, acquisitions: &[&Acquisition]) -> String {
    let obs = &app.observer;
    let now = stamp(Utc::now());
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".to_string(),
        "PRODID:-//orbitui//orbitui//EN".to_string(),
        "CALSCALE:GREGORIAN".to_string(),
        "METHOD:PUBLISH".to_string(),
        format!("X-WR-CALNAME:{}", ics_text(&format!("orbitui · {}", obs.name))),
    ];
    for a in acquisitions {
        let info = &app.sats[a.sat].info;
        let mut details = vec![
            format!("{} {}", info.sensor, if a.ascending { "ascending" } else { "descending" }),
            format!("{:.0} km {} of track, off-nadir {:.1}°", a.cross_km.abs(), if a.cross_km >= 0.0 { "right" } else { "left" }, a.off_nadir),
            format!("sun {:.0}°", a.sun_el),
        ];
        if obs.area.is_some() {
            details.push(format!("AOI covered {:.0}%", a.coverage * 100.0));
        }
        if let Some(c) = app.cloud_at(a.time) {
            details.push(format!("clouds {c:.0}% (forecast)"));
        }
        let plan = app.plan_label(a);
        if !plan.is_empty() {
            details.push(format!("plan: {plan}"));
        }
        lines.extend([
            "BEGIN:VEVENT".to_string(),
            format!("UID:{}-{}@orbitui", info.norad, stamp(a.time)),
            format!("DTSTAMP:{now}"),
            format!("DTSTART:{}", stamp(a.time - Duration::seconds(30))),
            format!("DTEND:{}", stamp(a.time + Duration::seconds(30))),
            format!("SUMMARY:{}", ics_text(&format!("{} over {}", info.name, obs.name))),
            format!("DESCRIPTION:{}", ics_text(&details.join("\n"))),
            format!("GEO:{:.5};{:.5}", obs.lat, obs.lon),
            "END:VEVENT".to_string(),
        ]);
    }
    lines.push("END:VCALENDAR".to_string());
    lines.iter().map(|l| fold(l)).collect()
}

/// A file name for the export: `orbitui-manaus-20261021.csv`.
pub fn file_name(app: &App, extension: &str) -> String {
    const ACCENTED: &str = "áàâãäåéèêëíìîïóòôõöúùûüçñý";
    const PLAIN: &str = "aaaaaaeeeeiiiiooooouuuucny";
    let slug: String = app
        .observer
        .name
        .to_lowercase()
        .chars()
        .map(|c| ACCENTED.chars().position(|a| a == c).and_then(|i| PLAIN.chars().nth(i)).unwrap_or(c))
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    format!("orbitui-{}-{}.{extension}", if slug.is_empty() { "aoi" } else { &slug }, app.clock.now().format("%Y%m%d"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quoting() {
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("plain"), "plain");
    }

    #[test]
    fn ics_folding_and_escaping() {
        assert_eq!(ics_text("a;b,c\nd"), "a\\;b\\,c\\nd");
        let long = format!("DESCRIPTION:{}", "é".repeat(60));
        let folded = fold(&long);
        assert!(folded.split("\r\n").all(|l| l.len() <= 75));
        assert_eq!(folded.replace("\r\n ", "").trim_end(), long);
    }
}
