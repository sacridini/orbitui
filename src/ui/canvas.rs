//! Canvas helpers: square units and clipped lines.

use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::canvas::{Context, Line};

/// Canvas bounds where one unit has the same physical length on both axes,
/// assuming terminal cells twice as tall as wide. The origin is the centre.
#[derive(Clone, Copy)]
pub struct Plane {
    pub xb: [f64; 2],
    pub yb: [f64; 2],
}

impl Plane {
    pub fn square(area: Rect) -> Self {
        let w = area.width as f64;
        let h = area.height as f64 * 2.0;
        Plane { xb: [-w, w], yb: [-h, h] }
    }

    /// Half the shorter side, in units.
    pub fn half_min(&self) -> f64 {
        self.xb[1].min(self.yb[1])
    }

    /// Units per cell horizontally.
    pub fn cell_width(&self) -> f64 {
        2.0
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.xb[0] && x <= self.xb[1] && y >= self.yb[0] && y <= self.yb[1]
    }

    /// Draws the part of a segment inside the bounds (Liang–Barsky).
    pub fn line(&self, ctx: &mut Context, x1: f64, y1: f64, x2: f64, y2: f64, color: Color) {
        const EPS: f64 = 1e-6;
        let (dx, dy) = (x2 - x1, y2 - y1);
        let (mut t0, mut t1) = (0.0f64, 1.0f64);
        let edges = [
            (-dx, x1 - (self.xb[0] + EPS)),
            (dx, (self.xb[1] - EPS) - x1),
            (-dy, y1 - (self.yb[0] + EPS)),
            (dy, (self.yb[1] - EPS) - y1),
        ];
        for (p, q) in edges {
            if p == 0.0 {
                if q < 0.0 {
                    return;
                }
            } else {
                let r = q / p;
                if p < 0.0 {
                    if r > t1 {
                        return;
                    }
                    t0 = t0.max(r);
                } else {
                    if r < t0 {
                        return;
                    }
                    t1 = t1.min(r);
                }
            }
        }
        ctx.draw(&Line::new(x1 + t0 * dx, y1 + t0 * dy, x1 + t1 * dx, y1 + t1 * dy, color));
    }

    /// Joins consecutive visible points; `None` breaks the line.
    pub fn polyline(&self, ctx: &mut Context, points: impl IntoIterator<Item = Option<(f64, f64)>>, color: Color, dashed: bool) {
        let mut prev: Option<(f64, f64)> = None;
        for (k, p) in points.into_iter().enumerate() {
            if let (Some(a), Some(b)) = (prev, p)
                && (!dashed || k % 2 == 0)
            {
                self.line(ctx, a.0, a.1, b.0, b.1, color);
            }
            prev = p;
        }
    }
}

/// Deterministic pseudo-random numbers in [0, 1) (for the star field).
pub struct Lcg(pub u64);

impl Lcg {
    pub fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}
