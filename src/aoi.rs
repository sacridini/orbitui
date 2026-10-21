//! Areas of interest given as polygons (GeoJSON).
//!
//! An area is sampled on a grid of a few hundred points; an overflight
//! covers the share of samples that fall inside the swath.

use crate::astro::{self, MEAN_RADIUS_KM};
use serde_json::Value;

/// Samples aimed for inside an area.
const TARGET_SAMPLES: usize = 300;

#[derive(Clone, Debug)]
pub struct Area {
    /// Rings as (lat, lon); holes are rings too (even-odd rule).
    pub rings: Vec<Vec<(f64, f64)>>,
    /// Points inside the area, as (lat, lon).
    pub samples: Vec<(f64, f64)>,
    pub centroid: (f64, f64),
    pub area_km2: f64,
}

/// What a GeoJSON document describes.
pub enum Shape {
    Point(f64, f64),
    Area(Area),
}

impl Area {
    pub fn new(rings: Vec<Vec<(f64, f64)>>) -> Result<Area, String> {
        let rings: Vec<_> = rings.into_iter().filter(|r| r.len() >= 3).collect();
        if rings.is_empty() {
            return Err("no polygon with at least three vertices".into());
        }
        let (mut lat0, mut lon0, mut lat1, mut lon1) = (90.0f64, 180.0f64, -90.0f64, -180.0f64);
        for &(lat, lon) in rings.iter().flatten() {
            (lat0, lon0, lat1, lon1) = (lat0.min(lat), lon0.min(lon), lat1.max(lat), lon1.max(lon));
        }
        let mut area = Area { rings, samples: Vec::new(), centroid: ((lat0 + lat1) / 2.0, (lon0 + lon1) / 2.0), area_km2: 0.0 };

        // Refine the grid until enough samples land inside.
        let mut n = 16;
        loop {
            let (dlat, dlon) = ((lat1 - lat0) / n as f64, (lon1 - lon0) / n as f64);
            let grid: Vec<(f64, f64)> = (0..n)
                .flat_map(|i| (0..n).map(move |j| (lat0 + (i as f64 + 0.5) * dlat, lon0 + (j as f64 + 0.5) * dlon)))
                .collect();
            let inside: Vec<(f64, f64)> = grid.iter().copied().filter(|&(lat, lon)| area.contains(lat, lon)).collect();
            if inside.len() >= TARGET_SAMPLES || n >= 128 {
                let km_lat = (lat1 - lat0) * 110.574;
                let km_lon = (lon1 - lon0) * 111.320 * area.centroid.0.to_radians().cos();
                area.area_km2 = km_lat * km_lon * inside.len() as f64 / grid.len() as f64;
                area.samples = inside;
                break;
            }
            n *= 2;
        }
        if area.samples.is_empty() {
            area.samples.push(area.centroid);
        } else {
            let k = area.samples.len() as f64;
            let (sl, so) = area.samples.iter().fold((0.0, 0.0), |(a, b), &(lat, lon)| (a + lat, b + lon));
            area.centroid = (sl / k, so / k);
            // Keep the work per overflight bounded.
            let stride = area.samples.len().div_ceil(TARGET_SAMPLES * 2);
            area.samples = area.samples.iter().copied().step_by(stride).collect();
        }
        Ok(area)
    }

    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        in_rings(&self.rings, lat, lon)
    }

    /// Share of the area inside the swath of a satellite at (lat, lon)
    /// flying along `heading`, with swath edges `near..far` km.
    pub fn coverage(&self, lat: f64, lon: f64, heading: f64, near: f64, far: f64) -> f64 {
        let covered = self
            .samples
            .iter()
            .filter(|&&(plat, plon)| {
                let x = cross_track_km(lat, lon, heading, plat, plon);
                x >= near && x <= far
            })
            .count();
        covered as f64 / self.samples.len() as f64
    }

    /// The area as GeoJSON, for storage.
    pub fn to_geojson(&self) -> String {
        let polys: Vec<Value> = self
            .rings
            .iter()
            .map(|r| Value::from(vec![Value::from(r.iter().map(|&(lat, lon)| Value::from(vec![lon, lat])).collect::<Vec<_>>())]))
            .collect();
        serde_json::json!({ "type": "MultiPolygon", "coordinates": polys }).to_string()
    }
}

/// Even-odd point in polygon, in lat/lon; holes are rings too.
pub fn in_rings(rings: &[Vec<(f64, f64)>], lat: f64, lon: f64) -> bool {
    let mut inside = false;
    for ring in rings {
        let mut j = ring.len() - 1;
        for i in 0..ring.len() {
            let ((ya, xa), (yb, xb)) = (ring[i], ring[j]);
            if (ya > lat) != (yb > lat) && lon < (xb - xa) * (lat - ya) / (yb - ya) + xa {
                inside = !inside;
            }
            j = i;
        }
    }
    inside
}

/// Signed distance (km) of a point from the great circle through (lat, lon)
/// with `heading`; positive to the right.
pub fn cross_track_km(lat: f64, lon: f64, heading: f64, plat: f64, plon: f64) -> f64 {
    let d = astro::central_angle(lat, lon, plat, plon);
    let b = astro::bearing(lat, lon, plat, plon);
    (d.sin() * (b - heading).to_radians().sin()).asin() * MEAN_RADIUS_KM
}

/// Reads a GeoJSON document: polygons make an area, otherwise the first
/// point. Returns the name found in the properties, if any.
pub fn parse_geojson(text: &str) -> Result<(Shape, Option<String>), String> {
    let doc: Value = serde_json::from_str(text).map_err(|e| format!("not GeoJSON: {e}"))?;
    let mut rings = Vec::new();
    let mut points = Vec::new();
    let mut name = None;
    collect(&doc, &mut rings, &mut points, &mut name)?;
    if !rings.is_empty() {
        return Ok((Shape::Area(Area::new(rings)?), name));
    }
    match points.first() {
        Some(&(lat, lon)) => Ok((Shape::Point(lat, lon), name)),
        None => Err("no polygon or point in the GeoJSON".into()),
    }
}

fn position(v: &Value) -> Option<(f64, f64)> {
    let a = v.as_array()?;
    let (lon, lat) = (a.first()?.as_f64()?, a.get(1)?.as_f64()?);
    (lat.abs() <= 90.0 && lon.abs() <= 180.0).then_some((lat, lon))
}

fn ring(v: &Value) -> Result<Vec<(f64, f64)>, String> {
    v.as_array()
        .ok_or("ring is not an array")?
        .iter()
        .map(|p| position(p).ok_or_else(|| "bad coordinate".to_string()))
        .collect()
}

fn collect(v: &Value, rings: &mut Vec<Vec<(f64, f64)>>, points: &mut Vec<(f64, f64)>, name: &mut Option<String>) -> Result<(), String> {
    let polygon = |c: &Value, rings: &mut Vec<Vec<(f64, f64)>>| -> Result<(), String> {
        for r in c.as_array().ok_or("polygon is not an array")? {
            rings.push(ring(r)?);
        }
        Ok(())
    };
    match v.get("type").and_then(Value::as_str) {
        Some("FeatureCollection") => {
            for f in v.get("features").and_then(Value::as_array).into_iter().flatten() {
                collect(f, rings, points, name)?;
            }
        }
        Some("Feature") => {
            if name.is_none()
                && let Some(props) = v.get("properties")
            {
                *name = ["name", "NAME", "Name", "nome", "NOME"]
                    .iter()
                    .find_map(|k| props.get(*k).and_then(Value::as_str))
                    .map(str::to_string);
            }
            if let Some(g) = v.get("geometry") {
                collect(g, rings, points, name)?;
            }
        }
        Some("GeometryCollection") => {
            for g in v.get("geometries").and_then(Value::as_array).into_iter().flatten() {
                collect(g, rings, points, name)?;
            }
        }
        Some("Polygon") => polygon(&v["coordinates"], rings)?,
        Some("MultiPolygon") => {
            for p in v["coordinates"].as_array().ok_or("multipolygon is not an array")? {
                polygon(p, rings)?;
            }
        }
        Some("Point") => points.extend(position(&v["coordinates"])),
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQUARE: &str = r#"{"type":"Feature","properties":{"name":"Box"},
        "geometry":{"type":"Polygon","coordinates":[[[-48,-16],[-47,-16],[-47,-15],[-48,-15],[-48,-16]]]}}"#;

    #[test]
    fn parses_and_samples_a_polygon() {
        let (shape, name) = parse_geojson(SQUARE).unwrap();
        assert_eq!(name.as_deref(), Some("Box"));
        let Shape::Area(area) = shape else { panic!("expected an area") };
        assert!(area.samples.len() >= 100);
        assert!((area.centroid.0 + 15.5).abs() < 0.05 && (area.centroid.1 + 47.5).abs() < 0.05);
        // 1° × 1° at 15.5°S is ~110.6 × 107.3 km.
        assert!((area.area_km2 - 11_870.0).abs() < 300.0, "{}", area.area_km2);
        assert!(area.contains(-15.5, -47.5) && !area.contains(-14.0, -47.5));
    }

    #[test]
    fn coverage_of_a_swath() {
        let Shape::Area(area) = parse_geojson(SQUARE).unwrap().0 else { panic!() };
        // Southbound track along 47.5°W: a 290 km swath covers the whole box,
        // a 54 km one about half of it, and one offset 300 km to the right none.
        assert!(area.coverage(-15.5, -47.5, 180.0, -145.0, 145.0) > 0.99);
        let half = area.coverage(-15.5, -47.5, 180.0, -27.0, 27.0);
        assert!((half - 0.5).abs() < 0.1, "{half}");
        assert_eq!(area.coverage(-15.5, -47.5, 180.0, 300.0, 600.0), 0.0);
        // Heading south, the east is on the left.
        assert!(cross_track_km(-15.5, -47.5, 180.0, -15.5, -47.0) < 0.0);
    }

    #[test]
    fn holes_and_round_trip() {
        let donut = r#"{"type":"Polygon","coordinates":[[[0,0],[4,0],[4,4],[0,4],[0,0]],[[1,1],[3,1],[3,3],[1,3],[1,1]]]}"#;
        let Shape::Area(area) = parse_geojson(donut).unwrap().0 else { panic!() };
        assert!(!area.contains(2.0, 2.0) && area.contains(0.5, 0.5));
        let Shape::Area(again) = parse_geojson(&area.to_geojson()).unwrap().0 else { panic!() };
        assert_eq!(again.rings, area.rings);
        assert!(matches!(parse_geojson(r#"{"type":"Point","coordinates":[-60,-3]}"#).unwrap().0, Shape::Point(-3.0, -60.0)));
        assert!(parse_geojson("{}").is_err());
    }
}
