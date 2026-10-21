//! Places by name, from the Open-Meteo geocoding API (GeoNames data; no
//! key, CORS enabled).
//!
//! Names match only in the language they are asked in ("Pequim" is found in
//! Portuguese, not in English), and the language of a typed name is unknown,
//! so a search asks in the user's language and a few common ones, and ranks
//! the places found by population: "Berlim", "Berlin" and "Berlín" all put
//! Berlin first.

use serde::Deserialize;

#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub id: u64,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub elevation: f64,
    pub country: Option<String>,
    pub admin1: Option<String>,
    pub population: Option<u64>,
}

impl Place {
    /// "Berlim, Alemanha" or "Berlim, Amazonas, Brasil".
    pub fn label(&self) -> String {
        let mut parts = vec![self.name.as_str()];
        if let Some(a) = self.admin1.as_deref().filter(|a| *a != self.name) {
            parts.push(a);
        }
        if let Some(c) = self.country.as_deref() {
            parts.push(c);
        }
        parts.join(", ")
    }
}

/// Languages a search asks in: the user's first, then common ones.
pub fn languages() -> Vec<String> {
    let user = sys_locale::get_locale()
        .and_then(|l| l.split(['-', '_']).next().map(str::to_ascii_lowercase))
        .filter(|l| l.len() == 2);
    let mut out: Vec<String> = user.into_iter().collect();
    for l in ["en", "pt", "es", "fr", "de"] {
        if !out.iter().any(|o| o == l) {
            out.push(l.to_string());
        }
    }
    out
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn url(query: &str, language: &str) -> String {
    format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=10&language={language}&format=json",
        encode(query.trim())
    )
}

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    results: Vec<Raw>,
}

#[derive(Deserialize)]
struct Raw {
    id: u64,
    name: String,
    latitude: f64,
    longitude: f64,
    elevation: Option<f64>,
    country: Option<String>,
    admin1: Option<String>,
    population: Option<u64>,
}

pub fn parse(text: &str) -> Result<Vec<Place>, String> {
    let r: Response = serde_json::from_str(text).map_err(|e| format!("geocoding: {e}"))?;
    Ok(r.results
        .into_iter()
        .map(|p| Place {
            id: p.id,
            name: p.name,
            lat: p.latitude,
            lon: p.longitude,
            elevation: p.elevation.unwrap_or(0.0),
            country: p.country,
            admin1: p.admin1,
            population: p.population,
        })
        .collect())
}

/// Adds places not already found (a place keeps the name of the first
/// language that found it).
pub fn merge(found: &mut Vec<Place>, more: Vec<Place>) {
    for p in more {
        if !found.iter().any(|f| f.id == p.id) {
            found.push(p);
        }
    }
}

/// Most populous first: the likeliest meaning of a bare name.
pub fn rank(places: &mut [Place]) {
    places.sort_by_key(|p| std::cmp::Reverse(p.population.unwrap_or(0)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_merges() {
        let text = r#"{"results":[{"id":2950159,"name":"Berlim","latitude":52.52437,"longitude":13.41053,"elevation":74.0,
            "country":"Alemanha","admin1":"Berlim","population":3426354},
            {"id":3752625,"name":"Berlim","latitude":-3.43889,"longitude":-63.41694,"country":"Brasil","admin1":"Amazonas"}]}"#;
        let mut places = parse(text).unwrap();
        assert_eq!(places[0].label(), "Berlim, Alemanha");
        assert_eq!(places[1].label(), "Berlim, Amazonas, Brasil");
        assert_eq!(places[1].elevation, 0.0);
        let again = parse(text).unwrap();
        merge(&mut places, again);
        assert_eq!(places.len(), 2);
        places.reverse();
        rank(&mut places);
        assert_eq!(places[0].country.as_deref(), Some("Alemanha"));
        assert!(languages().contains(&"pt".to_string()));
        assert!(parse(r#"{"generationtime_ms":0.1}"#).unwrap().is_empty());
    }

    #[test]
    fn encodes_queries() {
        assert!(url("São Paulo", "pt").contains("name=S%C3%A3o%20Paulo&"));
    }
}
