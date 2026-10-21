//! Satellites propagated with SGP4, and the observer on the ground.

use crate::aoi::Area;
use crate::astro::{self, Geodetic, Look, Vec3};
use crate::catalog::SatInfo;
use chrono::{DateTime, Duration, Utc};
use sgp4::{Constants, Elements, MinutesSinceEpoch};
use std::sync::Arc;

pub struct Sat {
    pub info: SatInfo,
    pub elements: Elements,
    constants: Constants,
}

#[derive(Clone, Copy, Debug)]
pub struct SatState {
    pub ecef: Vec3,
    pub geo: Geodetic,
    /// Moving north (ascending node side of the orbit).
    pub ascending: bool,
    /// Inertial speed in km/s.
    pub speed: f64,
}

impl Sat {
    pub fn new(mut info: SatInfo, elements: Elements) -> Option<Self> {
        let constants = Constants::from_elements(&elements).ok()?;
        info.resolve(astro::altitude_from_mean_motion(elements.mean_motion));
        Some(Sat { info, elements, constants })
    }

    pub fn epoch(&self) -> DateTime<Utc> {
        self.elements.datetime.and_utc()
    }

    pub fn period_minutes(&self) -> f64 {
        1440.0 / self.elements.mean_motion
    }

    pub fn state(&self, t: DateTime<Utc>) -> Option<SatState> {
        let minutes = (t - self.epoch()).num_milliseconds() as f64 / 60_000.0;
        let p = self.constants.propagate(MinutesSinceEpoch(minutes)).ok()?;
        let ecef = astro::teme_to_ecef(p.position, astro::gmst(astro::julian_date(t)));
        Some(SatState {
            ecef,
            geo: astro::ecef_to_geodetic(ecef),
            ascending: p.velocity[2] > 0.0,
            speed: astro::norm(p.velocity),
        })
    }

    /// Sub-satellite points sampled every `step_s` seconds in `[from, to]`.
    pub fn ground_track(&self, from: DateTime<Utc>, to: DateTime<Utc>, step_s: i64) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        let mut t = from;
        while t <= to {
            if let Some(s) = self.state(t) {
                out.push((s.geo.lat, s.geo.lon));
            }
            t += Duration::seconds(step_s);
        }
        out
    }
}

/// The area of interest: a point on the ground, or a polygon whose centroid
/// stands for it in look angles and Sun elevation.
#[derive(Clone, Debug)]
pub struct Observer {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub alt_km: f64,
    pub ecef: Vec3,
    pub area: Option<Arc<Area>>,
}

impl Observer {
    pub fn new(name: impl Into<String>, lat: f64, lon: f64, alt_m: f64) -> Self {
        let alt_km = alt_m / 1000.0;
        Observer {
            name: name.into(),
            lat,
            lon,
            alt_km,
            ecef: astro::geodetic_to_ecef(lat, lon, alt_km),
            area: None,
        }
    }

    pub fn with_area(name: impl Into<String>, area: Area) -> Self {
        let (lat, lon) = area.centroid;
        Observer { area: Some(Arc::new(area)), ..Observer::new(name, lat, lon, 0.0) }
    }

    pub fn look(&self, target: Vec3) -> Look {
        astro::look_angles(self.lat, self.lon, self.ecef, target)
    }

    pub fn coords(&self) -> String {
        format_coords(self.lat, self.lon)
    }
}

pub fn format_coords(lat: f64, lon: f64) -> String {
    format!(
        "{:.2}°{} {:.2}°{}",
        lat.abs(),
        if lat < 0.0 { 'S' } else { 'N' },
        lon.abs(),
        if lon < 0.0 { 'W' } else { 'E' }
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::catalog::builtin;

    pub fn sentinel_2a() -> Sat {
        let elements = Elements::from_tle(
            Some("SENTINEL-2A".into()),
            b"1 40697U 15028A   26293.94887774  .00000002  00000+0  17214-4 0  9998",
            b"2 40697  98.5632   6.9617 0001081  96.5011 263.6296 14.30823655591716",
        )
        .unwrap();
        Sat::new(builtin()[0], elements).unwrap()
    }

    #[test]
    fn sentinel_2_orbit_is_sun_synchronous_leo() {
        let sat = sentinel_2a();
        let s = sat.state(sat.epoch() + Duration::hours(3)).unwrap();
        // Mean altitude is 786 km; the geodetic height swings ~±20 km.
        assert!((770.0..830.0).contains(&s.geo.alt), "{}", s.geo.alt);
        assert!(s.geo.lat.abs() <= 81.5);
        assert!((sat.period_minutes() - 100.6).abs() < 0.5);
    }
}
