//! Earth observation missions tracked by orbitui: the built-in list and the
//! user's own, kept in `satellites.toml`.
//!
//! ```toml
//! builtin = true          # keep the built-in missions (default)
//! exclude = [25544]       # built-in missions to hide, by NORAD id
//!
//! [[satellite]]
//! norad = 57320           # the only required field
//! name = "My EO sat"      # default: the name in the TLE
//! sensor = "PAN"
//! swath_km = 120          # nadir-centred swath; 0 tracks without imaging
//! near_km = 250           # or explicit edges, positive right of the track
//! far_km = 480
//! kind = "optical"        # optical | sar | station
//! color = "#ff8800"       # or a colour name
//! ```

use crate::platform;
use ratatui::style::Color;
use serde::{Deserialize, Serialize};

pub const USER_KEY: &str = "satellites.toml";

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
    /// Empty for user satellites without a name: the TLE's name is used.
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
    /// Added by the user, so it can be removed from the app.
    pub user: bool,
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
    SatInfo { norad, name, short, sensor, swath_km, near_km: -half, far_km: half, kind, color, user: false }
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
        user: false,
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

fn yes() -> bool {
    true
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct UserFile {
    #[serde(default = "yes")]
    pub builtin: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<u64>,
    #[serde(default, rename = "satellite", skip_serializing_if = "Vec::is_empty")]
    pub satellites: Vec<UserSat>,
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
        let kind = match self.kind.as_deref().map(str::to_ascii_lowercase).as_deref() {
            None | Some("optical") => Kind::Optical,
            Some("sar" | "radar") => Kind::Sar,
            Some("station" | "track") => Kind::Station,
            Some(other) => return Err(format!("satellite {}: unknown kind \"{other}\"", self.norad)),
        };
        let (near_km, far_km) = match (self.near_km, self.far_km, self.swath_km) {
            (Some(n), Some(f), _) => (n, f),
            (_, _, Some(w)) => (-w / 2.0, w / 2.0),
            _ => (0.0, 0.0),
        };
        if far_km < near_km {
            return Err(format!("satellite {}: far_km is smaller than near_km", self.norad));
        }
        let color = match &self.color {
            Some(c) => parse_color(c).ok_or_else(|| format!("satellite {}: unknown colour \"{c}\"", self.norad))?,
            None => USER_COLORS[index % USER_COLORS.len()],
        };
        let name = self.name.clone().unwrap_or_default();
        let short = self.short.clone().unwrap_or_else(|| short_label(&name, self.norad));
        Ok(SatInfo {
            norad: self.norad,
            name: leak(name),
            short: leak(short),
            sensor: leak(self.sensor.clone().unwrap_or_else(|| "-".into())),
            swath_km: self.swath_km.unwrap_or(far_km - near_km),
            near_km,
            far_km,
            kind: if far_km > near_km { kind } else { Kind::Station },
            color,
            user: true,
        })
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

/// The built-in missions, minus those excluded, plus the user's; a user
/// entry with a built-in NORAD id replaces it. Errors leave the built-in list.
pub fn load() -> (Vec<SatInfo>, Option<String>) {
    let file = match read_user() {
        Ok(f) => f,
        Err(e) => return (builtin(), Some(e)),
    };
    let mut list: Vec<SatInfo> = if file.builtin { builtin() } else { Vec::new() };
    list.retain(|s| !file.exclude.contains(&s.norad) && !file.satellites.iter().any(|u| u.norad == s.norad));
    for (i, user) in file.satellites.iter().enumerate() {
        match user.info(i) {
            Ok(info) => list.push(info),
            Err(e) => return (list, Some(e)),
        }
    }
    (list, None)
}

/// Parses the in-app form: `NORAD [swath | near..far] [optical|sar|station] [name]`.
pub fn parse_add(input: &str) -> Result<UserSat, String> {
    let mut tokens = input.split_whitespace();
    let norad = tokens
        .next()
        .and_then(|t| t.parse().ok())
        .ok_or("expected: NORAD [swath km | near..far] [optical|sar|station] [name]")?;
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
        } else if sat.kind.is_none() && matches!(t.to_ascii_lowercase().as_str(), "optical" | "sar" | "radar" | "station" | "track") {
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
        let text = "exclude = [25544]\n[[satellite]]\nnorad = 57320\nswath_km = 120\ncolor = \"#ff8800\"\n";
        let file: UserFile = toml::from_str(text).unwrap();
        assert!(file.builtin);
        let info = file.satellites[0].info(0).unwrap();
        assert_eq!(info.color, Color::Rgb(255, 136, 0));
        assert_eq!((info.near_km, info.far_km, info.short), (-60.0, 60.0, "57320"));
        let again: UserFile = toml::from_str(&toml::to_string(&file).unwrap()).unwrap();
        assert_eq!(again.satellites, file.satellites);
    }

    #[test]
    fn colours() {
        assert_eq!(parse_color("magenta"), Some(Color::Magenta));
        assert_eq!(parse_color("#00ff00"), Some(Color::Rgb(0, 255, 0)));
        assert_eq!(parse_color("nope"), None);
    }
}
