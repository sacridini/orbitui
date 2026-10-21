//! Earth observation missions tracked by orbitui.

use ratatui::style::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Passive optical sensor: images only when the target is sunlit.
    Optical,
    /// Synthetic aperture radar: images day and night.
    Sar,
    /// No imaging payload of interest, tracked for reference (ISS).
    Station,
}

#[derive(Clone, Copy, Debug)]
pub struct SatInfo {
    pub norad: u64,
    pub name: &'static str,
    /// Label drawn on the globe.
    pub short: &'static str,
    pub sensor: &'static str,
    /// Full ground swath of the sensor in km.
    pub swath_km: f64,
    /// Swath edges as signed cross-track ground distances in km, positive to
    /// the right of the direction of flight. Nadir-looking sensors are
    /// centred on the track; side-looking radars are offset.
    pub near_km: f64,
    pub far_km: f64,
    pub kind: Kind,
    pub color: Color,
}

impl SatInfo {
    pub fn images(&self) -> bool {
        self.far_km > self.near_km
    }

    /// Whether a target at signed cross-track distance `x_km` is in the swath.
    pub fn covers(&self, x_km: f64) -> bool {
        self.images() && x_km >= self.near_km && x_km <= self.far_km
    }
}

const fn sat(
    norad: u64,
    name: &'static str,
    short: &'static str,
    sensor: &'static str,
    swath_km: f64,
    kind: Kind,
    color: Color,
) -> SatInfo {
    let half = swath_km / 2.0;
    SatInfo { norad, name, short, sensor, swath_km, near_km: -half, far_km: half, kind, color }
}

/// Sentinel-1 is right-looking. Interferometric Wide swath spans incidence
/// angles 29.1°–46.0°, which from 693 km is ~340–620 km off track.
const fn sentinel_1(norad: u64, name: &'static str, short: &'static str) -> SatInfo {
    SatInfo {
        norad,
        name,
        short,
        sensor: "C-SAR IW",
        swath_km: 250.0,
        near_km: 340.0,
        far_km: 620.0,
        kind: Kind::Sar,
        color: Color::LightMagenta,
    }
}

use Kind::*;

pub const CATALOG: &[SatInfo] = &[
    sat(40697, "Sentinel-2A", "S2A", "MSI", 290.0, Optical, Color::LightGreen),
    sat(42063, "Sentinel-2B", "S2B", "MSI", 290.0, Optical, Color::LightGreen),
    sat(60989, "Sentinel-2C", "S2C", "MSI", 290.0, Optical, Color::LightGreen),
    sat(39084, "Landsat 8", "L8", "OLI/TIRS", 185.0, Optical, Color::LightCyan),
    sat(49260, "Landsat 9", "L9", "OLI-2/TIRS-2", 185.0, Optical, Color::LightCyan),
    sentinel_1(39634, "Sentinel-1A", "S1A"),
    sentinel_1(62261, "Sentinel-1C", "S1C"),
    sentinel_1(66315, "Sentinel-1D", "S1D"),
    sat(41335, "Sentinel-3A", "S3A", "OLCI", 1270.0, Optical, Color::Yellow),
    sat(43437, "Sentinel-3B", "S3B", "OLCI", 1270.0, Optical, Color::Yellow),
    sat(40336, "CBERS-4", "CB4", "WFI", 866.0, Optical, Color::LightBlue),
    sat(44883, "CBERS-4A", "CB4A", "WFI", 684.0, Optical, Color::LightBlue),
    sat(47699, "Amazonia-1", "AMZ1", "WFI", 850.0, Optical, Color::LightBlue),
    sat(25994, "Terra", "TERRA", "MODIS", 2330.0, Optical, Color::LightRed),
    sat(27424, "Aqua", "AQUA", "MODIS", 2330.0, Optical, Color::LightRed),
    sat(37849, "Suomi NPP", "SNPP", "VIIRS", 3060.0, Optical, Color::Red),
    sat(43013, "NOAA-20", "N20", "VIIRS", 3060.0, Optical, Color::Red),
    sat(54234, "NOAA-21", "N21", "VIIRS", 3060.0, Optical, Color::Red),
    sat(25544, "ISS", "ISS", "-", 0.0, Station, Color::White),
];

/// The missions tracked out of the box.
pub fn builtin() -> Vec<SatInfo> {
    CATALOG.to_vec()
}
