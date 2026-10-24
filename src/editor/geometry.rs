//! Transforming nodes in page space: moving, resizing and rotating.
//!
//! A node's transform is relative to its parent. To apply a page-space change `m` to a node
//! whose parent maps to page space with `parent`, the new local transform is
//! `parent⁻¹ · m · parent · transform`.

use libforma::geom::{Affine, BezPath, Point, Rect, Shape, Vec2};
use libforma::model::{Geometry, Node, NodeKind};
use libforma::query;
use libforma::text::TextEngine;

/// The new local transform after applying the page-space change `m`.
pub fn apply_in_page_space(parent: Affine, transform: Affine, m: Affine) -> Affine {
    parent.inverse() * m * parent * transform
}

/// Whether a transform only translates.
fn is_translation(t: Affine) -> bool {
    let [a, b, c, d, _, _] = t.as_coeffs();
    (a - 1.0).abs() < 1e-9 && b.abs() < 1e-9 && c.abs() < 1e-9 && (d - 1.0).abs() < 1e-9
}

/// Resizes a node by the page-space change `m` (a scale plus translation).
///
/// Unrotated shapes, images, text and frames change their size instead of being stretched, so
/// strokes and font sizes stay the same, like in other design tools. Anything else (groups,
/// rotated nodes) is scaled through its transform.
pub fn resized(original: &Node, parent: Affine, m: Affine, text: &mut TextEngine) -> Node {
    let mut node = original.clone();
    let page_transform = parent * original.transform;
    let resizable = is_translation(page_transform)
        && matches!(
            original.kind,
            NodeKind::Shape(_) | NodeKind::Image(_) | NodeKind::Text(_) | NodeKind::Frame(_)
        );
    if !resizable {
        node.transform = apply_in_page_space(parent, original.transform, m);
        return node;
    }

    let local = query::local_bounds(original, text);
    let page_rect = page_transform.transform_rect_bbox(local);
    let new_rect = m.transform_rect_bbox(page_rect);
    let sx = if local.width() > 0.0 {
        new_rect.width() / local.width()
    } else {
        1.0
    };
    let sy = if local.height() > 0.0 {
        new_rect.height() / local.height()
    } else {
        1.0
    };
    // Where the new local bounds start, relative to the local origin.
    let new_local_origin = match &mut node.kind {
        NodeKind::Shape(shape) => {
            shape.geometry = scale_geometry(&shape.geometry, sx, sy);
            shape.geometry.bounds().origin()
        }
        NodeKind::Image(image) => {
            image.size = new_rect.size();
            Point::ORIGIN
        }
        // A text box gets the new width; when its height changes too (top or bottom handles,
        // corners), the box gets a fixed height. Changing only the width keeps the height as
        // it was, automatic or fixed.
        NodeKind::Text(t) => {
            t.width = Some(new_rect.width().max(1.0));
            if (new_rect.height() - page_rect.height()).abs() > 1e-6 {
                t.height = Some(new_rect.height().max(1.0));
            }
            Point::ORIGIN
        }
        // A frame changes size; its children keep their place.
        NodeKind::Frame(frame) => {
            frame.size = new_rect.size();
            Point::ORIGIN
        }
        NodeKind::Group { .. } => unreachable!(),
    };

    // Place the new bounds at `new_rect` in page space.
    let parent_offset = parent.translation();
    let translation = new_rect.origin().to_vec2() - new_local_origin.to_vec2() - parent_offset;
    node.transform = Affine::translate(translation);
    node
}

fn scale_geometry(geometry: &Geometry, sx: f64, sy: f64) -> Geometry {
    let scale_size =
        |size: libforma::geom::Size| libforma::geom::Size::new(size.width * sx, size.height * sy);
    match geometry {
        Geometry::Rect {
            size,
            corner_radius,
        } => Geometry::Rect {
            size: scale_size(*size),
            corner_radius: *corner_radius,
        },
        Geometry::Ellipse { size } => Geometry::Ellipse {
            size: scale_size(*size),
        },
        Geometry::Polygon { size, sides } => Geometry::Polygon {
            size: scale_size(*size),
            sides: *sides,
        },
        Geometry::Star {
            size,
            points,
            inner_ratio,
        } => Geometry::Star {
            size: scale_size(*size),
            points: *points,
            inner_ratio: *inner_ratio,
        },
        Geometry::Line { end } => Geometry::Line {
            end: Point::new(end.x * sx, end.y * sy),
        },
        Geometry::Path { path } => Geometry::Path {
            path: Affine::scale_non_uniform(sx, sy) * path.clone(),
        },
    }
}

/// Moves a path so its bounding box starts at the local origin, returning the offset that
/// must be added to the node's transform to keep it in place.
pub fn normalize_path(path: &BezPath) -> (BezPath, Vec2) {
    let origin = path.bounding_box().origin().to_vec2();
    (Affine::translate(-origin) * path.clone(), origin)
}

/// Scale-and-translate taking `from` to `to` (both axis-aligned rectangles).
pub fn rect_to_rect(from: Rect, to: Rect) -> Affine {
    let sx = if from.width().abs() > 1e-9 {
        to.width() / from.width()
    } else {
        1.0
    };
    let sy = if from.height().abs() > 1e-9 {
        to.height() / from.height()
    } else {
        1.0
    };
    Affine::translate(to.origin().to_vec2())
        * Affine::scale_non_uniform(sx, sy)
        * Affine::translate(-from.origin().to_vec2())
}

/// Rotation by `angle` radians around `center`.
pub fn rotate_about(center: Point, angle: f64) -> Affine {
    Affine::rotate_about(angle, center)
}

#[cfg(test)]
mod tests {
    use super::*;
    use libforma::paint::{Color, Stroke};

    #[test]
    fn resizing_a_rect_changes_its_size_not_its_stroke() {
        let node = Node::shape(Geometry::rect(10.0, 20.0))
            .at(5.0, 5.0)
            .with_stroke(Stroke::new(Color::BLACK, 2.0));
        let m = rect_to_rect(
            Rect::new(5.0, 5.0, 15.0, 25.0),
            Rect::new(0.0, 0.0, 40.0, 10.0),
        );
        let out = resized(&node, Affine::IDENTITY, m, &mut TextEngine::new());
        assert_eq!(out.transform, Affine::translate((0.0, 0.0)));
        let NodeKind::Shape(shape) = &out.kind else {
            unreachable!()
        };
        assert_eq!(shape.geometry.bounds(), Rect::new(0.0, 0.0, 40.0, 10.0));
        assert_eq!(shape.stroke.as_ref().unwrap().width, 2.0);
    }

    #[test]
    fn resizing_inside_a_translated_group() {
        let node = Node::shape(Geometry::ellipse(10.0, 10.0)).at(10.0, 0.0);
        let parent = Affine::translate((100.0, 100.0));
        let m = rect_to_rect(
            Rect::new(110.0, 100.0, 120.0, 110.0),
            Rect::new(110.0, 100.0, 130.0, 110.0),
        );
        let out = resized(&node, parent, m, &mut TextEngine::new());
        assert_eq!(out.transform, Affine::translate((10.0, 0.0)));
        let NodeKind::Shape(shape) = &out.kind else {
            unreachable!()
        };
        assert_eq!(shape.geometry.bounds(), Rect::new(0.0, 0.0, 20.0, 10.0));
    }

    #[test]
    fn rotated_nodes_scale_through_their_transform() {
        let node = Node::shape(Geometry::rect(10.0, 10.0)).with_transform(Affine::rotate(0.5));
        let m = Affine::scale(2.0);
        let out = resized(&node, Affine::IDENTITY, m, &mut TextEngine::new());
        assert_eq!(out.transform, Affine::scale(2.0) * Affine::rotate(0.5));
    }

    #[test]
    fn page_space_changes_respect_parents() {
        let parent = Affine::translate((50.0, 0.0)) * Affine::scale(2.0);
        let local = Affine::translate((1.0, 1.0));
        let m = Affine::translate((10.0, 0.0));
        let new = apply_in_page_space(parent, local, m);
        // In page space the node moved by exactly (10, 0).
        assert_eq!(
            (parent * new).translation(),
            (parent * local).translation() + Vec2::new(10.0, 0.0)
        );
    }
}
