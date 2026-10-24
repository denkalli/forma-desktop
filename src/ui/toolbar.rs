//! The floating toolbar at the bottom of the canvas.

use eframe::egui::{self, Ui};
use libforma::boolean::BooleanOp;

use crate::app::Action;
use crate::editor::{Editor, Tool};
use crate::i18n::tr;
use crate::theme::Theme;
use crate::theme::egui::c;
use crate::theme::icons::Icon;
use crate::ui::inspector::PAGE_PRESETS;
use crate::ui::widgets::icon_button;

pub fn tool_icon(tool: Tool) -> Icon {
    match tool {
        Tool::Select => Icon::MousePointer2,
        Tool::Frame => Icon::Frame,
        Tool::Rect => Icon::Square,
        Tool::Ellipse => Icon::Circle,
        Tool::Polygon => Icon::Triangle,
        Tool::Star => Icon::Star,
        Tool::Line => Icon::Minus,
        Tool::Pen => Icon::PenTool,
        Tool::Text => Icon::Type,
    }
}

fn tooltip(tool: Tool) -> String {
    match tool.shortcut() {
        Some(key) => format!("{} ({key})", tool.label()),
        None => tool.label().to_owned(),
    }
}

/// Shows the toolbar centred at the bottom of `canvas`.
pub fn show(
    ctx: &egui::Context,
    canvas: egui::Rect,
    editor: &mut Editor,
    theme: &Theme,
    actions: &mut Vec<Action>,
) {
    let pos = egui::pos2(canvas.center().x, canvas.bottom() - 16.0);
    egui::Area::new(egui::Id::unique("toolbar"))
        .fixed_pos(pos)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(c(theme.panel))
                .stroke(egui::Stroke::new(1.0, c(theme.border)))
                .corner_radius(theme.radius_large)
                .shadow(ui.visuals().popup_shadow)
                .inner_margin(egui::Margin::same(5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        tools(ui, canvas, editor, theme, actions);
                    });
                });
        });
}

/// A small arrow next to a tool button that opens a menu.
fn menu_arrow(ui: &mut Ui) -> egui::Response {
    ui.add(egui::Button::new(crate::theme::icons::icon(Icon::ChevronDown).size(11.0)).frame(false))
}

fn tools(
    ui: &mut Ui,
    canvas: egui::Rect,
    editor: &mut Editor,
    theme: &Theme,
    actions: &mut Vec<Action>,
) {
    if icon_button(
        ui,
        theme,
        Icon::MousePointer2,
        &tooltip(Tool::Select),
        editor.tool == Tool::Select,
    )
    .clicked()
    {
        editor.set_tool(Tool::Select);
    }

    // Frames: drag to draw one, or pick a ready-made size to add it at the centre of the view.
    if icon_button(
        ui,
        theme,
        tool_icon(Tool::Frame),
        &tooltip(Tool::Frame),
        editor.tool == Tool::Frame,
    )
    .clicked()
    {
        editor.set_tool(Tool::Frame);
    }
    let arrow = menu_arrow(ui);
    egui::Popup::menu(&arrow).show(|ui| {
        for (name, w, h) in PAGE_PRESETS {
            if ui
                .button(format!("{}  ·  {w:.0} × {h:.0}", tr(name)))
                .clicked()
            {
                let half = canvas.size() / 2.0;
                let center = editor.view.page_point(libforma::geom::Vec2::new(
                    f64::from(half.x),
                    f64::from(half.y),
                ));
                editor.insert_frame(name, libforma::geom::Size::new(w, h), center);
                ui.close();
            }
        }
    });

    // Shapes: the last used shape, with a menu for the others.
    let shape = editor.shape_tool;
    if icon_button(
        ui,
        theme,
        tool_icon(shape),
        &tooltip(shape),
        editor.tool.is_shape(),
    )
    .clicked()
    {
        editor.set_tool(shape);
    }
    let arrow = menu_arrow(ui);
    egui::Popup::menu(&arrow).show(|ui| {
        for tool in Tool::SHAPES {
            let label = (crate::theme::icons::icon(tool_icon(tool)), tooltip(tool));
            if ui
                .add(egui::Button::selectable(editor.tool == tool, label))
                .clicked()
            {
                editor.set_tool(tool);
                ui.close();
            }
        }
    });

    for tool in [Tool::Pen, Tool::Text] {
        if icon_button(
            ui,
            theme,
            tool_icon(tool),
            &tooltip(tool),
            editor.tool == tool,
        )
        .clicked()
        {
            editor.set_tool(tool);
        }
    }
    if icon_button(ui, theme, Icon::Image, tr("Image"), false).clicked() {
        actions.push(Action::InsertImage);
    }

    if editor.can_combine_selection() {
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);
        for (op, icon, label) in [
            (BooleanOp::Union, Icon::SquaresUnite, tr("Union")),
            (BooleanOp::Subtract, Icon::SquaresSubtract, tr("Subtract")),
            (
                BooleanOp::Intersect,
                Icon::SquaresIntersect,
                tr("Intersect"),
            ),
            (
                BooleanOp::Exclude,
                Icon::SquaresExclude,
                tr("Exclude overlap"),
            ),
        ] {
            if icon_button(ui, theme, icon, label, false).clicked() {
                editor.boolean_selection(op);
            }
        }
    }
}
