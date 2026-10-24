//! Editing overlays drawn on top of the rendered page: selection, handles, guides, the
//! selection rectangle and path points. They are drawn by egui, so they never need a new
//! Vello render.

use std::sync::Arc;

use eframe::egui::text::{LayoutJob, TextFormat, TextWrapping};
use eframe::egui::{self, Color32, Galley, Pos2, Stroke, StrokeKind};
use libforma::NodeId;
use libforma::geom::{Point, Rect};

use super::Mapping;
use crate::editor::{Editor, Handle};
use crate::theme::Theme;
use crate::theme::egui::c;

/// Color of snapping guides.
const GUIDE: Color32 = Color32::from_rgb(0xff, 0x3d, 0x7f);
const HANDLE_SIZE: f32 = 7.0;

/// Gap between a frame and its name label, in screen points.
const LABEL_GAP: f32 = 4.0;
/// Extra area around a label that still counts as a click on it.
const LABEL_PAD: f32 = 2.0;

/// The name shown above a top-level frame.
pub struct FrameLabel {
    pub id: NodeId,
    galley: Arc<Galley>,
    /// Top-left corner of the text on screen.
    pos: Pos2,
    /// Area that reacts to the pointer, on screen.
    pub rect: egui::Rect,
}

/// Lays out the names of the top-level frames, cut with "…" to the width of their frame.
pub fn frame_labels(painter: &egui::Painter, editor: &mut Editor, map: Mapping) -> Vec<FrameLabel> {
    editor
        .frame_labels()
        .into_iter()
        .map(|(id, name, bounds)| {
            let frame = screen_rect(map, bounds);
            let mut job = LayoutJob::single_section(
                name,
                TextFormat::simple(egui::FontId::proportional(11.0), Color32::PLACEHOLDER),
            );
            job.wrap = TextWrapping::truncate_at_width(frame.width().max(12.0));
            let galley = painter.layout_job(job);
            let pos = frame.left_top() - egui::vec2(0.0, LABEL_GAP + galley.size().y);
            let rect = egui::Rect::from_min_size(pos, galley.size()).expand(LABEL_PAD);
            FrameLabel {
                id,
                galley,
                pos,
                rect,
            }
        })
        .collect()
}

pub fn draw(
    painter: &egui::Painter,
    editor: &mut Editor,
    map: Mapping,
    labels: &[FrameLabel],
    theme: &Theme,
) {
    let accent = c(theme.accent);
    let thin = Stroke::new(1.0, accent);

    for label in labels {
        let color = if editor.is_selected(label.id) {
            theme.accent
        } else if editor.interaction.hover == Some(label.id) {
            theme.text
        } else {
            theme.text_muted
        };
        painter.galley(label.pos, label.galley.clone(), c(color));
    }

    if let Some(hover) = editor.interaction.hover
        && !editor.is_selected(hover)
        && let Some((bounds, transform)) = editor.node_outline(hover)
    {
        outline(painter, map, bounds, transform, Stroke::new(1.5, accent));
    }

    let editing_points = editor.interaction.editing_points();
    for id in editor.selection.clone() {
        if let Some((bounds, transform)) = editor.node_outline(id) {
            outline(painter, map, bounds, transform, thin);
        }
    }

    if editing_points.is_none()
        && editor.interaction.editing_text().is_none()
        && !editor.interaction.is_drawing_path()
        && let Some(bounds) = editor.selection_bounds()
    {
        let r = screen_rect(map, bounds);
        if editor.selection.len() > 1 {
            painter.rect_stroke(r, 0.0, thin, StrokeKind::Middle);
        }
        for handle in Handle::ALL {
            let p = map.to_screen(handle.position(bounds));
            let square = egui::Rect::from_center_size(p, egui::vec2(HANDLE_SIZE, HANDLE_SIZE));
            painter.rect_filled(square, 1.0, Color32::WHITE);
            painter.rect_stroke(square, 1.0, thin, StrokeKind::Middle);
        }
        // Size label under the selection.
        let label = format!("{} × {}", fmt(bounds.width()), fmt(bounds.height()));
        let galley =
            painter.layout_no_wrap(label, egui::FontId::proportional(11.0), c(theme.on_accent));
        let pill = egui::Rect::from_center_size(
            egui::pos2(r.center().x, r.bottom() + 16.0),
            galley.size() + egui::vec2(10.0, 4.0),
        );
        painter.rect_filled(pill, 4.0, accent);
        painter.galley(pill.min + egui::vec2(5.0, 2.0), galley, Color32::WHITE);
    }

    if let Some(rect) = editor.interaction.marquee {
        let r = screen_rect(map, rect);
        painter.rect_filled(r, 0.0, accent.gamma_multiply(0.12));
        painter.rect_stroke(r, 0.0, thin, StrokeKind::Middle);
    }

    for guide in &editor.interaction.guides {
        let (a, b) = if guide.vertical {
            (
                Point::new(guide.position, guide.from),
                Point::new(guide.position, guide.to),
            )
        } else {
            (
                Point::new(guide.from, guide.position),
                Point::new(guide.to, guide.position),
            )
        };
        painter.line_segment(
            [map.to_screen(a), map.to_screen(b)],
            Stroke::new(1.0, GUIDE),
        );
    }

    // Path being drawn with the pen.
    for (anchor, handle) in editor.pen_anchors() {
        let p = map.to_screen(anchor);
        if let Some(h) = handle {
            let h = map.to_screen(h);
            let mirrored = p - (h - p);
            painter.line_segment([mirrored, h], thin);
            painter.circle_filled(h, 3.0, accent);
            painter.circle_filled(mirrored, 3.0, accent);
        }
        anchor_square(painter, p, accent);
    }

    // Points of the path being edited.
    for point in editor.edit_points() {
        let p = map.to_screen(point.pos);
        if point.anchor {
            anchor_square(painter, p, accent);
        } else {
            if let Some(owner) = point.owner {
                painter.line_segment([map.to_screen(owner), p], thin);
            }
            painter.circle_filled(p, 3.5, Color32::WHITE);
            painter.circle_stroke(p, 3.5, thin);
        }
    }
}

fn anchor_square(painter: &egui::Painter, p: Pos2, accent: Color32) {
    let square = egui::Rect::from_center_size(p, egui::vec2(6.0, 6.0));
    painter.rect_filled(square, 0.0, Color32::WHITE);
    painter.rect_stroke(square, 0.0, Stroke::new(1.0, accent), StrokeKind::Middle);
}

fn outline(
    painter: &egui::Painter,
    map: Mapping,
    bounds: Rect,
    transform: libforma::geom::Affine,
    stroke: Stroke,
) {
    let corners = [
        Point::new(bounds.x0, bounds.y0),
        Point::new(bounds.x1, bounds.y0),
        Point::new(bounds.x1, bounds.y1),
        Point::new(bounds.x0, bounds.y1),
    ];
    let points: Vec<Pos2> = corners
        .iter()
        .map(|p| map.to_screen(transform * *p))
        .collect();
    painter.add(egui::Shape::closed_line(points, stroke));
}

fn screen_rect(map: Mapping, r: Rect) -> egui::Rect {
    egui::Rect::from_two_pos(
        map.to_screen(Point::new(r.x0, r.y0)),
        map.to_screen(Point::new(r.x1, r.y1)),
    )
}

fn fmt(v: f64) -> String {
    let rounded = (v * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    }
}
