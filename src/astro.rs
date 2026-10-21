//! Reference frames, Earth geometry and the Sun.
//!
//! Angles are in degrees at the API boundary and radians inside. Precision
//! targets what a terminal can show: tens of metres for positions, a few
//! hundredths of a degree for the Sun.

use chrono::{DateTime, Utc};
use std::f64::consts::TAU;

/// WGS84 equatorial radius in km.
pub const EARTH_RADIUS_KM: f64 = 6378.137;
/// Mean Earth radius in km, used for great-circle distances.
pub const MEAN_RADIUS_KM: f64 = 6371.0;
const FLATTENING: f64 = 1.0 / 298.257_223_563;
const E2: f64 = FLATTENING * (2.0 - FLATTENING);

pub type Vec3 = [f64; 3];

pub fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn norm(a: Vec3) -> f64 {
    dot(a, a).sqrt()
}

pub fn julian_date(t: DateTime<Utc>) -> f64 {
    t.timestamp_millis() as f64 / 86_400_000.0 + 2_440_587.5
}

/// Greenwich mean sidereal time (IAU 1982) in radians.
pub fn gmst(jd: f64) -> f64 {
    let t = (jd - 2_451_545.0) / 36_525.0;
    let seconds = 67_310.548_41 + (876_600.0 * 3600.0 + 8_640_184.812_866) * t
        + 0.093_104 * t * t
        - 6.2e-6 * t * t * t;
    (seconds.rem_euclid(86_400.0) / 240.0).to_radians()
}

/// Rotates a TEME vector into the Earth-fixed frame (polar motion ignored).
pub fn teme_to_ecef(r: Vec3, gmst: f64) -> Vec3 {
    let (s, c) = gmst.sin_cos();
    [c * r[0] + s * r[1], -s * r[0] + c * r[1], r[2]]
}

#[derive(Clone, Copy, Debug)]
pub struct Geodetic {
    pub lat: f64,
    pub lon: f64,
    /// Height above the ellipsoid in km.
    pub alt: f64,
}

pub fn ecef_to_geodetic(r: Vec3) -> Geodetic {
    let p = (r[0] * r[0] + r[1] * r[1]).sqrt();
    let lon = r[1].atan2(r[0]);
    let mut lat = r[2].atan2(p * (1.0 - E2));
    let mut alt = 0.0;
    for _ in 0..4 {
        let sin = lat.sin();
        let n = EARTH_RADIUS_KM / (1.0 - E2 * sin * sin).sqrt();
        alt = p / lat.cos() - n;
        lat = r[2].atan2(p * (1.0 - E2 * n / (n + alt)));
    }
    Geodetic { lat: lat.to_degrees(), lon: lon.to_degrees(), alt }
}

pub fn geodetic_to_ecef(lat: f64, lon: f64, alt_km: f64) -> Vec3 {
    let (lat, lon) = (lat.to_radians(), lon.to_radians());
    let sin = lat.sin();
    let n = EARTH_RADIUS_KM / (1.0 - E2 * sin * sin).sqrt();
    [
        (n + alt_km) * lat.cos() * lon.cos(),
        (n + alt_km) * lat.cos() * lon.sin(),
        (n * (1.0 - E2) + alt_km) * sin,
    ]
}

/// Unit vector of a point on the sphere.
pub fn unit_vector(lat: f64, lon: f64) -> Vec3 {
    let (lat, lon) = (lat.to_radians(), lon.to_radians());
    [lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()]
}

#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub az: f64,
    pub el: f64,
    pub range_km: f64,
}

/// Azimuth, elevation and range of `target` seen from an observer.
pub fn look_angles(lat: f64, lon: f64, observer: Vec3, target: Vec3) -> Look {
    let (lat, lon) = (lat.to_radians(), lon.to_radians());
    let rho = [target[0] - observer[0], target[1] - observer[1], target[2] - observer[2]];
    let (sl, cl) = lat.sin_cos();
    let (so, co) = lon.sin_cos();
    let south = sl * co * rho[0] + sl * so * rho[1] - cl * rho[2];
    let east = -so * rho[0] + co * rho[1];
    let zenith = cl * co * rho[0] + cl * so * rho[1] + sl * rho[2];
    let range_km = norm(rho);
    Look {
        az: east.atan2(-south).rem_euclid(TAU).to_degrees(),
        el: (zenith / range_km).clamp(-1.0, 1.0).asin().to_degrees(),
        range_km,
    }
}

/// Direction of the Sun in the true-of-date inertial frame (Astronomical
/// Almanac low-precision formulae, ~0.01°).
pub fn sun_direction(jd: f64) -> Vec3 {
    let n = jd - 2_451_545.0;
    let l = (280.460 + 0.985_647_4 * n).to_radians();
    let g = (357.528 + 0.985_600_3 * n).to_radians();
    let lambda = l + (1.915f64.to_radians()) * g.sin() + (0.020f64.to_radians()) * (2.0 * g).sin();
    let eps = (23.439 - 0.000_000_4 * n).to_radians();
    [lambda.cos(), eps.cos() * lambda.sin(), eps.sin() * lambda.sin()]
}

/// Point on Earth with the Sun at the zenith, as (lat, lon).
pub fn subsolar_point(t: DateTime<Utc>) -> (f64, f64) {
    let jd = julian_date(t);
    let s = sun_direction(jd);
    let dec = s[2].asin();
    let ra = s[1].atan2(s[0]);
    (dec.to_degrees(), wrap_lon((ra - gmst(jd)).to_degrees()))
}

/// Elevation of the Sun at a point, given the subsolar point.
pub fn sun_elevation(lat: f64, lon: f64, subsolar: (f64, f64)) -> f64 {
    90.0 - central_angle(lat, lon, subsolar.0, subsolar.1).to_degrees()
}

pub fn wrap_lon(lon: f64) -> f64 {
    (lon + 180.0).rem_euclid(360.0) - 180.0
}

/// Great-circle angle between two points in radians (haversine).
pub fn central_angle(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = p2 - p1;
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * a.sqrt().min(1.0).asin()
}

/// Initial bearing from point 1 to point 2 in degrees clockwise from north.
pub fn bearing(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dl = (lon2 - lon1).to_radians();
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    y.atan2(x).to_degrees().rem_euclid(360.0)
}

/// Point reached travelling `angle` radians along a great circle.
pub fn destination(lat: f64, lon: f64, bearing_deg: f64, angle: f64) -> (f64, f64) {
    let (p1, l1, b) = (lat.to_radians(), lon.to_radians(), bearing_deg.to_radians());
    let p2 = (p1.sin() * angle.cos() + p1.cos() * angle.sin() * b.cos()).asin();
    let l2 = l1 + (b.sin() * angle.sin() * p1.cos()).atan2(angle.cos() - p1.sin() * p2.sin());
    (p2.to_degrees(), wrap_lon(l2.to_degrees()))
}

/// Earth central angle (radians) of the circle where a satellite at
/// `alt_km` is seen above `min_el` degrees.
pub fn coverage_angle(alt_km: f64, min_el: f64) -> f64 {
    let el = min_el.to_radians();
    (MEAN_RADIUS_KM * el.cos() / (MEAN_RADIUS_KM + alt_km)).acos() - el
}

/// Off-nadir look angle (degrees) from a satellite at `alt_km` to a ground
/// point `angle` radians away from its nadir.
pub fn off_nadir(alt_km: f64, angle: f64) -> f64 {
    let r = MEAN_RADIUS_KM;
    (r * angle.sin()).atan2(r + alt_km - r * angle.cos()).to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn geodetic_round_trip() {
        let r = geodetic_to_ecef(-15.79, -47.88, 1.2);
        let g = ecef_to_geodetic(r);
        assert!((g.lat + 15.79).abs() < 1e-9);
        assert!((g.lon + 47.88).abs() < 1e-9);
        assert!((g.alt - 1.2).abs() < 1e-6);
    }

    #[test]
    fn gmst_at_j2000() {
        // GMST at 2000-01-01 12:00 UT1 is 280.46062°.
        let g = gmst(2_451_545.0).to_degrees();
        assert!((g - 280.460_62).abs() < 1e-3, "{g}");
    }

    #[test]
    fn subsolar_near_equinox_noon() {
        // Around the March equinox at 12:00 UTC the Sun is near (0°, 0°).
        let t = Utc.with_ymd_and_hms(2025, 3, 20, 12, 0, 0).unwrap();
        let (lat, lon) = subsolar_point(t);
        assert!(lat.abs() < 0.5, "{lat}");
        assert!(lon.abs() < 3.0, "{lon}");
    }

    #[test]
    fn zenith_target() {
        let obs = geodetic_to_ecef(10.0, 20.0, 0.0);
        let target = geodetic_to_ecef(10.0, 20.0, 700.0);
        let look = look_angles(10.0, 20.0, obs, target);
        assert!(look.el > 89.9);
        assert!((look.range_km - 700.0).abs() < 1e-6);
    }

    #[test]
    fn destination_and_bearing_agree() {
        let (lat, lon) = destination(-23.5, -46.6, 60.0, 0.05);
        assert!((bearing(-23.5, -46.6, lat, lon) - 60.0).abs() < 1e-6);
        assert!((central_angle(-23.5, -46.6, lat, lon) - 0.05).abs() < 1e-9);
    }
}
