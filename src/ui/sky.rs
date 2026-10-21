//! Polar sky plot of a pass as seen from the AOI.

use super::canvas::Plane;
use super::Theme;
use crate::app::App;
use crate::predict;
use chrono::{DateTime, Utc};
use ratatui::prelude::*;
use ratatui::widgets::canvas::{Canvas, Circle};

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme, sat: usize, (from, to): (DateTime<Utc>, DateTime<Utc>), mark: DateTime<Utc>) {
    let block = super::panel("sky from AOI".into(), app, theme);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let Some(s) = app.sats.get(sat) else { return };
    if inner.width < 10 || inner.height < 5 {
        return;
    }
    let plane = Plane::square(inner);
    let r = plane.half_min() * 0.78;
    let at = |az: f64, el: f64| {
        let rho = r * (90.0 - el.max(0.0)) / 90.0;
        let a = az.to_radians();
        (rho * a.sin(), rho * a.cos())
    };
    let track = predict::sky_track(s, &app.observer, from, to, 10);
    let look_at = |t: DateTime<Utc>| s.state(t).map(|st| app.observer.look(st.ecef));
    let mark_look = look_at(mark).filter(|l| l.el >= 0.0);
    let now = app.clock.now();
    let now_look = look_at(now).filter(|l| l.el >= 0.0);
    let color = s.info.color;

    let canvas = Canvas::default().marker(app.marker).x_bounds(plane.xb).y_bounds(plane.yb).paint(|ctx| {
        for (el, c) in [(0.0, theme.limb), (30.0, theme.grid), (60.0, theme.grid)] {
            ctx.draw(&Circle { x: 0.0, y: 0.0, radius: r * (90.0 - el) / 90.0, color: c });
        }
        plane.line(ctx, -r, 0.0, r, 0.0, theme.grid);
        plane.line(ctx, 0.0, -r, 0.0, r, theme.grid);
        ctx.layer();
        plane.polyline(ctx, track.iter().map(|&(az, el)| Some(at(az, el))), color, false);
        ctx.layer();
        let label = Style::new().fg(theme.dim);
        let cw = plane.cell_width();
        ctx.print(-cw / 2.0, r + 2.5, Span::styled("N", label));
        ctx.print(-cw / 2.0, -r - 3.0, Span::styled("S", label));
        ctx.print(r + cw / 2.0, 0.0, Span::styled("E", label));
        ctx.print(-r - cw * 1.5, 0.0, Span::styled("W", label));
        if r > 24.0 {
            ctx.print(r / 3.0 + 0.5, 1.0, Span::styled("60°", Style::new().fg(theme.dim)));
            ctx.print(2.0 * r / 3.0 + 0.5, 1.0, Span::styled("30°", Style::new().fg(theme.dim)));
        }
        if let (Some(first), Some(last)) = (track.first(), track.last()) {
            let (x, y) = at(first.0, first.1);
            ctx.print(x, y, Span::styled("▲", Style::new().fg(theme.good)));
            let (x, y) = at(last.0, last.1);
            ctx.print(x, y, Span::styled("▼", Style::new().fg(theme.bad)));
        }
        if let Some(l) = mark_look {
            let (x, y) = at(l.az, l.el);
            ctx.print(x, y, Span::styled("♦", Style::new().fg(theme.aoi).add_modifier(Modifier::BOLD)));
        }
        if let Some(l) = now_look {
            let (x, y) = at(l.az, l.el);
            ctx.print(x, y, Span::styled("●", Style::new().fg(color).add_modifier(Modifier::BOLD)));
        }
    });
    f.render_widget(canvas, inner);
}
