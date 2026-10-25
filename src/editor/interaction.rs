//! Pointer interaction on the canvas: selecting, moving, resizing, rotating, drawing shapes
//! and paths, editing text and path points.
//!
//! Positions are in page space. The canvas converts screen input to [`PointerEvent`]s and
//! draws the overlays described by [`Interaction`].

use std::f64::consts::{FRAC_PI_4, PI};

use libforma::geom::{Affine, BezPath, PathEl, Point, Rect, Size, Vec2};
use libforma::history::{Command, History};
use libforma::model::{Document, Geometry, Node, NodeKind, NodeLocation, TextStyle};
use libforma::paint::{Color, Stroke};
use libforma::{NodeId, query};

use super::geometry::{apply_in_page_space, normalize_path, rect_to_rect, resized, rotate_about};
use super::{Editor, Tool};

/// Fill of newly drawn shapes.
pub const DEFAULT_FILL: Color = Color::rgb(0xd4, 0xd4, 0xd8);
/// Size of shapes created with a click instead of a drag.
const DEFAULT_SIZE: f64 = 100.0;
/// Distance, in screen points, a press must travel to become a drag.
const DRAG_THRESHOLD: f64 = 3.0;
/// Reach of handles and points, in screen points.
const HANDLE_REACH: f64 = 6.0;
/// Rotation zone outside the corner handles, in screen points.
const ROTATE_REACH: f64 = 18.0;
/// Snapping distance, in screen points.
const SNAP_DISTANCE: f64 = 6.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub command: bool,
}

/// Pointer input in page space.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PointerEvent {
    Press { pos: Point, double: bool },
    Drag { pos: Point },
    Release { pos: Point },
    Hover { pos: Point },
}

/// Information about the view needed to interpret pointer input.
#[derive(Clone, Copy, Debug)]
pub struct ToolContext {
    pub modifiers: Modifiers,
    /// Size of one screen point in page units (1 / zoom).
    pub pixel: f64,
    /// Theme-derived color used when creating a text node.
    pub default_text_color: Color,
}

/// Cursor the canvas should show.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CursorHint {
    #[default]
    Default,
    Move,
    Resize(Handle),
    Rotate,
    Crosshair,
    Text,
    Point,
}

/// Resize handles around a selection.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Handle {
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
}

impl Handle {
    pub const ALL: [Handle; 8] = [
        Handle::NW,
        Handle::N,
        Handle::NE,
        Handle::E,
        Handle::SE,
        Handle::S,
        Handle::SW,
        Handle::W,
    ];

    /// Position of the handle on `r`.
    pub fn position(self, r: Rect) -> Point {
        let (cx, cy) = (r.center().x, r.center().y);
        match self {
            Handle::N => Point::new(cx, r.y0),
            Handle::NE => Point::new(r.x1, r.y0),
            Handle::E => Point::new(r.x1, cy),
            Handle::SE => Point::new(r.x1, r.y1),
            Handle::S => Point::new(cx, r.y1),
            Handle::SW => Point::new(r.x0, r.y1),
            Handle::W => Point::new(r.x0, cy),
            Handle::NW => Point::new(r.x0, r.y0),
        }
    }

    pub fn is_corner(self) -> bool {
        matches!(self, Handle::NE | Handle::SE | Handle::SW | Handle::NW)
    }

    fn moves_left(self) -> bool {
        matches!(self, Handle::W | Handle::NW | Handle::SW)
    }
    fn moves_right(self) -> bool {
        matches!(self, Handle::E | Handle::NE | Handle::SE)
    }
    fn moves_top(self) -> bool {
        matches!(self, Handle::N | Handle::NE | Handle::NW)
    }
    fn moves_bottom(self) -> bool {
        matches!(self, Handle::S | Handle::SE | Handle::SW)
    }
}

/// A snapping guide line to draw while moving.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Guide {
    /// A vertical line at `x = position`, or horizontal at `y = position`.
    pub vertical: bool,
    pub position: f64,
    /// Extent of the line along its direction.
    pub from: f64,
    pub to: f64,
}

/// An editable point of a path: an anchor or a curve control point.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct EditPoint {
    /// Position in page space.
    pub pos: Point,
    pub anchor: bool,
    /// The anchor a control point belongs to, for drawing the handle line.
    pub owner: Option<Point>,
    element: usize,
    point: usize,
}

/// An anchor of a path being drawn with the pen.
#[derive(Clone, Copy, PartialEq, Debug)]
struct PenAnchor {
    pos: Point,
    /// Outgoing control point; the incoming one mirrors it.
    handle: Option<Point>,
}

struct PenState {
    id: NodeId,
    key: u64,
    anchors: Vec<PenAnchor>,
    closed: bool,
}

/// Text being edited in place.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TextEditState {
    pub id: NodeId,
    key: u64,
    /// The node was created for this edit and is removed if left empty.
    created: bool,
}

enum Drag {
    /// Pressed but not moved enough to start a drag yet.
    Pending {
        start: Point,
        target: Option<NodeId>,
        was_selected: bool,
    },
    Marquee {
        start: Point,
        base: Vec<NodeId>,
        /// Top-level frame the rectangle started in: it selects that frame's children.
        scope: Option<NodeId>,
    },
    Move {
        start: Point,
        key: u64,
        items: Vec<(NodeId, Affine, Affine)>,
        bounds: Rect,
        /// Bounds of the nodes the selection can snap to.
        snap: Vec<Rect>,
    },
    Resize {
        handle: Handle,
        key: u64,
        start_bounds: Rect,
        items: Vec<(Node, Affine)>,
    },
    Rotate {
        key: u64,
        center: Point,
        start_angle: f64,
        items: Vec<(NodeId, Affine, Affine)>,
    },
    Create {
        tool: Tool,
        start: Point,
        key: u64,
        node: Option<NodeId>,
    },
    PenHandle,
    EditPoint {
        id: NodeId,
        key: u64,
        element: usize,
        point: usize,
        start: Point,
        original: BezPath,
        to_local: Affine,
    },
}

/// State of the current pointer interaction and what the canvas should draw for it.
#[derive(Default)]
pub struct Interaction {
    drag: Option<Drag>,
    pen: Option<PenState>,
    text_edit: Option<TextEditState>,
    point_edit: Option<NodeId>,
    /// Node under the pointer.
    pub hover: Option<NodeId>,
    /// Selection rectangle being dragged.
    pub marquee: Option<Rect>,
    /// Snapping guides to draw.
    pub guides: Vec<Guide>,
    /// Page-space areas of the frame name labels, back to front. The canvas sets them every
    /// frame, since their size depends on its text layout.
    pub frame_labels: Vec<(NodeId, Rect)>,
    messages: Vec<String>,
}

impl Interaction {
    /// Queues a message for the user.
    pub fn report(&mut self, message: String) {
        self.messages.push(message);
    }

    pub fn take_messages(&mut self) -> Vec<String> {
        std::mem::take(&mut self.messages)
    }

    pub fn editing_text(&self) -> Option<NodeId> {
        self.text_edit.map(|t| t.id)
    }

    pub fn editing_points(&self) -> Option<NodeId> {
        self.point_edit
    }

    pub fn is_drawing_path(&self) -> bool {
        self.pen.is_some()
    }

    /// Whether a drag is in progress (autosave waits for it to end).
    pub fn is_busy(&self) -> bool {
        self.drag.is_some() || self.pen.is_some() || self.text_edit.is_some()
    }

    /// Abandons any drag in progress, keeping what was already applied.
    pub fn cancel(&mut self, history: &mut History, _doc: &mut Document) {
        history.end_merge();
        self.drag = None;
        self.pen = None;
        self.text_edit = None;
        self.point_edit = None;
        self.marquee = None;
        self.guides.clear();
    }
}

fn shape_geometry(tool: Tool, size: Size) -> Geometry {
    match tool {
        Tool::Ellipse => Geometry::Ellipse { size },
        Tool::Polygon => Geometry::Polygon { size, sides: 3 },
        Tool::Star => Geometry::Star {
            size,
            points: 5,
            inner_ratio: 0.4,
        },
        _ => Geometry::Rect {
            size,
            corner_radius: 0.0,
        },
    }
}

/// Rectangle from a drag, with Shift for a square and Alt to grow from the center.
fn drag_rect(start: Point, pos: Point, modifiers: Modifiers) -> Rect {
    let mut delta = pos - start;
    if modifiers.shift {
        let side = delta.x.abs().max(delta.y.abs());
        delta = Vec2::new(side.copysign(delta.x), side.copysign(delta.y));
    }
    if modifiers.alt {
        Rect::from_points(start - delta, start + delta)
    } else {
        Rect::from_points(start, start + delta)
    }
}

/// Snaps a vector to multiples of 45°.
fn snap_45(v: Vec2) -> Vec2 {
    let angle = (v.atan2() / FRAC_PI_4).round() * FRAC_PI_4;
    Vec2::from_angle(angle) * v.hypot()
}

fn pen_path(anchors: &[PenAnchor], closed: bool) -> BezPath {
    let mut path = BezPath::new();
    let Some(first) = anchors.first() else {
        return path;
    };
    path.move_to(first.pos);
    let segment = |path: &mut BezPath, from: &PenAnchor, to: &PenAnchor| {
        let out = from.handle;
        let into = to.handle.map(|h| to.pos - (h - to.pos));
        match (out, into) {
            (None, None) => path.line_to(to.pos),
            _ => path.curve_to(out.unwrap_or(from.pos), into.unwrap_or(to.pos), to.pos),
        }
    };
    for pair in anchors.windows(2) {
        segment(&mut path, &pair[0], &pair[1]);
    }
    if closed && anchors.len() > 1 {
        segment(&mut path, anchors.last().unwrap(), first);
        path.close_path();
    }
    path
}

/// Editable points of a path, in its local space.
fn path_points(path: &BezPath) -> Vec<(Point, bool, Option<Point>, usize, usize)> {
    let mut points = Vec::new();
    let mut last = Point::ORIGIN;
    for (i, el) in path.elements().iter().enumerate() {
        match *el {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => {
                points.push((p, true, None, i, 0));
                last = p;
            }
            PathEl::QuadTo(c, p) => {
                points.push((c, false, Some(last), i, 0));
                points.push((p, true, None, i, 1));
                last = p;
            }
            PathEl::CurveTo(c1, c2, p) => {
                points.push((c1, false, Some(last), i, 0));
                points.push((c2, false, Some(p), i, 1));
                points.push((p, true, None, i, 2));
                last = p;
            }
            PathEl::ClosePath => {}
        }
    }
    points
}

fn element_point(el: &mut PathEl, point: usize) -> Option<&mut Point> {
    match el {
        PathEl::MoveTo(p) | PathEl::LineTo(p) => (point == 0).then_some(p),
        PathEl::QuadTo(c, p) => match point {
            0 => Some(c),
            1 => Some(p),
            _ => None,
        },
        PathEl::CurveTo(c1, c2, p) => match point {
            0 => Some(c1),
            1 => Some(c2),
            2 => Some(p),
            _ => None,
        },
        PathEl::ClosePath => None,
    }
}

fn is_anchor(el: &PathEl, point: usize) -> bool {
    matches!(
        (el, point),
        (PathEl::MoveTo(_) | PathEl::LineTo(_), 0)
            | (PathEl::QuadTo(..), 1)
            | (PathEl::CurveTo(..), 2)
    )
}

/// Moves one point of a path by `delta`. Moving an anchor also moves its control points,
/// and for closed subpaths the start anchor moves together with the matching end anchor.
fn move_path_point(original: &BezPath, element: usize, point: usize, delta: Vec2) -> BezPath {
    let mut els: Vec<PathEl> = original.elements().to_vec();
    let Some(el) = els.get(element) else {
        return original.clone();
    };
    let anchor = is_anchor(el, point);
    let anchor_pos = original.elements()[element].end_point();
    if let Some(p) = element_point(&mut els[element], point) {
        *p += delta;
    }
    if anchor {
        // Incoming control point of this anchor.
        if let PathEl::CurveTo(_, c2, _) = &mut els[element] {
            *c2 += delta;
        }
        // Outgoing control point (first control point of the next element).
        if let Some(PathEl::CurveTo(c1, _, _)) = els.get_mut(element + 1) {
            *c1 += delta;
        }
        // The start of a closed subpath usually coincides with its last anchor.
        if matches!(original.elements()[element], PathEl::MoveTo(_)) {
            let end = original.elements()[element + 1..]
                .iter()
                .position(|e| matches!(e, PathEl::MoveTo(_)))
                .map_or(els.len(), |i| element + 1 + i);
            if let Some(last) = (element + 1..end)
                .rev()
                .find(|&i| !matches!(els[i], PathEl::ClosePath))
                && original.elements()[last].end_point() == anchor_pos
                && last != element
            {
                let last_point = match els[last] {
                    PathEl::CurveTo(..) => 2,
                    PathEl::QuadTo(..) => 1,
                    _ => 0,
                };
                if let Some(p) = element_point(&mut els[last], last_point) {
                    *p += delta;
                }
                if let PathEl::CurveTo(_, c2, _) = &mut els[last] {
                    *c2 += delta;
                }
            }
        }
    }
    BezPath::from_vec(els)
}

impl Editor {
    /// Handles pointer input on the canvas and returns the cursor to show.
    pub fn pointer(&mut self, event: PointerEvent, ctx: ToolContext) -> CursorHint {
        match event {
            PointerEvent::Hover { pos } => self.hover(pos, ctx),
            PointerEvent::Press { pos, double } => {
                self.press(pos, double, ctx);
                CursorHint::Default
            }
            PointerEvent::Drag { pos } => self.drag(pos, ctx),
            PointerEvent::Release { pos } => {
                self.release(pos, ctx);
                CursorHint::Default
            }
        }
    }

    fn hover(&mut self, pos: Point, ctx: ToolContext) -> CursorHint {
        self.interaction.hover = None;
        match self.tool {
            Tool::Select => {}
            Tool::Text => return CursorHint::Text,
            _ => return CursorHint::Crosshair,
        }
        if self.interaction.point_edit.is_some() {
            if self.edit_point_at(pos, ctx).is_some() {
                return CursorHint::Point;
            }
        } else if let Some(hit) = self.handle_or_label_at(pos, ctx) {
            return hit;
        }
        if let Some(frame) = self.frame_label_at(pos) {
            self.interaction.hover = Some(frame);
            return CursorHint::Default;
        }
        let page = self.doc.page(self.page);
        let chain =
            page.and_then(|p| query::hit_test(p, pos, ctx.pixel * 2.0, self.scene.text_engine()));
        if let Some(chain) = chain {
            self.interaction.hover = self.pick_from_chain(&chain);
        }
        CursorHint::Default
    }

    /// The node of a hit chain a click acts on: the deepest already-selected node, or the
    /// top-level node.
    ///
    /// Top-level frames are boards rather than layers: a click inside one acts on its content.
    /// Only an empty frame, or one that is already selected, is picked by clicking its empty
    /// area; otherwise that area picks nothing, so a drag there draws a selection rectangle.
    fn pick_from_chain(&self, chain: &[NodeId]) -> Option<NodeId> {
        let board = self.doc.node(chain[0]).filter(|n| n.is_frame());
        if let (Some(frame), [_]) = (board, chain) {
            let pick = frame.children().is_empty() || self.is_selected(frame.id);
            return pick.then_some(frame.id);
        }
        let first = usize::from(board.is_some());
        let picked = chain[first..]
            .iter()
            .rev()
            .find(|id| self.is_selected(**id))
            .copied()
            .or_else(|| {
                // Siblings of a selected node inside a group are picked at the same depth.
                let depth = self
                    .selection
                    .iter()
                    .filter_map(|s| self.doc.locate(*s))
                    .filter(|loc| loc.parent.is_some())
                    .find_map(|loc| chain.iter().position(|id| Some(*id) == loc.parent));
                depth.and_then(|d| chain.get(d + 1).copied())
            })
            .unwrap_or(chain[first]);
        Some(picked)
    }

    /// The layer a click at `pos` would act on (the same rules as the select tool), or
    /// `None` over empty canvas.
    pub fn layer_at(&mut self, pos: Point, pixel: f64) -> Option<NodeId> {
        if let Some(frame) = self.frame_label_at(pos) {
            return Some(frame);
        }
        let page = self.doc.page(self.page)?;
        let chain = query::hit_test(page, pos, pixel * 2.0, self.scene.text_engine())?;
        self.pick_from_chain(&chain)
    }

    /// Top-level frame whose name label is under `pos`.
    fn frame_label_at(&self, pos: Point) -> Option<NodeId> {
        self.interaction
            .frame_labels
            .iter()
            .rev()
            .find(|(_, rect)| rect.contains(pos))
            .map(|(id, _)| *id)
    }

    /// Selection handle under `pos`. A frame label wins over the rotate area around the
    /// corners, which it overlaps, but not over a resize handle.
    fn handle_or_label_at(&mut self, pos: Point, ctx: ToolContext) -> Option<CursorHint> {
        let hint = self.selection_handle_at(pos, ctx)?;
        let resize = matches!(hint, CursorHint::Resize(_));
        (resize || self.frame_label_at(pos).is_none()).then_some(hint)
    }

    /// Resize or rotate handle of the selection under `pos`.
    fn selection_handle_at(&mut self, pos: Point, ctx: ToolContext) -> Option<CursorHint> {
        let bounds = self.selection_bounds()?;
        let reach = HANDLE_REACH * ctx.pixel;
        for handle in Handle::ALL {
            if (handle.position(bounds) - pos).hypot() <= reach {
                return Some(CursorHint::Resize(handle));
            }
        }
        let rotate = ROTATE_REACH * ctx.pixel;
        let outside = !bounds.inflate(reach, reach).contains(pos);
        if outside
            && Handle::ALL
                .iter()
                .filter(|h| h.is_corner())
                .any(|h| (h.position(bounds) - pos).hypot() <= rotate)
        {
            return Some(CursorHint::Rotate);
        }
        None
    }

    fn press(&mut self, pos: Point, double: bool, ctx: ToolContext) {
        if self.interaction.text_edit.is_some() {
            self.end_text_edit();
        }
        match self.tool {
            Tool::Select => self.press_select(pos, double, ctx),
            Tool::Pen => self.press_pen(pos, double, ctx),
            Tool::Text => self.create_text(pos, ctx.default_text_color),
            tool => {
                let key = self.new_merge_key();
                self.interaction.drag = Some(Drag::Create {
                    tool,
                    start: pos,
                    key,
                    node: None,
                });
            }
        }
    }

    fn press_select(&mut self, pos: Point, double: bool, ctx: ToolContext) {
        // Point editing: grab a point, or leave the mode when clicking elsewhere.
        if let Some(id) = self.interaction.point_edit {
            if let Some((element, point)) = self.edit_point_at(pos, ctx) {
                let key = self.new_merge_key();
                let original = self.node_path(id).unwrap_or_default();
                let to_local = self
                    .node_outline(id)
                    .map_or(Affine::IDENTITY, |(_, t)| t.inverse());
                self.interaction.drag = Some(Drag::EditPoint {
                    id,
                    key,
                    element,
                    point,
                    start: pos,
                    original,
                    to_local,
                });
                return;
            }
            self.interaction.point_edit = None;
        }

        if let Some(hint) = self.handle_or_label_at(pos, ctx) {
            let key = self.new_merge_key();
            match hint {
                CursorHint::Resize(handle) => {
                    let start_bounds = self.selection_bounds().unwrap_or_default();
                    let items = self
                        .selection
                        .clone()
                        .into_iter()
                        .filter_map(|id| {
                            Some((self.doc.node(id)?.clone(), self.parent_transform(id)))
                        })
                        .collect();
                    self.interaction.drag = Some(Drag::Resize {
                        handle,
                        key,
                        start_bounds,
                        items,
                    });
                }
                CursorHint::Rotate => {
                    let center = self.selection_bounds().unwrap_or_default().center();
                    let items = self.transform_items();
                    self.interaction.drag = Some(Drag::Rotate {
                        key,
                        center,
                        start_angle: (pos - center).atan2(),
                        items,
                    });
                }
                _ => {}
            }
            return;
        }

        // Clicking or dragging a frame's name selects or moves the frame.
        if let Some(frame) = self.frame_label_at(pos) {
            self.press_target(pos, frame, ctx);
            return;
        }

        let page = self.doc.page(self.page);
        let chain =
            page.and_then(|p| query::hit_test(p, pos, ctx.pixel * 2.0, self.scene.text_engine()));
        let Some(chain) = chain else {
            if !ctx.modifiers.shift {
                self.selection.clear();
            }
            self.interaction.drag = Some(Drag::Pending {
                start: pos,
                target: None,
                was_selected: false,
            });
            return;
        };

        let Some(target) = self.pick_from_chain(&chain) else {
            // The empty area of a frame with content: start a selection rectangle there.
            if !ctx.modifiers.shift {
                self.selection.clear();
            }
            self.interaction.drag = Some(Drag::Pending {
                start: pos,
                target: None,
                was_selected: false,
            });
            return;
        };
        if double {
            self.double_click(&chain, target);
            return;
        }
        self.press_target(pos, target, ctx);
    }

    /// Selects the pressed node (or toggles it with Shift) and waits to see whether the press
    /// becomes a click or a move.
    fn press_target(&mut self, pos: Point, target: NodeId, ctx: ToolContext) {
        let was_selected = self.is_selected(target);
        if ctx.modifiers.shift {
            self.toggle_selected(target);
        } else if !was_selected {
            self.select_only(target);
        }
        self.interaction.drag = Some(Drag::Pending {
            start: pos,
            target: Some(target),
            was_selected,
        });
    }

    /// Double click: enter a group, edit text, or edit the points of a shape.
    fn double_click(&mut self, chain: &[NodeId], target: NodeId) {
        let depth = chain.iter().position(|id| *id == target).unwrap_or(0);
        let Some(node) = self.doc.node(target) else {
            return;
        };
        if node.is_container() {
            if let Some(inner) = chain.get(depth + 1) {
                self.select_only(*inner);
            }
            return;
        }
        let (is_text, is_shape) = (
            matches!(node.kind, NodeKind::Text(_)),
            matches!(node.kind, NodeKind::Shape(_)),
        );
        self.select_only(target);
        if is_text {
            self.start_text_edit(target, false);
        } else if is_shape {
            self.start_point_edit(target);
        }
    }

    fn transform_items(&self) -> Vec<(NodeId, Affine, Affine)> {
        self.selection
            .iter()
            .filter_map(|id| {
                Some((
                    *id,
                    self.doc.node(*id)?.transform,
                    self.parent_transform(*id),
                ))
            })
            .collect()
    }

    fn drag(&mut self, pos: Point, ctx: ToolContext) -> CursorHint {
        let Some(drag) = self.interaction.drag.take() else {
            return CursorHint::Default;
        };
        let (drag, cursor) = self.continue_drag(drag, pos, ctx);
        self.interaction.drag = drag;
        cursor
    }

    fn continue_drag(
        &mut self,
        drag: Drag,
        pos: Point,
        ctx: ToolContext,
    ) -> (Option<Drag>, CursorHint) {
        match drag {
            Drag::Pending {
                start,
                target,
                was_selected,
            } => {
                if (pos - start).hypot() < DRAG_THRESHOLD * ctx.pixel {
                    return (
                        Some(Drag::Pending {
                            start,
                            target,
                            was_selected,
                        }),
                        CursorHint::Default,
                    );
                }
                let next = if target.is_some() && !self.selection.is_empty() {
                    let items = self.transform_items();
                    let bounds = self.selection_bounds().unwrap_or_default();
                    let snap = self.snap_targets();
                    Drag::Move {
                        start,
                        key: self.new_merge_key(),
                        items,
                        bounds,
                        snap,
                    }
                } else {
                    Drag::Marquee {
                        start,
                        base: self.selection.clone(),
                        scope: self.top_frame_at(start, &[]),
                    }
                };
                self.continue_drag(next, pos, ctx)
            }
            Drag::Marquee { start, base, scope } => {
                let rect = Rect::from_points(start, pos);
                self.interaction.marquee = Some(rect);
                let mut selection = base.clone();
                for id in self.nodes_in_rect(rect, scope) {
                    if !selection.contains(&id) {
                        selection.push(id);
                    }
                }
                self.selection = selection;
                (
                    Some(Drag::Marquee { start, base, scope }),
                    CursorHint::Default,
                )
            }
            Drag::Move {
                start,
                key,
                items,
                bounds,
                snap,
            } => {
                let mut delta = pos - start;
                if ctx.modifiers.shift {
                    if delta.x.abs() > delta.y.abs() {
                        delta.y = 0.0;
                    } else {
                        delta.x = 0.0;
                    }
                }
                self.interaction.guides.clear();
                if !ctx.modifiers.command {
                    delta = self.snap_delta(bounds, delta, &snap, ctx.pixel);
                }
                let commands = items
                    .iter()
                    .map(|(id, transform, parent)| Command::SetTransform {
                        id: *id,
                        transform: apply_in_page_space(
                            *parent,
                            *transform,
                            Affine::translate(delta),
                        ),
                    })
                    .collect();
                self.apply_merged(Command::Batch(commands), key);
                (
                    Some(Drag::Move {
                        start,
                        key,
                        items,
                        bounds,
                        snap,
                    }),
                    CursorHint::Move,
                )
            }
            Drag::Resize {
                handle,
                key,
                start_bounds,
                items,
            } => {
                let target = resize_rect(start_bounds, handle, pos, ctx.modifiers);
                let m = rect_to_rect(start_bounds, target);
                let text = self.scene.text_engine();
                let commands = items
                    .iter()
                    .map(|(node, parent)| {
                        let mut node = resized(node, *parent, m, text);
                        if let Some(children) = node.children_mut() {
                            children.clear();
                        }
                        Command::UpdateNode { node }
                    })
                    .collect();
                self.apply_merged(Command::Batch(commands), key);
                (
                    Some(Drag::Resize {
                        handle,
                        key,
                        start_bounds,
                        items,
                    }),
                    CursorHint::Resize(handle),
                )
            }
            Drag::Rotate {
                key,
                center,
                start_angle,
                items,
            } => {
                let mut angle = (pos - center).atan2() - start_angle;
                if ctx.modifiers.shift {
                    let step = PI / 12.0;
                    angle = (angle / step).round() * step;
                }
                let m = rotate_about(center, angle);
                let commands = items
                    .iter()
                    .map(|(id, transform, parent)| Command::SetTransform {
                        id: *id,
                        transform: apply_in_page_space(*parent, *transform, m),
                    })
                    .collect();
                self.apply_merged(Command::Batch(commands), key);
                (
                    Some(Drag::Rotate {
                        key,
                        center,
                        start_angle,
                        items,
                    }),
                    CursorHint::Rotate,
                )
            }
            Drag::Create {
                tool,
                start,
                key,
                node,
            } => {
                if node.is_none() && (pos - start).hypot() < DRAG_THRESHOLD * ctx.pixel {
                    return (
                        Some(Drag::Create {
                            tool,
                            start,
                            key,
                            node,
                        }),
                        CursorHint::Crosshair,
                    );
                }
                let id = self.place_shape(tool, start, pos, ctx.modifiers, key, node);
                (
                    Some(Drag::Create {
                        tool,
                        start,
                        key,
                        node: id,
                    }),
                    CursorHint::Crosshair,
                )
            }
            Drag::PenHandle => {
                if let Some(pen) = &mut self.interaction.pen
                    && let Some(last) = pen.anchors.last_mut()
                    && (pos - last.pos).hypot() >= DRAG_THRESHOLD * ctx.pixel
                {
                    last.handle = Some(pos);
                }
                self.update_pen_node();
                (Some(Drag::PenHandle), CursorHint::Crosshair)
            }
            Drag::EditPoint {
                id,
                key,
                element,
                point,
                start,
                original,
                to_local,
            } => {
                let local_delta = to_local * pos - to_local * start;
                let path = move_path_point(&original, element, point, local_delta);
                self.update_node_merged(id, key, |node| {
                    if let NodeKind::Shape(shape) = &mut node.kind {
                        shape.geometry = Geometry::Path { path };
                    }
                });
                (
                    Some(Drag::EditPoint {
                        id,
                        key,
                        element,
                        point,
                        start,
                        original,
                        to_local,
                    }),
                    CursorHint::Point,
                )
            }
        }
    }

    fn release(&mut self, pos: Point, ctx: ToolContext) {
        let drag = self.interaction.drag.take();
        self.interaction.marquee = None;
        self.interaction.guides.clear();
        if let Some(Drag::Move { key, .. }) = &drag {
            self.reparent_moved(*key);
        }
        // A path being drawn or a new text stays one undo step across clicks and typing.
        if self.interaction.pen.is_none() && self.interaction.text_edit.is_none() {
            self.history.end_merge();
        }
        match drag {
            Some(Drag::Pending {
                target: Some(target),
                was_selected: true,
                ..
            }) if !ctx.modifiers.shift => {
                // Clicking one node of a multiple selection selects just that node.
                self.select_only(target);
            }
            Some(Drag::Create {
                tool, key, node, ..
            }) => {
                let id = match node {
                    Some(id) => Some(id),
                    None => {
                        // A click: create a default-sized shape centred on the pointer.
                        let half = DEFAULT_SIZE / 2.0;
                        let (a, b) = if tool == Tool::Line {
                            (pos - Vec2::new(half, 0.0), pos + Vec2::new(half, 0.0))
                        } else {
                            (pos - Vec2::new(half, half), pos + Vec2::new(half, half))
                        };
                        self.place_shape(tool, a, b, Modifiers::default(), key, None)
                    }
                };
                self.history.end_merge();
                if let Some(id) = id {
                    self.select_only(id);
                }
                self.tool = Tool::Select;
            }
            Some(Drag::PenHandle) => {
                // Keep drawing: the next press adds another anchor.
            }
            _ => {}
        }
    }

    /// Creates or updates the shape being drawn from `a` to `b`.
    fn place_shape(
        &mut self,
        tool: Tool,
        a: Point,
        b: Point,
        modifiers: Modifiers,
        key: u64,
        existing: Option<NodeId>,
    ) -> Option<NodeId> {
        if tool == Tool::Frame {
            return self.place_frame(a, drag_rect(a, b, modifiers), key, existing);
        }
        let (geometry, origin) = if tool == Tool::Line {
            let mut v = b - a;
            if modifiers.shift {
                v = snap_45(v);
            }
            let (start, end) = if modifiers.alt {
                (a - v, a + v)
            } else {
                (a, a + v)
            };
            (
                Geometry::Line {
                    end: (end - start).to_point(),
                },
                start,
            )
        } else {
            let rect = drag_rect(a, b, modifiers);
            (shape_geometry(tool, rect.size()), rect.origin())
        };
        let transform = Affine::translate(origin.to_vec2());
        match existing {
            Some(id) => {
                let transform = self.parent_transform(id).inverse() * transform;
                self.update_node_merged(id, key, |node| {
                    node.transform = transform;
                    if let NodeKind::Shape(shape) = &mut node.kind {
                        shape.geometry = geometry;
                    }
                });
                Some(id)
            }
            None => {
                let node = Node::shape(geometry);
                let node = if tool == Tool::Line {
                    node.with_stroke(Stroke::new(Color::BLACK, 2.0))
                } else {
                    node.with_fill(DEFAULT_FILL)
                };
                self.insert_drawn(node, transform, a, key)
            }
        }
    }

    /// Creates or updates the frame being drawn over `rect`.
    fn place_frame(
        &mut self,
        anchor: Point,
        rect: Rect,
        key: u64,
        existing: Option<NodeId>,
    ) -> Option<NodeId> {
        let transform = Affine::translate(rect.origin().to_vec2());
        if let Some(id) = existing {
            let transform = self.parent_transform(id).inverse() * transform;
            self.update_node_merged(id, key, |node| {
                node.transform = transform;
                if let NodeKind::Frame(frame) = &mut node.kind {
                    frame.size = rect.size();
                }
            });
            return Some(id);
        }
        let name = self.next_frame_name();
        let node = Node::frame(name, rect.width(), rect.height());
        self.insert_drawn(node, transform, anchor, key)
    }

    /// Where a node drawn starting at `pos` goes: on top inside the top-level frame under
    /// `pos`, or on top of the page. Also returns the page transform of that parent.
    fn insertion_point(&self, pos: Point) -> (NodeLocation, Affine) {
        let page = self.current_page();
        let frame = self.top_frame_at(pos, &[]).and_then(|id| page.find(id));
        match frame {
            Some(frame) => (
                NodeLocation {
                    page: page.id,
                    parent: Some(frame.id),
                    index: frame.children().len(),
                },
                frame.transform,
            ),
            None => (
                NodeLocation::top_level(page.id, page.children.len()),
                Affine::IDENTITY,
            ),
        }
    }

    /// Inserts a node being drawn at `anchor`, with `transform` in page space, as part of the
    /// merged edit `key`. Returns its id.
    fn insert_drawn(
        &mut self,
        mut node: Node,
        transform: Affine,
        anchor: Point,
        key: u64,
    ) -> Option<NodeId> {
        let (location, parent) = self.insertion_point(anchor);
        node.transform = parent.inverse() * transform;
        let id = node.id;
        self.apply_merged(Command::InsertNode { location, node }, key)
            .then_some(id)
    }

    // ---- Frames -------------------------------------------------------------------------

    /// Front-most visible, unlocked top-level frame whose area contains `pos`, skipping the
    /// ids in `except`.
    pub(super) fn top_frame_at(&self, pos: Point, except: &[NodeId]) -> Option<NodeId> {
        self.current_page()
            .children
            .iter()
            .rev()
            .filter(|n| n.visible && !n.locked && !except.contains(&n.id))
            .find(|n| match &n.kind {
                NodeKind::Frame(frame) => {
                    n.transform.determinant().abs() > 1e-12
                        && frame.size.to_rect().contains(n.transform.inverse() * pos)
                }
                _ => false,
            })
            .map(|n| n.id)
    }

    /// After a move, puts each moved node into the top-level frame its centre fell in, or
    /// onto the page if it left every frame, keeping its place on the page. Only nodes on the
    /// page or directly in a top-level frame change parent (top-level frames stay on the
    /// page, and nodes in groups stay in their group). Merged into the move's undo step.
    fn reparent_moved(&mut self, key: u64) {
        for id in self.selection.clone() {
            let Some(location) = self.doc.locate(id) else {
                continue;
            };
            let page = self.doc.page(self.page).expect("current page");
            let movable = match location.parent {
                None => page.find(id).is_some_and(|n| !n.is_frame()),
                Some(parent) => page.children.iter().any(|n| n.id == parent && n.is_frame()),
            };
            if !movable {
                continue;
            }
            let (Some(bounds), Some(transform)) = (
                query::page_bounds(page, id, self.scene.text_engine()),
                query::page_transform(page, id),
            ) else {
                continue;
            };
            let target = self.top_frame_at(bounds.center(), &[id]);
            if target == location.parent {
                continue;
            }
            let page = self.current_page();
            let (to, parent) = match target.and_then(|t| page.find(t)) {
                Some(frame) => (
                    NodeLocation {
                        page: page.id,
                        parent: Some(frame.id),
                        index: frame.children().len(),
                    },
                    frame.transform,
                ),
                None => {
                    // Just in front of the frame it left.
                    let index = location
                        .parent
                        .and_then(|p| page.children.iter().position(|n| n.id == p))
                        .map_or(page.children.len(), |i| i + 1);
                    (NodeLocation::top_level(page.id, index), Affine::IDENTITY)
                }
            };
            let commands = vec![
                Command::MoveNode { id, to },
                Command::SetTransform {
                    id,
                    transform: parent.inverse() * transform,
                },
            ];
            self.apply_merged(Command::Batch(commands), key);
        }
    }

    /// Nodes a selection rectangle covers: top-level nodes, or the children of the frame
    /// `scope`. Hidden and locked nodes are skipped.
    fn nodes_in_rect(&mut self, rect: Rect, scope: Option<NodeId>) -> Vec<NodeId> {
        let page = self.doc.page(self.page).expect("current page");
        let text = self.scene.text_engine();
        let Some(frame) = scope.and_then(|id| page.find(id)) else {
            return query::nodes_in_rect(page, rect, text);
        };
        frame
            .children()
            .iter()
            .filter(|n| n.visible && !n.locked)
            .filter(|n| {
                query::page_bounds(page, n.id, text).is_some_and(|b| {
                    b.x0 <= rect.x1 && b.x1 >= rect.x0 && b.y0 <= rect.y1 && b.y1 >= rect.y0
                })
            })
            .map(|n| n.id)
            .collect()
    }

    // ---- Snapping -----------------------------------------------------------------------

    /// Bounds the selection can snap to: the visible top-level nodes (frames included) and
    /// the siblings of the selected nodes inside their frame or group.
    fn snap_targets(&mut self) -> Vec<Rect> {
        let Some(page) = self.doc.page(self.page) else {
            return vec![];
        };
        let selected: Vec<NodeId> = self.selection.clone();
        let mut candidates: Vec<NodeId> = page.children.iter().map(|n| n.id).collect();
        for id in &selected {
            if let Some(parent) = self.doc.locate(*id).and_then(|loc| loc.parent)
                && let Some(parent) = page.find(parent)
            {
                candidates.extend(parent.children().iter().map(|n| n.id));
            }
        }
        candidates.sort();
        candidates.dedup();
        let text = self.scene.text_engine();
        candidates
            .into_iter()
            .filter(|id| !selected.contains(id))
            .filter(|id| page.find(*id).is_some_and(|n| n.visible))
            .filter_map(|id| query::page_bounds(page, id, text))
            .collect()
    }

    fn snap_delta(&mut self, bounds: Rect, delta: Vec2, targets: &[Rect], pixel: f64) -> Vec2 {
        let moved = bounds + delta;
        let reach = SNAP_DISTANCE * pixel;
        // The closest (distance, line, target) for the three edges of one axis.
        let best = |edges: [f64; 3], lines: &dyn Fn(&Rect) -> [f64; 3]| {
            let mut best: Option<(f64, f64, Rect)> = None;
            for target in targets {
                for line in lines(target) {
                    for edge in edges {
                        let d = line - edge;
                        if d.abs() <= reach && best.is_none_or(|(bd, ..)| d.abs() < bd.abs()) {
                            best = Some((d, line, *target));
                        }
                    }
                }
            }
            best
        };
        let mut delta = delta;
        let xs = |r: &Rect| [r.x0, r.center().x, r.x1];
        let ys = |r: &Rect| [r.y0, r.center().y, r.y1];
        if let Some((d, x, target)) = best(xs(&moved), &xs) {
            delta.x += d;
            self.interaction.guides.push(Guide {
                vertical: true,
                position: x,
                from: target.y0.min(moved.y0),
                to: target.y1.max(moved.y1),
            });
        }
        if let Some((d, y, target)) = best(ys(&moved), &ys) {
            delta.y += d;
            self.interaction.guides.push(Guide {
                vertical: false,
                position: y,
                from: target.x0.min(moved.x0),
                to: target.x1.max(moved.x1),
            });
        }
        delta
    }

    // ---- Text ---------------------------------------------------------------------------

    fn create_text(&mut self, pos: Point, color: Color) {
        let style = TextStyle {
            font_size: 32.0,
            color,
            ..TextStyle::default()
        };
        let node = Node::text("", style);
        let key = self.new_merge_key();
        if let Some(id) = self.insert_drawn(node, Affine::translate(pos.to_vec2()), pos, key) {
            self.select_only(id);
            self.interaction.text_edit = Some(TextEditState {
                id,
                key,
                created: true,
            });
        }
        self.tool = Tool::Select;
    }

    /// Starts editing a text node in place.
    pub fn start_text_edit(&mut self, id: NodeId, created: bool) {
        let key = self.new_merge_key();
        self.interaction.text_edit = Some(TextEditState { id, key, created });
        self.revision += 1;
    }

    /// Content of the text being edited.
    pub fn text_edit_content(&self) -> Option<(NodeId, String)> {
        let id = self.interaction.text_edit?.id;
        match &self.doc.node(id)?.kind {
            NodeKind::Text(t) => Some((id, t.content.clone())),
            _ => None,
        }
    }

    /// Replaces the content of the text being edited (merged into one undo step).
    pub fn set_text_edit_content(&mut self, content: String) {
        let Some(edit) = self.interaction.text_edit else {
            return;
        };
        self.update_node_merged(edit.id, edit.key, |node| {
            if let NodeKind::Text(t) = &mut node.kind {
                t.content = content;
            }
        });
    }

    /// Finishes in-place text editing. A new text left empty is removed.
    pub fn end_text_edit(&mut self) {
        let Some(edit) = self.interaction.text_edit.take() else {
            return;
        };
        let empty = matches!(
            self.doc.node(edit.id).map(|n| &n.kind),
            Some(NodeKind::Text(t)) if t.content.trim().is_empty()
        );
        if empty && edit.created {
            let _ = self.history.discard(&mut self.doc);
            self.selection.retain(|s| *s != edit.id);
        } else {
            self.history.end_merge();
        }
        self.revision += 1;
    }

    // ---- Pen ----------------------------------------------------------------------------

    fn press_pen(&mut self, pos: Point, double: bool, ctx: ToolContext) {
        let reach = HANDLE_REACH * 1.5 * ctx.pixel;
        if let Some(pen) = &mut self.interaction.pen {
            if double {
                // The first click of the double click already added an anchor.
                self.finish_path();
                return;
            }
            let closes = pen.anchors.len() >= 2 && (pen.anchors[0].pos - pos).hypot() <= reach;
            if closes {
                pen.closed = true;
                self.update_pen_node();
                self.finish_path();
                return;
            }
            pen.anchors.push(PenAnchor { pos, handle: None });
            self.update_pen_node();
        } else {
            let mut path = BezPath::new();
            path.move_to(pos);
            let node =
                Node::shape(Geometry::Path { path }).with_stroke(Stroke::new(Color::BLACK, 2.0));
            let key = self.new_merge_key();
            // The path is kept in page space; the transform undoes the parent's.
            let Some(id) = self.insert_drawn(node, Affine::IDENTITY, pos, key) else {
                return;
            };
            self.select_only(id);
            self.interaction.pen = Some(PenState {
                id,
                key,
                anchors: vec![PenAnchor { pos, handle: None }],
                closed: false,
            });
        }
        self.interaction.drag = Some(Drag::PenHandle);
    }

    fn update_pen_node(&mut self) {
        let Some(pen) = &self.interaction.pen else {
            return;
        };
        let (id, key) = (pen.id, pen.key);
        let path = pen_path(&pen.anchors, pen.closed);
        self.update_node_merged(id, key, |node| {
            if let NodeKind::Shape(shape) = &mut node.kind {
                shape.geometry = Geometry::Path { path };
            }
        });
    }

    /// Anchors of the path being drawn, with their control points, for the overlay.
    pub fn pen_anchors(&self) -> Vec<(Point, Option<Point>)> {
        self.interaction
            .pen
            .as_ref()
            .map(|p| p.anchors.iter().map(|a| (a.pos, a.handle)).collect())
            .unwrap_or_default()
    }

    /// Finishes the path being drawn. Paths with a single point are dropped.
    pub fn finish_path(&mut self) {
        let Some(pen) = self.interaction.pen.take() else {
            return;
        };
        self.interaction.drag = None;
        if pen.anchors.len() < 2 {
            let _ = self.history.discard(&mut self.doc);
            self.selection.retain(|s| *s != pen.id);
            self.revision += 1;
        } else {
            let (path, offset) = normalize_path(&pen_path(&pen.anchors, pen.closed));
            let closed = pen.closed;
            let transform = self.parent_transform(pen.id).inverse() * Affine::translate(offset);
            self.update_node_merged(pen.id, pen.key, |node| {
                node.transform = transform;
                if let NodeKind::Shape(shape) = &mut node.kind {
                    shape.geometry = Geometry::Path { path };
                    if closed {
                        shape.fill = Some(DEFAULT_FILL.into());
                    }
                }
            });
            self.history.end_merge();
            self.select_only(pen.id);
        }
        self.tool = Tool::Select;
    }

    // ---- Point editing ------------------------------------------------------------------

    /// Starts editing the points of a shape, converting it to a path first if needed.
    pub fn start_point_edit(&mut self, id: NodeId) {
        let Some(node) = self.doc.node(id) else {
            return;
        };
        let NodeKind::Shape(shape) = &node.kind else {
            return;
        };
        if !matches!(shape.geometry, Geometry::Path { .. }) {
            let path = shape.geometry.to_path();
            self.update_node(id, |node| {
                if let NodeKind::Shape(shape) = &mut node.kind {
                    shape.geometry = Geometry::Path { path };
                }
            });
        }
        self.interaction.point_edit = Some(id);
    }

    pub fn end_point_edit(&mut self) {
        self.interaction.point_edit = None;
    }

    fn node_path(&self, id: NodeId) -> Option<BezPath> {
        match &self.doc.node(id)?.kind {
            NodeKind::Shape(shape) => match &shape.geometry {
                Geometry::Path { path } => Some(path.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Points of the path being edited, in page space.
    pub fn edit_points(&mut self) -> Vec<EditPoint> {
        let Some(id) = self.interaction.point_edit else {
            return vec![];
        };
        let Some(path) = self.node_path(id) else {
            return vec![];
        };
        let Some((_, transform)) = self.node_outline(id) else {
            return vec![];
        };
        path_points(&path)
            .into_iter()
            .map(|(p, anchor, owner, element, point)| EditPoint {
                pos: transform * p,
                anchor,
                owner: owner.map(|o| transform * o),
                element,
                point,
            })
            .collect()
    }

    fn edit_point_at(&mut self, pos: Point, ctx: ToolContext) -> Option<(usize, usize)> {
        let reach = HANDLE_REACH * ctx.pixel;
        let points = self.edit_points();
        // Control points first: they sit on top of anchors when a curve has no handles.
        points
            .iter()
            .filter(|p| !p.anchor)
            .chain(points.iter().filter(|p| p.anchor))
            .find(|p| (p.pos - pos).hypot() <= reach)
            .map(|p| (p.element, p.point))
    }

    /// Escape: finish the path, leave text or point editing, or clear the selection.
    pub fn escape(&mut self) {
        if self.interaction.pen.is_some() {
            self.finish_path();
        } else if self.interaction.text_edit.is_some() {
            self.end_text_edit();
        } else if self.interaction.point_edit.is_some() {
            self.end_point_edit();
        } else if self.tool != Tool::Select {
            self.tool = Tool::Select;
        } else {
            self.selection.clear();
        }
    }

    /// Changes the active tool, finishing whatever was in progress.
    pub fn set_tool(&mut self, tool: Tool) {
        if self.interaction.pen.is_some() {
            self.finish_path();
        }
        self.end_text_edit();
        self.end_point_edit();
        if tool.is_shape() {
            self.shape_tool = tool;
        }
        self.tool = tool;
    }
}

fn resize_rect(start: Rect, handle: Handle, pos: Point, modifiers: Modifiers) -> Rect {
    let mut r = start;
    if handle.moves_left() {
        r.x0 = pos.x;
    }
    if handle.moves_right() {
        r.x1 = pos.x;
    }
    if handle.moves_top() {
        r.y0 = pos.y;
    }
    if handle.moves_bottom() {
        r.y1 = pos.y;
    }
    if modifiers.shift && handle.is_corner() && start.width() > 0.0 && start.height() > 0.0 {
        // Keep the aspect ratio, following whichever side moved more.
        let aspect = start.width() / start.height();
        let (w, h) = (r.x1 - r.x0, r.y1 - r.y0);
        let sign = |v: f64| if v < 0.0 { -1.0 } else { 1.0 };
        if (w / aspect).abs() > h.abs() {
            let new_h = (w / aspect).abs() * sign(h);
            if handle.moves_top() {
                r.y0 = r.y1 - new_h;
            } else {
                r.y1 = r.y0 + new_h;
            }
        } else {
            let new_w = (h * aspect).abs() * sign(w);
            if handle.moves_left() {
                r.x0 = r.x1 - new_w;
            } else {
                r.x1 = r.x0 + new_w;
            }
        }
    }
    if modifiers.alt {
        let c = start.center();
        let half = Vec2::new(
            if handle.moves_left() {
                c.x - r.x0
            } else if handle.moves_right() {
                r.x1 - c.x
            } else {
                start.width() / 2.0
            },
            if handle.moves_top() {
                c.y - r.y0
            } else if handle.moves_bottom() {
                r.y1 - c.y
            } else {
                start.height() / 2.0
            },
        );
        r = Rect::from_points(c - half, c + half);
    }
    let r = r.abs();
    // Keep a minimum size so the transform stays invertible.
    Rect::from_origin_size(r.origin(), (r.width().max(0.5), r.height().max(0.5)))
}
