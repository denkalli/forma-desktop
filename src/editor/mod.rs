//! Editor state: the open document, its history, the selection, the active tool and the
//! viewport. Nothing here depends on the GUI toolkit; the canvas and panels read this state
//! and call its methods, and every change to the document goes through a
//! [`Command`](libforma::history::Command).

mod export;
mod geometry;
mod interaction;
mod ops;

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::i18n::tr;
use libforma::format::{self, WriteOptions};
use libforma::geom::{Affine, Point, Rect, Size, Vec2};
use libforma::history::{Command, History};
use libforma::model::{Document, Node, NodeLocation, Page};
use libforma::query;
use libforma::render::OffscreenRenderer;
use libforma::scene::{DisplayList, SceneBuilder};
use libforma::{NodeId, PageId};

pub use export::{ExportFormat, ExportTarget};
pub use interaction::{CursorHint, Handle, Interaction, Modifiers, PointerEvent, ToolContext};
pub use ops::{Align, Axis, default_name};

/// Fill given to new shapes.
pub fn interaction_default_fill() -> libforma::paint::Color {
    interaction::DEFAULT_FILL
}

/// Size of the thumbnail stored in `.forma` files (longest side, in pixels).
const THUMBNAIL_SIZE: f64 = 256.0;

/// What a pointer press on the canvas does.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tool {
    #[default]
    Select,
    Frame,
    Rect,
    Ellipse,
    Polygon,
    Star,
    Line,
    Pen,
    Text,
}

impl Tool {
    /// Tools grouped under the shapes button of the toolbar.
    pub const SHAPES: [Tool; 5] = [
        Tool::Rect,
        Tool::Ellipse,
        Tool::Polygon,
        Tool::Star,
        Tool::Line,
    ];

    pub fn is_shape(self) -> bool {
        Self::SHAPES.contains(&self)
    }

    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => tr("Select"),
            Tool::Frame => tr("Frame"),
            Tool::Rect => tr("Rectangle"),
            Tool::Ellipse => tr("Ellipse"),
            Tool::Polygon => tr("Polygon"),
            Tool::Star => tr("Star"),
            Tool::Line => tr("Line"),
            Tool::Pen => tr("Pen"),
            Tool::Text => tr("Text"),
        }
    }

    pub fn shortcut(self) -> Option<char> {
        match self {
            Tool::Select => Some('V'),
            Tool::Frame => Some('F'),
            Tool::Rect => Some('R'),
            Tool::Ellipse => Some('O'),
            Tool::Line => Some('L'),
            Tool::Pen => Some('P'),
            Tool::Text => Some('T'),
            Tool::Polygon | Tool::Star => None,
        }
    }
}

/// Zoom and pan of the canvas.
///
/// A page point `p` appears at `canvas origin + pan + p * zoom`, in screen points.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Viewport {
    pub zoom: f64,
    pub pan: Vec2,
    /// Fit the content to the canvas on the next frame (the canvas size is not known here).
    pub fit_requested: bool,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
            fit_requested: true,
        }
    }
}

impl Viewport {
    pub const MIN_ZOOM: f64 = 0.02;
    pub const MAX_ZOOM: f64 = 64.0;

    /// Page space to canvas space (screen points relative to the canvas origin).
    pub fn transform(&self) -> Affine {
        Affine::translate(self.pan) * Affine::scale(self.zoom)
    }

    /// Fits `content` (page space) inside a canvas of `canvas` size, with a margin. With no
    /// content, shows the page origin at the center of the canvas at 100 %.
    pub fn fit(&mut self, content: Option<Rect>, canvas: Size) {
        self.fit_requested = false;
        let Some(content) = content else {
            self.zoom = 1.0;
            self.pan = Vec2::new(canvas.width / 2.0, canvas.height / 2.0);
            return;
        };
        let margin = 64.0;
        let zoom = ((canvas.width - 2.0 * margin) / content.width().max(1.0))
            .min((canvas.height - 2.0 * margin) / content.height().max(1.0))
            .clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        self.zoom = zoom;
        let center = content.center().to_vec2() * zoom;
        self.pan = Vec2::new(canvas.width / 2.0, canvas.height / 2.0) - center;
    }

    /// The page point shown at `canvas` (screen points relative to the canvas origin).
    pub fn page_point(&self, canvas: Vec2) -> Point {
        ((canvas - self.pan) / self.zoom).to_point()
    }

    /// Zooms by `factor`, keeping the canvas point `anchor` fixed.
    pub fn zoom_around(&mut self, factor: f64, anchor: Vec2) {
        let zoom = (self.zoom * factor).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        let factor = zoom / self.zoom;
        self.pan = anchor + (self.pan - anchor) * factor;
        self.zoom = zoom;
    }
}

/// An open document.
pub struct Editor {
    pub doc: Document,
    /// File the document is saved to; `None` until the first save.
    pub path: Option<PathBuf>,
    pub history: History,
    pub page: PageId,
    /// Selected nodes, in selection order.
    pub selection: Vec<NodeId>,
    pub tool: Tool,
    /// Shape tool shown on the toolbar's shapes button.
    pub shape_tool: Tool,
    pub view: Viewport,
    pub interaction: Interaction,
    /// Increases on every change to the document, so views know when to redraw.
    pub revision: u64,
    /// Time of the last edit, for autosave.
    pub last_edit: Option<Instant>,
    /// Creation time read from the file, kept across saves.
    created: Option<jiff::Timestamp>,
    scene: SceneBuilder,
    next_merge_key: u64,
    /// Families of the fonts the user added to Forma.
    user_fonts: std::collections::BTreeSet<String>,
}

impl Editor {
    /// A new, unsaved document with one page. With a `frame` (name and size) the page holds
    /// that frame at the origin; without one it is an empty canvas.
    pub fn new_document(title: &str, frame: Option<(&str, Size)>) -> Self {
        let mut page = Page::new(tr("Page 1"));
        if let Some((name, size)) = frame {
            page = page.with_child(Node::frame(name, size.width, size.height));
        }
        Self::with_document(Document::new(title).with_page(page), None, None)
    }

    /// A new, unsaved design that starts as a copy of the template file at `path`.
    pub fn from_template(path: &Path, title: &str) -> libforma::Result<Self> {
        let mut doc = format::open(path)?;
        doc.title = title.to_owned();
        if doc.pages.is_empty() {
            doc.pages.push(Page::new(crate::i18n::tr("Page 1")));
        }
        Ok(Self::with_document(doc, None, None))
    }

    /// Opens a `.forma` file.
    pub fn open(path: &Path) -> libforma::Result<Self> {
        let file = std::io::BufReader::new(std::fs::File::open(path)?);
        let (mut doc, manifest) = format::read_with_manifest(file)?;
        if doc.pages.is_empty() {
            doc.pages.push(Page::new(tr("Page 1")));
        }
        Ok(Self::with_document(
            doc,
            Some(path.to_path_buf()),
            Some(manifest.created),
        ))
    }

    fn with_document(
        doc: Document,
        path: Option<PathBuf>,
        created: Option<jiff::Timestamp>,
    ) -> Self {
        let page = doc.pages[0].id;
        Self {
            doc,
            path,
            history: History::with_limit(500),
            page,
            selection: Vec::new(),
            tool: Tool::Select,
            shape_tool: Tool::Rect,
            view: Viewport::default(),
            interaction: Interaction::default(),
            revision: 0,
            last_edit: None,
            created,
            scene: SceneBuilder::new(),
            next_merge_key: 0,
            user_fonts: Default::default(),
        }
    }

    /// Name shown in the title bar and panels.
    pub fn display_name(&self) -> String {
        if !self.doc.title.is_empty() {
            return self.doc.title.clone();
        }
        self.path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| tr("Untitled").into())
    }

    pub fn is_modified(&self) -> bool {
        self.history.is_modified()
    }

    pub fn current_page(&self) -> &Page {
        self.doc
            .page(self.page)
            .unwrap_or_else(|| &self.doc.pages[0])
    }

    /// Display list of the current page, as the canvas should draw it.
    pub fn display_list(&mut self) -> DisplayList {
        let page_id = self.current_page().id;
        // The node being edited as text is drawn by the text field instead.
        if let Some(editing) = self.interaction.editing_text() {
            let mut page = self.current_page().clone();
            if let Some(node) = page.find_mut(editing) {
                node.visible = false;
            }
            return self.scene.build_page(&self.doc, &page);
        }
        let page = self.doc.page(page_id).expect("current page exists");
        self.scene.build_page(&self.doc, page)
    }

    // ---- Changing the document -------------------------------------------------------

    /// Applies a command as its own undo step. Errors are reported, not returned: a
    /// failing command leaves the document unchanged.
    pub fn apply(&mut self, command: Command) -> bool {
        let result = self.history.execute(&mut self.doc, command);
        self.after_change(result)
    }

    /// Applies a command, merging it into the previous step with the same key.
    pub fn apply_merged(&mut self, command: Command, key: u64) -> bool {
        let result = self.history.execute_merged(&mut self.doc, command, key);
        self.after_change(result)
    }

    /// A fresh key for [`apply_merged`](Self::apply_merged).
    pub fn new_merge_key(&mut self) -> u64 {
        self.next_merge_key += 1;
        self.next_merge_key
    }

    fn after_change(&mut self, result: libforma::Result<()>) -> bool {
        match result {
            Ok(()) => {
                self.touch();
                true
            }
            Err(err) => {
                self.interaction.report(crate::i18n::trf(
                    "Could not apply the change: {error}",
                    &[("error", &err.to_string())],
                ));
                false
            }
        }
    }

    fn touch(&mut self) {
        self.revision += 1;
        self.last_edit = Some(Instant::now());
        self.fix_selection();
    }

    pub fn undo(&mut self) {
        self.interaction.cancel(&mut self.history, &mut self.doc);
        if let Ok(true) = self.history.undo(&mut self.doc) {
            self.touch();
        }
    }

    pub fn redo(&mut self) {
        self.interaction.cancel(&mut self.history, &mut self.doc);
        if let Ok(true) = self.history.redo(&mut self.doc) {
            self.touch();
        }
    }

    /// Drops selected ids that no longer exist, and makes sure the current page exists.
    fn fix_selection(&mut self) {
        if self.doc.page(self.page).is_none() {
            self.page = self.doc.pages.first().map(|p| p.id).unwrap_or(self.page);
        }
        let page = self.page;
        let doc = &self.doc;
        self.selection
            .retain(|id| doc.locate(*id).is_some_and(|loc| loc.page == page));
    }

    // ---- Pages and selection ----------------------------------------------------------

    pub fn set_page(&mut self, page: PageId) {
        if self.page != page && self.doc.page(page).is_some() {
            self.interaction.cancel(&mut self.history, &mut self.doc);
            self.page = page;
            self.selection.clear();
            self.view.fit_requested = true;
            self.revision += 1;
        }
    }

    pub fn select_only(&mut self, id: NodeId) {
        self.selection = vec![id];
    }

    /// Adds or removes a node from the selection.
    pub fn toggle_selected(&mut self, id: NodeId) {
        if let Some(i) = self.selection.iter().position(|s| *s == id) {
            self.selection.remove(i);
        } else {
            self.selection.push(id);
        }
    }

    pub fn is_selected(&self, id: NodeId) -> bool {
        self.selection.contains(&id)
    }

    pub fn select_all(&mut self) {
        self.selection = self
            .current_page()
            .children
            .iter()
            .filter(|n| n.visible && !n.locked)
            .map(|n| n.id)
            .collect();
    }

    /// Union of the page-space bounds of the selected nodes.
    pub fn selection_bounds(&mut self) -> Option<Rect> {
        let page = self.doc.page(self.page)?;
        let text = self.scene.text_engine();
        self.selection
            .iter()
            .filter_map(|id| query::page_bounds(page, *id, text))
            .reduce(|a, b| a.union(b))
    }

    /// Page-space outline of a node: its local bounds and the transform to page space.
    pub fn node_outline(&mut self, id: NodeId) -> Option<(Rect, Affine)> {
        let page = self.doc.page(self.page)?;
        let node = page.find(id)?;
        let transform = query::page_transform(page, id)?;
        Some((
            query::local_bounds(node, self.scene.text_engine()),
            transform,
        ))
    }

    // ---- Fonts --------------------------------------------------------------------------

    /// Font families that text can use, sorted.
    pub fn font_families(&mut self) -> Vec<String> {
        self.scene.text_engine().families()
    }

    /// Weights available in `family`, sorted (empty if unknown).
    pub fn font_weights(&mut self, family: &str) -> Vec<u16> {
        self.scene.text_engine().weights(family)
    }

    /// File data and face index of `family`'s regular face, for previews.
    pub fn font_data(&mut self, family: &str) -> Option<(Vec<u8>, u32)> {
        self.scene.text_engine().font_data(family)
    }

    /// Makes the fonts in a font file available to this document and returns their
    /// families. The canvas redraws, since text may have been waiting for the font.
    pub fn register_font(&mut self, data: Vec<u8>) -> Vec<String> {
        let names = self.scene.text_engine().register_font(data);
        self.user_fonts.extend(names.iter().cloned());
        self.revision += 1;
        names
    }

    /// Families of the fonts the user added.
    pub fn user_fonts(&self) -> &std::collections::BTreeSet<String> {
        &self.user_fonts
    }

    /// Page-space bounds of everything on the current page, or `None` if it is empty.
    pub fn content_bounds(&mut self) -> Option<Rect> {
        let page = self.doc.page(self.page)?;
        query::content_bounds(page, self.scene.text_engine())
    }

    /// Id, name and page-space bounds of each visible top-level frame of the current page,
    /// back to front.
    pub fn frame_labels(&mut self) -> Vec<(NodeId, String, Rect)> {
        let Some(page) = self.doc.page(self.page) else {
            return Vec::new();
        };
        let text = self.scene.text_engine();
        page.children
            .iter()
            .filter(|n| n.is_frame() && n.visible)
            .filter_map(|n| {
                let bounds = query::page_bounds(page, n.id, text)?;
                Some((n.id, default_name(n), bounds))
            })
            .collect()
    }

    /// Inserts a node at the top of the current page and selects it.
    pub fn insert_top_level(&mut self, node: Node) -> bool {
        let page = self.current_page();
        let location = NodeLocation::top_level(page.id, page.children.len());
        let id = node.id;
        if self.apply(Command::InsertNode { location, node }) {
            self.select_only(id);
            true
        } else {
            false
        }
    }

    // ---- Files ------------------------------------------------------------------------

    /// Saves to `path` (and makes it the document's file). `renderer` draws the thumbnail.
    pub fn save_to(
        &mut self,
        path: &Path,
        renderer: Option<&mut OffscreenRenderer>,
    ) -> libforma::Result<()> {
        let thumbnail = renderer.and_then(|r| self.thumbnail(r).ok());
        let created = *self.created.get_or_insert_with(jiff::Timestamp::now);
        format::save(
            &self.doc,
            path,
            &WriteOptions {
                created: Some(created),
                thumbnail,
            },
        )?;
        self.path = Some(path.to_path_buf());
        self.history.mark_saved();
        self.last_edit = None;
        Ok(())
    }

    /// Thumbnail of the first frame of the first page, or of all its content if it has no
    /// frame.
    fn thumbnail(&mut self, renderer: &mut OffscreenRenderer) -> libforma::Result<Vec<u8>> {
        let page = &self.doc.pages[0];
        let list = match query::frames(page).first() {
            Some(frame) => self.scene.build_node(&self.doc, page, *frame),
            None => None,
        }
        .unwrap_or_else(|| self.scene.build_page(&self.doc, page));
        let size = list.size();
        let scale = (THUMBNAIL_SIZE / size.width.max(size.height).max(1.0)).min(1.0);
        let image = renderer.render_image(&list, &self.doc.assets, scale)?;
        libforma::export::png::encode(&image)
    }
}

#[cfg(test)]
mod tests;
