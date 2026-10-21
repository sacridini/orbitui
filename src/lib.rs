//! orbitui: planner of Earth observation satellite overpasses.
//!
//! The library holds everything but the frontends: orbits, predictions and
//! the ratatui interface. `src/main.rs` drives it in a terminal and `web/`
//! in the browser.

pub mod aoi;
pub mod app;
pub mod astro;
pub mod catalog;
pub mod clouds;
pub mod coast;
pub mod export;
pub mod config;
pub mod orbit;
pub mod platform;
pub mod predict;
pub mod tle;
pub mod ui;
