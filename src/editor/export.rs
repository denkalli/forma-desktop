//! Exporting frames, the selection or a whole page as PNG, SVG or PDF.

use libforma::geom::{Affine, Rect};
use libforma::model::{Document, Node, NodeKind, Page};
use libforma::query;
use libforma::render::OffscreenRenderer;
use libforma::scene::DisplayList;
use libforma::{NodeId, PageId};

use super::Editor;
use crate::i18n::tr;

/// File formats for export.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ExportFormat {
    #[default]
    Png,
    Svg,
    Pdf,
    /// An editable document containing the selected nodes.
    Forma,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Png => "png",
            ExportFormat::Svg => "svg",
            ExportFormat::Pdf => "pdf",
            ExportFormat::Forma => "forma",
        }
    }
}

/// What gets exported. Each frame is its own file (or PDF page); the selection and the page
/// are one image each.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExportTarget {
    /// Every top-level frame of the current page, or of all pages.
    Frames { all_pages: bool },
    /// The selected frames.
    SelectedFrames,
    /// The selected layers together, framed to their bounds.
    Selection,
    /// Everything on the current page, framed to its content.
    Page,
}

impl Editor {
    /// Copies the selected nodes into a standalone document, preserving their editable
    /// structure and the image assets they reference.
    pub fn selected_document(&mut self) -> libforma::Result<Document> {
        let page = self.current_page();
        let page_id = page.id;
        let page_name = page.name.clone();
        let selected: std::collections::HashSet<_> = self.selection.iter().copied().collect();
        fn roots(
            nodes: &[Node],
            parent: Affine,
            has_selected_parent: bool,
            selected: &std::collections::HashSet<NodeId>,
            out: &mut Vec<Node>,
        ) {
            for node in nodes {
                let is_selected = selected.contains(&node.id);
                if is_selected && !has_selected_parent {
                    let mut copy = node.clone();
                    copy.transform = parent * node.transform;
                    out.push(copy);
                } else if !is_selected {
                    roots(
                        node.children(),
                        parent * node.transform,
                        false,
                        selected,
                        out,
                    );
                }
            }
        }
        let mut nodes = Vec::new();
        roots(
            &page.children,
            Affine::IDENTITY,
            false,
            &selected,
            &mut nodes,
        );
        if nodes.is_empty() {
            return Err(libforma::Error::Render(tr("Nothing to export").into()));
        }
        let ids: Vec<_> = nodes.iter().map(|node| node.id).collect();
        let area = self
            .scene
            .build_nodes(&self.doc, page_id, &ids)?
            .into_iter()
            .map(|list| list.area)
            .reduce(|a, b| a.union(b))
            .unwrap_or(Rect::ZERO);
        let normalize = Affine::translate((-area.x0, -area.y0));
        for node in &mut nodes {
            node.transform = normalize * node.transform;
        }
        let mut doc = Document::new(format!("{} - Selection", self.display_name()))
            .with_page(Page::new(page_name));
        doc.pages[0].children = nodes;
        let mut used = std::collections::BTreeSet::new();
        doc.walk(&mut |node| {
            if let NodeKind::Image(image) = &node.kind {
                used.insert(image.asset);
            }
        });
        for id in used {
            if let Some(asset) = self.doc.assets.get(id) {
                doc.assets
                    .insert(asset.media_type.clone(), asset.data.clone());
            }
        }
        Ok(doc)
    }

    /// The selected frames, back to front.
    fn selected_frames(&self) -> Vec<NodeId> {
        self.selection_in_order()
            .into_iter()
            .map(|(id, _)| id)
            .filter(|id| self.doc.node(*id).is_some_and(|n| n.is_frame()))
            .collect()
    }

    /// How many images `target` produces (PDF pages, or files for PNG and SVG). Zero when
    /// there is nothing to export.
    pub fn export_count(&self, target: ExportTarget) -> usize {
        match target {
            ExportTarget::Frames { all_pages: false } => query::frames(self.current_page()).len(),
            ExportTarget::Frames { all_pages: true } => {
                self.doc.pages.iter().map(|p| query::frames(p).len()).sum()
            }
            ExportTarget::SelectedFrames => self.selected_frames().len(),
            ExportTarget::Selection => usize::from(!self.selection.is_empty()),
            ExportTarget::Page => {
                usize::from(self.current_page().children.iter().any(|n| n.visible))
            }
        }
    }

    /// The target offered first: the selected frames, the selection, the page's frames, or
    /// else the whole page.
    pub fn default_export_target(&self) -> ExportTarget {
        [
            ExportTarget::SelectedFrames,
            ExportTarget::Selection,
            ExportTarget::Frames { all_pages: false },
        ]
        .into_iter()
        .find(|t| self.export_count(*t) > 0)
        .unwrap_or(ExportTarget::Page)
    }

    /// Display lists to export, each with a name for its file.
    pub fn export_lists(
        &mut self,
        target: ExportTarget,
    ) -> libforma::Result<Vec<(String, DisplayList)>> {
        let page = self.current_page().id;
        match target {
            ExportTarget::Frames { all_pages } => {
                let pages: Vec<PageId> = if all_pages {
                    self.doc.pages.iter().map(|p| p.id).collect()
                } else {
                    vec![page]
                };
                let mut lists = Vec::new();
                for page in pages {
                    let frames = self.doc.page(page).map(query::frames).unwrap_or_default();
                    lists.extend(self.named_lists(page, &frames)?);
                }
                Ok(lists)
            }
            ExportTarget::SelectedFrames => {
                let frames = self.selected_frames();
                self.named_lists(page, &frames)
            }
            ExportTarget::Selection => {
                let ids: Vec<NodeId> = self
                    .selection_in_order()
                    .into_iter()
                    .map(|(id, _)| id)
                    .collect();
                let lists = self.scene.build_nodes(&self.doc, page, &ids)?;
                // One image: the nodes back to front, framed to all of them.
                let merged = lists.into_iter().reduce(|mut all, list| {
                    all.area = all.area.union(list.area);
                    all.items.extend(list.items);
                    all
                });
                Ok(merged
                    .map(|list| (self.display_name(), list))
                    .into_iter()
                    .collect())
            }
            ExportTarget::Page => {
                let page = self.doc.page(page).expect("current page");
                let area = query::content_bounds(page, self.scene.text_engine());
                Ok(area
                    .map(|area| {
                        (
                            page.name.clone(),
                            self.scene.build_area(&self.doc, page, area),
                        )
                    })
                    .into_iter()
                    .collect())
            }
        }
    }

    /// One display list per node, named after the node.
    fn named_lists(
        &mut self,
        page: PageId,
        ids: &[NodeId],
    ) -> libforma::Result<Vec<(String, DisplayList)>> {
        let lists = self.scene.build_nodes(&self.doc, page, ids)?;
        Ok(ids
            .iter()
            .map(|id| {
                self.doc
                    .node(*id)
                    .map(super::default_name)
                    .unwrap_or_default()
            })
            .zip(lists)
            .collect())
    }

    /// Exports `target`. Returns one `(name, bytes)` per image, or a single entry for PDF
    /// (one PDF page per image).
    pub fn export(
        &mut self,
        format: ExportFormat,
        target: ExportTarget,
        scale: f64,
        renderer: Option<&mut OffscreenRenderer>,
    ) -> libforma::Result<Vec<(String, Vec<u8>)>> {
        use libforma::export::{pdf, png, svg};
        let lists = self.export_lists(target)?;
        if lists.is_empty() {
            return Err(libforma::Error::Render(tr("Nothing to export").into()));
        }
        match format {
            ExportFormat::Forma => Err(libforma::Error::Render(
                tr("Save the selection as a Forma design instead of exporting it").into(),
            )),
            ExportFormat::Pdf => {
                let lists: Vec<DisplayList> = lists.into_iter().map(|(_, l)| l).collect();
                Ok(vec![(
                    String::new(),
                    pdf::to_pdf(&lists, &self.doc.assets)?,
                )])
            }
            ExportFormat::Svg => lists
                .iter()
                .map(|(name, list)| {
                    Ok((
                        name.clone(),
                        svg::to_svg(list, &self.doc.assets)?.into_bytes(),
                    ))
                })
                .collect(),
            ExportFormat::Png => {
                let renderer = renderer
                    .ok_or_else(|| libforma::Error::Render(tr("GPU unavailable").into()))?;
                lists
                    .iter()
                    .map(|(name, list)| {
                        let image = renderer.render_image(list, &self.doc.assets, scale)?;
                        Ok((name.clone(), png::encode(&image)?))
                    })
                    .collect()
            }
        }
    }
}
