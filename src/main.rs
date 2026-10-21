//! orbitui: terminal planner of Earth observation satellite overpasses.

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use clap::{Parser, ValueEnum};
use crossterm::event::{self, Event, KeyCode as Code, KeyEventKind, KeyModifiers};
use orbitui::app::{self, App, KeyCode, KeyEvent, Settings, View};
use orbitui::orbit::Observer;
use orbitui::{config, platform, tle, ui};
use ratatui::symbols::Marker;
use std::time::Duration;

#[derive(Clone, Copy, ValueEnum)]
enum MarkerArg {
    /// Braille in modern terminals, half blocks in the legacy Windows console
    Auto,
    /// 2×4 dots per cell, the sharpest
    Braille,
    /// 1×2 coloured blocks per cell, works with any font
    Halfblock,
    /// One dot per cell
    Dot,
}

#[derive(Parser)]
#[command(version, about = "Plan Earth observation satellite overpasses from the terminal")]
struct Cli {
    /// Latitude of the area of interest in degrees (south is negative)
    #[arg(long, allow_negative_numbers = true, requires = "lon")]
    lat: Option<f64>,
    /// Longitude of the area of interest in degrees (west is negative)
    #[arg(long, allow_negative_numbers = true, requires = "lat")]
    lon: Option<f64>,
    /// Area of interest from a GeoJSON file (polygons, or a point)
    #[arg(long, conflicts_with_all = ["lat", "lon"])]
    aoi: Option<String>,
    /// Name shown for the area of interest
    #[arg(long)]
    name: Option<String>,
    /// Altitude of the area of interest in metres
    #[arg(long, default_value_t = 0.0, allow_negative_numbers = true)]
    alt: f64,
    /// Days to forecast ahead (1–14)
    #[arg(long, default_value_t = 3.0)]
    days: f64,
    /// Lowest peak elevation of a listed horizon pass, in degrees
    #[arg(long, default_value_t = 10.0)]
    min_elev: f64,
    /// Start the simulation clock at this instant (RFC 3339, e.g. 2026-11-01T12:00:00Z)
    #[arg(long)]
    time: Option<DateTime<Utc>>,
    /// Start on the flat world map instead of the globe
    #[arg(long)]
    map: bool,
    /// Download fresh elements into the cache, print a summary and exit
    #[arg(long, conflicts_with = "offline")]
    fetch: bool,
    /// Load an ESA acquisition plan (KML); repeat for several
    #[arg(long, value_name = "FILE")]
    plan: Vec<String>,
    /// Write the imaging opportunities as CSV to this file ("-" for stdout) and exit
    #[arg(long, value_name = "FILE")]
    csv: Option<String>,
    /// Write the imaging opportunities as iCalendar to this file ("-" for stdout) and exit
    #[arg(long, value_name = "FILE")]
    ics: Option<String>,
    /// Print the forecast (UTC) as text and exit
    #[arg(long)]
    list: bool,
    /// Never download elements; use the cache or the bundled snapshot
    #[arg(long)]
    offline: bool,
    /// How the globe and plots are drawn
    #[arg(long, value_enum, default_value_t = MarkerArg::Auto)]
    marker: MarkerArg,
    /// 16 colours and plain borders, for limited consoles
    #[arg(long)]
    compat: bool,
    /// Print one frame as plain text and exit (e.g. 160x48)
    #[arg(long, hide = true)]
    render: Option<String>,
    /// Screen for --render: monitor or agenda
    #[arg(long, hide = true, default_value = "monitor")]
    view: String,
    /// Keys typed before --render draws (\n enter, \e esc, \d down, \u up)
    #[arg(long, hide = true, default_value = "")]
    keys: String,
}

/// The classic Windows console host lacks braille glyphs in its fonts.
fn legacy_console() -> bool {
    cfg!(windows)
        && ["WT_SESSION", "TERM_PROGRAM", "TERM", "ConEmuANSI", "ALACRITTY_LOG", "WEZTERM_PANE"]
            .iter()
            .all(|v| std::env::var_os(v).is_none())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if !(1.0..=14.0).contains(&cli.days) {
        bail!("--days must be between 1 and 14");
    }

    let observer = match (cli.lat, cli.lon) {
        (Some(lat), Some(lon)) => {
            if lat.abs() > 90.0 || lon.abs() > 180.0 {
                bail!("--lat must be within ±90 and --lon within ±180");
            }
            let obs = Observer::new(cli.name.clone().unwrap_or_else(|| "AOI".into()), lat, lon, cli.alt);
            config::save(&obs);
            obs
        }
        _ => config::load().unwrap_or_else(config::default_observer),
    };

    let legacy = legacy_console();
    let compat = cli.compat || legacy;
    let marker = match cli.marker {
        MarkerArg::Braille => Marker::Braille,
        MarkerArg::Halfblock => Marker::HalfBlock,
        MarkerArg::Dot => Marker::Dot,
        MarkerArg::Auto if compat => Marker::HalfBlock,
        MarkerArg::Auto => Marker::Braille,
    };
    let offline = cli.offline || cli.fetch;
    let settings = Settings { days: cli.days, min_el: cli.min_elev, offline };
    let mut app = App::new(observer, settings, marker, compat);
    app.globe.flat = cli.map;
    for path in &cli.plan {
        let text = platform::read_file(path).map_err(anyhow::Error::msg)?;
        app.load_plan(path, &text).map_err(anyhow::Error::msg)?;
    }
    if let Some(path) = &cli.aoi {
        let text = platform::read_file(path).map_err(anyhow::Error::msg)?;
        let name = cli.name.clone().unwrap_or_else(|| config::file_stem(path));
        app.set_geojson(&text, &name).map_err(|e| anyhow::anyhow!("{path}: {e}"))?;
    }
    if let Some(t) = cli.time {
        app.clock = app::Clock::starting_at(t);
    }

    if cli.fetch {
        return fetch(app);
    }
    if cli.csv.is_some() || cli.ics.is_some() {
        settle(&mut app);
        app.forecast_now();
        let acquisitions = app.upcoming_acquisitions();
        for (target, content) in [
            (&cli.csv, cli.csv.as_ref().map(|_| orbitui::export::csv(&app, &acquisitions))),
            (&cli.ics, cli.ics.as_ref().map(|_| orbitui::export::ics(&app, &acquisitions))),
        ] {
            match (target.as_deref(), content) {
                (Some("-"), Some(text)) => print!("{text}"),
                (Some(path), Some(text)) => std::fs::write(path, text).with_context(|| path.to_string())?,
                _ => {}
            }
        }
        return Ok(());
    }
    if cli.list {
        settle(&mut app);
        print_forecast(&mut app);
        return Ok(());
    }
    if let Some(size) = &cli.render {
        settle(&mut app);
        return render_once(app, size, &cli.view, &cli.keys);
    }

    let terminal = ratatui::init();
    let result = run(terminal, app);
    ratatui::restore();
    result
}

fn run(mut terminal: ratatui::DefaultTerminal, mut app: App) -> Result<()> {
    while !app.quit {
        app.tick();
        terminal.draw(|f| ui::draw(f, &mut app))?;
        if event::poll(Duration::from_millis(100))? {
            // Drain everything queued so key repeat never lags behind.
            loop {
                // Windows reports key releases too; act on presses only.
                if let Event::Key(key) = event::read()?
                    && key.kind != KeyEventKind::Release
                    && let Some(key) = convert(key)
                {
                    app.on_key(key);
                }
                if app.quit || !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn convert(key: crossterm::event::KeyEvent) -> Option<KeyEvent> {
    let code = match key.code {
        Code::Char(c) => KeyCode::Char(c),
        Code::Enter => KeyCode::Enter,
        Code::Esc => KeyCode::Esc,
        Code::Backspace => KeyCode::Backspace,
        Code::Tab => KeyCode::Tab,
        Code::BackTab => KeyCode::BackTab,
        Code::Up => KeyCode::Up,
        Code::Down => KeyCode::Down,
        Code::Left => KeyCode::Left,
        Code::Right => KeyCode::Right,
        Code::PageUp => KeyCode::PageUp,
        Code::PageDown => KeyCode::PageDown,
        Code::Home => KeyCode::Home,
        Code::End => KeyCode::End,
        _ => return None,
    };
    Some(KeyEvent { code, ctrl: key.modifiers.contains(KeyModifiers::CONTROL) })
}

/// Lets the downloads started by the app finish (for one-shot output).
fn settle(app: &mut App) {
    let began = std::time::Instant::now();
    app.tick();
    while app.busy() && began.elapsed() < Duration::from_secs(30) {
        std::thread::sleep(Duration::from_millis(50));
        app.tick();
    }
}

/// Downloads fresh elements through the app's own download and reports them.
fn fetch(mut app: App) -> Result<()> {
    app.refresh_tle();
    while app.refreshing {
        app.tick();
        std::thread::sleep(Duration::from_millis(50));
    }
    if app.tle_source != tle::Source::Celestrak {
        bail!("{}", app.status().unwrap_or("download failed"));
    }
    for info in &app.catalog {
        match app.sats.iter().find(|s| s.info.norad == info.norad) {
            Some(sat) => println!("{:<12} epoch {} UTC", info.name, sat.epoch().format("%Y-%m-%d %H:%M")),
            None => println!("{:<12} missing", info.name),
        }
    }
    if let Some(path) = platform::location("cache/tle.txt") {
        println!("saved to {}", path.display());
    }
    Ok(())
}

fn render_once(mut app: App, size: &str, view: &str, keys: &str) -> Result<()> {
    let (w, h) = size.split_once('x').context("--render expects WIDTHxHEIGHT")?;
    let (w, h): (u16, u16) = (w.parse()?, h.parse()?);
    app.view = if view == "agenda" { View::Agenda } else { View::Monitor };
    app.forecast_now();
    let mut chars = keys.chars();
    while let Some(c) = chars.next() {
        let code = match c {
            '\\' => match chars.next() {
                Some('n') => KeyCode::Enter,
                Some('e') => KeyCode::Esc,
                Some('d') => KeyCode::Down,
                Some('u') => KeyCode::Up,
                _ => continue,
            },
            c => KeyCode::Char(c),
        };
        app.on_key(KeyEvent { code, ctrl: false });
        app.tick();
    }
    app.forecast_now();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h))?;
    terminal.draw(|f| ui::draw(f, &mut app))?;
    let buffer = terminal.backend().buffer();
    for y in 0..h {
        let line: String = (0..w).map(|x| buffer[(x, y)].symbol()).collect();
        println!("{}", line.trim_end());
    }
    Ok(())
}

fn print_forecast(app: &mut App) {
    app.forecast_now();
    let Some(fc) = &app.forecast else { return };
    let now = app.clock.now();
    println!("# {} ({}), from {} UTC", app.observer.name, app.observer.coords(), now.format("%Y-%m-%d %H:%M"));
    println!("
# imaging opportunities");
    println!(
        "{:<12} {:<19} {:>4} {:>9} {:>8} {:>6} {:>6} {:>6}  plan",
        "satellite", "time_utc", "dir", "xtrack_km", "offnadir", "sun_el", "cover", "cloud"
    );
    for a in fc.acquisitions.iter().filter(|a| a.time >= now) {
        let sat = &app.sats[a.sat];
        if !a.usable(sat.info.kind) {
            continue;
        }
        println!(
            "{:<12} {:<19} {:>4} {:>9.1} {:>8.1} {:>6.1} {:>5.0}% {:>6}  {}",
            sat.info.name,
            a.time.format("%Y-%m-%d %H:%M:%S"),
            if a.ascending { "asc" } else { "desc" },
            a.cross_km,
            a.off_nadir,
            a.sun_el,
            a.coverage * 100.0,
            app.cloud_at(a.time).map_or("-".to_string(), |c| format!("{c:.0}%")),
            app.plan_label(a)
        );
    }
    println!("
# passes");
    println!("{:<12} {:<19} {:<8} {:<8} {:>6}", "satellite", "aos_utc", "tca", "los", "max_el");
    for p in fc.passes.iter().filter(|p| p.los >= now) {
        println!(
            "{:<12} {:<19} {:<8} {:<8} {:>6.1}",
            app.sats[p.sat].info.name,
            p.aos.format("%Y-%m-%d %H:%M:%S"),
            p.tca.format("%H:%M:%S"),
            p.los.format("%H:%M:%S"),
            p.max_el
        );
    }
}
