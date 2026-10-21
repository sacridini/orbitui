//! Cloud cover forecast at the area of interest, from Open-Meteo
//! (hourly, 16 days, no key; weather data by Open-Meteo.com, CC BY 4.0).

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Deserialize;

/// How long a forecast is kept before it is fetched again.
pub const MAX_AGE_HOURS: i64 = 3;

#[derive(Clone, Debug)]
pub struct Clouds {
    /// Time of the first hourly value.
    pub start: DateTime<Utc>,
    /// Cloud cover in percent, one value per hour; gaps are NaN.
    pub hourly: Vec<f64>,
    /// Where the forecast is for.
    pub at: (f64, f64),
    pub fetched: DateTime<Utc>,
}

pub fn url(lat: f64, lon: f64) -> String {
    format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat:.4}&longitude={lon:.4}\
         &hourly=cloud_cover&forecast_days=16&timezone=GMT"
    )
}

#[derive(Deserialize)]
struct Response {
    hourly: Hourly,
}

#[derive(Deserialize)]
struct Hourly {
    time: Vec<String>,
    cloud_cover: Vec<Option<f64>>,
}

pub fn parse(text: &str, at: (f64, f64)) -> Result<Clouds, String> {
    let r: Response = serde_json::from_str(text).map_err(|e| format!("Open-Meteo: {e}"))?;
    let first = r.hourly.time.first().ok_or("Open-Meteo: no hourly data")?;
    let start = NaiveDateTime::parse_from_str(first, "%Y-%m-%dT%H:%M").map_err(|e| format!("Open-Meteo time: {e}"))?.and_utc();
    let hourly = r.hourly.cloud_cover.iter().map(|v| v.unwrap_or(f64::NAN)).collect();
    Ok(Clouds { start, hourly, at, fetched: Utc::now() })
}

impl Clouds {
    /// Cloud cover (%) at `t`, interpolated between hours; `None` outside
    /// the forecast.
    pub fn at(&self, t: DateTime<Utc>) -> Option<f64> {
        let h = (t - self.start).num_seconds() as f64 / 3600.0;
        if h < 0.0 || h > (self.hourly.len() - 1) as f64 {
            return None;
        }
        let i = h.floor() as usize;
        let (a, b) = (self.hourly[i], *self.hourly.get(i + 1).unwrap_or(&self.hourly[i]));
        let v = a + (b - a) * (h - i as f64);
        v.is_finite().then_some(v)
    }

    pub fn is_stale(&self, at: (f64, f64)) -> bool {
        let moved = (self.at.0 - at.0).abs() > 0.01 || (self.at.1 - at.1).abs() > 0.01;
        moved || Utc::now() - self.fetched > chrono::Duration::hours(MAX_AGE_HOURS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn parses_and_interpolates() {
        let text = r#"{"hourly":{"time":["2026-10-21T00:00","2026-10-21T01:00","2026-10-21T02:00"],"cloud_cover":[0,50,null]}}"#;
        let c = parse(text, (0.0, 0.0)).unwrap();
        let t = |h, m| Utc.with_ymd_and_hms(2026, 10, 21, h, m, 0).unwrap();
        assert_eq!(c.at(t(0, 0)), Some(0.0));
        assert_eq!(c.at(t(0, 30)), Some(25.0));
        assert_eq!(c.at(t(1, 30)), None);
        assert_eq!(c.at(t(3, 0)), None);
        assert!(c.is_stale((1.0, 0.0)) && !c.is_stale((0.0, 0.0)));
    }
}
