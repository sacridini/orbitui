//! Application state, background work and key handling.

use crate::catalog::{self, SatInfo};
use crate::coast::World;
use crate::config;
use crate::orbit::{Observer, Sat};
use crate::platform;
use crate::predict::{self, Acquisition, Forecast, Pass};
use crate::tle::{self, Raw, Source, TleSet};
use chrono::{DateTime, Duration, Utc};
use ratatui::symbols::Marker;
use ratatui::widgets::TableState;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use web_time::Instant;

pub enum Msg {
    Forecast { job: u64, forecast: Forecast },
    /// One Celestrak response of the download in progress.
    Tle(Result<String, String>),
}

/// Keys, independent of the terminal or browser that produced them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Enter,
    Esc,
    Backspace,
    Tab,
    BackTab,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
}

#[derive(Clone, Copy, Debug)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub ctrl: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Aoi,
    AddSatellite,
}

/// A line of text being typed by the user.
pub struct Prompt {
    pub kind: PromptKind,
    pub text: String,
}

/// A download of elements: the Celestrak groups first, then the satellites
/// they lacked, one by one.
struct TleFetch {
    pending: usize,
    by_id: bool,
    found: Vec<Raw>,
}

/// Simulation time: real time, shifted, sped up or frozen.
pub struct Clock {
    anchor_real: Instant,
    anchor_sim: DateTime<Utc>,
    pub warp: f64,
    pub paused: bool,
}

const WARPS: [f64; 11] = [-3600.0, -600.0, -60.0, -10.0, -1.0, 1.0, 10.0, 60.0, 600.0, 3600.0, 21600.0];

impl Clock {
    pub fn real() -> Self {
        Clock { anchor_real: Instant::now(), anchor_sim: Utc::now(), warp: 1.0, paused: false }
    }

    pub fn starting_at(t: DateTime<Utc>) -> Self {
        Clock { anchor_sim: t, ..Clock::real() }
    }

    pub fn now(&self) -> DateTime<Utc> {
        if self.paused {
            return self.anchor_sim;
        }
        let ms = self.anchor_real.elapsed().as_secs_f64() * self.warp * 1000.0;
        self.anchor_sim + Duration::milliseconds(ms as i64)
    }

    fn rebase(&mut self) {
        self.anchor_sim = self.now();
        self.anchor_real = Instant::now();
    }

    fn step_warp(&mut self, faster: bool) {
        let i = WARPS.iter().position(|&w| w == self.warp).unwrap_or(5);
        let j = if faster { (i + 1).min(WARPS.len() - 1) } else { i.saturating_sub(1) };
        self.rebase();
        self.warp = WARPS[j];
    }

    fn toggle_pause(&mut self) {
        self.rebase();
        self.paused = !self.paused;
    }

    fn jump(&mut self, by: Duration) {
        self.rebase();
        self.anchor_sim += by;
    }

    pub fn is_live(&self) -> bool {
        !self.paused && self.warp == 1.0 && (self.now() - Utc::now()).num_seconds().abs() < 2
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Monitor,
    Agenda,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Center {
    Aoi,
    Satellite,
    Free,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Labels {
    All,
    Selected,
    Off,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EventList {
    Acquisitions,
    Passes,
}

pub struct Globe {
    pub center: Center,
    pub lat: f64,
    pub lon: f64,
    pub zoom: f64,
    pub spin: bool,
    pub flat: bool,
}

pub struct Settings {
    pub days: f64,
    pub min_el: f64,
    pub offline: bool,
}

#[derive(Clone, Copy)]
pub enum Event<'a> {
    Acq(&'a Acquisition),
    Pass(&'a Pass),
}

impl Event<'_> {
    pub fn sat(&self) -> usize {
        match self {
            Event::Acq(a) => a.sat,
            Event::Pass(p) => p.sat,
        }
    }

    /// Time that identifies and orders the event.
    pub fn time(&self) -> DateTime<Utc> {
        match self {
            Event::Acq(a) => a.time,
            Event::Pass(p) => p.tca,
        }
    }

    pub fn end(&self) -> DateTime<Utc> {
        match self {
            Event::Acq(a) => a.time,
            Event::Pass(p) => p.los,
        }
    }
}

pub struct App {
    pub sats: Arc<Vec<Sat>>,
    pub observer: Observer,
    pub world: World,
    pub clock: Clock,
    pub view: View,
    pub selected: usize,
    pub sat_table: TableState,
    pub event_table: TableState,
    selected_event: Option<(usize, DateTime<Utc>)>,
    pub globe: Globe,
    pub labels: Labels,
    pub list: EventList,
    pub only_selected: bool,
    pub show_night: bool,
    pub utc: bool,
    pub timeline_hours: i64,
    pub forecast: Option<Forecast>,
    window: Option<(DateTime<Utc>, DateTime<Utc>)>,
    job: u64,
    /// Forecast advancing between frames when there are no threads.
    slices: Option<predict::Job>,
    pub computing: bool,
    pub catalog: Vec<SatInfo>,
    tle_fetch: Option<TleFetch>,
    /// The elements in use, to rebuild the fleet when the catalog changes.
    tle_raw: Vec<Raw>,
    pub tle_source: Source,
    pub tle_fetched: Option<DateTime<Utc>>,
    pub refreshing: bool,
    status: Option<(String, Instant)>,
    pub prompt: Option<Prompt>,
    pub help: bool,
    pub marker: Marker,
    pub compat: bool,
    pub settings: Settings,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    last_tick: Instant,
    pub quit: bool,
}

fn build_sats(raw: &[Raw], catalog: &[SatInfo]) -> Vec<Sat> {
    catalog
        .iter()
        .filter_map(|info| {
            let r = raw.iter().find(|r| r.norad() == Some(info.norad))?;
            let mut info = *info;
            if info.name.is_empty() {
                // Unnamed user satellite: take the TLE's name.
                info.name = Box::leak(r.name.clone().into_boxed_str());
                info.short = Box::leak(catalog::short_label(&r.name, info.norad).into_boxed_str());
            }
            Sat::new(info, r.elements()?)
        })
        .collect()
}

impl App {
    pub fn new(observer: Observer, settings: Settings, marker: Marker, compat: bool) -> Self {
        let set = tle::load_local();
        let (catalog, catalog_error) = catalog::load();
        let (tx, rx) = mpsc::channel();
        let mut app = App {
            sats: Arc::new(build_sats(&set.raw, &catalog)),
            tle_raw: set.raw.clone(),
            catalog,
            tle_fetch: None,
            slices: None,
            globe: Globe { center: Center::Aoi, lat: observer.lat, lon: observer.lon, zoom: 1.0, spin: false, flat: false },
            observer,
            world: World::load(),
            clock: Clock::real(),
            view: View::Monitor,
            selected: 0,
            sat_table: TableState::default().with_selected(Some(0)),
            event_table: TableState::default(),
            selected_event: None,
            labels: Labels::All,
            list: EventList::Acquisitions,
            only_selected: false,
            show_night: false,
            utc: false,
            timeline_hours: 24,
            forecast: None,
            window: None,
            job: 0,
            computing: false,
            tle_source: set.source,
            tle_fetched: set.fetched,
            refreshing: false,
            status: None,
            prompt: None,
            help: false,
            marker,
            compat,
            settings,
            tx,
            rx,
            last_tick: Instant::now(),
            quit: false,
        };
        if let Some(e) = catalog_error {
            app.set_status(e);
        }
        if set.is_stale() && !app.settings.offline {
            app.refresh_tle();
        } else {
            app.fetch_missing();
        }
        app
    }

    /// Computes the forecast in the foreground (for `--render`).
    pub fn forecast_now(&mut self) {
        let (start, end) = self.window_at(self.clock.now());
        self.forecast = Some(predict::forecast(&self.sats, &self.observer, start, end, self.settings.min_el));
        self.window = Some((start, end));
    }

    pub fn set_status(&mut self, text: impl Into<String>) {
        self.status = Some((text.into(), Instant::now()));
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_ref().filter(|(_, at)| at.elapsed().as_secs() < 6).map(|(s, _)| s.as_str())
    }

    pub fn sat(&self) -> Option<&Sat> {
        self.sats.get(self.selected)
    }

    fn window_at(&self, now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
        (now - Duration::hours(1), now + Duration::seconds((self.settings.days * 86_400.0) as i64))
    }

    fn request_forecast(&mut self) {
        self.job += 1;
        let job = self.job;
        let (start, end) = self.window_at(self.clock.now());
        let task = predict::Job::new(Arc::clone(&self.sats), self.observer.clone(), start, end, self.settings.min_el);
        self.window = Some((start, end));
        self.computing = true;
        if platform::THREADS {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                tx.send(Msg::Forecast { job, forecast: task.run() }).ok();
            });
        } else {
            self.slices = Some(task);
        }
    }

    pub fn refresh_tle(&mut self) {
        if self.refreshing {
            return;
        }
        self.refreshing = true;
        self.tle_fetch = Some(TleFetch { pending: tle::GROUPS.len(), by_id: false, found: Vec::new() });
        for group in tle::GROUPS {
            self.get(tle::group_url(group), Msg::Tle);
        }
    }

    /// Downloads elements only for catalog satellites that have none.
    fn fetch_missing(&mut self) {
        let have: HashSet<u64> = self.tle_raw.iter().filter_map(Raw::norad).collect();
        let missing: Vec<u64> = self.catalog.iter().map(|s| s.norad).filter(|id| !have.contains(id)).collect();
        if missing.is_empty() || self.refreshing || self.settings.offline {
            return;
        }
        self.refreshing = true;
        self.tle_fetch = Some(TleFetch { pending: missing.len(), by_id: true, found: self.tle_raw.clone() });
        for id in missing {
            self.get(tle::catnr_url(id), Msg::Tle);
        }
    }

    /// Re-reads the catalog after the user changed it.
    fn reload_catalog(&mut self) {
        let (catalog, error) = catalog::load();
        self.catalog = catalog;
        self.forecast = None;
        self.selected_event = None;
        self.sats = Arc::new(build_sats(&self.tle_raw, &self.catalog));
        self.select_sat(self.selected.min(self.sats.len().saturating_sub(1)));
        self.request_forecast();
        if let Some(e) = error {
            self.set_status(e);
        }
        self.fetch_missing();
    }

    fn add_satellite(&mut self, input: &str) -> Result<(), String> {
        let sat = catalog::parse_add(input)?;
        let builtin = catalog::CATALOG.iter().any(|b| b.norad == sat.norad);
        let only_id = catalog::UserSat { norad: sat.norad, ..Default::default() } == sat;
        if builtin && only_id {
            // A bare id of a built-in mission brings it back.
            let mut file = catalog::read_user()?;
            file.exclude.retain(|&id| id != sat.norad);
            file.satellites.retain(|s| s.norad != sat.norad);
            catalog::write_user(&file);
        } else {
            catalog::add_user(sat.clone())?;
        }
        self.reload_catalog();
        if let Some(i) = self.sats.iter().position(|s| s.info.norad == sat.norad) {
            self.select_sat(i);
        }
        self.set_status(format!("Satellite {} added to {}", sat.norad, catalog::USER_KEY));
        Ok(())
    }

    fn remove_selected(&mut self) {
        let Some(sat) = self.sat() else { return };
        let (norad, name) = (sat.info.norad, sat.info.name);
        match catalog::remove(norad) {
            Ok(()) => {
                self.reload_catalog();
                self.set_status(format!("{name} removed; press i and type {norad} to bring it back"));
            }
            Err(e) => self.set_status(e),
        }
    }

    fn get(&self, url: String, wrap: fn(Result<String, String>) -> Msg) {
        let tx = self.tx.clone();
        platform::get(url, move |body| {
            tx.send(wrap(body)).ok();
        });
    }

    /// Collects one Celestrak response; finishes the download after the last.
    fn on_tle_response(&mut self, body: Result<String, String>) {
        let wanted: HashSet<u64> = self.catalog.iter().map(|s| s.norad).collect();
        let Some(fetch) = &mut self.tle_fetch else { return };
        fetch.pending = fetch.pending.saturating_sub(1);
        if let Ok(text) = &body {
            for r in tle::parse(text) {
                if let Some(id) = r.norad()
                    && wanted.contains(&id)
                    && !fetch.found.iter().any(|f| f.norad() == Some(id))
                {
                    fetch.found.push(r);
                }
            }
        }
        if fetch.pending > 0 {
            return;
        }
        if !fetch.by_id {
            let have: HashSet<u64> = fetch.found.iter().filter_map(Raw::norad).collect();
            let missing: Vec<u64> = self.catalog.iter().map(|s| s.norad).filter(|id| !have.contains(id)).collect();
            if !missing.is_empty() {
                fetch.by_id = true;
                fetch.pending = missing.len();
                for id in missing {
                    self.get(tle::catnr_url(id), Msg::Tle);
                }
                return;
            }
        }
        let found = std::mem::take(&mut fetch.found);
        self.tle_fetch = None;
        self.refreshing = false;
        if found.is_empty() {
            let why = body.err().unwrap_or_else(|| "no elements".into());
            self.set_status(format!("TLE download failed, using {}: {why}", self.tle_source.label()));
            return;
        }
        self.install(tle::store(found));
    }

    fn install(&mut self, set: TleSet) {
        let sats = build_sats(&set.raw, &self.catalog);
        let n = sats.len();
        if n == 0 {
            return;
        }
        // Indices of the old forecast may not match the new list.
        self.forecast = None;
        self.selected_event = None;
        self.sats = Arc::new(sats);
        self.tle_raw = set.raw;
        self.select_sat(self.selected.min(n - 1));
        self.tle_source = set.source;
        self.tle_fetched = set.fetched;
        self.request_forecast();
        self.set_status(format!("Elements updated from Celestrak ({n} satellites)"));
    }

    /// Advances animation, collects background results, keeps the forecast
    /// window ahead of the simulation clock.
    pub fn tick(&mut self) {
        let dt = self.last_tick.elapsed().as_secs_f64();
        self.last_tick = Instant::now();
        if self.globe.spin && self.globe.center == Center::Free {
            self.globe.lon = crate::astro::wrap_lon(self.globe.lon + 8.0 * dt);
        }

        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Forecast { job, forecast } if job == self.job => {
                    self.forecast = Some(forecast);
                    self.computing = false;
                }
                Msg::Forecast { .. } => {}
                Msg::Tle(body) => self.on_tle_response(body),
            }
        }

        if let Some(task) = &mut self.slices
            && let Some(forecast) = task.step(web_time::Duration::from_millis(12))
        {
            self.forecast = Some(forecast);
            self.slices = None;
            self.computing = false;
        }

        let now = self.clock.now();
        let stale = match self.window {
            None => true,
            Some((start, end)) => {
                let used = (now - start).num_seconds() as f64 / (end - start).num_seconds() as f64;
                // While a job runs, only restart it once time leaves its window.
                if self.computing { !(0.0..0.9).contains(&used) } else { !(0.0..0.34).contains(&used) }
            }
        };
        if stale {
            self.request_forecast();
        }
    }

    /// Centre of the globe for the current mode.
    pub fn globe_center(&self, now: DateTime<Utc>) -> (f64, f64) {
        match self.globe.center {
            Center::Aoi => (self.observer.lat, self.observer.lon),
            Center::Free => (self.globe.lat, self.globe.lon),
            Center::Satellite => self
                .sat()
                .and_then(|s| s.state(now))
                .map(|s| (s.geo.lat, s.geo.lon))
                .unwrap_or((self.globe.lat, self.globe.lon)),
        }
    }

    /// Events listed in the agenda, after filters.
    pub fn events(&self) -> Vec<Event<'_>> {
        let Some(fc) = &self.forecast else { return Vec::new() };
        let since = self.clock.now() - Duration::minutes(10);
        let keep_sat = |sat: usize| !self.only_selected || sat == self.selected;
        match self.list {
            EventList::Acquisitions => fc
                .acquisitions
                .iter()
                .filter(|a| a.time >= since && keep_sat(a.sat))
                .filter(|a| self.show_night || a.usable(self.sats[a.sat].info.kind))
                .map(Event::Acq)
                .collect(),
            EventList::Passes => fc
                .passes
                .iter()
                .filter(|p| p.los >= since && keep_sat(p.sat))
                .map(Event::Pass)
                .collect(),
        }
    }

    /// Index of the selected event in `events`, following it as the list
    /// scrolls with time.
    pub fn selected_event_index(&self, events: &[Event]) -> Option<usize> {
        if events.is_empty() {
            return None;
        }
        let Some((sat, time)) = self.selected_event else { return Some(0) };
        events
            .iter()
            .position(|e| e.sat() == sat && e.time() == time)
            .or_else(|| events.iter().position(|e| e.time() >= time))
            .or(Some(events.len() - 1))
    }

    fn move_event(&mut self, delta: isize) {
        let (key, sat) = {
            let events = self.events();
            let Some(i) = self.selected_event_index(&events) else { return };
            let j = (i as isize + delta).clamp(0, events.len() as isize - 1) as usize;
            let e = events[j];
            ((e.sat(), e.time()), e.sat())
        };
        self.selected_event = Some(key);
        self.select_sat(sat);
    }

    fn select_sat(&mut self, i: usize) {
        if i < self.sats.len() {
            self.selected = i;
            self.sat_table.select(Some(i));
        }
    }

    fn move_sat(&mut self, delta: isize) {
        let n = self.sats.len() as isize;
        if n > 0 {
            self.select_sat((self.selected as isize + delta).rem_euclid(n) as usize);
        }
    }

    fn set_observer(&mut self, obs: Observer) {
        config::save(&obs);
        self.set_status(format!("AOI set to {} ({})", obs.name, obs.coords()));
        self.observer = obs;
        self.selected_event = None;
        self.forecast = None;
        self.request_forecast();
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.ctrl && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if let Some(prompt) = &mut self.prompt {
            match key.code {
                KeyCode::Esc => self.prompt = None,
                KeyCode::Enter => {
                    let (kind, text) = (prompt.kind, prompt.text.clone());
                    let result = match kind {
                        PromptKind::Aoi => match config::parse_aoi(&text) {
                            Some((lat, lon, name)) => {
                                self.set_observer(Observer::new(name, lat, lon, 0.0));
                                Ok(())
                            }
                            None => Err("Expected: lat, lon [name]  e.g.  -3.10, -60.02 Manaus".to_string()),
                        },
                        PromptKind::AddSatellite => self.add_satellite(&text),
                    };
                    match result {
                        Ok(()) => self.prompt = None,
                        Err(e) => self.set_status(e),
                    }
                }
                KeyCode::Backspace => {
                    prompt.text.pop();
                }
                KeyCode::Char(c) => prompt.text.push(c),
                _ => {}
            }
            return;
        }
        if self.help {
            self.help = false;
            return;
        }

        let monitor = self.view == View::Monitor;
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('?') => self.help = true,
            KeyCode::Tab | KeyCode::BackTab => {
                self.view = if monitor { View::Agenda } else { View::Monitor };
            }
            KeyCode::Char('1') => self.view = View::Monitor,
            KeyCode::Char('2') => self.view = View::Agenda,
            KeyCode::Down | KeyCode::Char('j') => {
                if monitor { self.move_sat(1) } else { self.move_event(1) }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if monitor { self.move_sat(-1) } else { self.move_event(-1) }
            }
            KeyCode::PageDown => {
                if monitor { self.move_sat(5) } else { self.move_event(10) }
            }
            KeyCode::PageUp => {
                if monitor { self.move_sat(-5) } else { self.move_event(-10) }
            }
            KeyCode::Home if !monitor => self.move_event(-100_000),
            KeyCode::End if !monitor => self.move_event(100_000),
            KeyCode::Char('m') => self.globe.flat = !self.globe.flat,
            KeyCode::Char('c') => {
                let (lat, lon) = self.globe_center(self.clock.now());
                (self.globe.lat, self.globe.lon) = (lat, lon);
                self.globe.center = match self.globe.center {
                    Center::Aoi => Center::Satellite,
                    Center::Satellite => Center::Free,
                    Center::Free => Center::Aoi,
                };
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Char('a' | 'd' | 'w' | 's') => {
                let (lat, lon) = self.globe_center(self.clock.now());
                (self.globe.lat, self.globe.lon) = (lat, lon);
                self.globe.center = Center::Free;
                let step = 10.0 / self.globe.zoom;
                match key.code {
                    KeyCode::Left | KeyCode::Char('a') => self.globe.lon = crate::astro::wrap_lon(lon - step),
                    KeyCode::Right | KeyCode::Char('d') => self.globe.lon = crate::astro::wrap_lon(lon + step),
                    KeyCode::Char('w') => self.globe.lat = (lat + step).min(90.0),
                    _ => self.globe.lat = (lat - step).max(-90.0),
                }
            }
            KeyCode::Char('o') => {
                if self.globe.center != Center::Free {
                    let (lat, lon) = self.globe_center(self.clock.now());
                    (self.globe.lat, self.globe.lon) = (lat, lon);
                    self.globe.center = Center::Free;
                }
                self.globe.spin = !self.globe.spin;
            }
            KeyCode::Char('+') | KeyCode::Char('=') => self.globe.zoom = (self.globe.zoom * 1.25).min(12.0),
            KeyCode::Char('-') | KeyCode::Char('_') => self.globe.zoom = (self.globe.zoom / 1.25).max(0.5),
            KeyCode::Char(']') => self.clock.step_warp(true),
            KeyCode::Char('[') => self.clock.step_warp(false),
            KeyCode::Char('.') => self.clock.jump(Duration::hours(1)),
            KeyCode::Char(',') => self.clock.jump(Duration::hours(-1)),
            KeyCode::Char('p') | KeyCode::Char(' ') => self.clock.toggle_pause(),
            KeyCode::Char('0') => {
                self.clock = Clock::real();
                self.set_status("Back to real time");
            }
            KeyCode::Char('l') => {
                self.labels = match self.labels {
                    Labels::All => Labels::Selected,
                    Labels::Selected => Labels::Off,
                    Labels::Off => Labels::All,
                }
            }
            KeyCode::Char('v') => {
                self.list = match self.list {
                    EventList::Acquisitions => EventList::Passes,
                    EventList::Passes => EventList::Acquisitions,
                };
                self.selected_event = None;
            }
            KeyCode::Char('f') => self.only_selected = !self.only_selected,
            KeyCode::Char('n') => self.show_night = !self.show_night,
            KeyCode::Char('u') => self.utc = !self.utc,
            KeyCode::Char('z') => {
                self.timeline_hours = match self.timeline_hours {
                    12 => 24,
                    24 => 48,
                    48 => 72,
                    _ => 12,
                }
            }
            KeyCode::Char('e') => {
                let text = format!("{:.4}, {:.4} {}", self.observer.lat, self.observer.lon, self.observer.name);
                self.prompt = Some(Prompt { kind: PromptKind::Aoi, text });
            }
            KeyCode::Char('i') => self.prompt = Some(Prompt { kind: PromptKind::AddSatellite, text: String::new() }),
            KeyCode::Char('D') => self.remove_selected(),
            KeyCode::Char('r') => {
                let recent = self
                    .tle_fetched
                    .is_some_and(|t| Utc::now() - t < Duration::minutes(tle::MIN_REFRESH_MINUTES));
                if self.settings.offline {
                    self.set_status("Offline mode: not downloading elements");
                } else if recent && self.tle_source != Source::Snapshot {
                    self.set_status("Elements are recent; Celestrak asks clients not to poll more often");
                } else {
                    self.set_status("Downloading elements from Celestrak…");
                    self.refresh_tle();
                }
            }
            _ => {}
        }
    }
}
