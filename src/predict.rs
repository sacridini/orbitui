//! Imaging opportunities and horizon passes over the area of interest.
//!
//! Each satellite is propagated on a coarse grid; closest approaches and
//! horizon crossings found on the grid are then refined to half a second.

use crate::astro::{self, MEAN_RADIUS_KM};
use crate::catalog::Kind;
use crate::orbit::{Observer, Sat};
use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;

/// Grid step in seconds (~140 km of ground track for a LEO satellite).
const STEP_S: f64 = 20.0;
const TOLERANCE_S: f64 = 0.5;

/// The AOI falls inside the sensor swath on this overflight.
#[derive(Clone, Debug)]
pub struct Acquisition {
    pub sat: usize,
    /// Time of closest approach.
    pub time: DateTime<Utc>,
    /// Signed cross-track distance of the AOI, positive right of the track.
    pub cross_km: f64,
    pub off_nadir: f64,
    /// Elevation of the satellite seen from the AOI.
    pub elevation: f64,
    pub azimuth: f64,
    pub sun_el: f64,
    pub ascending: bool,
    /// Share of the area of interest inside the swath (1 for a point).
    pub coverage: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Light {
    Day,
    Low,
    Night,
}

impl Acquisition {
    pub fn light(&self) -> Light {
        if self.sun_el >= 15.0 {
            Light::Day
        } else if self.sun_el >= 0.0 {
            Light::Low
        } else {
            Light::Night
        }
    }

    /// Whether the sensor can actually image on this overflight.
    pub fn usable(&self, kind: Kind) -> bool {
        match kind {
            Kind::Optical => self.sun_el > 0.0,
            Kind::Sar | Kind::Lidar => true,
            Kind::Station => false,
        }
    }
}

/// The satellite is above the AOI's horizon.
#[derive(Clone, Debug)]
pub struct Pass {
    pub sat: usize,
    pub aos: DateTime<Utc>,
    pub tca: DateTime<Utc>,
    pub los: DateTime<Utc>,
    pub max_el: f64,
    pub aos_az: f64,
    pub los_az: f64,
}

#[derive(Clone, Debug)]
pub struct Forecast {
    pub acquisitions: Vec<Acquisition>,
    pub passes: Vec<Pass>,
}

impl Forecast {
    pub fn next_acquisition(&self, sat: usize, after: DateTime<Utc>, kind: Kind) -> Option<&Acquisition> {
        self.acquisitions
            .iter()
            .find(|a| a.sat == sat && a.time >= after && a.usable(kind))
    }

    pub fn next_pass(&self, sat: usize, after: DateTime<Utc>) -> Option<&Pass> {
        self.passes.iter().find(|p| p.sat == sat && p.los >= after)
    }
}

fn at(start: DateTime<Utc>, seconds: f64) -> DateTime<Utc> {
    start + Duration::milliseconds((seconds * 1000.0).round() as i64)
}

/// Predicts acquisitions and passes (peaking above `min_el`) in `[start, end]`.
pub fn forecast(sats: &[Sat], obs: &Observer, start: DateTime<Utc>, end: DateTime<Utc>, min_el: f64) -> Forecast {
    let mut fc = Forecast { acquisitions: Vec::new(), passes: Vec::new() };
    for (i, sat) in sats.iter().enumerate() {
        scan(i, sat, obs, start, end, min_el, &mut fc);
    }
    fc.sort();
    fc
}

impl Forecast {
    fn sort(&mut self) {
        self.acquisitions.sort_by_key(|a| a.time);
        self.passes.sort_by_key(|p| p.aos);
    }
}

/// A forecast computed one satellite at a time, so that it can advance
/// between frames where there are no threads (the browser).
pub struct Job {
    sats: Arc<Vec<Sat>>,
    obs: Observer,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    min_el: f64,
    next: usize,
    fc: Forecast,
}

impl Job {
    pub fn new(sats: Arc<Vec<Sat>>, obs: Observer, start: DateTime<Utc>, end: DateTime<Utc>, min_el: f64) -> Self {
        Job { sats, obs, start, end, min_el, next: 0, fc: Forecast { acquisitions: Vec::new(), passes: Vec::new() } }
    }

    /// Scans satellites until `budget` is spent; the forecast once complete.
    pub fn step(&mut self, budget: web_time::Duration) -> Option<Forecast> {
        let began = web_time::Instant::now();
        while self.next < self.sats.len() {
            scan(self.next, &self.sats[self.next], &self.obs, self.start, self.end, self.min_el, &mut self.fc);
            self.next += 1;
            if began.elapsed() >= budget {
                break;
            }
        }
        (self.next == self.sats.len()).then(|| {
            let mut fc = std::mem::replace(&mut self.fc, Forecast { acquisitions: Vec::new(), passes: Vec::new() });
            fc.sort();
            fc
        })
    }

    pub fn run(mut self) -> Forecast {
        loop {
            if let Some(fc) = self.step(web_time::Duration::MAX) {
                return fc;
            }
        }
    }
}

fn scan(i: usize, sat: &Sat, obs: &Observer, start: DateTime<Utc>, end: DateTime<Utc>, min_el: f64, fc: &mut Forecast) {
    let total = (end - start).num_seconds() as f64;
    let distance = |s: f64| {
        sat.state(at(start, s))
            .map(|st| astro::central_angle(st.geo.lat, st.geo.lon, obs.lat, obs.lon))
            .unwrap_or(f64::MAX)
    };
    let elevation = |s: f64| sat.state(at(start, s)).map(|st| obs.look(st.ecef).el).unwrap_or(-90.0);

    // NaN never compares, so missing samples never trigger a detection.
    let (mut d2, mut d1, mut el_prev) = (f64::NAN, f64::NAN, f64::NAN);
    let mut aos: Option<f64> = None;
    let mut s = 0.0;
    while s <= total {
        let (d, el) = match sat.state(at(start, s)) {
            Some(st) => (
                astro::central_angle(st.geo.lat, st.geo.lon, obs.lat, obs.lon),
                obs.look(st.ecef).el,
            ),
            None => (f64::NAN, f64::NAN),
        };

        if d1 < d2 && d1 <= d {
            let tmin = golden_min(&distance, s - 2.0 * STEP_S, s);
            if let Some(a) = acquisition(i, sat, obs, at(start, tmin)) {
                fc.acquisitions.push(a);
            }
        }

        if el >= 0.0 && aos.is_none() {
            aos = Some(if el_prev.is_nan() { s } else { bisect(&elevation, s - STEP_S, s) });
        } else if el < 0.0
            && let Some(a) = aos.take()
        {
            let los = bisect(&elevation, s - STEP_S, s);
            let tca = golden_min(&|t| -elevation(t), a, los);
            let max_el = elevation(tca);
            if max_el >= min_el {
                let az = |t: f64| sat.state(at(start, t)).map(|st| obs.look(st.ecef).az).unwrap_or(0.0);
                fc.passes.push(Pass {
                    sat: i,
                    aos: at(start, a),
                    tca: at(start, tca),
                    los: at(start, los),
                    max_el,
                    aos_az: az(a),
                    los_az: az(los),
                });
            }
        }

        (d2, d1, el_prev) = (d1, d, el);
        s += STEP_S;
    }
}

fn acquisition(i: usize, sat: &Sat, obs: &Observer, t: DateTime<Utc>) -> Option<Acquisition> {
    if !sat.info.images() {
        return None;
    }
    let st = sat.state(t)?;
    let ahead = sat.state(t + Duration::seconds(10))?;
    let angle = astro::central_angle(st.geo.lat, st.geo.lon, obs.lat, obs.lon);
    let track = astro::bearing(st.geo.lat, st.geo.lon, ahead.geo.lat, ahead.geo.lon);
    let to_aoi = astro::bearing(st.geo.lat, st.geo.lon, obs.lat, obs.lon);
    let side = if (to_aoi - track).rem_euclid(360.0) < 180.0 { 1.0 } else { -1.0 };
    let cross_km = side * angle * MEAN_RADIUS_KM;
    let coverage = match &obs.area {
        Some(area) => area.coverage(st.geo.lat, st.geo.lon, track, |x| sat.info.covers(x)),
        None => f64::from(u8::from(sat.info.covers(cross_km))),
    };
    if coverage <= 0.0 {
        return None;
    }
    let look = obs.look(st.ecef);
    Some(Acquisition {
        sat: i,
        time: t,
        cross_km,
        off_nadir: astro::off_nadir(st.geo.alt, angle),
        elevation: look.el,
        azimuth: look.az,
        sun_el: astro::sun_elevation(obs.lat, obs.lon, astro::subsolar_point(t)),
        ascending: st.ascending,
        coverage,
    })
}

/// Azimuth/elevation samples of a satellite above the horizon in `[from, to]`.
pub fn sky_track(sat: &Sat, obs: &Observer, from: DateTime<Utc>, to: DateTime<Utc>, step_s: i64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut t = from;
    while t <= to {
        if let Some(st) = sat.state(t) {
            let look = obs.look(st.ecef);
            if look.el >= 0.0 {
                out.push((look.az, look.el));
            }
        }
        t += Duration::seconds(step_s);
    }
    out
}

fn golden_min(f: &impl Fn(f64) -> f64, mut a: f64, mut b: f64) -> f64 {
    const G: f64 = 0.618_033_988_749_895;
    let mut c = b - G * (b - a);
    let mut d = a + G * (b - a);
    let (mut fc, mut fd) = (f(c), f(d));
    while b - a > TOLERANCE_S {
        if fc < fd {
            (b, d, fd) = (d, c, fc);
            c = b - G * (b - a);
            fc = f(c);
        } else {
            (a, c, fc) = (c, d, fd);
            d = a + G * (b - a);
            fd = f(d);
        }
    }
    (a + b) / 2.0
}

/// Root of `f` in `[a, b]`, given `f(a)` and `f(b)` of opposite signs.
fn bisect(f: &impl Fn(f64) -> f64, mut a: f64, mut b: f64) -> f64 {
    let a_negative = f(a) < 0.0;
    while b - a > TOLERANCE_S {
        let m = (a + b) / 2.0;
        if (f(m) < 0.0) == a_negative {
            a = m;
        } else {
            b = m;
        }
    }
    (a + b) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orbit::tests::sentinel_2a;
    use chrono::Timelike;

    #[test]
    fn sentinel_2_images_brasilia_mid_morning_descending() {
        let sat = sentinel_2a();
        let obs = Observer::new("Brasília", -15.79, -47.88, 1170.0);
        let start = sat.epoch();
        let fc = forecast(std::slice::from_ref(&sat), &obs, start, start + Duration::days(11), 10.0);

        let daytime: Vec<_> = fc.acquisitions.iter().filter(|a| a.usable(Kind::Optical)).collect();
        assert!(!daytime.is_empty(), "no daytime acquisition in a full 10-day cycle");
        for a in daytime {
            // Sentinel-2 crosses the equator descending at 10:30 mean solar time.
            let solar = (a.time.num_seconds_from_midnight() as f64 / 3600.0 + obs.lon / 15.0).rem_euclid(24.0);
            assert!(!a.ascending);
            assert!((9.5..11.5).contains(&solar), "local solar time {solar}");
            assert!(a.cross_km.abs() <= 145.0);
            assert!(a.off_nadir < 11.0);
        }
        assert!(fc.passes.iter().all(|p| p.aos < p.tca && p.tca < p.los && p.max_el >= 10.0));
    }

    /// Reference culminations computed with Skyfield 1.55 from the same TLEs.
    #[test]
    fn passes_match_skyfield() {
        use crate::catalog::builtin;
        use chrono::TimeZone;
        let sat = |info, l1: &str, l2: &str| Sat::new(info, sgp4::Elements::from_tle(None, l1.as_bytes(), l2.as_bytes()).unwrap()).unwrap();
        let sats = [
            sat(
                builtin().into_iter().find(|s| s.short == "L8").unwrap(),
                "1 39084U 13008A   26293.96521410  .00000274  00000+0  70848-4 0  9999",
                "2 39084  98.2176   2.5182 0001284  93.9991 266.1354 14.57118128716214",
            ),
            sat(
                builtin().into_iter().find(|s| s.short == "ISS").unwrap(),
                "1 25544U 98067A   26293.66080995  .00008635  00000+0  16489-3 0  9997",
                "2 25544  51.6326  39.1774 0006750 287.9320  72.0932 15.48959761591200",
            ),
        ];
        let obs = Observer::new("Brasília", -15.7939, -47.8828, 1170.0);
        let start = Utc.with_ymd_and_hms(2026, 10, 21, 12, 0, 0).unwrap();
        let fc = forecast(&sats, &obs, start, start + Duration::days(1), 0.0);
        let expected = [
            (0, Utc.with_ymd_and_hms(2026, 10, 21, 13, 14, 42).unwrap(), 89.8),
            (1, Utc.with_ymd_and_hms(2026, 10, 21, 15, 55, 54).unwrap(), 37.8),
            (0, Utc.with_ymd_and_hms(2026, 10, 22, 1, 27, 46).unwrap(), 77.1),
        ];
        for (sat, tca, max_el) in expected {
            let p = fc.passes.iter().find(|p| p.sat == sat && (p.tca - tca).num_seconds().abs() <= 2);
            let p = p.unwrap_or_else(|| panic!("no pass of {} near {tca}", sats[sat].info.name));
            assert!((p.max_el - max_el).abs() < 0.15, "{} vs {max_el}", p.max_el);
        }
        // Landsat 8 culminates at 89.8°: the AOI is right under the track.
        let a = fc.acquisitions.iter().find(|a| a.sat == 0).unwrap();
        assert!(a.cross_km.abs() < 10.0 && !a.ascending);
    }

    #[test]
    fn root_finders() {
        assert!((bisect(&|t| t - 7.3, 0.0, 20.0) - 7.3).abs() <= TOLERANCE_S);
        assert!((golden_min(&|t| (t - 12.0).powi(2), 0.0, 40.0) - 12.0).abs() <= TOLERANCE_S);
    }
}
