//! The globe (orthographic) and the world map (equirectangular).

use super::canvas::{Lcg, Plane};
use super::Theme;
use crate::app::{App, Center, Labels};
use crate::astro::{self, EARTH_RADIUS_KM, MEAN_RADIUS_KM, Vec3, dot};
use crate::catalog::SatInfo;
use crate::orbit::SatState;
use chrono::{DateTime, Duration, Utc};
use ratatui::prelude::*;
use ratatui::widgets::canvas::{Canvas, Circle, Context, Points};

/// Everything time-dependent drawn on the globe and the map.
struct Scene {
    states: Vec<Option<SatState>>,
    sun: Vec3,
    subsolar: (f64, f64),
    past: Vec<(f64, f64)>,
    future: Vec<(f64, f64)>,
    edges: [Vec<(f64, f64)>; 2],
    footprint: Vec<(f64, f64)>,
}

impl Scene {
    fn new(app: &App, now: DateTime<Utc>) -> Self {
        let subsolar = astro::subsolar_point(now);
        let mut scene = Scene {
            states: app.sats.iter().map(|s| s.state(now)).collect(),
            sun: astro::unit_vector(subsolar.0, subsolar.1),
            subsolar,
            past: Vec::new(),
            future: Vec::new(),
            edges: [Vec::new(), Vec::new()],
            footprint: Vec::new(),
        };
        if let Some(sat) = app.sat() {
            let period = Duration::seconds((sat.period_minutes() * 60.0) as i64);
            scene.past = sat.ground_track(now - period / 2, now, 30);
            scene.future = sat.ground_track(now, now + period, 30);
            scene.edges = swath_edges(&scene.future[..scene.future.len() / 2], &sat.info);
            if let Some(Some(st)) = scene.states.get(app.selected) {
                let angle = astro::coverage_angle(st.geo.alt, 0.0);
                scene.footprint = (0..=72)
                    .map(|k| astro::destination(st.geo.lat, st.geo.lon, k as f64 * 5.0, angle))
                    .collect();
            }
        }
        scene
    }
}

/// Swath edges along a ground track, offset across track.
pub fn swath_edges(track: &[(f64, f64)], info: &SatInfo) -> [Vec<(f64, f64)>; 2] {
    let mut edges = [Vec::new(), Vec::new()];
    if !info.images() {
        return edges;
    }
    for w in track.windows(2) {
        let ((lat, lon), (lat2, lon2)) = (w[0], w[1]);
        let heading = astro::bearing(lat, lon, lat2, lon2);
        for (edge, x) in edges.iter_mut().zip([info.near_km, info.far_km]) {
            let side = if x >= 0.0 { 90.0 } else { -90.0 };
            edge.push(astro::destination(lat, lon, heading + side, x.abs() / MEAN_RADIUS_KM));
        }
    }
    edges
}

/// Orthographic view from infinity above (lat, lon).
struct Ortho {
    c: Vec3,
    e: Vec3,
    n: Vec3,
    r: f64,
}

impl Ortho {
    fn new(lat: f64, lon: f64, r: f64) -> Self {
        let (la, lo) = (lat.to_radians(), lon.to_radians());
        Ortho {
            c: [la.cos() * lo.cos(), la.cos() * lo.sin(), la.sin()],
            e: [-lo.sin(), lo.cos(), 0.0],
            n: [-la.sin() * lo.cos(), -la.sin() * lo.sin(), la.cos()],
            r,
        }
    }

    /// A point on the surface; hidden on the far side.
    fn surface(&self, p: Vec3) -> Option<(f64, f64)> {
        (dot(p, self.c) >= 0.0).then(|| (dot(p, self.e) * self.r, dot(p, self.n) * self.r))
    }

    fn surface_ll(&self, (lat, lon): (f64, f64)) -> Option<(f64, f64)> {
        self.surface(astro::unit_vector(lat, lon))
    }

    /// A point in space, in Earth radii; hidden only behind the disk.
    fn space(&self, p: Vec3) -> Option<(f64, f64)> {
        let (x, y, z) = (dot(p, self.e), dot(p, self.n), dot(p, self.c));
        (z >= 0.0 || x * x + y * y >= 1.0).then_some((x * self.r, y * self.r))
    }
}

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let now = app.clock.now();
    let mode = match (app.globe.flat, app.globe.center) {
        (true, _) => "map".to_string(),
        (false, Center::Aoi) => "globe · AOI".to_string(),
        (false, Center::Satellite) => format!("globe · {}", app.sat().map_or("-", |s| s.info.name)),
        (false, Center::Free) if app.globe.spin => "globe · spinning".to_string(),
        (false, Center::Free) => "globe · free".to_string(),
    };
    let zoom = if app.globe.zoom != 1.0 { format!(" ×{:.1}", app.globe.zoom) } else { String::new() };
    let block = super::panel(format!("{mode}{zoom}"), app, theme)
        .title_bottom(super::hint_line(&[("m", "map/globe"), ("c", "centre"), ("+/-", "zoom"), ("l", "labels")], theme).right_aligned());
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 8 || inner.height < 4 {
        return;
    }
    let scene = Scene::new(app, now);
    let plane = Plane::square(inner);
    let canvas = Canvas::default().marker(app.marker).x_bounds(plane.xb).y_bounds(plane.yb).paint(|ctx| {
        if app.globe.flat {
            paint_map(ctx, &plane, app, &scene, theme);
        } else {
            paint_globe(ctx, &plane, app, &scene, theme, now);
        }
    });
    f.render_widget(canvas, inner);
}

fn paint_globe(ctx: &mut Context, plane: &Plane, app: &App, scene: &Scene, theme: &Theme, now: DateTime<Utc>) {
    let r = plane.half_min() * 0.9 * app.globe.zoom;
    let (clat, clon) = app.globe_center(now);
    let view = Ortho::new(clat, clon, r);

    // Stars, fixed so they do not flicker.
    let mut rng = Lcg(0x5eed);
    let n = (plane.xb[1] * plane.yb[1] / 14.0) as usize;
    let mut stars = Vec::with_capacity(n);
    for _ in 0..n {
        let (x, y) = (plane.xb[0] + rng.next() * 2.0 * plane.xb[1], plane.yb[0] + rng.next() * 2.0 * plane.yb[1]);
        if x * x + y * y > (r * 1.12).powi(2) && rng.next() < 0.5 {
            stars.push((x, y));
        }
    }
    ctx.draw(&Points { coords: &stars, color: theme.star });

    // Night hemisphere as a dot screen, about one dot every three units.
    let step = (3.0 / (r * std::f64::consts::PI / 180.0)).clamp(0.6, 12.0);
    let mut night = Vec::new();
    let mut lat = -90.0 + step / 2.0;
    while lat < 90.0 {
        let lon_step = step / lat.to_radians().cos().max(0.05);
        let mut lon = -180.0;
        while lon < 180.0 {
            let p = astro::unit_vector(lat, lon);
            if dot(p, scene.sun) < -0.01
                && let Some(xy) = view.surface(p)
            {
                night.push(xy);
            }
            lon += lon_step;
        }
        lat += step;
    }
    ctx.draw(&Points { coords: &night, color: theme.night });
    ctx.layer();

    // Graticule and limb.
    for lon in (-180..180).step_by(30) {
        plane.polyline(ctx, (-90..=90).step_by(3).map(|lat| view.surface_ll((lat as f64, lon as f64))), theme.grid, false);
    }
    for lat in (-60..=60).step_by(30) {
        plane.polyline(ctx, (-180..=180).step_by(3).map(|lon| view.surface_ll((lat as f64, lon as f64))), theme.grid, false);
    }
    ctx.draw(&Circle { x: 0.0, y: 0.0, radius: r, color: theme.limb });

    // Terminator: the great circle 90° from the Sun.
    // The Sun never leaves the tropics, so it is never parallel to the axis.
    let u = cross(scene.sun, [0.0, 0.0, 1.0]);
    let u = scale(u, 1.0 / astro::norm(u));
    let v = cross(scene.sun, u);
    plane.polyline(
        ctx,
        (0..=120).map(|k| {
            let (s, c) = (k as f64 * 3.0).to_radians().sin_cos();
            view.surface([c * u[0] + s * v[0], c * u[1] + s * v[1], c * u[2] + s * v[2]])
        }),
        theme.terminator,
        true,
    );
    ctx.layer();

    // Coastlines, lit and unlit.
    let coast = if r > 140.0 { &app.world.fine } else { &app.world.coarse };
    for line in &coast.lines {
        let projected: Vec<Option<(f64, f64)>> = line.xyz.iter().map(|&p| view.surface(p)).collect();
        for k in 1..projected.len() {
            if let (Some(a), Some(b)) = (projected[k - 1], projected[k]) {
                let lit = dot(line.xyz[k], scene.sun) >= 0.0;
                plane.line(ctx, a.0, a.1, b.0, b.1, if lit { theme.land_day } else { theme.land_night });
            }
        }
    }
    ctx.layer();

    // Selected satellite: visibility circle, ground track, swath.
    if let Some(sat) = app.sat() {
        plane.polyline(ctx, scene.footprint.iter().map(|&p| view.surface_ll(p)), theme.footprint, true);
        plane.polyline(ctx, scene.past.iter().map(|&p| view.surface_ll(p)), theme.track_past, false);
        plane.polyline(ctx, scene.future.iter().map(|&p| view.surface_ll(p)), sat.info.color, false);
        for edge in &scene.edges {
            plane.polyline(ctx, edge.iter().map(|&p| view.surface_ll(p)), sat.info.color, true);
        }
        if let Some(Some(st)) = scene.states.get(app.selected) {
            let nadir = view.surface_ll((st.geo.lat, st.geo.lon));
            let up = view.space(scale(st.ecef, 1.0 / EARTH_RADIUS_KM));
            if let (Some(a), Some(b)) = (nadir, up) {
                plane.line(ctx, a.0, a.1, b.0, b.1, theme.footprint);
            }
        }
    }
    ctx.layer();

    if let Some(area) = &app.observer.area {
        for ring in &area.rings {
            plane.polyline(ctx, ring.iter().chain(ring.first()).map(|&p| view.surface_ll(p)), theme.aoi, false);
        }
    }
    ctx.layer();

    // Markers last, so they sit on top.
    if let Some((x, y)) = view.surface_ll(scene.subsolar) {
        ctx.print(x, y, Span::styled("☼", Style::new().fg(theme.sun)));
    }
    if let Some((x, y)) = view.surface_ll((app.observer.lat, app.observer.lon)) {
        aoi_marker(ctx, x, y, app, theme);
    }
    for (i, st) in scene.states.iter().enumerate() {
        if let Some(st) = st
            && i != app.selected
            && let Some((x, y)) = view.space(scale(st.ecef, 1.0 / EARTH_RADIUS_KM))
        {
            sat_marker(ctx, x, y, app, i);
        }
    }
    if let Some(Some(st)) = scene.states.get(app.selected)
        && let Some((x, y)) = view.space(scale(st.ecef, 1.0 / EARTH_RADIUS_KM))
    {
        sat_marker(ctx, x, y, app, app.selected);
    }
}

fn paint_map(ctx: &mut Context, plane: &Plane, app: &App, scene: &Scene, theme: &Theme) {
    let s = (plane.xb[1] / 180.0).min(plane.yb[1] / 90.0);
    let to = |(lat, lon): (f64, f64)| (lon * s, lat * s);

    let step = (3.0 / s).clamp(0.5, 15.0);
    let mut night = Vec::new();
    let mut lat = -90.0 + step / 2.0;
    while lat < 90.0 {
        let mut lon = -180.0 + step / 2.0;
        while lon < 180.0 {
            if astro::sun_elevation(lat, lon, scene.subsolar) < -0.5 {
                night.push(to((lat, lon)));
            }
            lon += step;
        }
        lat += step;
    }
    ctx.draw(&Points { coords: &night, color: theme.night });
    ctx.layer();

    for lon in (-180..=180).step_by(30) {
        let (x, y0) = to((-90.0, lon as f64));
        plane.line(ctx, x, y0, x, -y0, theme.grid);
    }
    for lat in (-90..=90).step_by(30) {
        let (x0, y) = to((lat as f64, -180.0));
        plane.line(ctx, x0, y, -x0, y, theme.grid);
    }
    ctx.layer();

    let coast = if s > 1.6 { &app.world.fine } else { &app.world.coarse };
    for line in &coast.lines {
        for (k, w) in line.points.windows(2).enumerate() {
            if (w[0].1 - w[1].1).abs() < 180.0 {
                let lit = dot(line.xyz[k + 1], scene.sun) >= 0.0;
                let (a, b) = (to(w[0]), to(w[1]));
                plane.line(ctx, a.0, a.1, b.0, b.1, if lit { theme.land_day } else { theme.land_night });
            }
        }
    }
    ctx.layer();

    let wrapped = |pts: &[(f64, f64)]| -> Vec<Option<(f64, f64)>> {
        let mut out = Vec::with_capacity(pts.len());
        for (k, &p) in pts.iter().enumerate() {
            if k > 0 && (pts[k - 1].1 - p.1).abs() > 180.0 {
                out.push(None);
            }
            out.push(Some(to(p)));
        }
        out
    };
    if let Some(sat) = app.sat() {
        plane.polyline(ctx, wrapped(&scene.footprint), theme.footprint, true);
        plane.polyline(ctx, wrapped(&scene.past), theme.track_past, false);
        plane.polyline(ctx, wrapped(&scene.future), sat.info.color, false);
        for edge in &scene.edges {
            plane.polyline(ctx, wrapped(edge), sat.info.color, true);
        }
    }
    ctx.layer();

    if let Some(area) = &app.observer.area {
        for ring in &area.rings {
            let closed: Vec<(f64, f64)> = ring.iter().chain(ring.first()).copied().collect();
            plane.polyline(ctx, wrapped(&closed), theme.aoi, false);
        }
    }
    ctx.layer();

    let (x, y) = to(scene.subsolar);
    ctx.print(x, y, Span::styled("☼", Style::new().fg(theme.sun)));
    let (x, y) = to((app.observer.lat, app.observer.lon));
    aoi_marker(ctx, x, y, app, theme);
    let order = (0..scene.states.len()).filter(|&i| i != app.selected).chain([app.selected]);
    for i in order {
        if let Some(Some(st)) = scene.states.get(i) {
            let (x, y) = to((st.geo.lat, st.geo.lon));
            sat_marker(ctx, x, y, app, i);
        }
    }
}

fn aoi_marker(ctx: &mut Context, x: f64, y: f64, app: &App, theme: &Theme) {
    let style = Style::new().fg(theme.aoi).add_modifier(Modifier::BOLD);
    if app.labels == Labels::Off {
        ctx.print(x, y, Span::styled("♦", style));
    } else {
        ctx.print(x, y, Line::from(vec![Span::styled("♦ ", style), Span::styled(app.observer.name.clone(), style)]));
    }
}

fn sat_marker(ctx: &mut Context, x: f64, y: f64, app: &App, i: usize) {
    let info = &app.sats[i].info;
    let dot_style = Style::new().fg(info.color).add_modifier(Modifier::BOLD);
    if i == app.selected {
        let tag = Style::new().fg(Color::Black).bg(info.color).add_modifier(Modifier::BOLD);
        ctx.print(x, y, Line::from(vec![Span::styled("● ", dot_style), Span::styled(format!(" {} ", info.name), tag)]));
    } else if app.labels == Labels::All {
        ctx.print(x, y, Line::from(vec![Span::styled("● ", dot_style), Span::styled(info.short, Style::new().fg(info.color))]));
    } else {
        ctx.print(x, y, Span::styled("●", dot_style));
    }
}

fn scale(v: Vec3, k: f64) -> Vec3 {
    [v[0] * k, v[1] * k, v[2] * k]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
