//! The properties panel on the right.

use std::collections::HashMap;

use eframe::egui::{self, Response, RichText, Ui};
use libforma::NodeId;
use libforma::geom::{Point, Rect};
use libforma::model::{Geometry, Node, NodeKind, TextAlign, TextSizing, TextVerticalAlign};
use libforma::paint::{BlendMode, Color, GradientStop, LineCap, LineJoin, Paint, Stroke};

use crate::app::Action;
use crate::editor::{Align, Axis, Editor, default_name};
use crate::i18n::{tr, tr_noop, trf};
use crate::theme::Theme;
use crate::theme::egui::c;
use crate::theme::icons::{Icon, icon};
use crate::ui::font_picker::{self, FontPickerState};
use crate::ui::layers::kind_icon;
use crate::ui::widgets::{
    FIELD_HEIGHT, accent_button, color_field, field_row, icon_strip, int_field, number_field,
    section_header, select, separator, small_icon_button, strong,
};

/// Frame sizes offered by the frame tool, the frame section and the new-design dialog.
/// Names are in English; show them through `tr`.
pub const PAGE_PRESETS: [(&str, f64, f64); 6] = [
    (tr_noop("Square post"), 1080.0, 1080.0),
    (tr_noop("Story"), 1080.0, 1920.0),
    (tr_noop("Slide 16:9"), 1920.0, 1080.0),
    (tr_noop("A4"), 794.0, 1123.0),
    (tr_noop("A3 poster"), 1123.0, 1587.0),
    (tr_noop("Video thumbnail"), 1280.0, 720.0),
];

/// Keeps one undo step per continuous edit of a field (dragging a number, typing).
#[derive(Default)]
pub struct InspectorState {
    keys: HashMap<egui::Id, u64>,
    pub fonts: FontPickerState,
}

impl InspectorState {
    /// Merge key for an edit made through `response`, or `None` for a one-off change.
    fn key(&mut self, editor: &mut Editor, response: &Response) -> Option<u64> {
        if response.dragged() || response.has_focus() {
            Some(
                *self
                    .keys
                    .entry(response.id)
                    .or_insert_with(|| editor.new_merge_key()),
            )
        } else {
            self.keys.remove(&response.id);
            None
        }
    }
}

/// Collects the changes made by the widgets of one node.
struct Tracker {
    changed: bool,
    key: Option<u64>,
}

impl Tracker {
    fn new() -> Self {
        Self {
            changed: false,
            key: None,
        }
    }

    fn note(
        &mut self,
        state: &mut InspectorState,
        editor: &mut Editor,
        response: &Response,
        changed: bool,
    ) {
        if changed {
            self.changed = true;
            self.key = self.key.or(state.key(editor, response));
        }
    }
}

pub fn show(
    ui: &mut Ui,
    editor: &mut Editor,
    theme: &Theme,
    state: &mut InspectorState,
    actions: &mut Vec<Action>,
) {
    top_bar(ui, editor, theme, actions);
    separator(ui, theme);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| match editor.selection.len() {
            0 => page_section(ui, editor, theme, state, actions),
            1 => {
                let id = editor.selection[0];
                node_sections(ui, editor, theme, state, id, actions);
            }
            n => {
                ui.label(strong(trf("{n} layers selected", &[("n", &n.to_string())])));
                ui.add_space(6.0);
                align_section(ui, editor, theme);
                separator(ui, theme);
                geometry_section(ui, editor, theme, state, None);
                separator(ui, theme);
                field_row(ui, |[a, b]| {
                    if a.add(wide_button(icon(Icon::Group), tr("Group"))).clicked() {
                        editor.group_selection();
                    }
                    if b.add(wide_button(icon(Icon::Trash2), tr("Delete")))
                        .clicked()
                    {
                        editor.delete_selection();
                    }
                });
            }
        });
}

/// A button spanning its column.
fn wide_button<'a>(i: RichText, label: &'a str) -> egui::Button<'a> {
    egui::Button::new((i, label)).min_size(egui::vec2(0.0, FIELD_HEIGHT))
}

fn top_bar(ui: &mut Ui, editor: &mut Editor, theme: &Theme, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        ui.set_min_height(30.0);
        if accent_button(ui, theme, (icon(Icon::Download), tr("Export"))).clicked() {
            actions.push(Action::OpenExport);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let zoom = format!("{:.0} %", editor.view.zoom * 100.0);
            let zoom_button =
                ui.add(egui::Button::new((zoom, icon(Icon::ChevronDown))).frame(false));
            egui::Popup::menu(&zoom_button).show(|ui| {
                let center = libforma::geom::Vec2::new(400.0, 300.0);
                if ui.button(tr("Zoom in   Ctrl +")).clicked() {
                    editor.view.zoom_around(1.25, center);
                }
                if ui.button(tr("Zoom out   Ctrl −")).clicked() {
                    editor.view.zoom_around(0.8, center);
                }
                if ui.button(tr("Zoom to fit   Shift 0")).clicked() {
                    editor.view.fit_requested = true;
                }
                if ui.button(tr("Show or hide rulers   Shift R")).clicked() {
                    actions.push(Action::ToggleRulers);
                }
                ui.separator();
                for z in [0.5, 1.0, 2.0] {
                    if ui.button(format!("{:.0} %", z * 100.0)).clicked() {
                        let factor = z / editor.view.zoom;
                        editor.view.zoom_around(factor, center);
                    }
                }
            });
        });
    });
}

/// Shown with nothing selected: the page's name, its frames and export.
fn page_section(
    ui: &mut Ui,
    editor: &mut Editor,
    theme: &Theme,
    state: &mut InspectorState,
    actions: &mut Vec<Action>,
) {
    let page = editor.current_page().clone();
    section_header(ui, tr("Page"), |_| {});

    let mut name = page.name.clone();
    let r = ui.add(
        egui::TextEdit::singleline(&mut name)
            .hint_text(tr("Page name"))
            .desired_width(f32::INFINITY),
    );
    if r.changed() {
        let key = state.key(editor, &r);
        editor.update_page(page.id, key, |p| p.name = name);
    }
    ui.add_space(4.0);
    let frames = libforma::query::frames(&page).len();
    let summary = match frames {
        0 => tr("No frames").to_owned(),
        1 => tr("1 frame").to_owned(),
        n => trf("{n} frames", &[("n", &n.to_string())]),
    };
    ui.label(RichText::new(summary).color(c(theme.text_faint)));

    separator(ui, theme);
    section_header(ui, tr("Export"), |ui| {
        if small_icon_button(ui, theme, Icon::Plus, tr("Export…")).clicked() {
            actions.push(Action::OpenExport);
        }
    });
    let hint = if frames > 0 {
        tr("Each frame becomes a file, or a page of the PDF.")
    } else {
        tr("Without frames, the whole page becomes one image.")
    };
    ui.label(RichText::new(hint).color(c(theme.text_muted)).size(11.5));
}

/// Size presets, fill and clipping of a frame.
fn frame_section(
    ui: &mut Ui,
    editor: &mut Editor,
    theme: &Theme,
    state: &mut InspectorState,
    node: &mut Node,
    t: &mut Tracker,
) {
    let NodeKind::Frame(frame) = &mut node.kind else {
        return;
    };
    section_header(ui, tr("Frame"), |_| {});
    let mut presets = None;
    let mut flip = false;
    field_row(ui, |[a, b]| {
        presets = Some(a.add(wide_button(icon(Icon::LayoutTemplate), tr("Sizes"))));
        flip = b
            .add(wide_button(icon(Icon::RectangleHorizontal), tr("Rotate")))
            .on_hover_text(tr("Swaps the width and the height"))
            .clicked();
    });
    let presets = presets.expect("row shown");
    egui::Popup::menu(&presets).show(|ui| {
        for (name, w, h) in PAGE_PRESETS {
            let size = libforma::geom::Size::new(w, h);
            let label = format!("{}  ·  {w:.0} × {h:.0}", tr(name));
            if ui
                .add(egui::Button::selectable(frame.size == size, label))
                .clicked()
            {
                frame.size = size;
                t.changed = true;
                ui.close();
            }
        }
    });
    if flip {
        frame.size = libforma::geom::Size::new(frame.size.height, frame.size.width);
        t.changed = true;
    }
    ui.add_space(4.0);
    if ui
        .checkbox(&mut frame.clip, tr("Clip content"))
        .on_hover_text(tr("Hides what goes past the edges of the frame"))
        .changed()
    {
        t.changed = true;
    }

    separator(ui, theme);
    section_header(ui, tr("Fill"), |ui| {
        let toggle = if frame.fill.is_some() {
            Icon::Minus
        } else {
            Icon::Plus
        };
        if small_icon_button(ui, theme, toggle, tr("Transparent or filled background")).clicked() {
            frame.fill = match frame.fill {
                Some(_) => None,
                None => Some(Paint::solid(Color::WHITE)),
            };
            t.changed = true;
        }
    });
    let bounds = frame.size.to_rect();
    match &mut frame.fill {
        Some(fill) => paint_editor(ui, editor, theme, state, "frame-fill", fill, bounds, t),
        None => {
            ui.label(RichText::new(tr("Transparent")).color(c(theme.text_faint)));
        }
    }
}

fn node_sections(
    ui: &mut Ui,
    editor: &mut Editor,
    theme: &Theme,
    state: &mut InspectorState,
    id: NodeId,
    actions: &mut Vec<Action>,
) {
    let Some(original) = editor.doc.node(id).cloned() else {
        return;
    };
    ui.horizontal(|ui| {
        ui.label(icon(kind_icon(&original)).color(c(theme.text_muted)));
        ui.label(strong(default_name(&original)));
    });
    ui.add_space(6.0);
    align_section(ui, editor, theme);
    separator(ui, theme);
    geometry_section(ui, editor, theme, state, Some(id));
    separator(ui, theme);

    let mut node = original.clone();
    let mut tracker = Tracker::new();
    layer_section(ui, editor, theme, state, &mut node, &mut tracker);
    match &mut node.kind {
        NodeKind::Shape(_) => {
            separator(ui, theme);
            shape_section(ui, editor, theme, state, &mut node, &mut tracker);
            separator(ui, theme);
            fill_section(ui, editor, theme, state, &mut node, &mut tracker);
            separator(ui, theme);
            stroke_section(ui, editor, theme, state, &mut node, &mut tracker);
        }
        NodeKind::Text(_) => {
            separator(ui, theme);
            text_section(ui, editor, theme, state, &mut node, &mut tracker, actions);
        }
        NodeKind::Frame(_) => {
            separator(ui, theme);
            frame_section(ui, editor, theme, state, &mut node, &mut tracker);
        }
        NodeKind::Image(_) | NodeKind::Group { .. } => {}
    }
    if tracker.changed && node != original {
        match tracker.key {
            Some(key) => editor.update_node_merged(id, key, |n| *n = node),
            None => editor.update_node(id, |n| *n = node),
        };
    }

    if original.is_group() {
        separator(ui, theme);
        if ui
            .add(
                wide_button(icon(Icon::Ungroup), tr("Ungroup"))
                    .min_size(egui::vec2(ui.available_width(), FIELD_HEIGHT)),
            )
            .clicked()
        {
            editor.ungroup_selection();
        }
    }
    if let NodeKind::Shape(shape) = &original.kind {
        separator(ui, theme);
        let label = if matches!(shape.geometry, Geometry::Path { .. }) {
            tr("Edit points")
        } else {
            tr("Convert to vector and edit points")
        };
        if ui
            .add(
                wide_button(icon(Icon::Spline), label)
                    .min_size(egui::vec2(ui.available_width(), FIELD_HEIGHT)),
            )
            .clicked()
        {
            editor.start_point_edit(id);
        }
    }
}

fn align_section(ui: &mut Ui, editor: &mut Editor, theme: &Theme) {
    let aligns = [
        (Align::Left, Icon::AlignStartVertical, tr("Align left")),
        (
            Align::CenterX,
            Icon::AlignCenterVertical,
            tr("Align horizontal centers"),
        ),
        (Align::Right, Icon::AlignEndVertical, tr("Align right")),
        (Align::Top, Icon::AlignStartHorizontal, tr("Align top")),
        (
            Align::CenterY,
            Icon::AlignCenterHorizontal,
            tr("Align vertical centers"),
        ),
        (Align::Bottom, Icon::AlignEndHorizontal, tr("Align bottom")),
    ];
    let mut items: Vec<(Icon, &str, bool)> =
        aligns.iter().map(|(_, i, tip)| (*i, *tip, false)).collect();
    let distribute = editor.selection.len() >= 3;
    if distribute {
        items.push((
            Icon::AlignHorizontalSpaceBetween,
            tr("Distribute horizontally (Alt+Shift+H)"),
            false,
        ));
        items.push((
            Icon::AlignVerticalSpaceBetween,
            tr("Distribute vertically (Alt+Shift+V)"),
            false,
        ));
    }
    match icon_strip(ui, theme, &items) {
        Some(i) if i < aligns.len() => editor.align_selection(aligns[i].0),
        Some(6) => editor.distribute_selection(Axis::Horizontal),
        Some(_) => editor.distribute_selection(Axis::Vertical),
        None => {}
    }
}

/// Position, size and rotation in page space.
fn geometry_section(
    ui: &mut Ui,
    editor: &mut Editor,
    theme: &Theme,
    state: &mut InspectorState,
    single: Option<NodeId>,
) {
    let Some(bounds) = editor.selection_bounds() else {
        return;
    };
    let (mut x, mut y, mut w, mut h) = (bounds.x0, bounds.y0, bounds.width(), bounds.height());
    let mut tracker = Tracker::new();
    field_row(ui, |[a, b]| {
        let r = number_field(a, theme, "X", &mut x, 1.0, "");
        tracker.note(state, editor, &r, r.changed());
        let r = number_field(b, theme, "Y", &mut y, 1.0, "");
        tracker.note(state, editor, &r, r.changed());
    });
    field_row(ui, |[a, b]| {
        let r = number_field(a, theme, tr("W"), &mut w, 1.0, "");
        tracker.note(state, editor, &r, r.changed());
        let r = number_field(b, theme, tr("H"), &mut h, 1.0, "");
        tracker.note(state, editor, &r, r.changed());
    });
    if tracker.changed {
        let target = Rect::from_origin_size(Point::new(x, y), (w.max(0.5), h.max(0.5)));
        editor.set_selection_bounds(target, tracker.key);
    }
    if let Some(id) = single {
        let mut degrees = editor.rotation(id);
        field_row(ui, |[a, _]| {
            let r = number_field(a, theme, "↻", &mut degrees, 1.0, "°");
            if r.changed() {
                let key = state.key(editor, &r);
                editor.set_rotation(id, degrees, key);
            }
        });
    }
}

fn layer_section(
    ui: &mut Ui,
    editor: &mut Editor,
    theme: &Theme,
    state: &mut InspectorState,
    node: &mut Node,
    t: &mut Tracker,
) {
    section_header(ui, tr("Layer"), |_| {});
    field_row(ui, |[a, b]| {
        let mut opacity = f64::from(node.opacity) * 100.0;
        let r = number_field(a, theme, "", &mut opacity, 0.5, " %");
        if r.changed() {
            node.opacity = (opacity.clamp(0.0, 100.0) / 100.0) as f32;
        }
        let changed = r.changed();
        t.note(state, editor, &r.on_hover_text(tr("Opacity")), changed);
        let options: Vec<(BlendMode, &str)> =
            BLENDS.iter().map(|b| (*b, blend_label(*b))).collect();
        if select(b, "blend", &mut node.blend, &options) {
            t.changed = true;
        }
    });
}

const BLENDS: [BlendMode; 16] = [
    BlendMode::Normal,
    BlendMode::Multiply,
    BlendMode::Screen,
    BlendMode::Overlay,
    BlendMode::Darken,
    BlendMode::Lighten,
    BlendMode::ColorDodge,
    BlendMode::ColorBurn,
    BlendMode::HardLight,
    BlendMode::SoftLight,
    BlendMode::Difference,
    BlendMode::Exclusion,
    BlendMode::Hue,
    BlendMode::Saturation,
    BlendMode::Color,
    BlendMode::Luminosity,
];

fn blend_label(blend: BlendMode) -> &'static str {
    match blend {
        BlendMode::Normal => tr("Normal"),
        BlendMode::Multiply => tr("Multiply"),
        BlendMode::Screen => tr("Screen"),
        BlendMode::Overlay => tr("Overlay"),
        BlendMode::Darken => tr("Darken"),
        BlendMode::Lighten => tr("Lighten"),
        BlendMode::ColorDodge => tr("Color dodge"),
        BlendMode::ColorBurn => tr("Color burn"),
        BlendMode::HardLight => tr("Hard light"),
        BlendMode::SoftLight => tr("Soft light"),
        BlendMode::Difference => tr("Difference"),
        BlendMode::Exclusion => tr("Exclusion"),
        BlendMode::Hue => tr("Hue"),
        BlendMode::Saturation => tr("Saturation"),
        BlendMode::Color => tr("Color"),
        BlendMode::Luminosity => tr("Luminosity"),
    }
}

fn shape_section(
    ui: &mut Ui,
    editor: &mut Editor,
    theme: &Theme,
    state: &mut InspectorState,
    node: &mut Node,
    t: &mut Tracker,
) {
    let NodeKind::Shape(shape) = &mut node.kind else {
        return;
    };
    match &mut shape.geometry {
        Geometry::Rect {
            corner_radius,
            size,
        } => {
            section_header(ui, tr("Rectangle"), |_| {});
            let max = size.width.min(size.height) / 2.0;
            field_row(ui, |[a, _]| {
                let r = number_field(a, theme, tr("Radius"), corner_radius, 0.5, "");
                *corner_radius = corner_radius.clamp(0.0, max.max(0.0));
                t.note(state, editor, &r, r.changed());
            });
        }
        Geometry::Polygon { sides, .. } => {
            section_header(ui, tr("Polygon"), |_| {});
            field_row(ui, |[a, _]| {
                let r = int_field(a, theme, tr("Sides"), sides, 3..=64);
                t.note(state, editor, &r, r.changed());
            });
        }
        Geometry::Star {
            points,
            inner_ratio,
            ..
        } => {
            section_header(ui, tr("Star"), |_| {});
            field_row(ui, |[a, b]| {
                let r = int_field(a, theme, tr("Points"), points, 3..=64);
                t.note(state, editor, &r, r.changed());
                let mut ratio = *inner_ratio * 100.0;
                let r = number_field(b, theme, tr("Inner"), &mut ratio, 0.5, " %");
                *inner_ratio = (ratio / 100.0).clamp(0.01, 1.0);
                let changed = r.changed();
                t.note(state, editor, &r.on_hover_text(tr("Inner radius")), changed);
            });
        }
        Geometry::Ellipse { .. } | Geometry::Line { .. } | Geometry::Path { .. } => {
            section_header(ui, tr("Shape"), |_| {});
            ui.label(
                RichText::new(match shape.geometry {
                    Geometry::Line { .. } => tr("Line"),
                    Geometry::Path { .. } => tr("Free vector"),
                    _ => tr("Ellipse"),
                })
                .color(c(theme.text_muted)),
            );
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum PaintKind {
    Solid,
    Linear,
    Radial,
}

fn paint_kind(paint: &Paint) -> PaintKind {
    match paint {
        Paint::Solid { .. } => PaintKind::Solid,
        Paint::LinearGradient { .. } => PaintKind::Linear,
        Paint::RadialGradient { .. } => PaintKind::Radial,
    }
}

/// Edits a paint (solid or gradient). `bounds` is the node's local box, for new gradients.
#[allow(clippy::too_many_arguments)]
fn paint_editor(
    ui: &mut Ui,
    editor: &mut Editor,
    theme: &Theme,
    state: &mut InspectorState,
    id_salt: &str,
    paint: &mut Paint,
    bounds: Rect,
    t: &mut Tracker,
) {
    let kind = paint_kind(paint);
    let mut new_kind = kind;
    field_row(ui, |[a]| {
        select(
            a,
            (id_salt, "kind"),
            &mut new_kind,
            &[
                (PaintKind::Solid, tr("Solid")),
                (PaintKind::Linear, tr("Linear gradient")),
                (PaintKind::Radial, tr("Radial gradient")),
            ],
        );
    });
    if new_kind != kind {
        let first = match paint {
            Paint::Solid { color } => *color,
            Paint::LinearGradient { stops, .. } | Paint::RadialGradient { stops, .. } => {
                stops.first().map_or(Color::BLACK, |s| s.color)
            }
        };
        let stops = vec![
            GradientStop::new(0.0, first),
            GradientStop::new(1.0, first.with_alpha(0)),
        ];
        *paint = match new_kind {
            PaintKind::Solid => Paint::solid(first),
            PaintKind::Linear => Paint::LinearGradient {
                start: Point::new(bounds.x0, bounds.center().y),
                end: Point::new(bounds.x1, bounds.center().y),
                stops,
            },
            PaintKind::Radial => Paint::RadialGradient {
                center: bounds.center(),
                radius: bounds.width().max(bounds.height()) / 2.0,
                stops,
            },
        };
        t.changed = true;
    }
    match paint {
        Paint::Solid { color } => {
            field_row(ui, |[a]| {
                let (r, changed) = color_field(a, theme, (id_salt, "solid"), color);
                t.note(state, editor, &r, changed);
            });
        }
        Paint::LinearGradient { stops, .. } | Paint::RadialGradient { stops, .. } => {
            for (i, stop) in stops.iter_mut().enumerate() {
                // Stop position, then its color: a third and two thirds of the width.
                ui.horizontal(|ui| {
                    let third = (ui.available_width() - 8.0) / 3.0;
                    ui.allocate_ui(egui::vec2(third, FIELD_HEIGHT), |ui| {
                        let mut pos = f64::from(stop.offset) * 100.0;
                        let r = number_field(ui, theme, "", &mut pos, 0.5, " %");
                        if r.changed() {
                            stop.offset = (pos.clamp(0.0, 100.0) / 100.0) as f32;
                        }
                        t.note(state, editor, &r, r.changed());
                    });
                    let (r, changed) =
                        color_field(ui, theme, (id_salt, "stop", i), &mut stop.color);
                    t.note(state, editor, &r, changed);
                });
                ui.add_space(2.0);
            }
        }
    }
}

fn fill_section(
    ui: &mut Ui,
    editor: &mut Editor,
    theme: &Theme,
    state: &mut InspectorState,
    node: &mut Node,
    t: &mut Tracker,
) {
    let NodeKind::Shape(shape) = &mut node.kind else {
        return;
    };
    let bounds = shape.geometry.bounds();
    let open = shape.geometry.is_open();
    section_header(ui, tr("Fill"), |ui| {
        if open {
            return;
        }
        let toggle = if shape.fill.is_some() {
            Icon::Minus
        } else {
            Icon::Plus
        };
        if small_icon_button(ui, theme, toggle, tr("Add or remove fill")).clicked() {
            shape.fill = match shape.fill {
                Some(_) => None,
                None => Some(Paint::solid(crate::editor::interaction_default_fill())),
            };
            t.changed = true;
        }
    });
    if open {
        ui.label(RichText::new(tr("Lines have no fill")).color(c(theme.text_faint)));
    } else if let Some(fill) = &mut shape.fill {
        paint_editor(ui, editor, theme, state, "fill", fill, bounds, t);
    }
}

fn stroke_section(
    ui: &mut Ui,
    editor: &mut Editor,
    theme: &Theme,
    state: &mut InspectorState,
    node: &mut Node,
    t: &mut Tracker,
) {
    let NodeKind::Shape(shape) = &mut node.kind else {
        return;
    };
    let bounds = shape.geometry.bounds();
    section_header(ui, tr("Stroke"), |ui| {
        let toggle = if shape.stroke.is_some() {
            Icon::Minus
        } else {
            Icon::Plus
        };
        if small_icon_button(ui, theme, toggle, tr("Add or remove stroke")).clicked() {
            shape.stroke = match shape.stroke {
                Some(_) => None,
                None => Some(Stroke::new(Color::BLACK, 1.0)),
            };
            t.changed = true;
        }
    });
    let Some(stroke) = &mut shape.stroke else {
        return;
    };
    paint_editor(
        ui,
        editor,
        theme,
        state,
        "stroke",
        &mut stroke.paint,
        bounds,
        t,
    );
    field_row(ui, |[a, b]| {
        let r = number_field(a, theme, tr("Width"), &mut stroke.width, 0.1, "");
        stroke.width = stroke.width.max(0.0);
        t.note(state, editor, &r, r.changed());
        let mut dashed = !stroke.dash.is_empty();
        if b.checkbox(&mut dashed, tr("Dashed")).changed() {
            stroke.dash = if dashed {
                vec![stroke.width * 4.0, stroke.width * 2.0]
            } else {
                vec![]
            };
            t.changed = true;
        }
    });
    field_row(ui, |[a, b]| {
        let joins = [
            (LineJoin::Miter, tr("Sharp corners")),
            (LineJoin::Round, tr("Round corners")),
            (LineJoin::Bevel, tr("Beveled")),
        ];
        let caps = [
            (LineCap::Butt, tr("Flat ends")),
            (LineCap::Round, tr("Round ends")),
            (LineCap::Square, tr("Square ends")),
        ];
        let joined = select(a, "join", &mut stroke.join, &joins);
        let capped = select(b, "cap", &mut stroke.cap, &caps);
        if joined || capped {
            t.changed = true;
        }
    });
}

fn text_section(
    ui: &mut Ui,
    editor: &mut Editor,
    theme: &Theme,
    state: &mut InspectorState,
    node: &mut Node,
    t: &mut Tracker,
    actions: &mut Vec<Action>,
) {
    // The box as it is drawn now, for switching between sizing modes.
    let current_box = editor
        .node_outline(node.id)
        .map(|(bounds, _)| bounds.size());
    let NodeKind::Text(text) = &mut node.kind else {
        return;
    };
    section_header(ui, tr("Text"), |_| {});
    let r = ui.add(
        egui::TextEdit::multiline(&mut text.content)
            .desired_rows(2)
            .desired_width(f32::INFINITY),
    );
    t.note(state, editor, &r, r.changed());
    ui.add_space(4.0);

    if let Some(family) = font_picker::show(
        ui,
        theme,
        editor,
        &mut state.fonts,
        &text.style.font_family,
        actions,
    ) {
        // Keep the weight if the new family has it, else take the closest one.
        let weights = editor.font_weights(&family);
        if let Some(w) = weights
            .iter()
            .min_by_key(|w| (i32::from(**w) - i32::from(text.style.font_weight)).abs())
        {
            text.style.font_weight = *w;
        }
        text.style.font_family = family;
        t.changed = true;
    }
    ui.add_space(4.0);

    field_row(ui, |[a, b]| {
        let mut weights = editor.font_weights(&text.style.font_family);
        if weights.is_empty() {
            weights = vec![100, 200, 300, 400, 500, 600, 700, 800, 900];
        }
        if !weights.contains(&text.style.font_weight) {
            weights.push(text.style.font_weight);
            weights.sort_unstable();
        }
        let options: Vec<(u16, &str)> = weights.iter().map(|w| (*w, weight_label(*w))).collect();
        if select(a, "weight", &mut text.style.font_weight, &options) {
            t.changed = true;
        }
        let r = number_field(b, theme, "", &mut text.style.font_size, 0.2, " px");
        text.style.font_size = text.style.font_size.clamp(1.0, 2000.0);
        let changed = r.changed();
        t.note(state, editor, &r.on_hover_text(tr("Font size")), changed);
    });
    field_row(ui, |[a, b]| {
        let mut line = f64::from(text.style.line_height) * 100.0;
        let r = number_field(a, theme, "↕", &mut line, 1.0, " %");
        text.style.line_height = (line / 100.0).clamp(0.5, 5.0) as f32;
        let changed = r.changed();
        t.note(state, editor, &r.on_hover_text(tr("Line height")), changed);
        let mut spacing = f64::from(text.style.letter_spacing);
        let r = number_field(b, theme, "↔", &mut spacing, 0.05, " px");
        text.style.letter_spacing = spacing as f32;
        let changed = r.changed();
        t.note(
            state,
            editor,
            &r.on_hover_text(tr("Letter spacing")),
            changed,
        );
    });

    ui.add_space(4.0);
    ui.label(
        RichText::new(tr("Alignment"))
            .color(c(theme.text_muted))
            .size(11.0),
    );
    let aligns = [
        (TextAlign::Start, Icon::TextAlignStart, tr("Align left")),
        (TextAlign::Center, Icon::TextAlignCenter, tr("Center")),
        (TextAlign::End, Icon::TextAlignEnd, tr("Align right")),
        (TextAlign::Justify, Icon::TextAlignJustify, tr("Justify")),
    ];
    let mut items: Vec<(Icon, &str, bool)> = aligns
        .iter()
        .map(|(a, i, tip)| (*i, *tip, text.align == *a))
        .collect();
    items.push((Icon::Italic, tr("Italic"), text.style.italic));
    match icon_strip(ui, theme, &items) {
        Some(4) => {
            text.style.italic = !text.style.italic;
            t.changed = true;
        }
        Some(i) => {
            text.align = aligns[i].0;
            t.changed = true;
        }
        None => {}
    }

    // Box sizing and vertical position, like other design tools.
    ui.add_space(4.0);
    ui.label(
        RichText::new(tr("Text box"))
            .color(c(theme.text_muted))
            .size(11.0),
    );
    let sizing = text.sizing();
    let verticals = [
        (
            TextVerticalAlign::Top,
            Icon::AlignVerticalJustifyStart,
            tr("Top"),
        ),
        (
            TextVerticalAlign::Middle,
            Icon::AlignVerticalJustifyCenter,
            tr("Middle"),
        ),
        (
            TextVerticalAlign::Bottom,
            Icon::AlignVerticalJustifyEnd,
            tr("Bottom"),
        ),
    ];
    let mut items = vec![
        (
            Icon::MoveHorizontal,
            tr("Auto width"),
            sizing == TextSizing::AutoWidth,
        ),
        (
            Icon::MoveVertical,
            tr("Auto height"),
            sizing == TextSizing::AutoHeight,
        ),
        (Icon::Scan, tr("Fixed size"), sizing == TextSizing::Fixed),
    ];
    items.extend(
        verticals
            .iter()
            .map(|(v, i, tip)| (*i, *tip, text.vertical_align == *v)),
    );
    let size = current_box.unwrap_or_default();
    match icon_strip(ui, theme, &items) {
        Some(0) => {
            text.width = None;
            text.height = None;
            t.changed = true;
        }
        Some(1) => {
            text.width = Some(size.width.max(1.0));
            text.height = None;
            t.changed = true;
        }
        Some(2) => {
            text.width = Some(size.width.max(1.0));
            text.height = Some(size.height.max(1.0));
            t.changed = true;
        }
        Some(i) => {
            text.vertical_align = verticals[i - 3].0;
            t.changed = true;
        }
        None => {}
    }

    separator(ui, theme);
    section_header(ui, tr("Fill"), |_| {});
    field_row(ui, |[a]| {
        let (r, changed) = color_field(a, theme, "text-color", &mut text.style.color);
        t.note(state, editor, &r, changed);
    });
}

fn weight_label(weight: u16) -> &'static str {
    match weight {
        0..=150 => tr("Thin"),
        151..=250 => tr("Extra light"),
        251..=350 => tr("Light"),
        351..=450 => tr("Regular"),
        451..=550 => tr("Medium"),
        551..=650 => tr("Semibold"),
        651..=750 => tr("Bold"),
        751..=850 => tr("Extra bold"),
        _ => tr("Black"),
    }
}
