//! Earth observation missions tracked by orbitui: the built-in list, in
//! groups that can be switched on and off, and the user's own, all kept in
//! `satellites.toml`.
//!
//! ```toml
//! groups = ["copernicus", "landsat", "sar"]   # built-in groups shown
//! exclude = [25544]       # built-in missions to hide, by NORAD id
//!
//! [[satellite]]
//! norad = 57320           # the only required field
//! name = "My EO sat"      # default: the name in the elements
//! sensor = "PAN"
//! swath_km = 120          # nadir-centred swath; 0 tracks without imaging
//! near_km = 250           # or explicit edges, positive right of the track
//! far_km = 480
//! off_nadir = 30          # or: an agile sensor pointed up to 30° either side
//! incidence = [20, 45]    # or: a radar between these incidence angles…
//! side = "right"          # …looking right, left or both
//! kind = "optical"        # optical | sar | lidar | station
//! color = "#ff8800"       # or a colour name
//! ```
//!
//! Swath widths, pointing limits and incidence ranges are the missions'
//! published nominal values; tasked sensors are modelled by their whole
//! field of regard (where they can be pointed), not by an acquisition plan.

use crate::astro;
use crate::platform;
use ratatui::style::Color;
use serde::{Deserialize, Serialize};

pub const USER_KEY: &str = "satellites.toml";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Passive optical sensor: images only when the target is sunlit.
    Optical,
    /// Synthetic aperture radar: images day and night, through clouds.
    Sar,
    /// Laser altimeter: works day and night, but not through clouds.
    Lidar,
    /// No imaging payload of interest, tracked for reference (ISS).
    Station,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Right,
    Left,
    Both,
}

/// How a sensor's ground coverage follows from the satellite's altitude.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Geometry {
    /// Edges given in km (nadir-centred swaths, explicit edges).
    Edges,
    /// An agile satellite pointing its swath up to `off_nadir` degrees to
    /// either side: the edges are its field of regard.
    Tasked { off_nadir: f64 },
    /// A side-looking radar between two incidence angles. `tasked` radars
    /// choose their beam per acquisition: the range is their field of regard.
    Radar { near_inc: f64, far_inc: f64, side: Side, tasked: bool },
}

#[derive(Clone, Copy, Debug)]
pub struct SatInfo {
    pub norad: u64,
    /// Empty for user satellites without a name: the elements' name is used.
    pub name: &'static str,
    /// Label drawn on the globe.
    pub short: &'static str,
    pub sensor: &'static str,
    /// Width of one acquisition in km.
    pub swath_km: f64,
    /// Coverage edges as signed cross-track ground distances in km, positive
    /// to the right of the direction of flight. Resolved from `geometry` and
    /// the satellite's altitude.
    pub near_km: f64,
    pub far_km: f64,
    /// The edges also apply mirrored to the left (radars looking both ways).
    pub both_sides: bool,
    pub geometry: Geometry,
    pub kind: Kind,
    pub color: Color,
    pub groups: &'static [&'static str],
    /// Added by the user, so it can be removed from the app.
    pub user: bool,
}

impl SatInfo {
    pub fn images(&self) -> bool {
        self.far_km > self.near_km
    }

    /// Whether a target at signed cross-track distance `x_km` can be imaged.
    pub fn covers(&self, x_km: f64) -> bool {
        let inside = |x: f64| x >= self.near_km && x <= self.far_km;
        self.images() && (inside(x_km) || self.both_sides && inside(-x_km))
    }

    /// Acquisitions must be requested (agile optics, tasked radars).
    pub fn tasked(&self) -> bool {
        matches!(self.geometry, Geometry::Tasked { .. } | Geometry::Radar { tasked: true, .. })
    }

    /// Edges for a satellite at `alt_km`.
    pub fn resolve(&mut self, alt_km: f64) {
        match self.geometry {
            Geometry::Edges => {}
            Geometry::Tasked { off_nadir } => {
                let reach = astro::ground_range_off_nadir(alt_km, off_nadir) + self.swath_km / 2.0;
                (self.near_km, self.far_km) = (-reach, reach);
            }
            Geometry::Radar { near_inc, far_inc, side, .. } => {
                let near = astro::ground_range_incidence(alt_km, near_inc);
                let far = astro::ground_range_incidence(alt_km, far_inc);
                (self.near_km, self.far_km) = match side {
                    Side::Left => (-far, -near),
                    Side::Right | Side::Both => (near, far),
                };
                self.both_sides = side == Side::Both;
            }
        }
    }

    /// How the coverage is obtained, for display.
    pub fn access(&self) -> String {
        match self.geometry {
            Geometry::Edges if !self.images() => "tracking only".into(),
            Geometry::Edges if self.near_km < 0.0 && self.far_km > 0.0 => format!("{:.0} km swath at nadir", self.swath_km),
            Geometry::Edges => format!("{:.0}–{:.0} km off track", self.near_km, self.far_km),
            Geometry::Tasked { off_nadir } => format!("tasked, pointed up to ±{off_nadir:.0}° off nadir"),
            Geometry::Radar { near_inc, far_inc, side, tasked } => format!(
                "{}incidence {near_inc:.0}–{far_inc:.0}°, {} looking",
                if tasked { "tasked, " } else { "" },
                match side {
                    Side::Right => "right",
                    Side::Left => "left",
                    Side::Both => "left or right",
                }
            ),
        }
    }
}

pub struct Group {
    pub id: &'static str,
    pub name: &'static str,
    pub default: bool,
}

pub const GROUPS: &[Group] = &[
    Group { id: "copernicus", name: "Copernicus Sentinels", default: true },
    Group { id: "landsat", name: "Landsat", default: true },
    Group { id: "cbers", name: "CBERS & Amazonia", default: true },
    Group { id: "modis-viirs", name: "MODIS & VIIRS", default: true },
    Group { id: "hyperspectral", name: "Hyperspectral (EnMAP, PRISMA…)", default: true },
    Group { id: "sar", name: "Radar (NISAR, ALOS, SAOCOM, RCM…)", default: false },
    Group { id: "china", name: "China (Gaofen, Ziyuan, HJ, LuTan)", default: false },
    Group { id: "vhr", name: "Very high resolution (Pléiades, WorldView…)", default: false },
    Group { id: "ocean", name: "Ocean, ice & atmosphere", default: false },
    Group { id: "india", name: "India (Resourcesat, Cartosat, EOS)", default: false },
    Group { id: "stations", name: "Space stations", default: true },
];

pub fn default_groups() -> Vec<String> {
    GROUPS.iter().filter(|g| g.default).map(|g| g.id.to_string()).collect()
}

use Kind::*;

#[allow(clippy::too_many_arguments)]
fn sat(
    norad: u64,
    name: &'static str,
    short: &'static str,
    sensor: &'static str,
    swath_km: f64,
    geometry: Geometry,
    kind: Kind,
    color: Color,
    groups: &'static [&'static str],
) -> SatInfo {
    let half = swath_km / 2.0;
    let mut info = SatInfo {
        norad,
        name,
        short,
        sensor,
        swath_km,
        near_km: -half,
        far_km: half,
        both_sides: false,
        geometry,
        kind,
        color,
        groups,
        user: false,
    };
    // Until the elements give the real altitude.
    info.resolve(700.0);
    info
}

const NADIR: Geometry = Geometry::Edges;

const fn tasked(off_nadir: f64) -> Geometry {
    Geometry::Tasked { off_nadir }
}

/// Tasked radars: the beams their modes offer, on the sides they can look.
const fn radar(near_inc: f64, far_inc: f64, side: Side) -> Geometry {
    Geometry::Radar { near_inc, far_inc, side, tasked: true }
}

/// Radars acquiring systematically in one geometry.
const fn radar_fixed(near_inc: f64, far_inc: f64, side: Side) -> Geometry {
    Geometry::Radar { near_inc, far_inc, side, tasked: false }
}

const COP: &[&str] = &["copernicus"];
const LAN: &[&str] = &["landsat"];
const CB: &[&str] = &["cbers"];
const MV: &[&str] = &["modis-viirs"];
const HYP: &[&str] = &["hyperspectral"];
const HYP_CN: &[&str] = &["hyperspectral", "china"];
const SAR: &[&str] = &["sar"];
const SAR_CN: &[&str] = &["sar", "china"];
const SAR_IN: &[&str] = &["sar", "india"];
const CN: &[&str] = &["china"];
const VHR: &[&str] = &["vhr"];
const VHR_IN: &[&str] = &["vhr", "india"];
const OCE: &[&str] = &["ocean"];
const OCE_CN: &[&str] = &["ocean", "china"];
const OCE_IN: &[&str] = &["ocean", "india"];
const IN: &[&str] = &["india"];
const ST: &[&str] = &["stations"];

const HYP_COLOR: Color = Color::Indexed(213);
const SAR_COLOR: Color = Color::Indexed(141);
const CN_COLOR: Color = Color::Indexed(203);
const VHR_COLOR: Color = Color::Indexed(117);
const OCE_COLOR: Color = Color::Indexed(80);
const IN_COLOR: Color = Color::Indexed(215);

/// The missions available out of the box, in every group.
pub fn builtin() -> Vec<SatInfo> {
    use Side::*;
    // Sentinel-1 Interferometric Wide swath: incidence 29.1°–46.0°, right.
    let s1 = radar_fixed(29.1, 46.0, Right);
    vec![
        sat(40697, "Sentinel-2A", "S2A", "MSI", 290.0, NADIR, Optical, Color::LightGreen, COP),
        sat(42063, "Sentinel-2B", "S2B", "MSI", 290.0, NADIR, Optical, Color::LightGreen, COP),
        sat(60989, "Sentinel-2C", "S2C", "MSI", 290.0, NADIR, Optical, Color::LightGreen, COP),
        sat(39634, "Sentinel-1A", "S1A", "C-SAR IW", 250.0, s1, Sar, Color::LightMagenta, COP),
        sat(62261, "Sentinel-1C", "S1C", "C-SAR IW", 250.0, s1, Sar, Color::LightMagenta, COP),
        sat(66315, "Sentinel-1D", "S1D", "C-SAR IW", 250.0, s1, Sar, Color::LightMagenta, COP),
        sat(41335, "Sentinel-3A", "S3A", "OLCI", 1270.0, NADIR, Optical, Color::Yellow, COP),
        sat(43437, "Sentinel-3B", "S3B", "OLCI", 1270.0, NADIR, Optical, Color::Yellow, COP),
        sat(42969, "Sentinel-5P", "S5P", "TROPOMI", 2600.0, NADIR, Optical, Color::Yellow, COP),
        sat(39084, "Landsat 8", "L8", "OLI/TIRS", 185.0, NADIR, Optical, Color::LightCyan, LAN),
        sat(49260, "Landsat 9", "L9", "OLI-2/TIRS-2", 185.0, NADIR, Optical, Color::LightCyan, LAN),
        sat(40336, "CBERS-4", "CB4", "WFI", 866.0, NADIR, Optical, Color::LightBlue, CB),
        sat(44883, "CBERS-4A", "CB4A", "WFI", 684.0, NADIR, Optical, Color::LightBlue, CB),
        sat(47699, "Amazonia-1", "AMZ1", "WFI", 850.0, NADIR, Optical, Color::LightBlue, CB),
        sat(25994, "Terra", "TERRA", "MODIS", 2330.0, NADIR, Optical, Color::LightRed, MV),
        sat(27424, "Aqua", "AQUA", "MODIS", 2330.0, NADIR, Optical, Color::LightRed, MV),
        sat(37849, "Suomi NPP", "SNPP", "VIIRS", 3060.0, NADIR, Optical, Color::Red, MV),
        sat(43013, "NOAA-20", "N20", "VIIRS", 3060.0, NADIR, Optical, Color::Red, MV),
        sat(54234, "NOAA-21", "N21", "VIIRS", 3060.0, NADIR, Optical, Color::Red, MV),
        // Hyperspectral imagers.
        sat(52159, "EnMAP", "ENMAP", "HSI", 30.0, tasked(30.0), Optical, HYP_COLOR, HYP),
        sat(44072, "PRISMA", "PRISM", "HYC", 30.0, tasked(14.7), Optical, HYP_COLOR, HYP),
        sat(60507, "Tanager-1", "TAN1", "VSWIR", 18.0, tasked(30.0), Optical, HYP_COLOR, HYP),
        sat(100886, "Tanager-2", "TAN2", "VSWIR", 18.0, tasked(30.0), Optical, HYP_COLOR, HYP),
        sat(49122, "Gaofen-5 02", "GF5-2", "AHSI", 60.0, NADIR, Optical, HYP_COLOR, HYP_CN),
        sat(54640, "Gaofen-5 01A", "GF5-1A", "AHSI", 60.0, NADIR, Optical, HYP_COLOR, HYP_CN),
        sat(44528, "ZY-1 02D", "ZY1-2D", "AHSI", 60.0, NADIR, Optical, HYP_COLOR, HYP_CN),
        sat(50465, "ZY-1 02E", "ZY1-2E", "AHSI", 60.0, NADIR, Optical, HYP_COLOR, HYP_CN),
        // Radars. NISAR looks left for the whole mission (Antarctic coverage).
        sat(65053, "NISAR", "NISAR", "L-SAR", 242.0, radar_fixed(33.0, 47.0, Left), Sar, SAR_COLOR, SAR),
        sat(63772, "Biomass", "BIOM", "P-SAR", 50.0, radar_fixed(23.0, 34.0, Both), Sar, SAR_COLOR, SAR),
        sat(39766, "ALOS-2", "ALOS2", "PALSAR-2", 70.0, radar(20.0, 50.0, Both), Sar, SAR_COLOR, SAR),
        sat(60182, "ALOS-4", "ALOS4", "PALSAR-3", 200.0, radar(20.0, 50.0, Both), Sar, SAR_COLOR, SAR),
        sat(43641, "SAOCOM 1A", "SAO1A", "L-SAR", 50.0, radar(20.0, 50.0, Both), Sar, SAR_COLOR, SAR),
        sat(46265, "SAOCOM 1B", "SAO1B", "L-SAR", 50.0, radar(20.0, 50.0, Both), Sar, SAR_COLOR, SAR),
        sat(44322, "RCM-1", "RCM1", "C-SAR", 350.0, radar(19.0, 54.0, Right), Sar, SAR_COLOR, SAR),
        sat(44324, "RCM-2", "RCM2", "C-SAR", 350.0, radar(19.0, 54.0, Right), Sar, SAR_COLOR, SAR),
        sat(44323, "RCM-3", "RCM3", "C-SAR", 350.0, radar(19.0, 54.0, Right), Sar, SAR_COLOR, SAR),
        sat(31698, "TerraSAR-X", "TSX", "X-SAR", 30.0, radar(20.0, 45.0, Both), Sar, SAR_COLOR, SAR),
        sat(36605, "TanDEM-X", "TDX", "X-SAR", 30.0, radar(20.0, 45.0, Both), Sar, SAR_COLOR, SAR),
        sat(43215, "PAZ", "PAZ", "X-SAR", 30.0, radar(20.0, 45.0, Both), Sar, SAR_COLOR, SAR),
        sat(44873, "CSG-1", "CSG1", "X-SAR", 40.0, radar(20.0, 50.0, Both), Sar, SAR_COLOR, SAR),
        sat(51444, "CSG-2", "CSG2", "X-SAR", 40.0, radar(20.0, 50.0, Both), Sar, SAR_COLOR, SAR),
        sat(67304, "CSG-3", "CSG3", "X-SAR", 40.0, radar(20.0, 50.0, Both), Sar, SAR_COLOR, SAR),
        sat(39227, "KOMPSAT-5", "K5", "X-SAR", 30.0, radar(20.0, 45.0, Both), Sar, SAR_COLOR, SAR),
        sat(41727, "Gaofen-3", "GF3", "C-SAR", 130.0, radar(20.0, 50.0, Both), Sar, SAR_COLOR, SAR_CN),
        sat(49495, "Gaofen-3 02", "GF3-2", "C-SAR", 130.0, radar(20.0, 50.0, Both), Sar, SAR_COLOR, SAR_CN),
        sat(52200, "Gaofen-3 03", "GF3-3", "C-SAR", 130.0, radar(20.0, 50.0, Both), Sar, SAR_COLOR, SAR_CN),
        sat(51284, "LuTan-1 A", "LT1A", "L-SAR", 50.0, radar(20.0, 46.0, Both), Sar, SAR_COLOR, SAR_CN),
        sat(51822, "LuTan-1 B", "LT1B", "L-SAR", 50.0, radar(20.0, 46.0, Both), Sar, SAR_COLOR, SAR_CN),
        sat(54035, "HJ-2E", "HJ2E", "S-SAR", 100.0, radar(20.0, 50.0, Both), Sar, SAR_COLOR, SAR_CN),
        sat(57519, "HJ-2F", "HJ2F", "S-SAR", 100.0, radar(20.0, 50.0, Both), Sar, SAR_COLOR, SAR_CN),
        sat(51656, "EOS-04", "EOS4", "C-SAR", 140.0, radar(20.0, 50.0, Both), Sar, SAR_COLOR, SAR_IN),
        // China, optical.
        sat(39150, "Gaofen-1", "GF1", "WFV", 800.0, NADIR, Optical, CN_COLOR, CN),
        sat(43259, "Gaofen-1 02", "GF1-2", "PMS", 66.0, NADIR, Optical, CN_COLOR, CN),
        sat(43260, "Gaofen-1 03", "GF1-3", "PMS", 66.0, NADIR, Optical, CN_COLOR, CN),
        sat(43262, "Gaofen-1 04", "GF1-4", "PMS", 66.0, NADIR, Optical, CN_COLOR, CN),
        sat(40118, "Gaofen-2", "GF2", "PMS", 45.0, NADIR, Optical, CN_COLOR, CN),
        sat(43484, "Gaofen-6", "GF6", "WFV", 800.0, NADIR, Optical, CN_COLOR, CN),
        sat(44703, "Gaofen-7", "GF7", "DLC", 20.0, NADIR, Optical, CN_COLOR, CN),
        sat(41556, "ZY-3 02", "ZY3-2", "TLC", 51.0, NADIR, Optical, CN_COLOR, CN),
        sat(45939, "ZY-3 03", "ZY3-3", "TLC", 51.0, NADIR, Optical, CN_COLOR, CN),
        sat(67131, "ZY-3 04", "ZY3-4", "TLC", 51.0, NADIR, Optical, CN_COLOR, CN),
        sat(46478, "HJ-2A", "HJ2A", "CCD", 800.0, NADIR, Optical, CN_COLOR, CN),
        sat(46479, "HJ-2B", "HJ2B", "CCD", 800.0, NADIR, Optical, CN_COLOR, CN),
        // Very high resolution, tasked.
        sat(38012, "Pléiades 1A", "PHR1A", "HiRI", 20.0, tasked(30.0), Optical, VHR_COLOR, VHR),
        sat(39019, "Pléiades 1B", "PHR1B", "HiRI", 20.0, tasked(30.0), Optical, VHR_COLOR, VHR),
        sat(48268, "Pléiades Neo 3", "PNEO3", "PNEO", 14.0, tasked(30.0), Optical, VHR_COLOR, VHR),
        sat(49070, "Pléiades Neo 4", "PNEO4", "PNEO", 14.0, tasked(30.0), Optical, VHR_COLOR, VHR),
        sat(38755, "SPOT 6", "SPOT6", "NAOMI", 60.0, tasked(30.0), Optical, VHR_COLOR, VHR),
        sat(40053, "SPOT 7", "SPOT7", "NAOMI", 60.0, tasked(30.0), Optical, VHR_COLOR, VHR),
        sat(40115, "WorldView-3", "WV3", "WV110", 13.1, tasked(30.0), Optical, VHR_COLOR, VHR),
        sat(38338, "KOMPSAT-3", "K3", "AEISS", 16.0, tasked(30.0), Optical, VHR_COLOR, VHR),
        sat(40536, "KOMPSAT-3A", "K3A", "AEISS-A", 12.0, tasked(30.0), Optical, VHR_COLOR, VHR),
        sat(44804, "Cartosat-3", "CS3", "PAN/MX", 16.0, tasked(26.0), Optical, VHR_COLOR, VHR_IN),
        // Ocean colour, ice and atmosphere.
        sat(58928, "PACE", "PACE", "OCI", 2663.0, NADIR, Optical, OCE_COLOR, OCE),
        sat(43065, "GCOM-C", "GCOMC", "SGLI", 1150.0, NADIR, Optical, OCE_COLOR, OCE),
        sat(43609, "HY-1C", "HY1C", "COCTS", 2900.0, NADIR, Optical, OCE_COLOR, OCE_CN),
        sat(45721, "HY-1D", "HY1D", "COCTS", 2900.0, NADIR, Optical, OCE_COLOR, OCE_CN),
        sat(54361, "Oceansat-3", "OS3", "OCM-3", 1400.0, NADIR, Optical, OCE_COLOR, OCE_IN),
        sat(43613, "ICESat-2", "ICES2", "ATLAS", 6.6, NADIR, Lidar, OCE_COLOR, OCE),
        // KaRIn's two 50 km swaths straddle a 20 km nadir gap, drawn as one.
        sat(54754, "SWOT", "SWOT", "KaRIn", 120.0, NADIR, Sar, OCE_COLOR, OCE),
        // India.
        sat(37387, "Resourcesat-2", "RS2", "LISS-III", 141.0, NADIR, Optical, IN_COLOR, IN),
        sat(41877, "Resourcesat-2A", "RS2A", "LISS-III", 141.0, NADIR, Optical, IN_COLOR, IN),
        sat(25544, "ISS", "ISS", "-", 0.0, NADIR, Station, Color::White, ST),
    ]
}

fn yes() -> bool {
    true
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct UserFile {
    /// Kept for files written before groups existed: false hides every group.
    #[serde(default = "yes", skip_serializing)]
    pub builtin: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<u64>,
    #[serde(default, rename = "satellite", skip_serializing_if = "Vec::is_empty")]
    pub satellites: Vec<UserSat>,
}

impl UserFile {
    pub fn enabled_groups(&self) -> Vec<String> {
        match (&self.groups, self.builtin) {
            (_, false) => Vec::new(),
            (Some(g), true) => g.clone(),
            (None, true) => default_groups(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct UserSat {
    pub norad: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swath_km: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub near_km: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub far_km: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub off_nadir: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incidence: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Colours given to user satellites without one, in turn.
const USER_COLORS: [Color; 6] =
    [Color::Rgb(255, 136, 0), Color::Rgb(0, 200, 160), Color::Rgb(220, 120, 255), Color::Rgb(255, 210, 90), Color::Rgb(120, 170, 255), Color::Rgb(255, 120, 150)];

/// User strings live as long as the program, like the built-in ones.
fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#')
        && hex.len() == 6
    {
        let v = u32::from_str_radix(hex, 16).ok()?;
        return Some(Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8));
    }
    s.parse().ok()
}

impl UserSat {
    fn info(&self, index: usize) -> Result<SatInfo, String> {
        let id = self.norad;
        let kind = match self.kind.as_deref().map(str::to_ascii_lowercase).as_deref() {
            None | Some("optical") => Kind::Optical,
            Some("sar" | "radar") => Kind::Sar,
            Some("lidar") => Kind::Lidar,
            Some("station" | "track") => Kind::Station,
            Some(other) => return Err(format!("satellite {id}: unknown kind \"{other}\"")),
        };
        let side = match self.side.as_deref().map(str::to_ascii_lowercase).as_deref() {
            None | Some("right") => Side::Right,
            Some("left") => Side::Left,
            Some("both") => Side::Both,
            Some(other) => return Err(format!("satellite {id}: side must be right, left or both, not \"{other}\"")),
        };
        let swath = self.swath_km.unwrap_or(0.0);
        let (geometry, near_km, far_km) = match (self.incidence, self.off_nadir, self.near_km, self.far_km) {
            (Some([a, b]), ..) if a < b => (radar(a, b, side), 0.0, 0.0),
            (Some(_), ..) => return Err(format!("satellite {id}: incidence must be [near, far]")),
            (None, Some(o), ..) => (tasked(o), 0.0, 0.0),
            (None, None, Some(n), Some(f)) if n <= f => (Geometry::Edges, n, f),
            (None, None, Some(_), Some(_)) => return Err(format!("satellite {id}: far_km is smaller than near_km")),
            _ => (Geometry::Edges, -swath / 2.0, swath / 2.0),
        };
        let color = match &self.color {
            Some(c) => parse_color(c).ok_or_else(|| format!("satellite {id}: unknown colour \"{c}\""))?,
            None => USER_COLORS[index % USER_COLORS.len()],
        };
        let name = self.name.clone().unwrap_or_default();
        let short = self.short.clone().unwrap_or_else(|| short_label(&name, id));
        let mut info = SatInfo {
            norad: id,
            name: leak(name),
            short: leak(short),
            sensor: leak(self.sensor.clone().unwrap_or_else(|| "-".into())),
            swath_km: self.swath_km.unwrap_or(far_km - near_km),
            near_km,
            far_km,
            both_sides: false,
            geometry,
            kind,
            color,
            groups: &[],
            user: true,
        };
        info.resolve(700.0);
        if !info.images() {
            info.kind = Kind::Station;
        }
        Ok(info)
    }
}

/// A short label from a name: "Jason-3" gives "JASON".
pub fn short_label(name: &str, norad: u64) -> String {
    let label: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).take(5).collect::<String>().to_uppercase();
    if label.is_empty() { norad.to_string() } else { label }
}

pub fn read_user() -> Result<UserFile, String> {
    match platform::load(USER_KEY) {
        Some(text) => toml::from_str(&text).map_err(|e| format!("{USER_KEY}: {}", e.message())),
        None => Ok(UserFile { builtin: true, ..Default::default() }),
    }
}

pub fn write_user(file: &UserFile) {
    let header = "# orbitui satellites; see https://github.com/sacridini/orbitui#satellites\n";
    if let Ok(text) = toml::to_string_pretty(file) {
        platform::save(USER_KEY, &format!("{header}{text}"));
    }
}

/// The built-in missions of the enabled groups, minus those excluded, plus
/// the user's; a user entry with a built-in NORAD id replaces it. Errors
/// leave the default groups.
pub fn load() -> (Vec<SatInfo>, Option<String>) {
    let file = match read_user() {
        Ok(f) => f,
        Err(e) => {
            let groups = default_groups();
            let list = builtin().into_iter().filter(|s| s.groups.iter().any(|g| groups.iter().any(|e| e == g))).collect();
            return (list, Some(e));
        }
    };
    let groups = file.enabled_groups();
    let mut list: Vec<SatInfo> = builtin()
        .into_iter()
        .filter(|s| s.groups.iter().any(|g| groups.iter().any(|e| e == g)))
        .filter(|s| !file.exclude.contains(&s.norad) && !file.satellites.iter().any(|u| u.norad == s.norad))
        .collect();
    for (i, user) in file.satellites.iter().enumerate() {
        match user.info(i) {
            Ok(info) => list.push(info),
            Err(e) => return (list, Some(e)),
        }
    }
    (list, None)
}

/// Saves the groups to show.
pub fn set_groups(groups: Vec<String>) -> Result<(), String> {
    let mut file = read_user()?;
    file.builtin = true;
    file.groups = Some(groups);
    write_user(&file);
    Ok(())
}

/// Parses the in-app form: `NORAD [swath | near..far] [optical|sar|lidar|station] [name]`.
pub fn parse_add(input: &str) -> Result<UserSat, String> {
    let mut tokens = input.split_whitespace();
    let norad = tokens
        .next()
        .and_then(|t| t.parse().ok())
        .ok_or("expected: NORAD [swath km | near..far] [optical|sar|lidar|station] [name]")?;
    let mut sat = UserSat { norad, ..Default::default() };
    let mut name = Vec::new();
    for t in tokens {
        if !name.is_empty() {
            name.push(t);
        } else if let Some((a, b)) = t.split_once("..") {
            let (a, b) = (a.parse().map_err(|_| format!("bad edge \"{a}\""))?, b.parse().map_err(|_| format!("bad edge \"{b}\""))?);
            (sat.near_km, sat.far_km) = (Some(a), Some(b));
        } else if let Ok(w) = t.parse::<f64>() {
            sat.swath_km = Some(w);
        } else if sat.kind.is_none() && matches!(t.to_ascii_lowercase().as_str(), "optical" | "sar" | "radar" | "lidar" | "station" | "track") {
            sat.kind = Some(t.to_ascii_lowercase());
        } else {
            name.push(t);
        }
    }
    if !name.is_empty() {
        sat.name = Some(name.join(" "));
    }
    sat.info(0)?;
    Ok(sat)
}

/// Adds (or replaces) a user satellite and saves the file.
pub fn add_user(sat: UserSat) -> Result<(), String> {
    let mut file = read_user()?;
    file.satellites.retain(|s| s.norad != sat.norad);
    file.satellites.push(sat);
    write_user(&file);
    Ok(())
}

/// Removes a user satellite, or hides a built-in one.
pub fn remove(norad: u64) -> Result<(), String> {
    let mut file = read_user()?;
    let before = file.satellites.len();
    file.satellites.retain(|s| s.norad != norad);
    if file.satellites.len() == before && !file.exclude.contains(&norad) {
        file.exclude.push(norad);
    }
    write_user(&file);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_add_form() {
        let s = parse_add("41240 0 Jason-3").unwrap();
        assert_eq!(parse_add("1 50 optical Station cam").unwrap().name.as_deref(), Some("Station cam"));
        assert_eq!((s.norad, s.swath_km, s.name.as_deref()), (41240, Some(0.0), Some("Jason-3")));
        let s = parse_add("39634 340..620 sar My radar").unwrap();
        assert_eq!((s.near_km, s.far_km, s.kind.as_deref()), (Some(340.0), Some(620.0), Some("sar")));
        let info = s.info(0).unwrap();
        assert!(info.covers(400.0) && !info.covers(0.0) && info.kind == Kind::Sar);
        assert!(parse_add("abc").is_err());
        assert!(parse_add("1 500..100").is_err());
    }

    #[test]
    fn user_file_round_trip() {
        let text = "exclude = [25544]\n[[satellite]]\nnorad = 57320\nswath_km = 120\ncolor = \"#ff8800\"\n\
                    [[satellite]]\nnorad = 1\noff_nadir = 30\nswath_km = 20\n\
                    [[satellite]]\nnorad = 2\nincidence = [20, 45]\nside = \"both\"\nkind = \"sar\"\n";
        let file: UserFile = toml::from_str(text).unwrap();
        assert!(file.builtin && file.enabled_groups() == default_groups());
        let info = file.satellites[0].info(0).unwrap();
        assert_eq!(info.color, Color::Rgb(255, 136, 0));
        assert_eq!((info.near_km, info.far_km, info.short), (-60.0, 60.0, "57320"));
        assert!(file.satellites[1].info(1).unwrap().tasked());
        let radar = file.satellites[2].info(2).unwrap();
        assert!(radar.covers(-400.0) && radar.covers(400.0) && !radar.covers(0.0));
        let again: UserFile = toml::from_str(&toml::to_string(&file).unwrap()).unwrap();
        assert_eq!(again.satellites, file.satellites);
    }

    #[test]
    fn geometry_from_altitude() {
        // Sentinel-1 IW from 693 km: ~340–620 km right of the track.
        let mut s1 = builtin().into_iter().find(|s| s.short == "S1A").unwrap();
        s1.resolve(693.0);
        assert!((s1.near_km - 343.0).abs() < 10.0 && (s1.far_km - 617.0).abs() < 10.0, "{} {}", s1.near_km, s1.far_km);
        assert!(s1.covers(450.0) && !s1.covers(-450.0));
        // NISAR looks left.
        let nisar = builtin().into_iter().find(|s| s.short == "NISAR").unwrap();
        assert!(nisar.covers(-450.0) && !nisar.covers(450.0));
        // EnMAP from 643 km reaches ~370 km either side, plus half its swath.
        let mut enmap = builtin().into_iter().find(|s| s.short == "ENMAP").unwrap();
        enmap.resolve(643.0);
        assert!((enmap.far_km - 400.0).abs() < 20.0, "{}", enmap.far_km);
        assert!(enmap.tasked() && enmap.covers(-390.0));
    }

    #[test]
    fn every_builtin_belongs_to_a_known_group() {
        let list = builtin();
        for s in &list {
            assert!(s.groups.iter().all(|g| GROUPS.iter().any(|k| k.id == *g)), "{}", s.name);
            assert!(!s.groups.is_empty(), "{}", s.name);
        }
        let mut ids: Vec<u64> = list.iter().map(|s| s.norad).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), list.len(), "duplicate NORAD id");
    }

    #[test]
    fn colours() {
        assert_eq!(parse_color("magenta"), Some(Color::Magenta));
        assert_eq!(parse_color("#00ff00"), Some(Color::Rgb(0, 255, 0)));
        assert_eq!(parse_color("nope"), None);
    }
}
