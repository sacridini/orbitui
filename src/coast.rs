//! Natural Earth coastlines (public domain), embedded in the binary.
//!
//! `tools/build_coastlines.py` writes the format read here.

use crate::astro::{self, Vec3};

static COAST_110M: &[u8] = include_bytes!("../assets/coast110.bin");
static COAST_50M: &[u8] = include_bytes!("../assets/coast50.bin");

pub struct Polyline {
    /// (lat, lon) in degrees.
    pub points: Vec<(f64, f64)>,
    /// Unit vectors of the points, for projecting onto the globe.
    pub xyz: Vec<Vec3>,
}

pub struct Coastlines {
    pub lines: Vec<Polyline>,
}

pub struct World {
    pub coarse: Coastlines,
    pub fine: Coastlines,
}

impl World {
    pub fn load() -> Self {
        World { coarse: decode(COAST_110M), fine: decode(COAST_50M) }
    }
}

fn decode(bytes: &[u8]) -> Coastlines {
    assert_eq!(&bytes[..4], b"SWC1", "bad coastline asset");
    let u32_at = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap()) as usize;
    let i16_at = |i: usize| i16::from_le_bytes(bytes[i..i + 2].try_into().unwrap()) as f64 / 100.0;
    let count = u32_at(4);
    let mut pos = 8;
    let mut lines = Vec::with_capacity(count);
    for _ in 0..count {
        let n = u32_at(pos);
        pos += 4;
        let points: Vec<(f64, f64)> = (0..n).map(|k| (i16_at(pos + 4 * k + 2), i16_at(pos + 4 * k))).collect();
        pos += 4 * n;
        let xyz = points.iter().map(|&(lat, lon)| astro::unit_vector(lat, lon)).collect();
        lines.push(Polyline { points, xyz });
    }
    Coastlines { lines }
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_assets() {
        let world = super::World::load();
        assert_eq!(world.coarse.lines.len(), 134);
        assert_eq!(world.fine.lines.iter().map(|l| l.points.len()).sum::<usize>(), 60416);
        assert!(world.fine.lines.iter().flat_map(|l| &l.points).all(|&(lat, lon)| lat.abs() <= 90.0 && lon.abs() <= 180.0));
    }
}
