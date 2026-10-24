//! Editing operations on the selection, pages and layers.

use libforma::boolean::{self, BooleanOp};
use libforma::geom::{Affine, Point, Vec2};
use libforma::history::Command;
use libforma::model::{Geometry, Node, NodeKind, NodeLocation, Page};
use libforma::{NodeId, PageId, query};
use serde::{Deserialize, Serialize};

use super::Editor;
use super::geometry::{apply_in_page_space, rect_to_rect, resized, rotate_about};
use crate::i18n::{tr, trf};
use libforma::geom::Rect;

/// Alignment of the selection: to each other, or to the page for a single node.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Left,
    CenterX,
    Right,
    Top,
    CenterY,
    Bottom,
}

/// Direction for distributing layers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// Prefix that marks Forma data on the system clipboard.
const CLIPBOARD_TAG: &str = "forma-nodes:";

/// Nodes copied to the clipboard, with the assets they use.
#[derive(Serialize, Deserialize)]
pub struct ClipboardPayload {
    nodes: Vec<Node>,
    /// Images used by the nodes, as (media type, bytes).
    assets: Vec<(String, Vec<u8>)>,
}

/// Name shown for a node without a name.
pub fn default_name(node: &Node) -> String {
    if !node.name.is_empty() {
        return node.name.clone();
    }
    match &node.kind {
        NodeKind::Group { .. } => tr("Group").into(),
        NodeKind::Frame(_) => tr("Frame").into(),
        NodeKind::Image(_) => tr("Image").into(),
        NodeKind::Text(t) => {
            let first_line = t.content.lines().next().unwrap_or("").trim();
            if first_line.is_empty() {
                tr("Text").into()
            } else {
                first_line.chars().take(40).collect()
            }
        }
        NodeKind::Shape(shape) => match shape.geometry {
            Geometry::Rect { .. } => tr("Rectangle"),
            Geometry::Ellipse { .. } => tr("Ellipse"),
            Geometry::Polygon { .. } => tr("Polygon"),
            Geometry::Star { .. } => tr("Star"),
            Geometry::Line { .. } => tr("Line"),
            Geometry::Path { .. } => tr("Vector"),
        }
        .into(),
    }
}

impl Editor {
    /// Selected ids sorted back to front by their place in the tree.
    pub(super) fn selection_in_order(&self) -> Vec<(NodeId, NodeLocation)> {
        let mut located: Vec<_> = self
            .selection
            .iter()
            .filter_map(|id| self.doc.locate(*id).map(|loc| (*id, loc)))
            .collect();
        located.sort_by_key(|(_, loc)| (loc.parent, loc.index));
        located
    }

    pub fn delete_selection(&mut self) {
        let commands: Vec<Command> = self
            .selection_in_order()
            .into_iter()
            .rev()
            .map(|(id, _)| Command::RemoveNode { id })
            .collect();
        if !commands.is_empty() && self.apply(Command::Batch(commands)) {
            self.selection.clear();
        }
    }

    /// Duplicates the selection above the originals, offset by `offset` page units.
    pub fn duplicate_selection(&mut self, offset: Vec2) {
        let mut commands = Vec::new();
        let mut new_ids = Vec::new();
        // Front to back, so earlier insertions do not shift later indices.
        for (id, loc) in self.selection_in_order().into_iter().rev() {
            let Some(node) = self.doc.node(id) else {
                continue;
            };
            let mut copy = node.duplicate();
            let parent = self.parent_transform(id);
            copy.transform = apply_in_page_space(parent, copy.transform, Affine::translate(offset));
            new_ids.push(copy.id);
            commands.push(Command::InsertNode {
                location: NodeLocation {
                    index: loc.index + 1,
                    ..loc
                },
                node: copy,
            });
        }
        if !commands.is_empty() && self.apply(Command::Batch(commands)) {
            new_ids.reverse();
            self.selection = new_ids;
        }
    }

    /// Transform from a node's parent space to page space.
    pub(super) fn parent_transform(&self, id: NodeId) -> Affine {
        let page = self.current_page();
        match (query::page_transform(page, id), page.find(id)) {
            (Some(t), Some(node)) => t * node.transform.inverse(),
            _ => Affine::IDENTITY,
        }
    }

    /// Moves the selection by `delta` page units as one undo step.
    pub fn nudge(&mut self, delta: Vec2) {
        let commands: Vec<Command> = self
            .selection
            .clone()
            .into_iter()
            .filter_map(|id| {
                let node = self.doc.node(id)?;
                let parent = self.parent_transform(id);
                Some(Command::SetTransform {
                    id,
                    transform: apply_in_page_space(
                        parent,
                        node.transform,
                        Affine::translate(delta),
                    ),
                })
            })
            .collect();
        if !commands.is_empty() {
            self.apply(Command::Batch(commands));
        }
    }

    /// Moves and resizes the selection so its bounds become `target` (page space).
    pub fn set_selection_bounds(&mut self, target: Rect, key: Option<u64>) {
        let Some(current) = self.selection_bounds() else {
            return;
        };
        let m = rect_to_rect(current, target);
        let mut commands = Vec::new();
        for id in self.selection.clone() {
            let Some(node) = self.doc.node(id).cloned() else {
                continue;
            };
            let parent = self.parent_transform(id);
            let mut node = if (current.size().to_vec2() - target.size().to_vec2()).hypot() < 1e-9 {
                let mut moved = node.clone();
                moved.transform = apply_in_page_space(parent, node.transform, m);
                moved
            } else {
                resized(&node, parent, m, self.scene.text_engine())
            };
            if let Some(children) = node.children_mut() {
                children.clear();
            }
            commands.push(Command::UpdateNode { node });
        }
        self.apply_keyed(Command::Batch(commands), key);
    }

    /// Rotation of a node in page space, in degrees.
    pub fn rotation(&self, id: NodeId) -> f64 {
        let t = query::page_transform(self.current_page(), id).unwrap_or_default();
        let [a, b, ..] = t.as_coeffs();
        b.atan2(a).to_degrees()
    }

    /// Rotates a node around its centre so its page-space rotation becomes `degrees`.
    pub fn set_rotation(&mut self, id: NodeId, degrees: f64, key: Option<u64>) {
        let Some((bounds, transform)) = self.node_outline(id) else {
            return;
        };
        let delta = (degrees - self.rotation(id)).to_radians();
        let center = transform * bounds.center();
        let Some(node) = self.doc.node(id) else {
            return;
        };
        let parent = self.parent_transform(id);
        let transform = apply_in_page_space(parent, node.transform, rotate_about(center, delta));
        self.apply_keyed(Command::SetTransform { id, transform }, key);
    }

    /// Aligns the selected nodes to each other, or a single node to its parent frame (a
    /// node outside any frame has nothing to align to).
    pub fn align_selection(&mut self, align: Align) {
        let reference = if let [id] = self.selection[..] {
            let Some(frame) = self.parent_frame(id) else {
                return;
            };
            let page = self.doc.page(self.page).expect("current page");
            let Some(bounds) = query::page_bounds(page, frame, self.scene.text_engine()) else {
                return;
            };
            bounds
        } else {
            let Some(bounds) = self.selection_bounds() else {
                return;
            };
            bounds
        };
        let mut commands = Vec::new();
        for id in self.selection.clone() {
            let page = self.doc.page(self.page).expect("current page");
            let Some(b) = query::page_bounds(page, id, self.scene.text_engine()) else {
                continue;
            };
            let delta = match align {
                Align::Left => Vec2::new(reference.x0 - b.x0, 0.0),
                Align::CenterX => Vec2::new(reference.center().x - b.center().x, 0.0),
                Align::Right => Vec2::new(reference.x1 - b.x1, 0.0),
                Align::Top => Vec2::new(0.0, reference.y0 - b.y0),
                Align::CenterY => Vec2::new(0.0, reference.center().y - b.center().y),
                Align::Bottom => Vec2::new(0.0, reference.y1 - b.y1),
            };
            let Some(node) = self.doc.node(id) else {
                continue;
            };
            let parent = self.parent_transform(id);
            commands.push(Command::SetTransform {
                id,
                transform: apply_in_page_space(parent, node.transform, Affine::translate(delta)),
            });
        }
        if !commands.is_empty() {
            self.apply(Command::Batch(commands));
        }
    }

    /// "Frame N", numbered after the frames already on the page.
    pub(super) fn next_frame_name(&self) -> String {
        let n = query::frames(self.current_page()).len() + 1;
        trf("Frame {n}", &[("n", &n.to_string())])
    }

    /// Adds a frame of `size` named `name`, centred on `center` (page space), on top of the
    /// page, and selects it.
    pub fn insert_frame(&mut self, name: &str, size: libforma::geom::Size, center: Point) -> bool {
        let origin = center - size.to_vec2() / 2.0;
        let node = Node::frame(name, size.width, size.height).at(origin.x, origin.y);
        self.set_tool(super::Tool::Select);
        self.insert_top_level(node)
    }

    /// Spaces three or more selected layers evenly along `axis`: the outermost edges stay
    /// where they are and the gaps between neighbours (ordered by their centres) become equal.
    pub fn distribute_selection(&mut self, axis: Axis) {
        if self.selection.len() < 3 {
            return;
        }
        let page = self.doc.page(self.page).expect("current page");
        let text = self.scene.text_engine();
        let mut items: Vec<(NodeId, Rect)> = self
            .selection
            .iter()
            .filter_map(|id| Some((*id, query::page_bounds(page, *id, text)?)))
            .collect();
        // (start, end) of a box along the axis.
        let span = |r: &Rect| match axis {
            Axis::Horizontal => (r.x0, r.x1),
            Axis::Vertical => (r.y0, r.y1),
        };
        items.sort_by(|a, b| {
            let (a0, a1) = span(&a.1);
            let (b0, b1) = span(&b.1);
            (a0 + a1).total_cmp(&(b0 + b1))
        });
        let start = items
            .iter()
            .map(|i| span(&i.1).0)
            .fold(f64::INFINITY, f64::min);
        let end = items
            .iter()
            .map(|i| span(&i.1).1)
            .fold(f64::NEG_INFINITY, f64::max);
        let sizes: f64 = items.iter().map(|i| span(&i.1).1 - span(&i.1).0).sum();
        let gap = (end - start - sizes) / (items.len() - 1) as f64;
        let mut position = start;
        let mut commands = Vec::new();
        for (id, bounds) in items {
            let (from, to) = span(&bounds);
            let delta = match axis {
                Axis::Horizontal => Vec2::new(position - from, 0.0),
                Axis::Vertical => Vec2::new(0.0, position - from),
            };
            position += to - from + gap;
            let Some(node) = self.doc.node(id) else {
                continue;
            };
            let parent = self.parent_transform(id);
            commands.push(Command::SetTransform {
                id,
                transform: apply_in_page_space(parent, node.transform, Affine::translate(delta)),
            });
        }
        self.apply(Command::Batch(commands));
    }

    /// The frame a node is directly inside, if any.
    fn parent_frame(&self, id: NodeId) -> Option<NodeId> {
        let parent = self.doc.locate(id)?.parent?;
        self.doc.node(parent)?.is_frame().then_some(parent)
    }

    /// Applies a command, merged under `key` when given.
    pub fn apply_keyed(&mut self, command: Command, key: Option<u64>) -> bool {
        match key {
            Some(key) => self.apply_merged(command, key),
            None => self.apply(command),
        }
    }

    /// Groups the selected nodes. They must share a parent.
    pub fn group_selection(&mut self) {
        let ordered = self.selection_in_order();
        let Some((_, first)) = ordered.first().copied() else {
            return;
        };
        if ordered
            .iter()
            .any(|(_, loc)| (loc.page, loc.parent) != (first.page, first.parent))
        {
            self.interaction
                .report(tr("Only layers at the same level can be grouped.").into());
            return;
        }
        let children: Vec<Node> = ordered
            .iter()
            .filter_map(|(id, _)| self.doc.node(*id).cloned())
            .collect();
        let group = Node::group(children);
        let group_id = group.id;
        let mut commands: Vec<Command> = ordered
            .iter()
            .rev()
            .map(|(id, _)| Command::RemoveNode { id: *id })
            .collect();
        commands.push(Command::InsertNode {
            location: NodeLocation {
                index: first.index,
                ..first
            },
            node: group,
        });
        if self.apply(Command::Batch(commands)) {
            self.select_only(group_id);
        }
    }

    /// Replaces selected groups by their children, keeping their look.
    pub fn ungroup_selection(&mut self) {
        let mut commands = Vec::new();
        let mut new_selection = Vec::new();
        for (id, loc) in self.selection_in_order().into_iter().rev() {
            let Some(group) = self.doc.node(id) else {
                continue;
            };
            let NodeKind::Group { children } = &group.kind else {
                new_selection.push(id);
                continue;
            };
            commands.push(Command::RemoveNode { id });
            for (i, child) in children.iter().enumerate() {
                let mut child = child.clone();
                child.transform = group.transform * child.transform;
                if group.opacity < 1.0 {
                    child.opacity *= group.opacity;
                }
                new_selection.push(child.id);
                commands.push(Command::InsertNode {
                    location: NodeLocation {
                        index: loc.index + i,
                        ..loc
                    },
                    node: child,
                });
            }
        }
        if !commands.is_empty() && self.apply(Command::Batch(commands)) {
            self.selection = new_selection;
        }
    }

    /// Moves the selection one step towards the front (`up`) or back.
    pub fn reorder_selection(&mut self, up: bool) {
        let mut ordered = self.selection_in_order();
        if up {
            ordered.reverse();
        }
        let mut commands = Vec::new();
        for (id, loc) in ordered {
            let Ok(siblings) = self.doc.siblings(loc.page, loc.parent) else {
                continue;
            };
            let index = if up {
                if loc.index + 1 >= siblings.len() {
                    continue;
                }
                loc.index + 1
            } else {
                if loc.index == 0 {
                    continue;
                }
                loc.index - 1
            };
            commands.push(Command::MoveNode {
                id,
                to: NodeLocation { index, ..loc },
            });
        }
        if !commands.is_empty() {
            self.apply(Command::Batch(commands));
        }
    }

    /// Changes a node with `edit`, as one undo step.
    pub fn update_node(&mut self, id: NodeId, edit: impl FnOnce(&mut Node)) -> bool {
        let Some(mut node) = self.doc.node(id).cloned() else {
            return false;
        };
        edit(&mut node);
        if Some(&node) == self.doc.node(id) {
            return false;
        }
        if let Some(children) = node.children_mut() {
            children.clear();
        }
        self.apply(Command::UpdateNode { node })
    }

    /// Changes a node, merging with the previous change that used `key` (sliders, drags).
    pub fn update_node_merged(
        &mut self,
        id: NodeId,
        key: u64,
        edit: impl FnOnce(&mut Node),
    ) -> bool {
        let Some(mut node) = self.doc.node(id).cloned() else {
            return false;
        };
        edit(&mut node);
        if Some(&node) == self.doc.node(id) {
            return false;
        }
        if let Some(children) = node.children_mut() {
            children.clear();
        }
        self.apply_merged(Command::UpdateNode { node }, key)
    }

    pub fn rename_node(&mut self, id: NodeId, name: String) {
        self.update_node(id, |n| n.name = name);
    }

    pub fn set_visible(&mut self, id: NodeId, visible: bool) {
        self.update_node(id, |n| n.visible = visible);
        if !visible {
            self.selection.retain(|s| *s != id);
        }
    }

    pub fn set_locked(&mut self, id: NodeId, locked: bool) {
        self.update_node(id, |n| n.locked = locked);
        if locked {
            self.selection.retain(|s| *s != id);
        }
    }

    /// Moves a node to another place in the layer tree.
    pub fn move_node(&mut self, id: NodeId, to: NodeLocation) {
        self.apply(Command::MoveNode { id, to });
    }

    /// Combines the selected shapes with a boolean operation.
    pub fn boolean_selection(&mut self, op: BooleanOp) {
        let ids: Vec<NodeId> = self.selection.clone();
        match boolean::combine_nodes(&self.doc, &ids, op) {
            Ok(command) => {
                let new_id = match &command {
                    Command::Batch(commands) => commands.iter().find_map(|c| match c {
                        Command::InsertNode { node, .. } => Some(node.id),
                        _ => None,
                    }),
                    _ => None,
                };
                if self.apply(command)
                    && let Some(id) = new_id
                {
                    self.select_only(id);
                }
            }
            Err(err) => self.interaction.report(trf(
                "Boolean operation unavailable: {error}",
                &[("error", &err.to_string())],
            )),
        }
    }

    /// Whether the selection can take part in a boolean operation.
    pub fn can_combine_selection(&self) -> bool {
        self.selection.len() >= 2
            && self.selection.iter().all(|id| {
                self.doc
                    .node(*id)
                    .is_some_and(|n| matches!(n.kind, NodeKind::Shape(_)))
            })
    }

    /// Adds an image (PNG or JPEG) into the first frame of the page, centred and scaled down
    /// to fit it. Without frames the image keeps its size and is centred on the origin.
    pub fn insert_image(&mut self, data: Vec<u8>) -> bool {
        let dimensions = image::ImageReader::new(std::io::Cursor::new(&data))
            .with_guessed_format()
            .ok()
            .and_then(|r| r.into_dimensions().ok());
        let Some((w, h)) = dimensions else {
            self.interaction
                .report(tr("Unsupported image format. Use PNG or JPEG.").into());
            return false;
        };
        let Some(asset) = self.doc.assets.insert_image(data) else {
            self.interaction
                .report(tr("Unsupported image format. Use PNG or JPEG.").into());
            return false;
        };
        let page = self.doc.page(self.page).expect("current page");
        let frame = query::frames(page)
            .first()
            .and_then(|id| query::page_bounds(page, *id, self.scene.text_engine()));
        let (w, h) = (f64::from(w), f64::from(h));
        let scale = frame.map_or(1.0, |f| {
            (f.width() * 0.8 / w).min(f.height() * 0.8 / h).min(1.0)
        });
        let size = libforma::geom::Size::new(w * scale, h * scale);
        let center = frame.map_or(libforma::geom::Point::ORIGIN, |f| f.center());
        let node =
            Node::image(asset, size).at(center.x - size.width / 2.0, center.y - size.height / 2.0);
        let id = node.id;
        let commands = self.place_in_frames(vec![node]);
        if self.apply(Command::Batch(commands)) {
            self.select_only(id);
            true
        } else {
            false
        }
    }

    /// Commands that insert nodes given in page space, each on top inside the top-level frame
    /// under its centre, or on top of the page. Frames always go on the page.
    fn place_in_frames(&mut self, nodes: Vec<Node>) -> Vec<Command> {
        let page_id = self.current_page().id;
        let mut added: std::collections::HashMap<Option<NodeId>, usize> = Default::default();
        let mut commands = Vec::new();
        for mut node in nodes {
            let center =
                node.transform * query::local_bounds(&node, self.scene.text_engine()).center();
            let frame = if node.is_frame() {
                None
            } else {
                self.top_frame_at(center, &[])
            };
            let page = self.current_page();
            let (count, parent) = match frame.and_then(|f| page.find(f)) {
                Some(f) => (f.children().len(), f.transform),
                None => (page.children.len(), Affine::IDENTITY),
            };
            let queued = added.entry(frame).or_default();
            node.transform = parent.inverse() * node.transform;
            commands.push(Command::InsertNode {
                location: NodeLocation {
                    page: page_id,
                    parent: frame,
                    index: count + *queued,
                },
                node,
            });
            *queued += 1;
        }
        commands
    }

    // ---- Pages ------------------------------------------------------------------------

    /// Adds a page after the current one. If the current page has frames, the new page gets
    /// an empty frame like the first of them.
    pub fn add_page(&mut self) {
        let current = self.current_page();
        let index = self
            .doc
            .page_index(current.id)
            .map_or(self.doc.pages.len(), |i| i + 1);
        let n = self.doc.pages.len() + 1;
        let mut page = Page::new(trf("Page {n}", &[("n", &n.to_string())]));
        if let Some(frame) = current.children.iter().find(|n| n.is_frame()) {
            let mut empty = Node::new(frame.kind.clone()).with_name(frame.name.clone());
            if let NodeKind::Frame(f) = &mut empty.kind {
                f.children.clear();
            }
            page.children.push(empty);
        }
        let id = page.id;
        if self.apply(Command::InsertPage { index, page }) {
            self.set_page(id);
        }
    }

    pub fn duplicate_page(&mut self, id: PageId) {
        let Some(index) = self.doc.page_index(id) else {
            return;
        };
        let mut page = self.doc.pages[index].clone();
        page.id = PageId::new();
        page.name = trf("{name} (copy)", &[("name", &page.name)]);
        page.children = page.children.iter().map(Node::duplicate).collect();
        let new_id = page.id;
        if self.apply(Command::InsertPage {
            index: index + 1,
            page,
        }) {
            self.set_page(new_id);
        }
    }

    pub fn delete_page(&mut self, id: PageId) {
        if self.doc.pages.len() <= 1 {
            self.interaction
                .report(tr("A design needs at least one page.").into());
            return;
        }
        let index = self.doc.page_index(id);
        if self.apply(Command::RemovePage { id })
            && let Some(index) = index
        {
            let next = self.doc.pages[index.min(self.doc.pages.len() - 1)].id;
            self.set_page(next);
        }
    }

    pub fn move_page(&mut self, id: PageId, up: bool) {
        let Some(index) = self.doc.page_index(id) else {
            return;
        };
        let target = if up {
            index.checked_sub(1)
        } else {
            (index + 1 < self.doc.pages.len()).then_some(index + 1)
        };
        let Some(target) = target else { return };
        let page = self.doc.pages[index].clone();
        self.apply(Command::Batch(vec![
            Command::RemovePage { id },
            Command::InsertPage {
                index: target,
                page,
            },
        ]));
    }

    /// Changes page properties (name, size, background). Nodes are untouched.
    pub fn update_page(
        &mut self,
        id: PageId,
        merge_key: Option<u64>,
        edit: impl FnOnce(&mut Page),
    ) {
        let Some(mut page) = self.doc.page(id).cloned() else {
            return;
        };
        edit(&mut page);
        if Some(&page) == self.doc.page(id) {
            return;
        }
        page.children.clear();
        let command = Command::UpdatePage { page };
        match merge_key {
            Some(key) => self.apply_merged(command, key),
            None => self.apply(command),
        };
    }

    // ---- Clipboard ----------------------------------------------------------------------

    /// The selection as clipboard text.
    pub fn copy_selection(&self) -> Option<String> {
        let nodes: Vec<Node> = self
            .selection_in_order()
            .into_iter()
            .filter_map(|(id, _)| {
                let mut node = self.doc.node(id)?.clone();
                // Keep the page-space position.
                node.transform = self.parent_transform(id) * node.transform;
                Some(node)
            })
            .collect();
        if nodes.is_empty() {
            return None;
        }
        let mut used = std::collections::BTreeSet::new();
        for node in &nodes {
            node.walk(&mut |n| {
                if let NodeKind::Image(image) = &n.kind {
                    used.insert(image.asset);
                }
            });
        }
        let assets = used
            .into_iter()
            .filter_map(|id| self.doc.assets.get(id))
            .map(|a| (a.media_type.clone(), a.data.to_vec()))
            .collect();
        let payload = ClipboardPayload { nodes, assets };
        serde_json::to_string(&payload)
            .ok()
            .map(|json| format!("{CLIPBOARD_TAG}{json}"))
    }

    /// Pastes clipboard text produced by [`copy_selection`](Self::copy_selection). Returns
    /// `false` if the text is not Forma data.
    /// Pastes layers from the clipboard centred on `at` (page space). Returns `false` if the
    /// text holds no Forma layers.
    pub fn paste_at(&mut self, text: &str, at: Point) -> bool {
        let Some(payload) = text
            .strip_prefix(CLIPBOARD_TAG)
            .and_then(|json| serde_json::from_str::<ClipboardPayload>(json).ok())
        else {
            return false;
        };
        let engine = self.scene.text_engine();
        let bounds = payload
            .nodes
            .iter()
            .map(|n| {
                n.transform
                    .transform_rect_bbox(query::local_bounds(n, engine))
            })
            .reduce(|a, b| a.union(b));
        let offset = bounds.map_or(Vec2::ZERO, |b| at - b.center());
        self.paste(text, offset)
    }

    pub fn paste(&mut self, text: &str, offset: Vec2) -> bool {
        let Some(json) = text.strip_prefix(CLIPBOARD_TAG) else {
            return false;
        };
        let Ok(payload) = serde_json::from_str::<ClipboardPayload>(json) else {
            return false;
        };
        for (media_type, data) in payload.assets {
            self.doc.assets.insert(media_type, data);
        }
        // The nodes are in page space; each lands in the frame under it, if any.
        let mut nodes = Vec::new();
        let mut ids = Vec::new();
        for node in payload.nodes {
            let mut node = node.duplicate();
            node.transform = Affine::translate(offset) * node.transform;
            ids.push(node.id);
            nodes.push(node);
        }
        let commands = self.place_in_frames(nodes);
        if self.apply(Command::Batch(commands)) {
            self.selection = ids;
        }
        true
    }
}
