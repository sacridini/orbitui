//! The last area of interest, remembered between sessions.

use crate::orbit::Observer;
use crate::platform;

const KEY: &str = "aoi";

/// Brasília, until the user picks an area.
pub fn default_observer() -> Observer {
    Observer::new("Brasília", -15.7939, -47.8828, 1170.0)
}

pub fn load() -> Option<Observer> {
    let text = platform::load(KEY)?;
    let mut name = String::from("AOI");
    let (mut lat, mut lon, mut alt) = (None, None, 0.0);
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim();
        match key.trim() {
            "name" => name = value.to_string(),
            "lat" => lat = value.parse().ok(),
            "lon" => lon = value.parse().ok(),
            "alt_m" => alt = value.parse().unwrap_or(0.0),
            _ => {}
        }
    }
    Some(Observer::new(name, lat?, lon?, alt))
}

pub fn save(obs: &Observer) {
    let text = format!(
        "name = {}\nlat = {}\nlon = {}\nalt_m = {}\n",
        obs.name,
        obs.lat,
        obs.lon,
        obs.alt_km * 1000.0
    );
    platform::save(KEY, &text);
}

/// Parses "lat, lon[, name]" or "lat lon [name]".
pub fn parse_aoi(input: &str) -> Option<(f64, f64, String)> {
    let mut parts = input.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter(|s| !s.is_empty());
    let lat: f64 = parts.next()?.parse().ok()?;
    let lon: f64 = parts.next()?.parse().ok()?;
    if lat.abs() > 90.0 || lon.abs() > 180.0 {
        return None;
    }
    let name = parts.collect::<Vec<_>>().join(" ");
    Some((lat, lon, if name.is_empty() { "AOI".into() } else { name }))
}

#[cfg(test)]
mod tests {
    use super::parse_aoi;

    #[test]
    fn parses_aoi_input() {
        assert_eq!(parse_aoi("-15.79, -47.88"), Some((-15.79, -47.88, "AOI".into())));
        assert_eq!(parse_aoi("-3.1 -60.0 Manaus AM"), Some((-3.1, -60.0, "Manaus AM".into())));
        assert_eq!(parse_aoi("95, 10"), None);
        assert_eq!(parse_aoi("abc"), None);
    }
}
