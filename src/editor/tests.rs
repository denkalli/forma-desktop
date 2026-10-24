//! Editor behaviour, driven through the same pointer events the canvas sends.

use libforma::geom::{Point, Rect, Vec2};
use libforma::model::{Geometry, NodeKind};

use super::*;

fn ctx() -> ToolContext {
    ToolContext {
        modifiers: Modifiers::default(),
        pixel: 1.0,
    }
}

fn shift() -> ToolContext {
    ToolContext {
        modifiers: Modifiers {
            shift: true,
            ..Modifiers::default()
        },
        pixel: 1.0,
    }
}

fn drag(editor: &mut Editor, from: (f64, f64), to: (f64, f64), ctx: ToolContext) {
    let (from, to) = (Point::from(from), Point::from(to));
    editor.pointer(
        PointerEvent::Press {
            pos: from,
            double: false,
        },
        ctx,
    );
    let mid = from.midpoint(to);
    editor.pointer(PointerEvent::Drag { pos: mid }, ctx);
    editor.pointer(PointerEvent::Drag { pos: to }, ctx);
    editor.pointer(PointerEvent::Release { pos: to }, ctx);
}

fn click(editor: &mut Editor, at: (f64, f64), ctx: ToolContext) {
    let at = Point::from(at);
    editor.pointer(
        PointerEvent::Press {
            pos: at,
            double: false,
        },
        ctx,
    );
    editor.pointer(PointerEvent::Release { pos: at }, ctx);
}

fn editor() -> Editor {
    Editor::new_document("Teste", None)
}

/// An editor whose page holds a 500 × 500 frame at the origin. Returns the frame's id.
fn editor_with_frame() -> (Editor, libforma::NodeId) {
    let editor = Editor::new_document("Teste", Some(("Frame 1", Size::new(500.0, 500.0))));
    let frame = libforma::query::frames(editor.current_page())[0];
    (editor, frame)
}

/// Draws a rectangle and returns its id.
fn rect(editor: &mut Editor, from: (f64, f64), to: (f64, f64)) -> libforma::NodeId {
    editor.set_tool(Tool::Rect);
    drag(editor, from, to, ctx());
    editor.selection[0]
}

fn bounds(editor: &mut Editor, id: libforma::NodeId) -> Rect {
    let page = editor.doc.page(editor.page).unwrap();
    libforma::query::page_bounds(page, id, editor.scene.text_engine()).unwrap()
}

#[test]
fn drawing_a_shape_is_one_undo_step() {
    let mut editor = editor();
    let id = rect(&mut editor, (10.0, 20.0), (110.0, 70.0));
    assert_eq!(bounds(&mut editor, id), Rect::new(10.0, 20.0, 110.0, 70.0));
    assert_eq!(editor.tool, Tool::Select);
    assert!(editor.is_modified());
    editor.undo();
    assert!(editor.current_page().children.is_empty());
    assert!(!editor.is_modified());
}

#[test]
fn clicking_with_a_shape_tool_creates_a_default_shape() {
    let mut editor = editor();
    editor.set_tool(Tool::Ellipse);
    click(&mut editor, (200.0, 200.0), ctx());
    let id = editor.selection[0];
    assert_eq!(
        bounds(&mut editor, id),
        Rect::new(150.0, 150.0, 250.0, 250.0)
    );
}

#[test]
fn shift_draws_squares() {
    let mut editor = editor();
    editor.set_tool(Tool::Rect);
    drag(&mut editor, (0.0, 0.0), (100.0, 40.0), shift());
    let id = editor.selection[0];
    assert_eq!(bounds(&mut editor, id).size(), (100.0, 100.0).into());
}

#[test]
fn moving_a_shape_is_one_undo_step() {
    let mut editor = editor();
    let id = rect(&mut editor, (100.0, 100.0), (150.0, 150.0));
    // Drag from inside the shape. Snapping is disabled with the command modifier.
    let free = ToolContext {
        modifiers: Modifiers {
            command: true,
            ..Modifiers::default()
        },
        pixel: 1.0,
    };
    drag(&mut editor, (120.0, 120.0), (157.0, 133.0), free);
    assert_eq!(
        bounds(&mut editor, id),
        Rect::new(137.0, 113.0, 187.0, 163.0)
    );
    editor.undo();
    assert_eq!(
        bounds(&mut editor, id),
        Rect::new(100.0, 100.0, 150.0, 150.0)
    );
}

#[test]
fn moving_snaps_to_the_frame_centre() {
    let (mut editor, _) = editor_with_frame();
    let id = rect(&mut editor, (100.0, 100.0), (150.0, 150.0));
    // Centre lands at x = 248, close enough to the frame centre (250) to snap.
    drag(&mut editor, (125.0, 125.0), (248.0, 125.0), ctx());
    assert_eq!(bounds(&mut editor, id).center().x, 250.0);
    assert!(
        editor.interaction.guides.is_empty(),
        "guides are cleared on release"
    );
}

#[test]
fn resizing_with_a_handle() {
    let mut editor = editor();
    let id = rect(&mut editor, (100.0, 100.0), (200.0, 150.0));
    drag(&mut editor, (200.0, 150.0), (300.0, 250.0), ctx());
    assert_eq!(
        bounds(&mut editor, id),
        Rect::new(100.0, 100.0, 300.0, 250.0)
    );
    let NodeKind::Shape(shape) = &editor.doc.node(id).unwrap().kind else {
        panic!()
    };
    assert_eq!(shape.geometry.bounds().size(), (200.0, 150.0).into());
}

#[test]
fn marquee_and_click_selection() {
    let mut editor = editor();
    let a = rect(&mut editor, (10.0, 10.0), (50.0, 50.0));
    let b = rect(&mut editor, (100.0, 10.0), (150.0, 50.0));
    let _c = rect(&mut editor, (300.0, 300.0), (350.0, 350.0));
    click(&mut editor, (400.0, 450.0), ctx());
    assert!(editor.selection.is_empty());
    drag(&mut editor, (0.0, 0.0), (160.0, 60.0), ctx());
    assert_eq!(editor.selection, vec![a, b]);
    click(&mut editor, (20.0, 20.0), ctx());
    assert_eq!(editor.selection, vec![a]);
    click(&mut editor, (120.0, 20.0), shift());
    assert_eq!(editor.selection, vec![a, b]);
}

#[test]
fn double_click_enters_groups() {
    let mut editor = editor();
    let a = rect(&mut editor, (10.0, 10.0), (50.0, 50.0));
    let b = rect(&mut editor, (60.0, 10.0), (90.0, 50.0));
    editor.selection = vec![a, b];
    editor.group_selection();
    let group = editor.selection[0];
    click(&mut editor, (20.0, 20.0), ctx());
    assert_eq!(editor.selection, vec![group]);
    editor.pointer(
        PointerEvent::Press {
            pos: Point::new(20.0, 20.0),
            double: true,
        },
        ctx(),
    );
    editor.pointer(
        PointerEvent::Release {
            pos: Point::new(20.0, 20.0),
        },
        ctx(),
    );
    assert_eq!(editor.selection, vec![a]);
}

#[test]
fn group_and_ungroup_keep_positions() {
    let mut editor = editor();
    let a = rect(&mut editor, (10.0, 10.0), (50.0, 50.0));
    let b = rect(&mut editor, (60.0, 10.0), (90.0, 50.0));
    editor.selection = vec![a, b];
    editor.group_selection();
    let group = editor.selection[0];
    editor.nudge(Vec2::new(5.0, 5.0));
    editor.select_only(group);
    editor.ungroup_selection();
    assert_eq!(bounds(&mut editor, a), Rect::new(15.0, 15.0, 55.0, 55.0));
    assert_eq!(editor.current_page().children.len(), 2);
}

#[test]
fn new_text_left_empty_disappears() {
    let mut editor = editor();
    editor.set_tool(Tool::Text);
    click(&mut editor, (50.0, 50.0), ctx());
    assert!(editor.interaction.editing_text().is_some());
    editor.end_text_edit();
    assert!(editor.current_page().children.is_empty());
    assert!(!editor.history.can_redo());

    editor.set_tool(Tool::Text);
    click(&mut editor, (50.0, 50.0), ctx());
    editor.set_text_edit_content("Olá".into());
    editor.set_text_edit_content("Olá, Forma".into());
    editor.end_text_edit();
    let id = editor.current_page().children[0].id;
    let NodeKind::Text(text) = &editor.doc.node(id).unwrap().kind else {
        panic!()
    };
    assert_eq!(text.content, "Olá, Forma");
    editor.undo();
    assert!(
        editor.current_page().children.is_empty(),
        "creating and typing is one step"
    );
}

#[test]
fn pen_draws_a_closed_path() {
    let mut editor = editor();
    editor.set_tool(Tool::Pen);
    for p in [(10.0, 10.0), (110.0, 10.0), (60.0, 90.0)] {
        click(&mut editor, p, ctx());
    }
    // Clicking the first point closes the path.
    click(&mut editor, (11.0, 11.0), ctx());
    assert_eq!(editor.tool, Tool::Select);
    let id = editor.selection[0];
    let NodeKind::Shape(shape) = &editor.doc.node(id).unwrap().kind else {
        panic!()
    };
    assert!(matches!(shape.geometry, Geometry::Path { .. }));
    assert!(shape.fill.is_some(), "closed paths get a fill");
    assert_eq!(bounds(&mut editor, id), Rect::new(10.0, 10.0, 110.0, 90.0));
    editor.undo();
    assert!(editor.current_page().children.is_empty());
}

#[test]
fn a_single_pen_click_leaves_nothing() {
    let mut editor = editor();
    editor.set_tool(Tool::Pen);
    click(&mut editor, (10.0, 10.0), ctx());
    editor.escape();
    assert!(editor.current_page().children.is_empty());
    assert!(!editor.is_modified());
}

#[test]
fn point_editing_moves_an_anchor() {
    let mut editor = editor();
    let id = rect(&mut editor, (0.0, 0.0), (100.0, 100.0));
    editor.start_point_edit(id);
    drag(&mut editor, (100.0, 100.0), (150.0, 120.0), ctx());
    assert_eq!(bounds(&mut editor, id), Rect::new(0.0, 0.0, 150.0, 120.0));
}

#[test]
fn copy_paste_and_duplicate() {
    let mut editor = editor();
    let a = rect(&mut editor, (10.0, 10.0), (50.0, 50.0));
    let text = editor.copy_selection().unwrap();
    assert!(editor.paste(&text, Vec2::new(10.0, 0.0)));
    let pasted = editor.selection[0];
    assert_ne!(pasted, a);
    assert_eq!(
        bounds(&mut editor, pasted),
        Rect::new(20.0, 10.0, 60.0, 50.0)
    );
    assert!(!editor.paste("hello", Vec2::ZERO));
    editor.duplicate_selection(Vec2::new(0.0, 100.0));
    assert_eq!(editor.current_page().children.len(), 3);
}

#[test]
fn align_single_node_to_parent_frame() {
    let (mut editor, frame) = editor_with_frame();
    // Outside a frame there is nothing to align to.
    let outside = rect(&mut editor, (610.0, 10.0), (650.0, 50.0));
    editor.align_selection(Align::Right);
    assert_eq!(
        bounds(&mut editor, outside),
        Rect::new(610.0, 10.0, 650.0, 50.0)
    );

    let id = rect(&mut editor, (10.0, 10.0), (50.0, 50.0));
    assert_eq!(editor.doc.locate(id).unwrap().parent, Some(frame));
    editor.align_selection(Align::CenterX);
    editor.align_selection(Align::Bottom);
    assert_eq!(
        bounds(&mut editor, id),
        Rect::new(230.0, 460.0, 270.0, 500.0)
    );
}

#[test]
fn save_open_and_modified_flag() {
    let dir = std::env::temp_dir().join(format!("forma-desktop-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("teste.forma");
    let mut editor = editor();
    rect(&mut editor, (10.0, 10.0), (50.0, 50.0));
    editor.save_to(&path, None).unwrap();
    assert!(!editor.is_modified());
    let reopened = Editor::open(&path).unwrap();
    assert_eq!(reopened.doc.pages, editor.doc.pages);
    assert!(!reopened.is_modified());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn deleting_pages_keeps_one() {
    let mut editor = editor();
    let first = editor.page;
    editor.add_page();
    assert_eq!(editor.doc.pages.len(), 2);
    assert_ne!(editor.page, first);
    let second = editor.page;
    editor.delete_page(second);
    assert_eq!(editor.page, first);
    editor.delete_page(first);
    assert_eq!(editor.doc.pages.len(), 1);
    assert_eq!(editor.interaction.take_messages().len(), 1);
}

/// Gives the frame a name label in the 100 × 20 area above its top-left corner, as the
/// canvas would.
fn label_frame(editor: &mut Editor, frame: libforma::NodeId) {
    editor.interaction.frame_labels = vec![(frame, Rect::new(0.0, -20.0, 100.0, 0.0))];
}

#[test]
fn clicking_a_frame_label_selects_the_frame() {
    let (mut editor, frame) = editor_with_frame();
    label_frame(&mut editor, frame);
    click(&mut editor, (30.0, -10.0), ctx());
    assert_eq!(editor.selection, vec![frame]);

    // Hovering the label highlights the frame.
    editor.selection.clear();
    editor.pointer(
        PointerEvent::Hover {
            pos: Point::new(30.0, -10.0),
        },
        ctx(),
    );
    assert_eq!(editor.interaction.hover, Some(frame));
}

#[test]
fn dragging_a_frame_label_moves_the_frame_in_one_step() {
    let (mut editor, frame) = editor_with_frame();
    label_frame(&mut editor, frame);
    drag(&mut editor, (30.0, -10.0), (130.0, 40.0), ctx());
    assert_eq!(
        bounds(&mut editor, frame),
        Rect::new(100.0, 50.0, 600.0, 550.0)
    );
    editor.undo();
    assert_eq!(
        bounds(&mut editor, frame),
        Rect::new(0.0, 0.0, 500.0, 500.0)
    );
}

#[test]
fn a_frame_label_wins_over_the_rotate_area_of_the_selection() {
    let (mut editor, frame) = editor_with_frame();
    let id = rect(&mut editor, (10.0, 10.0), (60.0, 60.0));
    editor.select_only(id);
    // Just outside the rectangle's top-left corner: the rotate area of the selection.
    let at = Point::new(2.0, 2.0);
    let hint = editor.pointer(PointerEvent::Hover { pos: at }, ctx());
    assert!(matches!(hint, CursorHint::Rotate));
    editor.interaction.frame_labels = vec![(frame, Rect::new(0.0, 0.0, 9.0, 9.0))];
    let hint = editor.pointer(PointerEvent::Hover { pos: at }, ctx());
    assert!(matches!(hint, CursorHint::Default));
    click(&mut editor, (2.0, 2.0), ctx());
    assert_eq!(editor.selection, vec![frame]);
}

#[test]
fn fitting_an_empty_canvas_centres_the_origin_at_100_percent() {
    let mut view = Viewport::default();
    view.fit(None, Size::new(800.0, 600.0));
    assert_eq!(view.zoom, 1.0);
    assert_eq!(view.pan, Vec2::new(400.0, 300.0));

    // Content is centred: a 1000 × 1000 frame in a 1128 × 728 canvas fits at 60 %.
    view.fit(
        Some(Rect::new(0.0, 0.0, 1000.0, 1000.0)),
        Size::new(1128.0, 728.0),
    );
    assert!((view.zoom - 0.6).abs() < 1e-9);
    assert_eq!(view.pan, Vec2::new(264.0, 64.0));
}

fn frame_size(editor: &Editor, id: libforma::NodeId) -> Size {
    match &editor.doc.node(id).unwrap().kind {
        NodeKind::Frame(frame) => frame.size,
        other => panic!("not a frame: {other:?}"),
    }
}

#[test]
fn dragging_with_the_frame_tool_draws_a_frame_in_one_step() {
    let mut editor = editor();
    editor.set_tool(Tool::Frame);
    drag(&mut editor, (10.0, 20.0), (210.0, 120.0), ctx());
    let [id] = editor.selection[..] else {
        panic!("one frame selected");
    };
    let node = editor.doc.node(id).unwrap();
    assert!(node.is_frame());
    assert_eq!(node.name, "Frame 1");
    // The frame changes size while dragging; it is never stretched.
    assert_eq!(node.transform, Affine::translate((10.0, 20.0)));
    assert_eq!(frame_size(&editor, id), Size::new(200.0, 100.0));
    assert_eq!(editor.tool, Tool::Select);

    editor.set_tool(Tool::Frame);
    drag(&mut editor, (300.0, 0.0), (400.0, 100.0), ctx());
    assert_eq!(
        editor.doc.node(editor.selection[0]).unwrap().name,
        "Frame 2"
    );
    editor.undo();
    editor.undo();
    assert!(editor.current_page().children.is_empty());
}

#[test]
fn clicking_with_the_frame_tool_makes_a_default_frame() {
    let mut editor = editor();
    editor.set_tool(Tool::Frame);
    click(&mut editor, (100.0, 100.0), ctx());
    let id = editor.selection[0];
    assert_eq!(frame_size(&editor, id), Size::new(100.0, 100.0));
    assert_eq!(bounds(&mut editor, id), Rect::new(50.0, 50.0, 150.0, 150.0));
}

#[test]
fn a_preset_frame_is_centred_in_the_view() {
    let mut editor = editor();
    editor.view.fit(None, Size::new(800.0, 600.0));
    editor.view.zoom_around(0.5, Vec2::new(400.0, 300.0));
    let center = editor.view.page_point(Vec2::new(400.0, 300.0));
    assert_eq!(center, Point::new(0.0, 0.0));

    editor.set_tool(Tool::Frame);
    assert!(editor.insert_frame("Story", Size::new(1080.0, 1920.0), center));
    let id = editor.selection[0];
    assert_eq!(editor.doc.node(id).unwrap().name, "Story");
    assert_eq!(
        bounds(&mut editor, id),
        Rect::new(-540.0, -960.0, 540.0, 960.0)
    );
    assert_eq!(editor.tool, Tool::Select);
}

fn parent(editor: &Editor, id: libforma::NodeId) -> Option<libforma::NodeId> {
    editor.doc.locate(id).unwrap().parent
}

/// A frame at (100, 50) with one 50 × 50 rectangle inside at (150, 100) page space.
fn frame_with_child() -> (Editor, libforma::NodeId, libforma::NodeId) {
    let (mut editor, frame) = editor_with_frame();
    editor.apply(Command::SetTransform {
        id: frame,
        transform: Affine::translate((100.0, 50.0)),
    });
    let child = rect(&mut editor, (150.0, 100.0), (200.0, 150.0));
    (editor, frame, child)
}

#[test]
fn shapes_drawn_in_a_frame_go_into_it_and_keep_their_place() {
    let (mut editor, frame, child) = frame_with_child();
    assert_eq!(parent(&editor, child), Some(frame));
    assert_eq!(
        bounds(&mut editor, child),
        Rect::new(150.0, 100.0, 200.0, 150.0)
    );
    assert_eq!(
        editor.doc.node(child).unwrap().transform,
        Affine::translate((50.0, 50.0))
    );

    // Starting outside the frame draws on the page, even when the shape reaches into it.
    let outside = rect(&mut editor, (20.0, 20.0), (300.0, 300.0));
    assert_eq!(parent(&editor, outside), None);
}

#[test]
fn text_and_paths_drawn_in_a_frame_go_into_it() {
    let (mut editor, frame, _) = frame_with_child();
    editor.set_tool(Tool::Text);
    click(&mut editor, (300.0, 300.0), ctx());
    let text = editor.selection[0];
    editor.set_text_edit_content("Olá".into());
    editor.end_text_edit();
    assert_eq!(parent(&editor, text), Some(frame));

    editor.set_tool(Tool::Pen);
    click(&mut editor, (200.0, 200.0), ctx());
    click(&mut editor, (300.0, 250.0), ctx());
    editor.finish_path();
    let path = editor.selection[0];
    assert_eq!(parent(&editor, path), Some(frame));
    assert_eq!(
        bounds(&mut editor, path),
        Rect::new(200.0, 200.0, 300.0, 250.0)
    );
}

#[test]
fn clicks_in_a_frame_act_on_its_content() {
    let (mut editor, frame, child) = frame_with_child();
    editor.selection.clear();
    click(&mut editor, (175.0, 125.0), ctx());
    assert_eq!(editor.selection, vec![child]);

    // The empty area of a frame with content selects nothing...
    click(&mut editor, (400.0, 400.0), ctx());
    assert!(editor.selection.is_empty());

    // ...and a drag there selects the frame's children, not the frame.
    drag(&mut editor, (120.0, 70.0), (180.0, 130.0), ctx());
    assert_eq!(editor.selection, vec![child]);
    assert_eq!(parent(&editor, child), Some(frame));
}

#[test]
fn an_empty_frame_is_selected_by_clicking_it() {
    let (mut editor, frame) = editor_with_frame();
    click(&mut editor, (250.0, 250.0), ctx());
    assert_eq!(editor.selection, vec![frame]);
}

#[test]
fn a_selected_frame_moves_when_dragged_from_its_empty_area() {
    let (mut editor, frame, child) = frame_with_child();
    editor.select_only(frame);
    drag(&mut editor, (400.0, 400.0), (450.0, 400.0), ctx());
    assert_eq!(editor.selection, vec![frame]);
    assert_eq!(
        bounds(&mut editor, frame),
        Rect::new(150.0, 50.0, 650.0, 550.0)
    );
    // The child moves with the frame and stays in it.
    assert_eq!(parent(&editor, child), Some(frame));
    assert_eq!(
        bounds(&mut editor, child),
        Rect::new(200.0, 100.0, 250.0, 150.0)
    );
}

#[test]
fn moving_a_layer_out_of_a_frame_puts_it_on_the_page_in_one_step() {
    let (mut editor, frame, child) = frame_with_child();
    drag(&mut editor, (175.0, 125.0), (875.0, 125.0), ctx());
    assert_eq!(parent(&editor, child), None);
    assert_eq!(
        bounds(&mut editor, child),
        Rect::new(850.0, 100.0, 900.0, 150.0)
    );
    // It sits just in front of the frame it left.
    let page = editor.current_page();
    let order: Vec<_> = page.children.iter().map(|n| n.id).collect();
    assert_eq!(order, vec![frame, child]);

    editor.undo();
    assert_eq!(parent(&editor, child), Some(frame));
    assert_eq!(
        bounds(&mut editor, child),
        Rect::new(150.0, 100.0, 200.0, 150.0)
    );
}

#[test]
fn moving_a_layer_onto_a_frame_puts_it_inside() {
    let (mut editor, frame, _) = frame_with_child();
    let loose = rect(&mut editor, (700.0, 100.0), (750.0, 150.0));
    assert_eq!(parent(&editor, loose), None);
    drag(&mut editor, (725.0, 125.0), (325.0, 325.0), ctx());
    assert_eq!(parent(&editor, loose), Some(frame));
    assert_eq!(
        bounds(&mut editor, loose),
        Rect::new(300.0, 300.0, 350.0, 350.0)
    );
}

#[test]
fn top_level_frames_and_grouped_layers_keep_their_parent_when_moved() {
    let (mut editor, frame, child) = frame_with_child();
    // A second frame dropped onto the first stays on the page.
    editor.set_tool(Tool::Frame);
    drag(&mut editor, (700.0, 0.0), (800.0, 100.0), ctx());
    let other = editor.selection[0];
    editor.interaction.frame_labels = vec![(other, Rect::new(700.0, -20.0, 800.0, 0.0))];
    drag(&mut editor, (750.0, -10.0), (350.0, 190.0), ctx());
    assert_eq!(parent(&editor, other), None);

    // A layer in a group inside the frame stays in its group.
    let second = rect(&mut editor, (300.0, 100.0), (350.0, 150.0));
    editor.selection = vec![child, second];
    editor.group_selection();
    let group = editor.selection[0];
    assert_eq!(parent(&editor, group), Some(frame));
    // Enter the group with a double click, then drag one of its layers off the frame.
    editor.pointer(
        PointerEvent::Press {
            pos: Point::new(175.0, 125.0),
            double: true,
        },
        ctx(),
    );
    editor.pointer(
        PointerEvent::Release {
            pos: Point::new(175.0, 125.0),
        },
        ctx(),
    );
    assert_eq!(editor.selection, vec![child]);
    drag(&mut editor, (175.0, 125.0), (1175.0, 125.0), ctx());
    assert_eq!(parent(&editor, child), Some(group));
}

#[test]
fn a_new_design_can_start_with_a_named_frame_or_empty() {
    let editor = Editor::new_document("Post", Some(("Story", Size::new(1080.0, 1920.0))));
    let page = editor.current_page();
    assert_eq!(page.children.len(), 1);
    assert_eq!(page.children[0].name, "Story");
    assert_eq!(
        frame_size(&editor, page.children[0].id),
        Size::new(1080.0, 1920.0)
    );

    let free = Editor::new_document("Livre", None);
    assert!(free.current_page().children.is_empty());
}

/// Number of SVG files `target` exports, with their names.
fn exported_names(editor: &mut Editor, target: ExportTarget) -> Vec<String> {
    editor
        .export(ExportFormat::Svg, target, 1.0, None)
        .map(|files| files.into_iter().map(|(name, _)| name).collect())
        .unwrap_or_default()
}

#[test]
fn export_targets_follow_frames_and_selection() {
    let mut editor = editor();
    // An empty page has nothing to export.
    assert_eq!(editor.export_count(ExportTarget::Page), 0);
    assert_eq!(editor.default_export_target(), ExportTarget::Page);
    assert!(
        editor
            .export(ExportFormat::Svg, ExportTarget::Page, 1.0, None)
            .is_err()
    );

    editor.set_tool(Tool::Frame);
    drag(&mut editor, (0.0, 0.0), (100.0, 100.0), ctx());
    let first = editor.selection[0];
    editor.set_tool(Tool::Frame);
    drag(&mut editor, (200.0, 0.0), (300.0, 100.0), ctx());
    let loose = rect(&mut editor, (400.0, 0.0), (450.0, 50.0));

    // With a frame selected, the selected frames come first.
    editor.select_only(first);
    assert_eq!(editor.default_export_target(), ExportTarget::SelectedFrames);
    assert_eq!(
        exported_names(&mut editor, ExportTarget::SelectedFrames),
        ["Frame 1"]
    );
    // A layer that is not a frame: the selection, as one image.
    editor.select_only(loose);
    assert_eq!(editor.export_count(ExportTarget::SelectedFrames), 0);
    assert_eq!(editor.default_export_target(), ExportTarget::Selection);
    assert_eq!(
        exported_names(&mut editor, ExportTarget::Selection),
        ["Teste"]
    );
    // Nothing selected: every frame of the page.
    editor.selection.clear();
    let frames = ExportTarget::Frames { all_pages: false };
    assert_eq!(editor.default_export_target(), frames);
    assert_eq!(exported_names(&mut editor, frames), ["Frame 1", "Frame 2"]);
    assert_eq!(exported_names(&mut editor, ExportTarget::Page), ["Page 1"]);

    // A PDF holds every target as one file.
    let pdf = editor.export(ExportFormat::Pdf, frames, 1.0, None).unwrap();
    assert_eq!(pdf.len(), 1);
    assert!(pdf[0].1.starts_with(b"%PDF"));
}

#[test]
fn the_selection_exports_framed_to_its_layers() {
    let mut editor = editor();
    let a = rect(&mut editor, (10.0, 10.0), (50.0, 50.0));
    let b = rect(&mut editor, (100.0, 20.0), (150.0, 90.0));
    editor.selection = vec![b, a];
    let lists = editor.export_lists(ExportTarget::Selection).unwrap();
    assert_eq!(lists.len(), 1);
    assert_eq!(lists[0].1.area, Rect::new(10.0, 10.0, 150.0, 90.0));
}

#[test]
fn frames_of_all_pages_are_exported_together() {
    let (mut editor, _) = editor_with_frame();
    editor.add_page();
    // The new page copies the first frame of the page it was added after.
    let all = ExportTarget::Frames { all_pages: true };
    assert_eq!(editor.export_count(all), 2);
    assert_eq!(exported_names(&mut editor, all).len(), 2);
}

#[test]
fn resizing_a_frame_changes_its_size_not_its_content() {
    let (mut editor, frame, child) = frame_with_child();
    editor.select_only(frame);
    // The bottom-right handle of the frame, which spans (100, 50) - (600, 550).
    drag(&mut editor, (600.0, 550.0), (700.0, 650.0), ctx());
    assert_eq!(frame_size(&editor, frame), Size::new(600.0, 600.0));
    assert_eq!(
        editor.doc.node(frame).unwrap().transform,
        Affine::translate((100.0, 50.0))
    );
    assert_eq!(
        bounds(&mut editor, child),
        Rect::new(150.0, 100.0, 200.0, 150.0)
    );
}

#[test]
fn with_a_frame_selected_a_click_on_its_content_selects_the_layer() {
    let (mut editor, frame, child) = frame_with_child();
    editor.select_only(frame);
    click(&mut editor, (175.0, 125.0), ctx());
    assert_eq!(editor.selection, vec![child]);
}

#[test]
fn hovering_the_empty_area_of_a_frame_with_content_shows_nothing() {
    let (mut editor, _, child) = frame_with_child();
    editor.selection.clear();
    let hover = |editor: &mut Editor, at: (f64, f64)| {
        editor.pointer(PointerEvent::Hover { pos: at.into() }, ctx());
        editor.interaction.hover
    };
    assert_eq!(hover(&mut editor, (400.0, 400.0)), None);
    assert_eq!(hover(&mut editor, (175.0, 125.0)), Some(child));
}

#[test]
fn layers_snap_to_their_siblings_in_a_frame() {
    let (mut editor, _, _) = frame_with_child();
    let second = rect(&mut editor, (300.0, 300.0), (350.0, 350.0));
    // Its left edge lands at x = 152, close to the first child's (150).
    drag(&mut editor, (325.0, 325.0), (177.0, 325.0), ctx());
    assert_eq!(bounds(&mut editor, second).x0, 150.0);
}

#[test]
fn duplicating_and_pasting_keep_layers_in_their_frame() {
    let (mut editor, frame, child) = frame_with_child();
    editor.select_only(child);
    editor.duplicate_selection(Vec2::new(10.0, 10.0));
    let copy = editor.selection[0];
    assert_eq!(parent(&editor, copy), Some(frame));
    assert_eq!(
        bounds(&mut editor, copy),
        Rect::new(160.0, 110.0, 210.0, 160.0)
    );

    // Pasted at the same place, the layer goes back into the frame under it...
    editor.select_only(child);
    let text = editor.copy_selection().unwrap();
    assert!(editor.paste(&text, Vec2::new(20.0, 0.0)));
    let pasted = editor.selection[0];
    assert_eq!(parent(&editor, pasted), Some(frame));
    assert_eq!(
        bounds(&mut editor, pasted),
        Rect::new(170.0, 100.0, 220.0, 150.0)
    );
    // ...and outside every frame it goes on the page.
    assert!(editor.paste(&text, Vec2::new(1000.0, 0.0)));
    assert_eq!(parent(&editor, editor.selection[0]), None);
}

/// A PNG of the given size.
fn png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 40, 40, 255]));
    let mut data = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut data),
            image::ImageFormat::Png,
        )
        .unwrap();
    data
}

#[test]
fn images_are_inserted_into_the_first_frame_and_fit_it() {
    let (mut editor, frame, _) = frame_with_child();
    assert!(editor.insert_image(png(1000, 500)));
    let image = editor.selection[0];
    assert_eq!(parent(&editor, image), Some(frame));
    // Scaled to 80 % of the frame's width and centred on it.
    assert_eq!(
        bounds(&mut editor, image),
        Rect::new(150.0, 200.0, 550.0, 400.0)
    );

    // Without frames, the image keeps its size and is centred on the origin.
    let mut free = self::editor();
    assert!(free.insert_image(png(40, 20)));
    let image = free.selection[0];
    assert_eq!(parent(&free, image), None);
    assert_eq!(
        bounds(&mut free, image),
        Rect::new(-20.0, -10.0, 20.0, 10.0)
    );

    assert!(!free.insert_image(b"not an image".to_vec()));
}

#[test]
fn a_new_page_gets_an_empty_copy_of_the_first_frame() {
    let (mut editor, frame, _) = frame_with_child();
    editor.add_page();
    let page = editor.current_page();
    assert_eq!(page.name, "Page 2");
    let [copy] = &page.children[..] else {
        panic!("one frame on the new page");
    };
    assert_ne!(copy.id, frame);
    assert_eq!(copy.name, "Frame 1");
    assert!(copy.children().is_empty());
    assert_eq!(frame_size(&editor, copy.id), Size::new(500.0, 500.0));

    // A page without frames gives an empty page.
    let mut free = self::editor();
    free.add_page();
    assert!(free.current_page().children.is_empty());
}

#[test]
fn opening_a_version_1_file_turns_its_page_into_a_frame() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../libforma/tests/data/showcase-v1.forma");
    let editor = Editor::open(&path).unwrap();
    let page = editor.current_page();
    let [frame] = &page.children[..] else {
        panic!("one top-level frame");
    };
    assert_eq!(frame.name, page.name);
    assert_eq!(default_name(frame), page.name);
    assert!(!frame.children().is_empty());
    assert_eq!(frame_size(&editor, frame.id), Size::new(1080.0, 1080.0));
    assert!(!editor.is_modified());
}

#[test]
fn distributing_evens_out_the_gaps() {
    let mut editor = editor();
    let a = rect(&mut editor, (0.0, 0.0), (10.0, 10.0));
    let c = rect(&mut editor, (100.0, 0.0), (140.0, 10.0));
    let b = rect(&mut editor, (20.0, 50.0), (40.0, 60.0));
    // Fewer than three layers: nothing to distribute.
    editor.selection = vec![a, c];
    editor.distribute_selection(Axis::Horizontal);
    assert_eq!(bounds(&mut editor, a).x0, 0.0);

    // Selection order does not matter; the outer edges stay.
    editor.selection = vec![c, a, b];
    editor.distribute_selection(Axis::Horizontal);
    assert_eq!(bounds(&mut editor, a), Rect::new(0.0, 0.0, 10.0, 10.0));
    assert_eq!(bounds(&mut editor, c), Rect::new(100.0, 0.0, 140.0, 10.0));
    // Widths 10 + 20 + 40 in a span of 140 leave two gaps of 35.
    assert_eq!(bounds(&mut editor, b), Rect::new(45.0, 50.0, 65.0, 60.0));
    editor.undo();
    assert_eq!(bounds(&mut editor, b), Rect::new(20.0, 50.0, 40.0, 60.0));
}

#[test]
fn distributing_works_inside_a_moved_frame() {
    let (mut editor, frame, child) = frame_with_child();
    let b = rect(&mut editor, (150.0, 160.0), (200.0, 170.0));
    let c = rect(&mut editor, (150.0, 400.0), (200.0, 450.0));
    assert_eq!(parent(&editor, b), Some(frame));
    editor.selection = vec![child, b, c];
    editor.distribute_selection(Axis::Vertical);
    // Span 100..450 holds 50 + 10 + 50: gaps of 120.
    assert_eq!(
        bounds(&mut editor, b),
        Rect::new(150.0, 270.0, 200.0, 280.0)
    );
    assert_eq!(bounds(&mut editor, child).y0, 100.0);
    assert_eq!(bounds(&mut editor, c).y0, 400.0);
}

/// Timings of the editor on a large page: 20 frames with 250 shapes and 15 texts each.
/// Run with `cargo test --release perf -- --ignored --nocapture`.
#[test]
#[ignore = "slow; run with --release --ignored --nocapture"]
fn perf_editing_a_large_document() {
    use libforma::model::TextStyle;
    use std::time::{Duration, Instant};

    let mut page = Page::new("Grande");
    let mut target = None;
    for f in 0..20 {
        let (col, row) = (f64::from(f % 5), f64::from(f / 5));
        let mut frame =
            Node::frame(format!("Quadro {f}"), 1080.0, 1080.0).at(col * 1280.0, row * 1280.0);
        for i in 0..250 {
            let (x, y) = (f64::from(i % 16) * 65.0, f64::from(i / 16) * 65.0);
            let shape = Node::shape(Geometry::rect(50.0, 50.0))
                .with_fill(libforma::paint::Color::rgb(i as u8, 90, 160))
                .at(x, y);
            if f == 0 && i == 15 {
                target = Some(shape.id);
            }
            frame = frame.with_child(shape);
        }
        for i in 0..15 {
            let style = TextStyle {
                font_size: 24.0,
                ..TextStyle::default()
            };
            frame = frame.with_child(
                Node::text(format!("Texto {i} do quadro {f}"), style).at(20.0, 40.0 * f64::from(i)),
            );
        }
        page = page.with_child(frame);
    }
    let mut editor = Editor::new_document("Grande", None);
    editor.page = page.id;
    editor.doc = Document::new("Grande").with_page(page);
    let target = target.unwrap();
    let report =
        |name: &str, d: Duration| println!("{name:<40} {:>8.3} ms", d.as_secs_f64() * 1000.0);

    let start = Instant::now();
    editor.display_list();
    report("first display_list (loads fonts)", start.elapsed());
    let start = Instant::now();
    for _ in 0..10 {
        editor.display_list();
    }
    report("display_list", start.elapsed() / 10);

    let start = Instant::now();
    for i in 0..100 {
        let pos = Point::new(f64::from(i) * 13.0, 500.0);
        editor.pointer(PointerEvent::Hover { pos }, ctx());
    }
    report("hover", start.elapsed() / 100);

    let start = Instant::now();
    let labels = editor.frame_labels();
    report("frame_labels", start.elapsed());
    assert_eq!(labels.len(), 20);

    // A drag of 60 steps, rebuilding the scene after each, as the canvas does. The rectangle
    // at (975, 0) is clear of the texts, and the drag ends outside the frame.
    let press = Instant::now();
    editor.pointer(
        PointerEvent::Press {
            pos: Point::new(1000.0, 25.0),
            double: false,
        },
        ctx(),
    );
    assert_eq!(editor.selection, vec![target]);
    report("press", press.elapsed());
    let start = Instant::now();
    for i in 1..=60 {
        let pos = Point::new(1000.0 + f64::from(i) * 5.0, 25.0 + f64::from(i) * 3.0);
        editor.pointer(PointerEvent::Drag { pos }, ctx());
        editor.display_list();
    }
    let step = start.elapsed() / 60;
    report("drag step + display_list", step);
    let release = Instant::now();
    editor.pointer(
        PointerEvent::Release {
            pos: Point::new(1300.0, 205.0),
        },
        ctx(),
    );
    report("release (reparent check)", release.elapsed());

    if !cfg!(debug_assertions) {
        // A drag step must fit comfortably in a 60 Hz frame.
        assert!(step < Duration::from_millis(10), "drag step took {step:?}");
    }
}

/// A text node with some content, created with the text tool. `None` without system fonts.
fn text_node(editor: &mut Editor, at: (f64, f64)) -> Option<libforma::NodeId> {
    editor.set_tool(Tool::Text);
    click(editor, at, ctx());
    let id = editor.selection[0];
    editor.set_text_edit_content("Forma design".into());
    editor.end_text_edit();
    (bounds(editor, id).width() > 0.0).then_some(id)
}

fn text_of(editor: &Editor, id: libforma::NodeId) -> libforma::model::TextNode {
    match &editor.doc.node(id).unwrap().kind {
        NodeKind::Text(t) => t.clone(),
        other => panic!("not text: {other:?}"),
    }
}

#[test]
fn text_boxes_resize_from_their_handles() {
    use libforma::model::TextSizing;
    let mut editor = editor();
    let Some(id) = text_node(&mut editor, (100.0, 100.0)) else {
        eprintln!("skipping: no system fonts");
        return;
    };
    assert_eq!(text_of(&editor, id).sizing(), TextSizing::AutoWidth);
    let b = bounds(&mut editor, id);

    // The right handle sets the width; the height follows the text.
    drag(
        &mut editor,
        (b.x1, b.center().y),
        (b.x1 + 100.0, b.center().y),
        ctx(),
    );
    let text = text_of(&editor, id);
    assert_eq!(text.sizing(), TextSizing::AutoHeight);
    assert_eq!(text.width, Some(b.width() + 100.0));

    // The bottom handle fixes the height.
    let b = bounds(&mut editor, id);
    drag(
        &mut editor,
        (b.center().x, b.y1),
        (b.center().x, b.y1 + 80.0),
        ctx(),
    );
    let text = text_of(&editor, id);
    assert_eq!(text.sizing(), TextSizing::Fixed);
    assert_eq!(bounds(&mut editor, id).height(), b.height() + 80.0);
    // The font size never changes.
    assert_eq!(text.style.font_size, 32.0);

    // A fixed box keeps its height when only the width changes.
    let b = bounds(&mut editor, id);
    drag(
        &mut editor,
        (b.x1, b.center().y),
        (b.x1 - 50.0, b.center().y),
        ctx(),
    );
    assert_eq!(text_of(&editor, id).height, Some(b.height()));
    editor.undo();
    editor.undo();
    editor.undo();
    assert_eq!(text_of(&editor, id).sizing(), TextSizing::AutoWidth);
}

#[test]
fn paste_here_centres_the_layers_on_the_point() {
    let (mut editor, frame, child) = frame_with_child();
    editor.select_only(child);
    let text = editor.copy_selection().unwrap();
    // Inside the frame: the copy lands in it, centred on the point.
    assert!(editor.paste_at(&text, Point::new(400.0, 300.0)));
    let pasted = editor.selection[0];
    assert_eq!(
        bounds(&mut editor, pasted),
        Rect::new(375.0, 275.0, 425.0, 325.0)
    );
    assert_eq!(parent(&editor, pasted), Some(frame));
    // On empty canvas it goes on the page.
    assert!(editor.paste_at(&text, Point::new(-500.0, -500.0)));
    assert_eq!(parent(&editor, editor.selection[0]), None);
    assert!(!editor.paste_at("hello", Point::ORIGIN));
}

#[test]
fn layer_at_follows_the_click_rules() {
    let (mut editor, frame, child) = frame_with_child();
    assert_eq!(editor.layer_at(Point::new(175.0, 125.0), 1.0), Some(child));
    // The empty area of a frame with content, and the canvas outside, give nothing.
    assert_eq!(editor.layer_at(Point::new(400.0, 400.0), 1.0), None);
    assert_eq!(editor.layer_at(Point::new(-50.0, -50.0), 1.0), None);
    editor.select_only(frame);
    assert_eq!(editor.layer_at(Point::new(400.0, 400.0), 1.0), Some(frame));
}
