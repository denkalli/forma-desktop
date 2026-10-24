//! The layer tree in the left panel. The front-most layer is listed first.

use std::collections::HashSet;

use eframe::egui::{self, Color32, Sense, Ui, Vec2};
use libforma::model::{Geometry, Node, NodeKind, NodeLocation};
use libforma::{NodeId, PageId};

use crate::editor::{Editor, default_name};
use crate::i18n::tr;
use crate::theme::Theme;
use crate::theme::egui::c;
use crate::theme::icons::{FAMILY, Icon};
use crate::ui::widgets::section_header;

const ROW_HEIGHT: f32 = 26.0;
const INDENT: f32 = 14.0;

#[derive(Default)]
pub struct LayersState {
    expanded: HashSet<NodeId>,
    renaming: Option<(NodeId, String)>,
    dragging: Option<NodeId>,
}

struct Row {
    id: NodeId,
    depth: usize,
    parent: Option<NodeId>,
    /// Index among siblings, back to front.
    index: usize,
    group: bool,
    rect: egui::Rect,
}

/// Where a dragged layer would be dropped.
enum Drop {
    /// Above (in front of) or below (behind) a row.
    Beside { row: usize, front: bool },
    /// Inside a group, on top.
    Into { row: usize },
}

pub fn kind_icon(node: &Node) -> Icon {
    match &node.kind {
        NodeKind::Group { .. } => Icon::Group,
        NodeKind::Frame(_) => Icon::Frame,
        NodeKind::Text(_) => Icon::Type,
        NodeKind::Image(_) => Icon::Image,
        NodeKind::Shape(shape) => match shape.geometry {
            Geometry::Rect { .. } => Icon::Square,
            Geometry::Ellipse { .. } => Icon::Circle,
            Geometry::Polygon { .. } => Icon::Triangle,
            Geometry::Star { .. } => Icon::Star,
            Geometry::Line { .. } => Icon::Minus,
            Geometry::Path { .. } => Icon::Spline,
        },
    }
}

pub fn show(ui: &mut Ui, editor: &mut Editor, theme: &Theme, state: &mut LayersState) {
    section_header(ui, tr("Layers"), |_| {});
    ui.add_space(2.0);

    // Selected nodes inside groups make their groups expand.
    for id in editor.selection.clone() {
        let mut parent = editor.doc.locate(id).and_then(|l| l.parent);
        while let Some(p) = parent {
            state.expanded.insert(p);
            parent = editor.doc.locate(p).and_then(|l| l.parent);
        }
    }

    let page_id = editor.page;
    let children = editor.current_page().children.clone();
    if children.is_empty() {
        ui.label(
            egui::RichText::new(tr("No layers yet. Use the toolbar to draw."))
                .color(c(theme.text_faint))
                .size(11.5),
        );
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut rows = Vec::new();
            list(ui, editor, theme, state, &children, None, 0, &mut rows);
            handle_drag(ui, editor, theme, state, &rows, page_id);
        });
}

#[allow(clippy::too_many_arguments)]
fn list(
    ui: &mut Ui,
    editor: &mut Editor,
    theme: &Theme,
    state: &mut LayersState,
    nodes: &[Node],
    parent: Option<NodeId>,
    depth: usize,
    rows: &mut Vec<Row>,
) {
    for (index, node) in nodes.iter().enumerate().rev() {
        let rect = row(ui, editor, theme, state, node, depth);
        rows.push(Row {
            id: node.id,
            depth,
            parent,
            index,
            group: node.is_container(),
            rect,
        });
        if node.is_container() && state.expanded.contains(&node.id) {
            list(
                ui,
                editor,
                theme,
                state,
                node.children(),
                Some(node.id),
                depth + 1,
                rows,
            );
        }
    }
}

fn row(
    ui: &mut Ui,
    editor: &mut Editor,
    theme: &Theme,
    state: &mut LayersState,
    node: &Node,
    depth: usize,
) -> egui::Rect {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), ROW_HEIGHT),
        Sense::click_and_drag(),
    );
    let selected = editor.is_selected(node.id);
    let hovered = response.hovered() || editor.interaction.hover == Some(node.id);
    let bg = if selected {
        c(theme.accent_soft)
    } else if hovered {
        c(theme.raised)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect, theme.radius_small as f32, bg);

    let dim = !node.visible;
    let text_color = c(if dim { theme.text_faint } else { theme.text });
    let icon_font = egui::FontId::new(13.0, egui::FontFamily::Name(FAMILY.into()));
    let mut x = rect.left() + 6.0 + depth as f32 * INDENT;

    if node.is_container() {
        let expanded = state.expanded.contains(&node.id);
        let arrow_rect =
            egui::Rect::from_center_size(egui::pos2(x + 6.0, rect.center().y), Vec2::splat(14.0));
        let arrow = ui.interact(
            arrow_rect,
            ui.scope_id().with(("expand", node.id)),
            Sense::click(),
        );
        ui.painter().text(
            arrow_rect.center(),
            egui::Align2::CENTER_CENTER,
            char::from(if expanded {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            }),
            egui::FontId::new(11.0, egui::FontFamily::Name(FAMILY.into())),
            c(theme.text_muted),
        );
        if arrow.clicked() {
            if expanded {
                state.expanded.remove(&node.id);
            } else {
                state.expanded.insert(node.id);
            }
        }
    }
    x += 14.0;
    ui.painter().text(
        egui::pos2(x + 6.0, rect.center().y),
        egui::Align2::CENTER_CENTER,
        char::from(kind_icon(node)),
        icon_font.clone(),
        c(if dim {
            theme.text_faint
        } else {
            theme.text_muted
        }),
    );
    x += 18.0;

    // Visibility and lock buttons, shown on hover or when not in the default state.
    let mut right = rect.right() - 4.0;
    for (icon, active, tooltip, is_visibility) in [
        (
            if node.locked {
                Icon::Lock
            } else {
                Icon::LockOpen
            },
            node.locked,
            "Travar",
            false,
        ),
        (
            if node.visible {
                Icon::Eye
            } else {
                Icon::EyeOff
            },
            !node.visible,
            tr("Show/hide"),
            true,
        ),
    ] {
        let button_rect = egui::Rect::from_center_size(
            egui::pos2(right - 10.0, rect.center().y),
            Vec2::splat(20.0),
        );
        right -= 22.0;
        if !(response.hovered() || active || ui.rect_contains_pointer(button_rect)) {
            continue;
        }
        let button = ui.interact(
            button_rect,
            ui.scope_id().with((tooltip, node.id)),
            Sense::click(),
        );
        ui.painter().text(
            button_rect.center(),
            egui::Align2::CENTER_CENTER,
            char::from(icon),
            icon_font.clone(),
            c(if button.hovered() {
                theme.text
            } else {
                theme.text_muted
            }),
        );
        if button.on_hover_text(tooltip).clicked() {
            if is_visibility {
                editor.set_visible(node.id, !node.visible);
            } else {
                editor.set_locked(node.id, !node.locked);
            }
        }
    }

    if let Some((id, buffer)) = &mut state.renaming
        && *id == node.id
    {
        let edit_rect = egui::Rect::from_min_max(
            egui::pos2(x, rect.top() + 2.0),
            egui::pos2(right, rect.bottom() - 2.0),
        );
        let edit = ui.put(edit_rect, egui::TextEdit::singleline(buffer));
        edit.request_focus();
        if edit.lost_focus() {
            let (id, name) = (*id, buffer.trim().to_owned());
            editor.rename_node(id, name);
            state.renaming = None;
        }
    } else {
        let galley = ui.painter().layout(
            default_name(node),
            egui::FontId::proportional(12.5),
            text_color,
            (right - x).max(10.0),
        );
        ui.painter().galley(
            egui::pos2(x, rect.center().y - galley.size().y / 2.0),
            galley,
            text_color,
        );
    }

    if response.clicked() {
        let shift = ui.input(|i| i.modifiers.shift || i.modifiers.command);
        if shift {
            editor.toggle_selected(node.id);
        } else {
            editor.select_only(node.id);
        }
    }
    if response.double_clicked() {
        state.renaming = Some((node.id, default_name(node)));
    }
    if response.drag_started() {
        state.dragging = Some(node.id);
        if !editor.is_selected(node.id) {
            editor.select_only(node.id);
        }
    }
    if response.hovered() {
        editor.interaction.hover = Some(node.id);
    }
    response.context_menu(|ui| {
        if ui.button(tr("Rename")).clicked() {
            state.renaming = Some((node.id, default_name(node)));
            ui.close();
        }
        if ui.button(tr("Duplicate")).clicked() {
            editor.select_only(node.id);
            editor.duplicate_selection(libforma::geom::Vec2::ZERO);
            ui.close();
        }
        if node.is_group() && ui.button(tr("Ungroup")).clicked() {
            editor.select_only(node.id);
            editor.ungroup_selection();
            ui.close();
        }
        ui.separator();
        if ui.button(tr("Delete")).clicked() {
            editor.select_only(node.id);
            editor.delete_selection();
            ui.close();
        }
    });
    rect
}

fn drop_target(rows: &[Row], pointer: egui::Pos2) -> Option<Drop> {
    let (i, row) = rows
        .iter()
        .enumerate()
        .find(|(_, r)| r.rect.y_range().contains(pointer.y))?;
    let t = (pointer.y - row.rect.top()) / row.rect.height();
    Some(if row.group && (0.3..0.7).contains(&t) {
        Drop::Into { row: i }
    } else {
        Drop::Beside {
            row: i,
            front: t < 0.5,
        }
    })
}

fn handle_drag(
    ui: &mut Ui,
    editor: &mut Editor,
    theme: &Theme,
    state: &mut LayersState,
    rows: &[Row],
    page: PageId,
) {
    let Some(dragged) = state.dragging else {
        return;
    };
    let (pointer, released) = ui.input(|i| (i.pointer.interact_pos(), i.pointer.any_released()));
    let Some(pointer) = pointer else { return };
    let target = drop_target(rows, pointer);
    let accent = c(theme.accent);
    match &target {
        Some(Drop::Beside { row, front }) => {
            let r = rows[*row].rect;
            let y = if *front { r.top() } else { r.bottom() };
            let x = r.left() + rows[*row].depth as f32 * INDENT + 20.0;
            ui.painter().line_segment(
                [egui::pos2(x, y), egui::pos2(r.right(), y)],
                egui::Stroke::new(2.0, accent),
            );
        }
        Some(Drop::Into { row }) => {
            ui.painter().rect_stroke(
                rows[*row].rect,
                theme.radius_small as f32,
                egui::Stroke::new(1.5, accent),
                egui::StrokeKind::Inside,
            );
        }
        None => {}
    }
    if !released {
        return;
    }
    state.dragging = None;
    let Some(target) = target else { return };
    let Some(from) = editor.doc.locate(dragged) else {
        return;
    };
    let to = match target {
        Drop::Into { row } => {
            let row = &rows[row];
            if row.id == dragged {
                return;
            }
            state.expanded.insert(row.id);
            let len = editor.doc.node(row.id).map_or(0, |n| n.children().len());
            NodeLocation::in_group(page, row.id, len)
        }
        Drop::Beside { row, front } => {
            let row = &rows[row];
            if row.id == dragged {
                return;
            }
            let mut index = if front { row.index + 1 } else { row.index };
            // Indices shift once the dragged node leaves the same list.
            if from.parent == row.parent && from.index < index {
                index -= 1;
            }
            NodeLocation {
                page,
                parent: row.parent,
                index,
            }
        }
    };
    if to != from {
        editor.move_node(dragged, to);
    }
}
