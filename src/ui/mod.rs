//! Screen layout and widgets.

mod canvas;
mod footprint;
mod globe;
mod sky;
mod timeline;

use crate::app::{App, Event, EventList, PromptKind, View};
use crate::catalog::Kind;
use crate::orbit::format_coords;
use crate::predict::Light;
use crate::tle::Source;
use chrono::{DateTime, Duration, Local, Utc};
use ratatui::layout::Flex;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Cell, Clear, Paragraph, Row, Table, Wrap};

pub struct Theme {
    pub accent: Color,
    pub text: Color,
    pub dim: Color,
    pub faint: Color,
    pub border: Color,
    pub grid: Color,
    pub limb: Color,
    pub night: Color,
    pub star: Color,
    pub land_day: Color,
    pub land_night: Color,
    pub terminator: Color,
    pub footprint: Color,
    pub track_past: Color,
    pub swath: Color,
    pub sun: Color,
    pub aoi: Color,
    pub now: Color,
    pub good: Color,
    pub warn: Color,
    pub bad: Color,
}

impl Theme {
    /// 256-colour palette, or the 16 ANSI colours for legacy consoles.
    pub fn new(compat: bool) -> Self {
        let pick = |rich: u8, basic: Color| if compat { basic } else { Color::Indexed(rich) };
        Theme {
            accent: Color::Cyan,
            text: Color::Gray,
            dim: Color::DarkGray,
            faint: pick(236, Color::DarkGray),
            border: pick(239, Color::DarkGray),
            grid: pick(237, Color::DarkGray),
            limb: pick(31, Color::Blue),
            night: pick(18, Color::Blue),
            star: pick(240, Color::DarkGray),
            land_day: pick(71, Color::Green),
            land_night: pick(23, Color::DarkGray),
            terminator: pick(136, Color::Yellow),
            footprint: pick(244, Color::Gray),
            track_past: pick(242, Color::DarkGray),
            swath: pick(238, Color::DarkGray),
            sun: pick(220, Color::Yellow),
            aoi: Color::White,
            now: pick(214, Color::Yellow),
            good: Color::Green,
            warn: Color::Yellow,
            bad: Color::Red,
        }
    }
}

pub fn panel(title: String, app: &App, theme: &Theme) -> Block<'static> {
    Block::bordered()
        .border_type(if app.compat { BorderType::Plain } else { BorderType::Rounded })
        .border_style(Style::new().fg(theme.border))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(title, Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::raw(" "),
        ]))
}

pub fn hint_line(keys: &[(&str, &str)], theme: &Theme) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (key, what) in keys {
        spans.push(Span::styled(key.to_string(), Style::new().fg(theme.accent)));
        spans.push(Span::styled(format!(" {what}  "), Style::new().fg(theme.dim)));
    }
    Line::from(spans)
}

pub fn fmt_time(app: &App, t: DateTime<Utc>, fmt: &str) -> String {
    if app.utc { t.format(fmt).to_string() } else { t.with_timezone(&Local).format(fmt).to_string() }
}

pub fn countdown(now: DateTime<Utc>, t: DateTime<Utc>) -> String {
    let s = (t - now).num_seconds();
    if s.abs() < 60 {
        return "now".into();
    }
    let a = s.abs();
    let (d, h, m) = (a / 86_400, a % 86_400 / 3600, a % 3600 / 60);
    let body = if d > 0 {
        format!("{d}d {h:02}h")
    } else if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{m}m")
    };
    if s < 0 { format!("-{body}") } else { body }
}

fn age(d: Duration) -> String {
    let h = d.num_hours();
    if h >= 48 { format!("{}d", h / 24) } else if h >= 1 { format!("{h}h") } else { format!("{}m", d.num_minutes().max(0)) }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let theme = Theme::new(app.compat);
    let [header, body, footer] = Layout::vertical([Constraint::Length(1), Constraint::Fill(1), Constraint::Length(1)]).areas(f.area());
    draw_header(f, header, app, &theme);
    match app.view {
        View::Monitor => draw_monitor(f, body, app, &theme),
        View::Agenda => draw_agenda(f, body, app, &theme),
    }
    draw_footer(f, footer, app, &theme);
    if app.help {
        draw_help(f, app, &theme);
    }
    if app.prompt.is_some() {
        draw_input(f, app, &theme);
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let now = app.clock.now();
    let sep = || Span::styled("  │  ", Style::new().fg(theme.border));
    let clock_state = if app.clock.is_live() {
        Span::styled("● LIVE", Style::new().fg(theme.good).add_modifier(Modifier::BOLD))
    } else if app.clock.paused {
        Span::styled("‖ PAUSED", Style::new().fg(theme.warn).add_modifier(Modifier::BOLD))
    } else if app.clock.warp == 1.0 {
        Span::styled("► SIMULATED", Style::new().fg(theme.warn).add_modifier(Modifier::BOLD))
    } else {
        let w = app.clock.warp;
        let arrow = if w < 0.0 { "◄◄" } else { "►►" };
        Span::styled(format!("{arrow} ×{}", w.abs()), Style::new().fg(theme.warn).add_modifier(Modifier::BOLD))
    };
    let tle_age = app.tle_fetched.map(|t| Utc::now() - t);
    let tle_color = match (app.tle_source, tle_age) {
        (Source::Snapshot, _) => theme.warn,
        (_, Some(a)) if a > Duration::days(3) => theme.bad,
        (_, Some(a)) if a > Duration::hours(crate::tle::MAX_AGE_HOURS) => theme.warn,
        _ => theme.good,
    };
    let tle_text = match tle_age {
        Some(a) => format!("TLE {} {}", app.tle_source.label(), age(a)),
        None => format!("TLE {}", app.tle_source.label()),
    };
    let mut spans = vec![
        Span::styled(" ▲ ORBITUI ", Style::new().fg(Color::Black).bg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("♦ ", Style::new().fg(theme.aoi)),
        Span::styled(app.observer.name.clone(), Style::new().fg(theme.aoi).add_modifier(Modifier::BOLD)),
        Span::styled(
            match &app.observer.area {
                Some(area) => format!(" ▱ {:.0} km² around {}", area.area_km2, app.observer.coords()),
                None => format!(" {}", app.observer.coords()),
            },
            Style::new().fg(theme.dim),
        ),
        sep(),
        Span::styled(now.format("%Y-%m-%d %H:%M:%S").to_string(), Style::new().fg(theme.text).add_modifier(Modifier::BOLD)),
        Span::styled(" UTC", Style::new().fg(theme.dim)),
        Span::styled(format!("  {}", now.with_timezone(&Local).format("%H:%M")), Style::new().fg(theme.text)),
        Span::styled(" local", Style::new().fg(theme.dim)),
        sep(),
        clock_state,
        sep(),
        Span::styled(tle_text, Style::new().fg(tle_color)),
    ];
    if app.refreshing {
        spans.push(Span::styled("  ⟳ downloading", Style::new().fg(theme.dim)));
    }
    if app.computing {
        spans.push(Span::styled("  ⟳ forecasting", Style::new().fg(theme.dim)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    if let Some(msg) = app.status() {
        f.render_widget(Paragraph::new(Line::from(vec![Span::styled(format!(" {msg}"), Style::new().fg(theme.warn))])), area);
        return;
    }
    let view = match app.view {
        View::Monitor => vec![("tab", "agenda"), ("↑↓", "satellite")],
        View::Agenda => vec![("tab", "monitor"), ("↑↓", "event"), ("v", "images/passes")],
    };
    let mut common = vec![("[ ]", "time warp"), ("space", "pause"), ("0", "now"), ("e", "AOI"), ("u", "UTC"), ("r", "TLE"), ("?", "help")];
    if crate::platform::THREADS {
        common.push(("q", "quit"));
    }
    let keys: Vec<(&str, &str)> = view.into_iter().chain(common).collect();
    f.render_widget(Paragraph::new(hint_line(&keys, theme)), area);
}

fn draw_monitor(f: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let [globe_area, side] = Layout::horizontal([Constraint::Percentage(63), Constraint::Fill(1)]).areas(area);
    let [list, details] = Layout::vertical([Constraint::Fill(1), Constraint::Length(11)]).areas(side);
    globe::render(f, globe_area, app, theme);
    draw_sat_table(f, list, app, theme);
    draw_sat_details(f, details, app, theme);
}

fn draw_sat_table(f: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let now = app.clock.now();
    let rows: Vec<Row> = app
        .sats
        .iter()
        .enumerate()
        .map(|(i, sat)| {
            let st = sat.state(now);
            let next = app.forecast.as_ref().and_then(|fc| fc.next_acquisition(i, now, sat.info.kind));
            let next = match (next, sat.info.kind) {
                (Some(a), _) => Span::styled(countdown(now, a.time), Style::new().fg(theme.text)),
                (None, Kind::Station) => Span::styled("-", Style::new().fg(theme.dim)),
                (None, _) if app.forecast.is_none() => Span::styled("…", Style::new().fg(theme.dim)),
                (None, _) => Span::styled(format!(">{:.0}d", app.settings.days), Style::new().fg(theme.dim)),
            };
            let (lat, lon, alt) = st.map_or((String::new(), String::new(), String::new()), |s| {
                (format!("{:>6.1}", s.geo.lat), format!("{:>6.1}", s.geo.lon), format!("{:>4.0}", s.geo.alt))
            });
            let dir = st.map_or("", |s| if s.ascending { "↑" } else { "↓" });
            Row::new(vec![
                Cell::from(Span::styled("●", Style::new().fg(sat.info.color))),
                Cell::from(Span::styled(sat.info.name, Style::new().fg(theme.text))),
                Cell::from(Span::styled(sat.info.sensor, Style::new().fg(theme.dim))),
                Cell::from(lat),
                Cell::from(lon),
                Cell::from(alt),
                Cell::from(Span::styled(dir, Style::new().fg(theme.dim))),
                Cell::from(next),
            ])
        })
        .collect();
    let header = Row::new(["", "SATELLITE", "SENSOR", "   LAT", "   LON", " ALT", "", "NEXT"])
        .style(Style::new().fg(theme.dim).add_modifier(Modifier::BOLD));
    let widths = [
        Constraint::Length(1),
        Constraint::Length(11),
        Constraint::Fill(1),
        Constraint::Length(6),
        Constraint::Length(6),
        Constraint::Length(4),
        Constraint::Length(1),
        Constraint::Length(7),
    ];
    let table = Table::new(rows, widths)
        .header(header)
        .block(panel(format!("satellites · {}", app.sats.len()), app, theme))
        .row_highlight_style(Style::new().bg(theme.faint).add_modifier(Modifier::BOLD))
        .highlight_symbol("▸");
    f.render_stateful_widget(table, area, &mut app.sat_table);
}

fn kv(key: &str, value: impl Into<String>, theme: &Theme) -> Vec<Span<'static>> {
    vec![
        Span::styled(format!("{key:<12}"), Style::new().fg(theme.dim)),
        Span::styled(value.into(), Style::new().fg(theme.text)),
    ]
}

fn draw_sat_details(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let Some(sat) = app.sat() else {
        f.render_widget(panel("details".into(), app, theme), area);
        return;
    };
    let now = app.clock.now();
    let info = &sat.info;
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(info.name, Style::new().fg(info.color).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!(
                "  {} · {} · NORAD {}{}",
                info.sensor,
                if info.images() { format!("swath {:.0} km", info.swath_km) } else { "no imager".into() },
                info.norad,
                if info.user { " · yours" } else { "" }
            ),
            Style::new().fg(theme.dim),
        ),
    ]));
    if let Some(st) = sat.state(now) {
        let look = app.observer.look(st.ecef);
        lines.push(Line::from(kv(
            "position",
            format!(
                "{}  {:.0} km  {:.2} km/s  {}",
                format_coords(st.geo.lat, st.geo.lon),
                st.geo.alt,
                st.speed,
                if st.ascending { "↑ asc" } else { "↓ desc" }
            ),
            theme,
        )));
        let mut seen = kv("from AOI", format!("az {:.1}°  el {:.1}°  {:.0} km", look.az, look.el, look.range_km), theme);
        if look.el >= 0.0 {
            seen.push(Span::styled("  ▲ above horizon", Style::new().fg(theme.good)));
        }
        lines.push(Line::from(seen));
    }
    lines.push(Line::from(kv(
        "orbit",
        format!(
            "{:.1} min  i {:.2}°  e {:.5}",
            sat.period_minutes(),
            sat.elements.inclination,
            sat.elements.eccentricity
        ),
        theme,
    )));
    let epoch_age = Utc::now() - sat.epoch();
    let mut epoch = kv("TLE epoch", format!("{} UTC  ", sat.epoch().format("%Y-%m-%d %H:%M")), theme);
    epoch.push(Span::styled(
        format!("{} old", age(epoch_age)),
        Style::new().fg(if epoch_age > Duration::days(7) { theme.bad } else { theme.dim }),
    ));
    lines.push(Line::from(epoch));
    lines.push(Line::raw(""));
    let fc = app.forecast.as_ref();
    let next_img = fc.and_then(|fc| fc.next_acquisition(app.selected, now, info.kind));
    lines.push(Line::from(match next_img {
        Some(a) => {
            let mut v = kv("next image", format!("{}  in {}", fmt_time(app, a.time, "%a %d %b %H:%M"), countdown(now, a.time)), theme);
            v.push(Span::styled(
                format!("  {:.0} km {}  ☼ {:.0}°", a.cross_km.abs(), if a.cross_km >= 0.0 { "R" } else { "L" }, a.sun_el),
                Style::new().fg(theme.dim),
            ));
            v
        }
        None if !info.images() => kv("next image", "-", theme),
        None => kv("next image", format!("none in the next {:.0} days", app.settings.days), theme),
    }));
    let next_pass = fc.and_then(|fc| fc.next_pass(app.selected, now));
    lines.push(Line::from(match next_pass {
        Some(p) => {
            let mut v = kv("next pass", format!("{}  in {}", fmt_time(app, p.aos, "%a %d %b %H:%M"), countdown(now, p.aos)), theme);
            v.push(Span::styled(format!("  max el {:.0}°", p.max_el), Style::new().fg(theme.dim)));
            v
        }
        None => kv("next pass", format!("none above {:.0}°", app.settings.min_el), theme),
    }));
    f.render_widget(Paragraph::new(lines).block(panel("details".into(), app, theme)), area);
}

fn draw_agenda(f: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let tl_height = timeline::height(app).min(area.height / 2).max(6);
    let [tl_area, bottom] = Layout::vertical([Constraint::Length(tl_height), Constraint::Fill(1)]).areas(area);
    let [table_area, right] = Layout::horizontal([Constraint::Percentage(54), Constraint::Fill(1)]).areas(bottom);
    let [fp_area, lower] = Layout::vertical([Constraint::Percentage(52), Constraint::Fill(1)]).areas(right);
    // Cells are twice as tall as wide: a square plot is twice as wide as tall.
    let sky_width = (lower.height * 2 + 2).min(lower.width * 11 / 20);
    let [sky_area, info_area] = Layout::horizontal([Constraint::Length(sky_width), Constraint::Fill(1)]).areas(lower);

    let (idx, selected_key, selected) = {
        let events = app.events();
        let idx = app.selected_event_index(&events);
        let ev = idx.map(|i| events[i]);
        (idx, ev.map(|e| (e.sat(), e.time())), ev.map(|e| owned(&e, app.observer.lon)))
    };
    app.event_table.select(idx);
    timeline::render(f, tl_area, app, theme, selected_key);
    draw_events(f, table_area, app, theme);
    match selected {
        Some(ev) => {
            footprint::render(f, fp_area, app, theme, ev.sat, ev.time);
            sky::render(f, sky_area, app, theme, ev.sat, (ev.from, ev.to), ev.time);
            draw_event_info(f, info_area, app, theme, &ev);
        }
        None => {
            let msg = if app.forecast.is_none() { "Forecasting…" } else { "No events: try v, n or f" };
            let p = Paragraph::new(msg).style(Style::new().fg(theme.dim)).block(panel("footprint".into(), app, theme));
            f.render_widget(p, right);
        }
    }
}

/// The selected event, detached from the forecast borrow.
struct Selected {
    sat: usize,
    time: DateTime<Utc>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    lines: Vec<(String, String)>,
    light: Option<Light>,
}

fn owned(e: &Event, app_lon: f64) -> Selected {
    match e {
        Event::Acq(a) => Selected {
            sat: a.sat,
            time: a.time,
            from: a.time - Duration::minutes(12),
            to: a.time + Duration::minutes(12),
            lines: vec![
                ("cross-track".into(), format!("{:.1} km {}", a.cross_km.abs(), if a.cross_km >= 0.0 { "right" } else { "left" })),
                ("AOI covered".into(), format!("{:.0}%", a.coverage * 100.0)),
                ("off-nadir".into(), format!("{:.1}°", a.off_nadir)),
                ("seen at".into(), format!("az {:.0}°  el {:.1}°", a.azimuth, a.elevation)),
                ("direction".into(), if a.ascending { "↑ ascending".into() } else { "↓ descending".into() }),
                ("solar time".into(), solar_time(a.time, app_lon)),
                ("sun".into(), format!("{:.1}° elevation", a.sun_el)),
            ],
            light: Some(a.light()),
        },
        Event::Pass(p) => Selected {
            sat: p.sat,
            time: p.tca,
            from: p.aos,
            to: p.los,
            lines: vec![
                ("AOS".into(), format!("az {:.0}°", p.aos_az)),
                ("max el".into(), format!("{:.1}°", p.max_el)),
                ("LOS".into(), format!("az {:.0}°", p.los_az)),
                ("duration".into(), format!("{}m {:02}s", (p.los - p.aos).num_minutes(), (p.los - p.aos).num_seconds() % 60)),
            ],
            light: None,
        },
    }
}

/// Local mean solar time at a longitude, the clock of sun-synchronous orbits.
fn solar_time(t: DateTime<Utc>, lon: f64) -> String {
    let hours = (t.timestamp() as f64 / 3600.0 + lon / 15.0).rem_euclid(24.0);
    let minutes = (hours * 60.0).round() as i64 % 1440;
    format!("{:02}:{:02} at AOI", minutes / 60, minutes % 60)
}

fn coverage_span(coverage: f64, theme: &Theme) -> Span<'static> {
    let color = if coverage >= 0.95 {
        theme.good
    } else if coverage >= 0.5 {
        theme.text
    } else {
        theme.warn
    };
    Span::styled(format!("{:>4.0}%", coverage * 100.0), Style::new().fg(color))
}

fn light_span(light: Light, sun_el: f64, theme: &Theme) -> Span<'static> {
    match light {
        Light::Day => Span::styled(format!("☼{sun_el:>3.0}°"), Style::new().fg(theme.sun)),
        Light::Low => Span::styled(format!("☼{sun_el:>3.0}°"), Style::new().fg(theme.warn)),
        Light::Night => Span::styled("night", Style::new().fg(theme.dim)),
    }
}

fn draw_events(f: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let now = app.clock.now();
    let (rows, header, widths, title) = {
        let events = app.events();
        let date = |t| fmt_time(app, t, "%a %d %b");
        let time = |t| fmt_time(app, t, "%H:%M:%S");
        let sat_cell = |i: usize| {
            let info = &app.sats[i].info;
            Cell::from(Span::styled(info.name, Style::new().fg(info.color)))
        };
        let rows: Vec<Row> = events
            .iter()
            .map(|e| {
                let past = e.end() < now;
                let row = match e {
                    Event::Acq(a) => {
                        let kind = app.sats[a.sat].info.kind;
                        let mut cells = vec![
                            sat_cell(a.sat),
                            Cell::from(date(a.time)),
                            Cell::from(time(a.time)),
                            Cell::from(countdown(now, a.time)),
                            Cell::from(if a.ascending { "↑" } else { "↓" }),
                            Cell::from(format!("{:>4.0} {}", a.cross_km.abs(), if a.cross_km >= 0.0 { "R" } else { "L" })),
                            Cell::from(format!("{:>4.1}°", a.off_nadir)),
                            Cell::from(if kind == Kind::Sar { Span::styled("SAR", Style::new().fg(theme.dim)) } else { light_span(a.light(), a.sun_el, theme) }),
                        ];
                        if app.observer.area.is_some() {
                            cells.push(Cell::from(coverage_span(a.coverage, theme)));
                        }
                        Row::new(cells)
                    }
                    Event::Pass(p) => Row::new(vec![
                        sat_cell(p.sat),
                        Cell::from(date(p.aos)),
                        Cell::from(time(p.aos)),
                        Cell::from(countdown(now, p.aos)),
                        Cell::from(format!("{:>3.0}°", p.max_el)),
                        Cell::from(format!("{:>3.0}°→{:<3.0}°", p.aos_az, p.los_az)),
                        Cell::from(format!("{}m", (p.los - p.aos).num_minutes())),
                        Cell::from(fmt_time(app, p.los, "%H:%M:%S")),
                    ]),
                };
                if past { row.style(Style::new().fg(theme.dim)) } else { row }
            })
            .collect();
        let (header, widths): (Vec<&str>, Vec<Constraint>) = match app.list {
            EventList::Acquisitions => {
                let mut columns = vec![("SATELLITE", 12), ("DATE", 10), ("TIME", 8), ("IN", 7), ("", 1), ("X-TRACK", 7), ("OFF-NAD", 7), ("SUN", 5)];
                if app.observer.area.is_some() {
                    columns.push(("COVER", 5));
                }
                columns.into_iter().map(|(h, w)| (h, Constraint::Length(w))).unzip()
            }
            EventList::Passes => (
                vec!["SATELLITE", "DATE", "AOS", "IN", "MAX", "AZIMUTH", "DUR", "LOS"],
                vec![
                    Constraint::Length(12),
                    Constraint::Length(10),
                    Constraint::Length(8),
                    Constraint::Length(7),
                    Constraint::Length(4),
                    Constraint::Length(9),
                    Constraint::Length(4),
                    Constraint::Length(8),
                ],
            ),
        };
        let what = match app.list {
            EventList::Acquisitions => "imaging opportunities",
            EventList::Passes => "passes",
        };
        let mut title = format!("{what} · {}", events.len());
        if app.only_selected {
            title.push_str(&format!(" · {}", app.sats[app.selected].info.name));
        }
        if app.show_night && app.list == EventList::Acquisitions {
            title.push_str(" · incl. night");
        }
        (rows, header, widths, title)
    };
    let table = Table::new(rows, widths)
        .header(Row::new(header).style(Style::new().fg(theme.dim).add_modifier(Modifier::BOLD)))
        .block(panel(title, app, theme).title_bottom(hint_line(&[("v", "images/passes"), ("n", "night"), ("f", "filter")], theme)))
        .row_highlight_style(Style::new().bg(theme.faint).add_modifier(Modifier::BOLD))
        .highlight_symbol("▸");
    f.render_stateful_widget(table, area, &mut app.event_table);
}

fn draw_event_info(f: &mut Frame, area: Rect, app: &App, theme: &Theme, ev: &Selected) {
    let info = &app.sats[ev.sat].info;
    let now = app.clock.now();
    let mut lines = vec![
        Line::from(Span::styled(info.name, Style::new().fg(info.color).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled(fmt_time(app, ev.time, "%a %d %b %Y"), Style::new().fg(theme.text))),
        Line::from(vec![
            Span::styled(fmt_time(app, ev.time, "%H:%M:%S"), Style::new().fg(theme.text).add_modifier(Modifier::BOLD)),
            Span::styled(if app.utc { " UTC" } else { " local" }, Style::new().fg(theme.dim)),
            Span::styled(format!("  ({})", countdown(now, ev.time)), Style::new().fg(theme.dim)),
        ]),
        Line::raw(""),
    ];
    for (k, v) in &ev.lines {
        lines.push(Line::from(kv(k, v.clone(), theme)));
    }
    if let Some(light) = ev.light {
        let verdict = match (info.kind, light) {
            (Kind::Sar, _) => Span::styled("✓ radar images day and night", Style::new().fg(theme.good)),
            (_, Light::Day) => Span::styled("✓ daylight acquisition", Style::new().fg(theme.good)),
            (_, Light::Low) => Span::styled("! low sun", Style::new().fg(theme.warn)),
            (_, Light::Night) => Span::styled("✗ night: optical sensor idle", Style::new().fg(theme.bad)),
        };
        lines.push(Line::raw(""));
        lines.push(Line::from(verdict));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }).block(panel("event".into(), app, theme)), area);
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [v] = Layout::vertical([Constraint::Length(height)]).flex(Flex::Center).areas(area);
    let [h] = Layout::horizontal([Constraint::Length(width)]).flex(Flex::Center).areas(v);
    h
}

fn draw_input(f: &mut Frame, app: &App, theme: &Theme) {
    let Some(prompt) = &app.prompt else { return };
    let (title, help, example) = match prompt.kind {
        PromptKind::Aoi if crate::platform::THREADS => {
            ("area of interest", "Latitude, longitude and a name, or a GeoJSON file or URL", "-3.10, -60.02 Manaus   ·   area.geojson")
        }
        PromptKind::Aoi => (
            "area of interest",
            "Latitude, longitude and a name, or a GeoJSON URL (or drop a file on the page)",
            "-3.10, -60.02 Manaus",
        ),
        PromptKind::AddSatellite => (
            "add satellite",
            "NORAD id, then optional swath (km or near..far), kind and name",
            "41240 0 Jason-3   ·   57320 120 optical   ·   39634 340..620 sar",
        ),
    };
    let area = centered(f.area(), 76.min(f.area().width), 8);
    let lines = vec![
        Line::from(Span::styled(help, Style::new().fg(theme.text))),
        Line::from(Span::styled(format!("e.g. {example}"), Style::new().fg(theme.dim))),
        Line::raw(""),
        Line::from(vec![
            Span::styled("› ", Style::new().fg(theme.accent)),
            Span::styled(prompt.text.clone(), Style::new().fg(theme.text).add_modifier(Modifier::BOLD)),
            Span::styled("█", Style::new().fg(theme.accent)),
        ]),
        Line::raw(""),
        hint_line(&[("enter", "apply"), ("esc", "cancel")], theme),
    ];
    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(lines).block(panel(title.into(), app, theme)), area);
}

fn draw_help(f: &mut Frame, app: &App, theme: &Theme) {
    let keys: &[(&str, &str)] = &[
        ("tab  1  2", "switch monitor / agenda"),
        ("↑ ↓  j k", "select satellite or event"),
        ("PgUp PgDn", "move faster"),
        ("m", "globe / flat map"),
        ("c", "centre globe: AOI → satellite → free"),
        ("← → a d w s", "rotate the globe"),
        ("o", "spin the globe"),
        ("+ -", "zoom"),
        ("l", "labels: all / selected / none"),
        ("[ ]", "time warp slower / faster"),
        (", .", "jump one hour back / ahead"),
        ("space  p", "pause time"),
        ("0", "back to real time"),
        ("v", "imaging opportunities / horizon passes"),
        ("n", "include night-time optical overflights"),
        ("f", "only the selected satellite"),
        ("z", "timeline span 12 / 24 / 48 / 72 h"),
        ("u", "local time / UTC"),
        ("e", "set the area of interest"),
        ("i", "add a satellite by NORAD id"),
        ("D", "remove the selected satellite"),
        ("r", "download fresh elements"),
        ("q  esc", "quit"),
    ];
    let mut lines: Vec<Line> = keys
        .iter()
        .map(|(k, what)| {
            Line::from(vec![
                Span::styled(format!("  {k:<14}"), Style::new().fg(theme.accent)),
                Span::styled(what.to_string(), Style::new().fg(theme.text)),
            ])
        })
        .collect();
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  Imaging opportunities are geometric: the AOI lies in the sensor swath.",
        Style::new().fg(theme.dim),
    )));
    lines.push(Line::from(Span::styled(
        "  Actual acquisitions follow each mission's observation plan.",
        Style::new().fg(theme.dim),
    )));
    let area = centered(f.area(), 78.min(f.area().width), (lines.len() as u16 + 2).min(f.area().height));
    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(lines).block(panel("keys".into(), app, theme)), area);
}
