//! Local view of the swath over the AOI around one overflight.

use super::canvas::Plane;
use super::globe::swath_edges;
use super::Theme;
use crate::app::App;
use crate::astro;
use chrono::{DateTime, Duration, Utc};
use ratatui::prelude::*;
use ratatui::widgets::canvas::Canvas;

const KM_PER_DEG_LAT: f64 = 110.574;
const KM_PER_DEG_LON: f64 = 111.320;

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme, sat: usize, time: DateTime<Utc>) {
    let Some(s) = app.sats.get(sat) else { return };
    let info = &s.info;
    let obs = &app.observer;

    // Signed cross-track distance of the AOI at closest approach.
    let cross = s.state(time).zip(s.state(time + Duration::seconds(10))).map(|(a, b)| {
        let track = astro::bearing(a.geo.lat, a.geo.lon, b.geo.lat, b.geo.lon);
        let to_aoi = astro::bearing(a.geo.lat, a.geo.lon, obs.lat, obs.lon);
        let side = if (to_aoi - track).rem_euclid(360.0) < 180.0 { 1.0 } else { -1.0 };
        (side * astro::central_angle(a.geo.lat, a.geo.lon, obs.lat, obs.lon) * astro::MEAN_RADIUS_KM, b.ascending)
    });
    let subtitle = match cross {
        Some((x, _)) if info.covers(x) => format!(" · AOI {:.0} km {} of track · inside swath", x.abs(), side_word(x)),
        Some((x, _)) => format!(" · AOI {:.0} km {} of track", x.abs(), side_word(x)),
        None => String::new(),
    };
    let block = super::panel(format!("footprint · {}{}", info.name, subtitle), app, theme);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 10 || inner.height < 4 {
        return;
    }

    // Extent: the swath, the AOI's offset and some margin, in km.
    let area_reach = obs.area.as_ref().map_or(0.0, |a| {
        a.rings.iter().flatten().map(|&(lat, lon)| astro::central_angle(lat, lon, obs.lat, obs.lon)).fold(0.0, f64::max)
            * astro::MEAN_RADIUS_KM
    });
    let reach = info.near_km.abs().max(info.far_km.abs()).max(cross.map_or(0.0, |c| c.0.abs())).max(area_reach);
    let extent = (reach * 1.2 + 40.0).max(120.0);
    let plane = Plane::square(inner);
    let k = plane.half_min() / extent;
    let coslat = obs.lat.to_radians().cos();
    let to = |(lat, lon): (f64, f64)| {
        let dlon = astro::wrap_lon(lon - obs.lon);
        (dlon * coslat * KM_PER_DEG_LON * k, (lat - obs.lat) * KM_PER_DEG_LAT * k)
    };

    // Long enough to cross the whole view at ~6.8 km/s ground speed.
    let span = Duration::seconds((plane.yb[1].max(plane.xb[1]) / k / 6.0) as i64 + 30);
    let track = s.ground_track(time - span, time + span, 2);
    let edges = swath_edges(&track, info);
    let heading_up = cross.is_some_and(|c| c.1);

    let canvas = Canvas::default().marker(app.marker).x_bounds(plane.xb).y_bounds(plane.yb).paint(|ctx| {
        // Graticule at a spacing that suits the extent.
        let step = if extent > 900.0 { 5.0 } else if extent > 350.0 { 2.0 } else if extent > 150.0 { 1.0 } else { 0.5 };
        let half_deg = extent * 2.5 / KM_PER_DEG_LAT;
        let snap = |v: f64| (v / step).floor() * step;
        let mut lat = snap(obs.lat - half_deg);
        while lat <= obs.lat + half_deg {
            let (x0, y) = to((lat, obs.lon - half_deg / coslat));
            let (x1, _) = to((lat, obs.lon + half_deg / coslat));
            plane.line(ctx, x0, y, x1, y, theme.grid);
            lat += step;
        }
        let mut lon = snap(obs.lon - half_deg / coslat);
        while lon <= obs.lon + half_deg / coslat {
            let (x, y0) = to((obs.lat - half_deg, lon));
            let (_, y1) = to((obs.lat + half_deg, lon));
            plane.line(ctx, x, y0, x, y1, theme.grid);
            lon += step;
        }
        ctx.layer();

        // Hatched swath: rungs between the edges, ~3 units apart.
        let rung_km = 3.0 / k;
        let stride = (rung_km / 13.6).ceil().max(1.0) as usize;
        for (a, b) in edges[0].iter().zip(&edges[1]).step_by(stride) {
            let (pa, pb) = (to(*a), to(*b));
            plane.line(ctx, pa.0, pa.1, pb.0, pb.1, theme.swath);
        }
        ctx.layer();

        let coast = &app.world.fine;
        let max_dist = extent * 3.0;
        for line in &coast.lines {
            for w in line.points.windows(2) {
                let (a, b) = (to(w[0]), to(w[1]));
                if a.0.abs().max(a.1.abs()) / k < max_dist && (a.0 - b.0).abs() / k < 500.0 {
                    plane.line(ctx, a.0, a.1, b.0, b.1, theme.land_day);
                }
            }
        }
        ctx.layer();

        if let Some(area) = &obs.area {
            for ring in &area.rings {
                plane.polyline(ctx, ring.iter().chain(ring.first()).map(|&p| Some(to(p))), theme.aoi, false);
            }
        }
        plane.polyline(ctx, track.iter().map(|&p| Some(to(p))), theme.track_past, true);
        for edge in &edges {
            plane.polyline(ctx, edge.iter().map(|&p| Some(to(p))), info.color, false);
        }
        ctx.layer();

        if let Some(st) = s.state(time) {
            let (x, y) = to((st.geo.lat, st.geo.lon));
            if plane.contains(x, y) {
                let arrow = if heading_up { "▲" } else { "▼" };
                ctx.print(x, y, Span::styled(arrow, Style::new().fg(info.color).add_modifier(Modifier::BOLD)));
            }
        }
        let style = Style::new().fg(theme.aoi).add_modifier(Modifier::BOLD);
        ctx.print(0.0, 0.0, Line::from(vec![Span::styled("♦ ", style), Span::styled(obs.name.clone(), style)]));

        // Scale bar in the lower-left corner.
        let bar_km = [50.0, 100.0, 200.0, 500.0, 1000.0].into_iter().rev().find(|&km| km * k <= plane.xb[1] * 0.6).unwrap_or(50.0);
        let cells = ((bar_km * k) / plane.cell_width()).round().max(3.0) as usize;
        let label = format!("{bar_km:.0} km");
        let bar = format!("├{}┤ {label}", "─".repeat(cells.saturating_sub(2)));
        ctx.print(plane.xb[0] + 1.0, plane.yb[0] + 1.0, Span::styled(bar, Style::new().fg(theme.dim)));
    });
    f.render_widget(canvas, inner);
}

fn side_word(x: f64) -> &'static str {
    if x >= 0.0 { "right" } else { "left" }
}
