# Changelog

## [0.3.0] - 2026-10-21

### Added

- Installation from the terminal: `curl … | sh` installer, Homebrew tap
  (`brew install sacridini/tap/orbitui`) and a signed APT repository
  (`sudo apt install orbitui`), plus binaries for Linux (x86_64, ARM64),
  macOS (Intel, Apple Silicon) and Windows, and `.deb`/`.rpm` packages.
- 83 missions in groups switched with `g`, `--groups` or `?groups=`:
  hyperspectral (EnMAP, PRISMA, Tanager, Gaofen-5, ZY-1), radar (NISAR,
  Biomass, ALOS-2/4, SAOCOM, RCM, TerraSAR-X, COSMO-SkyMed, Gaofen-3,
  LuTan-1…), China, very high resolution, ocean/ice/atmosphere, India.
- Coverage per sensor: nadir swaths, tasked optics by their pointing limit,
  radars by incidence range and look side.
- Area of interest by place name (`e`, `--place`, `?place=`).
- Areas of interest as GeoJSON polygons, with the share covered per pass.
- User-defined satellites (`i`, `D`, `satellites.toml`).
- Cloud cover forecast at the AOI (Open-Meteo).
- Export of the agenda as CSV and iCalendar (`x`, `--csv`, `--ics`).
- ESA acquisition plans for Sentinel-1 and Sentinel-2.

### Changed

- Orbital elements are fetched and cached as OMM (JSON), which carries
  six-digit catalog numbers.

### Fixed

- Tab no longer moves the browser's focus out of the web app.

## [0.2.0] - 2026-10-21

### Added

- Web version on GitHub Pages (WebAssembly, Ratzilla).

## [0.1.0] - 2026-10-21

### Added

- Terminal planner: globe and map, agenda of imaging opportunities and
  passes, timeline, footprint view and sky plot.
