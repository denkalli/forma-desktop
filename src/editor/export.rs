//! Exporting frames, the selection or a whole page as PNG, SVG or PDF.

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
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Png => "png",
            ExportFormat::Svg => "svg",
            ExportFormat::Pdf => "pdf",
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
