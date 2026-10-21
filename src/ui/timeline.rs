//! Gantt chart of acquisitions and passes over the next hours.

use super::Theme;
use crate::app::App;
use crate::astro;
use chrono::{DateTime, Duration, Local, Timelike, Utc};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

const LABEL: usize = 7;

fn window(app: &App) -> (DateTime<Utc>, Duration) {
    let span = Duration::hours(app.timeline_hours);
    (app.clock.now() - span / 12, span)
}

/// Satellites with something to show in the window, in catalog order.
fn rows(app: &App) -> Vec<usize> {
    let Some(fc) = &app.forecast else { return vec![app.selected] };
    let (t0, span) = window(app);
    let t1 = t0 + span;
    let mut show = vec![false; app.sats.len()];
    for a in fc.acquisitions.iter().filter(|a| a.time >= t0 && a.time <= t1) {
        if app.show_night || a.usable(app.sats[a.sat].info.kind) {
            show[a.sat] = true;
        }
    }
    for p in fc.passes.iter().filter(|p| p.los >= t0 && p.aos <= t1) {
        show[p.sat] = true;
    }
    if let Some(s) = show.get_mut(app.selected) {
        *s = true;
    }
    (0..show.len()).filter(|&i| show[i] && (!app.only_selected || i == app.selected)).collect()
}

/// Height the timeline wants, borders included.
pub fn height(app: &App) -> u16 {
    rows(app).len() as u16 + 4
}

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme, selected: Option<(usize, DateTime<Utc>)>) {
    let legend = Line::from(vec![
        Span::styled(" █", Style::new().fg(theme.good)),
        Span::styled(" image ", Style::new().fg(theme.dim)),
        Span::styled("▒", Style::new().fg(theme.dim)),
        Span::styled(" night ", Style::new().fg(theme.dim)),
        Span::styled("▁", Style::new().fg(theme.text)),
        Span::styled(" pass ", Style::new().fg(theme.dim)),
        Span::styled("│", Style::new().fg(theme.now)),
        Span::styled(" now ", Style::new().fg(theme.dim)),
    ])
    .right_aligned();
    let block = super::panel(format!("timeline · next {} h", app.timeline_hours), app, theme)
        .title_bottom(super::hint_line(&[("z", "span"), ("n", "night"), ("f", "only selected")], theme))
        .title_bottom(legend);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let width = inner.width as usize;
    if width <= LABEL + 4 || inner.height < 2 {
        return;
    }
    let ncol = width - LABEL;
    let (t0, span) = window(app);
    let dt = span.num_seconds() as f64 / ncol as f64;
    let col = |t: DateTime<Utc>| ((t - t0).num_seconds() as f64 / dt).floor() as isize;
    let in_range = |c: isize| (0..ncol as isize).contains(&c);
    let now_col = col(app.clock.now());
    let tick_hours = match app.timeline_hours {
        12 => 2,
        24 => 3,
        48 => 6,
        _ => 12,
    };

    // Hour ticks in the displayed time zone.
    let mut header = vec![(' ', Style::new()); ncol];
    let mut ticks = Vec::new();
    let first_hour = t0 - Duration::seconds(t0.timestamp().rem_euclid(3600));
    let mut free_from = 0isize;
    for h in 0..=app.timeline_hours + 1 {
        let t = first_hour + Duration::hours(h);
        let (hour, label) = if app.utc {
            (t.hour(), t.format("%a %d").to_string())
        } else {
            let l = t.with_timezone(&Local);
            (l.hour(), l.format("%a %d").to_string())
        };
        if hour % tick_hours != 0 {
            continue;
        }
        let c = col(t);
        if !in_range(c) {
            continue;
        }
        ticks.push(c as usize);
        let text = if hour == 0 { label } else { format!("{hour:02}h") };
        let style = if hour == 0 { Style::new().fg(theme.text).add_modifier(Modifier::BOLD) } else { Style::new().fg(theme.dim) };
        if c >= free_from && c as usize + text.chars().count() <= ncol {
            for (k, ch) in text.chars().enumerate() {
                header[c as usize + k] = (ch, style);
            }
            free_from = c + text.chars().count() as isize + 1;
        }
    }

    let base_row = || {
        let mut row = vec![('·', Style::new().fg(theme.faint)); ncol];
        for &c in &ticks {
            row[c] = ('┊', Style::new().fg(theme.faint));
        }
        row
    };

    // Daylight at the AOI.
    let mut sun = base_row();
    for (c, cell) in sun.iter_mut().enumerate() {
        let t = t0 + Duration::seconds(((c as f64 + 0.5) * dt) as i64);
        let el = astro::sun_elevation(app.observer.lat, app.observer.lon, astro::subsolar_point(t));
        *cell = if el > 0.0 {
            ('━', Style::new().fg(theme.sun))
        } else if el > -6.0 {
            ('─', Style::new().fg(theme.dim))
        } else {
            ('─', Style::new().fg(theme.night))
        };
    }

    let mut lines = vec![finish(&" ".repeat(LABEL), Style::new(), header, None, theme)];
    lines.push(finish(&format!("{:<LABEL$}", "☼ sun"), Style::new().fg(theme.sun), sun, now_col.try_into().ok(), theme));

    for i in rows(app).into_iter().take(inner.height.saturating_sub(2) as usize) {
        let info = &app.sats[i].info;
        let mut row = base_row();
        if let Some(fc) = &app.forecast {
            for p in fc.passes.iter().filter(|p| p.sat == i) {
                let (a, b) = (col(p.aos), col(p.los));
                for c in a.max(0)..=b.min(ncol as isize - 1) {
                    row[c as usize] = ('▁', Style::new().fg(info.color));
                }
            }
            for a in fc.acquisitions.iter().filter(|a| a.sat == i) {
                let c = col(a.time);
                if !in_range(c) {
                    continue;
                }
                let usable = a.usable(info.kind);
                if !usable && !app.show_night {
                    continue;
                }
                let mut cell = if usable { ('█', Style::new().fg(info.color)) } else { ('▒', Style::new().fg(theme.dim)) };
                if selected == Some((i, a.time)) {
                    cell = ('█', Style::new().fg(theme.aoi).add_modifier(Modifier::BOLD));
                }
                row[c as usize] = cell;
            }
            if let Some((_, t)) = selected.filter(|s| s.0 == i)
                && let Some(p) = fc.passes.iter().find(|p| p.sat == i && p.tca == t)
            {
                let (a, b) = (col(p.aos), col(p.los));
                for c in a.max(0)..=b.min(ncol as isize - 1) {
                    row[c as usize] = ('▀', Style::new().fg(theme.aoi).add_modifier(Modifier::BOLD));
                }
            }
        }
        let mut label_style = Style::new().fg(info.color);
        let marker = if i == app.selected {
            label_style = label_style.add_modifier(Modifier::BOLD);
            '▸'
        } else {
            ' '
        };
        let label = format!("{marker}{:<w$}", info.short, w = LABEL - 1);
        lines.push(finish(&label, label_style, row, now_col.try_into().ok(), theme));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// Joins a label and cells into a line, drawing the "now" cursor.
fn finish(label: &str, label_style: Style, mut cells: Vec<(char, Style)>, now: Option<usize>, theme: &Theme) -> Line<'static> {
    if let Some(c) = now
        && let Some(cell) = cells.get_mut(c)
        && matches!(cell.0, '·' | '┊' | '─' | '━')
    {
        *cell = ('│', Style::new().fg(theme.now));
    }
    let mut spans = vec![Span::styled(label.to_string(), label_style)];
    // Merge runs of the same style to keep the span count low.
    let mut run = String::new();
    let mut run_style = Style::new();
    for (ch, style) in cells {
        if style != run_style && !run.is_empty() {
            spans.push(Span::styled(std::mem::take(&mut run), run_style));
        }
        run_style = style;
        run.push(ch);
    }
    if !run.is_empty() {
        spans.push(Span::styled(run, run_style));
    }
    Line::from(spans)
}
