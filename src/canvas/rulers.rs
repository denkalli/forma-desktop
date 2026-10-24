//! Rulers along the top and left edges of the canvas, in page units.

use std::f32::consts::FRAC_PI_2;

use eframe::egui::{self, Pos2, Rect, Stroke};
use libforma::geom::{self, Point};

use super::Mapping;
use crate::theme::Theme;
use crate::theme::egui::c;

/// Thickness of a ruler, in screen points.
pub const RULER_SIZE: f32 = 20.0;
/// Labelled ticks are at least this far apart on screen.
const MIN_MAJOR_GAP: f64 = 50.0;

/// Page-unit distance between labelled ticks at `zoom` (1, 2 or 5 × 10ⁿ), and how many parts
/// the space between them is split into by small ticks.
pub fn ruler_step(zoom: f64) -> (f64, u32) {
    let target = MIN_MAJOR_GAP / zoom;
    let base = 10f64.powf(target.log10().floor());
    [(1.0, 5), (2.0, 4), (5.0, 5), (10.0, 5)]
        .into_iter()
        .map(|(m, parts)| (m * base, parts))
        .find(|(step, _)| *step >= target * (1.0 - 1e-9))
        .unwrap_or((10.0 * base, 5))
}

/// Multiples of `step` between `from` and `to` (page units).
pub fn ticks(from: f64, to: f64, step: f64) -> impl Iterator<Item = f64> {
    let first = (from / step).ceil() as i64;
    let last = (to / step).floor() as i64;
    (first..=last).map(move |i| i as f64 * step)
}

/// A tick label: whole numbers without decimals, and never "-0".
pub fn label(value: f64, step: f64) -> String {
    let decimals = if step >= 1.0 {
        0
    } else {
        (-step.log10().floor()) as usize
    };
    let text = format!("{value:.decimals$}");
    if text
        .trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.')
    {
        text.trim_start_matches('-').to_owned()
    } else {
        text
    }
}

/// Draws both rulers around `canvas` (the area they frame), highlighting the selection.
pub fn draw(
    painter: &egui::Painter,
    canvas: Rect,
    map: Mapping,
    zoom: f64,
    selection: Option<geom::Rect>,
    theme: &Theme,
) {
    let top = Rect::from_min_max(
        egui::pos2(canvas.left(), canvas.top() - RULER_SIZE),
        egui::pos2(canvas.right(), canvas.top()),
    );
    let left = Rect::from_min_max(
        egui::pos2(canvas.left() - RULER_SIZE, canvas.top()),
        egui::pos2(canvas.left(), canvas.bottom()),
    );
    let corner = Rect::from_min_max(
        egui::pos2(left.left(), top.top()),
        egui::pos2(left.right(), top.bottom()),
    );
    for r in [top, left, corner] {
        painter.rect_filled(r, 0.0, c(theme.panel));
    }
    let line = Stroke::new(1.0, c(theme.border));
    painter.line_segment([top.left_bottom(), top.right_bottom()], line);
    painter.line_segment([left.right_top(), left.right_bottom()], line);

    // The selection's extent, in the accent color.
    if let Some(sel) = selection {
        let a = map.to_screen(Point::new(sel.x0, sel.y0));
        let b = map.to_screen(Point::new(sel.x1, sel.y1));
        let fill = c(theme.accent_soft);
        let x = Rect::from_x_y_ranges(a.x.max(top.left())..=b.x.min(top.right()), top.y_range());
        let y = Rect::from_x_y_ranges(left.x_range(), a.y.max(left.top())..=b.y.min(left.bottom()));
        for r in [x, y] {
            if r.is_positive() {
                painter.rect_filled(r, 0.0, fill);
            }
        }
    }

    let (step, parts) = ruler_step(zoom);
    let minor = step / f64::from(parts);
    let page_min = map.to_page(canvas.min);
    let page_max = map.to_page(canvas.max);
    let tick = Stroke::new(1.0, c(theme.text_faint));
    let font = egui::FontId::proportional(9.5);
    let text = c(theme.text_muted);

    for x in ticks(page_min.x, page_max.x, minor) {
        let sx = map.to_screen(Point::new(x, 0.0)).x.round() + 0.5;
        let major = ((x / step) - (x / step).round()).abs() < 1e-6;
        let length = if major {
            RULER_SIZE * 0.45
        } else {
            RULER_SIZE * 0.2
        };
        painter.line_segment(
            [
                Pos2::new(sx, top.bottom()),
                Pos2::new(sx, top.bottom() - length),
            ],
            tick,
        );
        if major {
            painter.text(
                Pos2::new(sx + 3.0, top.top() + 2.0),
                egui::Align2::LEFT_TOP,
                label(x, step),
                font.clone(),
                text,
            );
        }
    }
    for y in ticks(page_min.y, page_max.y, minor) {
        let sy = map.to_screen(Point::new(0.0, y)).y.round() + 0.5;
        let major = ((y / step) - (y / step).round()).abs() < 1e-6;
        let length = if major {
            RULER_SIZE * 0.45
        } else {
            RULER_SIZE * 0.2
        };
        painter.line_segment(
            [
                Pos2::new(left.right(), sy),
                Pos2::new(left.right() - length, sy),
            ],
            tick,
        );
        if major {
            // Read from bottom to top, like the vertical rulers of other design tools.
            let galley = painter.layout_no_wrap(label(y, step), font.clone(), text);
            let pos = Pos2::new(left.left() + 2.0, sy - 3.0);
            painter.add(egui::epaint::TextShape::new(pos, galley, text).with_angle(-FRAC_PI_2));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_1_2_5_and_at_least_50_points_apart() {
        assert_eq!(ruler_step(1.0), (50.0, 5));
        assert_eq!(ruler_step(0.5), (100.0, 5));
        assert_eq!(ruler_step(0.3), (200.0, 4));
        assert_eq!(ruler_step(4.0), (20.0, 4));
        assert_eq!(ruler_step(10.0), (5.0, 5));
        assert_eq!(ruler_step(64.0), (1.0, 5));
        for zoom in [0.02, 0.07, 0.33, 1.0, 2.5, 17.0, 64.0] {
            assert!(ruler_step(zoom).0 * zoom >= MIN_MAJOR_GAP - 1e-9);
        }
    }

    #[test]
    fn ticks_cover_the_range() {
        let t: Vec<f64> = ticks(-25.0, 110.0, 50.0).collect();
        assert_eq!(t, [0.0, 50.0, 100.0]);
        assert_eq!(ticks(1.0, 2.0, 5.0).count(), 0);
    }

    #[test]
    fn labels_have_no_needless_decimals() {
        assert_eq!(label(100.0, 50.0), "100");
        assert_eq!(label(-0.0, 50.0), "0");
        assert_eq!(label(-150.0, 50.0), "-150");
        assert_eq!(label(2.5, 0.5), "2.5");
    }
}
