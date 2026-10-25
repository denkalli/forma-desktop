//! The canvas: shows the current page, turns pointer input into editor actions and draws
//! the editing overlays.

mod gpu;
mod overlay;
mod rulers;

use eframe::egui::{self, CursorIcon, Pos2, Rect, Sense, Vec2};
use eframe::egui_wgpu::RenderState;
use libforma::geom::{self, Affine, Point};

use crate::app::Action;
use crate::editor::{CursorHint, Editor, Handle, Modifiers, PointerEvent, ToolContext};
use crate::i18n::tr;
use crate::theme::Theme;
use crate::theme::egui::c;

pub use gpu::{CanvasGpu, FrameKey};

/// Canvas state that lives across frames.
pub struct CanvasState {
    /// The current press started on the canvas.
    pressing: bool,
    /// Panning with the middle button or Space.
    panning: bool,
    /// The text field of in-place text editing should take focus.
    focus_text: bool,
    pub rect: Rect,
    /// Where the right-click menu was opened (page space) and whether it was over a layer.
    context: Option<(Point, bool)>,
    /// "Paste here" waits for the clipboard text to arrive at this page point.
    paste_target: Option<Point>,
}

impl CanvasState {
    /// The point of a pending "Paste here", once.
    pub fn take_paste_target(&mut self) -> Option<Point> {
        self.paste_target.take()
    }
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            pressing: false,
            panning: false,
            focus_text: false,
            rect: Rect::NOTHING,
            context: None,
            paste_target: None,
        }
    }
}

/// Maps between page space and screen space.
#[derive(Clone, Copy)]
pub struct Mapping {
    pub rect: Rect,
    pub view: Affine,
}

impl Mapping {
    pub fn to_screen(self, p: Point) -> Pos2 {
        let p = self.view * p;
        self.rect.min + egui::vec2(p.x as f32, p.y as f32)
    }

    pub fn to_page(self, pos: Pos2) -> Point {
        let local = pos - self.rect.min;
        self.view.inverse() * Point::new(f64::from(local.x), f64::from(local.y))
    }
}

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    editor: &mut Editor,
    gpu: Option<&mut CanvasGpu>,
    render_state: Option<&RenderState>,
    state: &mut CanvasState,
    theme: &Theme,
    show_rulers: bool,
    actions: &mut Vec<Action>,
) {
    let full = ui.available_rect_before_wrap();
    // The rulers take a band at the top and left; the canvas is the rest.
    let rect = if show_rulers {
        Rect::from_min_max(full.min + Vec2::splat(rulers::RULER_SIZE), full.max)
    } else {
        full
    };
    state.rect = rect;
    let response = ui.allocate_rect(rect, Sense::click_and_drag());
    let ctx = ui.ctx().clone();

    if editor.view.fit_requested && rect.width() > 10.0 && rect.height() > 10.0 {
        let content = editor.content_bounds();
        editor.view.fit(
            content,
            geom::Size::new(f64::from(rect.width()), f64::from(rect.height())),
        );
    }

    handle_view_input(&ctx, &response, editor, state);
    let mapping = Mapping {
        rect,
        view: editor.view.transform(),
    };
    let painter = ui.painter_at(rect);
    let labels = overlay::frame_labels(&painter, editor, mapping);
    editor.interaction.frame_labels = labels
        .iter()
        .map(|l| {
            let rect =
                geom::Rect::from_points(mapping.to_page(l.rect.min), mapping.to_page(l.rect.max));
            (l.id, rect)
        })
        .collect();
    handle_pointer(&ctx, &response, editor, state, mapping, theme.canvas_text);
    context_menu(&response, editor, state, mapping, show_rulers, actions);

    // Render the page with Vello when anything changed.
    let ppp = ctx.pixels_per_point();
    painter.rect_filled(rect, 0.0, c(theme.canvas));
    draw_grid(&painter, mapping, editor.view.zoom, theme);
    if let (Some(gpu), Some(render_state)) = (gpu, render_state) {
        let size = [
            (rect.width() * ppp).round().max(1.0) as u32,
            (rect.height() * ppp).round().max(1.0) as u32,
        ];
        let key = FrameKey {
            revision: editor.revision,
            size,
            transform: (Affine::scale(f64::from(ppp)) * mapping.view).as_coeffs(),
        };
        let texture = if gpu.is_current(&key) {
            gpu.texture_id()
        } else {
            let list = editor.display_list();
            match gpu.render(render_state, key, &list, &editor.doc.assets) {
                Ok(id) => Some(id),
                Err(err) => {
                    editor.interaction.report(crate::i18n::trf(
                        "Could not draw the page: {error}",
                        &[("error", &err.to_string())],
                    ));
                    None
                }
            }
        };
        if let Some(texture) = texture {
            painter.image(
                texture,
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
    }

    overlay::draw(&painter, editor, mapping, &labels, theme);
    if show_rulers {
        let selection = editor.selection_bounds();
        rulers::draw(
            &ui.painter_at(full),
            rect,
            mapping,
            editor.view.zoom,
            selection,
            theme,
        );
    }
    text_edit_overlay(ui, editor, state, mapping, theme);
}

/// The right-click menu. Over a layer it selects the layer and offers layer commands; over
/// empty canvas it offers paste, select all, zoom and the rulers.
fn context_menu(
    response: &egui::Response,
    editor: &mut Editor,
    state: &mut CanvasState,
    mapping: Mapping,
    show_rulers: bool,
    actions: &mut Vec<Action>,
) {
    if response.secondary_clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let at = mapping.to_page(pos);
        let layer = editor.layer_at(at, 1.0 / editor.view.zoom);
        if let Some(id) = layer
            && !editor.is_selected(id)
        {
            editor.select_only(id);
        }
        state.context = Some((at, layer.is_some()));
    }
    let Some((at, on_layer)) = state.context else {
        return;
    };
    response.context_menu(|ui| {
        ui.set_min_width(200.0);
        if on_layer {
            if ui.button(tr("Duplicate   Ctrl D")).clicked() {
                editor.duplicate_selection(geom::Vec2::new(10.0, 10.0));
                ui.close();
            }
            if ui.button(tr("Group   Ctrl G")).clicked() {
                editor.group_selection();
                ui.close();
            }
            if ui.button(tr("Bring forward   Ctrl ]")).clicked() {
                editor.reorder_selection(true);
                ui.close();
            }
            if ui.button(tr("Send backward   Ctrl [")).clicked() {
                editor.reorder_selection(false);
                ui.close();
            }
            if ui.button(tr("Delete   Del")).clicked() {
                editor.delete_selection();
                ui.close();
            }
            ui.separator();
        }
        if ui.button(tr("Paste here")).clicked() {
            state.paste_target = Some(at);
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            ui.close();
        }
        if ui.button(tr("Select all   Ctrl A")).clicked() {
            editor.select_all();
            ui.close();
        }
        if ui.button(tr("Zoom to fit   Shift 0")).clicked() {
            editor.view.fit_requested = true;
            ui.close();
        }
        ui.separator();
        let mut rulers = show_rulers;
        if ui
            .checkbox(&mut rulers, tr("Show rulers   Shift R"))
            .clicked()
        {
            actions.push(Action::ToggleRulers);
            ui.close();
        }
    });
}

/// Grid spacings in page units, finest first.
const GRID_SPACINGS: [f64; 3] = [20.0, 100.0, 500.0];
/// The grid uses the finest spacing that is at least this far apart on screen, and is
/// hidden when even the coarsest one is closer.
const GRID_MIN_GAP: f64 = 12.0;

/// Page-space spacing of the dot grid at `zoom`, or `None` when it would be too dense.
pub fn grid_spacing(zoom: f64) -> Option<f64> {
    GRID_SPACINGS
        .into_iter()
        .find(|spacing| spacing * zoom >= GRID_MIN_GAP)
}

/// Dots at every grid intersection in view. They are fainter right after a level change, so
/// switching spacing does not make them pop.
fn draw_grid(painter: &egui::Painter, mapping: Mapping, zoom: f64, theme: &Theme) {
    let Some(spacing) = grid_spacing(zoom) else {
        return;
    };
    let gap = spacing * zoom;
    let fade = ((gap - GRID_MIN_GAP) / GRID_MIN_GAP).clamp(0.0, 1.0) as f32;
    let color = c(theme.canvas_grid).gamma_multiply(0.55 + 0.45 * fade);
    let top_left = mapping.to_page(mapping.rect.min);
    let bottom_right = mapping.to_page(mapping.rect.max);
    let (x0, x1) = (
        (top_left.x / spacing).ceil() as i64,
        (bottom_right.x / spacing).floor() as i64,
    );
    let (y0, y1) = (
        (top_left.y / spacing).ceil() as i64,
        (bottom_right.y / spacing).floor() as i64,
    );
    // At one or two pixels a square looks like a dot and costs far fewer vertices.
    let dot = Vec2::splat(if gap >= 2.0 * GRID_MIN_GAP { 2.0 } else { 1.5 });
    for iy in y0..=y1 {
        for ix in x0..=x1 {
            let p = Point::new(ix as f64 * spacing, iy as f64 * spacing);
            painter.rect_filled(
                Rect::from_center_size(mapping.to_screen(p), dot),
                0.0,
                color,
            );
        }
    }
}

/// Zoom with Ctrl+scroll or pinch, pan with scroll, middle button or Space+drag.
fn handle_view_input(
    ctx: &egui::Context,
    response: &egui::Response,
    editor: &mut Editor,
    state: &mut CanvasState,
) {
    let rect = response.rect;
    let (scroll, zoom, pointer, middle_down, space, primary_down, delta) = ctx.input(|i| {
        (
            i.smooth_scroll_delta,
            i.zoom_delta(),
            i.pointer.hover_pos(),
            i.pointer.button_down(egui::PointerButton::Middle),
            i.key_down(egui::Key::Space),
            i.pointer.primary_down(),
            i.pointer.delta(),
        )
    });
    let typing = ctx.egui_wants_keyboard_input();
    if response.hovered() {
        if zoom != 1.0 {
            let anchor = pointer.unwrap_or(rect.center()) - rect.min;
            editor.view.zoom_around(
                f64::from(zoom),
                geom::Vec2::new(f64::from(anchor.x), f64::from(anchor.y)),
            );
        } else if scroll != Vec2::ZERO {
            editor.view.pan += geom::Vec2::new(f64::from(scroll.x), f64::from(scroll.y));
        }
    }
    let wants_pan = middle_down || (space && !typing && primary_down);
    if wants_pan && (state.panning || response.hovered()) {
        state.panning = true;
        editor.view.pan += geom::Vec2::new(f64::from(delta.x), f64::from(delta.y));
        ctx.set_cursor_icon(CursorIcon::Grabbing);
    } else {
        state.panning = false;
        if space && !typing && response.hovered() {
            ctx.set_cursor_icon(CursorIcon::Grab);
        }
    }
}

fn handle_pointer(
    ctx: &egui::Context,
    response: &egui::Response,
    editor: &mut Editor,
    state: &mut CanvasState,
    mapping: Mapping,
    default_text_color: libforma::paint::Color,
) {
    if state.panning
        || ctx.input(|i| i.key_down(egui::Key::Space)) && !ctx.egui_wants_keyboard_input()
    {
        return;
    }
    let (pressed, released, down, pos, mods) = ctx.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_released(),
            i.pointer.primary_down(),
            i.pointer.latest_pos(),
            i.modifiers,
        )
    });
    let Some(pos) = pos else { return };
    let tool_ctx = ToolContext {
        modifiers: Modifiers {
            shift: mods.shift,
            alt: mods.alt,
            command: mods.command,
        },
        pixel: 1.0 / editor.view.zoom,
        default_text_color,
    };
    let page_pos = mapping.to_page(pos);

    let mut cursor = CursorHint::Default;
    let started = pressed && response.hovered();
    if started {
        state.pressing = true;
        editor.pointer(
            PointerEvent::Press {
                pos: page_pos,
                double: false,
            },
            tool_ctx,
        );
        if editor.interaction.editing_text().is_some() {
            state.focus_text = true;
        }
    }
    if state.pressing && released {
        state.pressing = false;
        editor.pointer(PointerEvent::Release { pos: page_pos }, tool_ctx);
        if response.double_clicked() {
            editor.pointer(
                PointerEvent::Press {
                    pos: page_pos,
                    double: true,
                },
                tool_ctx,
            );
            if editor.interaction.editing_text().is_some() {
                state.focus_text = true;
            }
        }
    } else if state.pressing && down && !started {
        cursor = editor.pointer(PointerEvent::Drag { pos: page_pos }, tool_ctx);
    } else if !started && response.hovered() {
        cursor = editor.pointer(PointerEvent::Hover { pos: page_pos }, tool_ctx);
    }
    if response.hovered() || state.pressing {
        ctx.set_cursor_icon(cursor_icon(cursor));
    }
}

fn cursor_icon(hint: CursorHint) -> CursorIcon {
    match hint {
        CursorHint::Default => CursorIcon::Default,
        CursorHint::Move => CursorIcon::Move,
        CursorHint::Resize(Handle::N | Handle::S) => CursorIcon::ResizeVertical,
        CursorHint::Resize(Handle::E | Handle::W) => CursorIcon::ResizeHorizontal,
        CursorHint::Resize(Handle::NE | Handle::SW) => CursorIcon::ResizeNeSw,
        CursorHint::Resize(Handle::NW | Handle::SE) => CursorIcon::ResizeNwSe,
        CursorHint::Rotate => CursorIcon::Alias,
        CursorHint::Crosshair => CursorIcon::Crosshair,
        CursorHint::Text => CursorIcon::Text,
        CursorHint::Point => CursorIcon::PointingHand,
    }
}

/// While a text node is edited in place, a text field replaces it on the canvas.
fn text_edit_overlay(
    ui: &mut egui::Ui,
    editor: &mut Editor,
    state: &mut CanvasState,
    mapping: Mapping,
    theme: &Theme,
) {
    let Some((id, mut content)) = editor.text_edit_content() else {
        return;
    };
    let Some(node) = editor.doc.node(id).cloned() else {
        return;
    };
    let libforma::NodeKind::Text(text) = &node.kind else {
        return;
    };
    let Some((_, transform)) = editor.node_outline(id) else {
        return;
    };
    let origin = mapping.to_screen(transform * Point::ORIGIN);
    // Uniform scale of the node on screen.
    let scale = (mapping.view * transform).determinant().abs().sqrt() as f32;
    let font_size = (text.style.font_size as f32 * scale).max(4.0);
    let width = text.width.map(|w| w as f32 * scale);
    let color = c(text.style.color);

    let area = egui::Area::new(egui::Id::unique(("text-edit", id)))
        .fixed_pos(origin)
        .order(egui::Order::Foreground);
    let response = area.show(ui.ctx(), |ui| {
        ui.set_clip_rect(ui.clip_rect().intersect(mapping.rect));
        let mut edit = egui::TextEdit::multiline(&mut content)
            .font(egui::FontId::proportional(font_size))
            .text_color(color)
            .frame(egui::Frame::NONE)
            .desired_rows(1)
            .margin(egui::Margin::ZERO);
        edit = match width {
            Some(w) => edit.desired_width(w),
            None => edit.desired_width(f32::INFINITY).clip_text(false),
        };
        let response = ui.add(edit);
        ui.painter().rect_stroke(
            response.rect.expand(2.0),
            2.0,
            egui::Stroke::new(1.0, c(theme.accent)),
            egui::StrokeKind::Outside,
        );
        response
    });
    let response = response.inner;
    if state.focus_text {
        response.request_focus();
        state.focus_text = false;
    }
    if response.changed() {
        editor.set_text_edit_content(content);
    }
    if response.lost_focus() {
        editor.end_text_edit();
    }
}

#[cfg(test)]
mod tests {
    use super::grid_spacing;

    #[test]
    fn grid_spacing_follows_the_zoom() {
        assert_eq!(grid_spacing(1.0), Some(20.0));
        assert_eq!(grid_spacing(0.5), Some(100.0));
        assert_eq!(grid_spacing(0.1), Some(500.0));
        // Even the coarsest spacing would be under 12 screen points apart.
        assert_eq!(grid_spacing(0.02), None);
        assert_eq!(grid_spacing(8.0), Some(20.0));
    }
}
