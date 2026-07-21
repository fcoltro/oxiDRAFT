//! Drawing entities: [`EntityKind`] (the tagged union of everything a drawing
//! can hold — curves, text, dimensions, hatches, block inserts), the stable
//! [`EntityId`] handle, and the per-entity attributes ([`Entity`]).

use crate::properties::{Color, LineTypeRef, LineWeight, XData};
use oxidraft_geometry::{BoundingBox, Curve, CurveSegment, Point2d, Transform2d};

/// A stable handle to an entity in a [`crate::Document`]; ids are never reused,
/// so a stored id stays valid across edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(pub u64);

/// How a hatch region is filled.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum HatchPattern {
    /// A solid fill.
    #[default]
    Solid,
    /// Parallel hatch lines at `angle_deg`, `spacing` apart.
    Lines { angle_deg: f64, spacing: f64 },
    /// Two crossing sets of hatch lines (cross-hatch).
    Cross { angle_deg: f64, spacing: f64 },
    /// A grid of dots `spacing` apart.
    Dots { spacing: f64 },
}

/// The geometry-and-content payload of an entity — one variant per kind of
/// thing a drawing can contain.
#[derive(Clone, Debug)]
pub enum EntityKind {
    /// A curve (line, arc, spline, …).
    Curve(Curve),
    /// A standalone point.
    Point(Point2d),
    /// A text label anchored at a point.
    Text {
        anchor: Point2d,
        content: String,
        height: f64,
        rotation: f64,
        font: Option<String>,
    },
    /// An infinite construction line through a point in a direction.
    XLine { through: Point2d, dir: (f64, f64) },
    /// A semi-infinite construction ray from a point.
    Ray { from: Point2d, dir: (f64, f64) },
    /// A block instance placed by name under a transform.
    Insert {
        block: String,
        transform: Transform2d,
    },
    /// A filled region bounded by `boundary`, minus any `holes`.
    Hatch {
        boundary: Vec<Curve>,
        holes: Vec<Vec<Curve>>,
        fill: (u8, u8, u8),
        pattern: HatchPattern,
    },
    /// An aligned linear dimension between two points.
    Dimension {
        p1: Point2d,
        p2: Point2d,
        line: Point2d,
        height: f64,
        override_text: Option<String>,
    },
    /// A horizontal or vertical (orthogonal) linear dimension.
    OrthoDim {
        p1: Point2d,
        p2: Point2d,
        line: Point2d,
        vertical: bool,
        height: f64,
        override_text: Option<String>,
    },
    /// An angular dimension of the angle at `center` between two points.
    AngularDim {
        center: Point2d,
        p1: Point2d,
        p2: Point2d,
        line: Point2d,
        height: f64,
        override_text: Option<String>,
    },
    /// A radius or diameter dimension of a circle/arc.
    RadialDim {
        center: Point2d,
        edge: Point2d,
        diameter: bool,
        height: f64,
        override_text: Option<String>,
    },
}

impl EntityKind {
    /// True when every defining number is finite; loaders use this to drop
    /// corrupt records before NaN/inf can poison the document. Direction
    /// vectors, heights, and rotations count — not just coordinates.
    pub fn is_finite(&self) -> bool {
        let pts_finite = |pts: &[&Point2d]| pts.iter().all(|p| p.is_finite());
        match self {
            EntityKind::Curve(c) => c.is_finite(),
            EntityKind::Point(p) => p.is_finite(),
            EntityKind::Text {
                anchor,
                height,
                rotation,
                ..
            } => anchor.is_finite() && height.is_finite() && rotation.is_finite(),
            EntityKind::XLine { through, dir } => {
                through.is_finite() && dir.0.is_finite() && dir.1.is_finite()
            }
            EntityKind::Ray { from, dir } => {
                from.is_finite() && dir.0.is_finite() && dir.1.is_finite()
            }
            EntityKind::Insert { transform, .. } => transform.is_finite(),
            EntityKind::Hatch {
                boundary, holes, ..
            } => {
                boundary.iter().all(|c| c.is_finite())
                    && holes.iter().flatten().all(|c| c.is_finite())
            }
            EntityKind::Dimension {
                p1,
                p2,
                line,
                height,
                ..
            }
            | EntityKind::OrthoDim {
                p1,
                p2,
                line,
                height,
                ..
            } => pts_finite(&[p1, p2, line]) && height.is_finite(),
            EntityKind::AngularDim {
                center,
                p1,
                p2,
                line,
                height,
                ..
            } => pts_finite(&[center, p1, p2, line]) && height.is_finite(),
            EntityKind::RadialDim {
                center,
                edge,
                height,
                ..
            } => pts_finite(&[center, edge]) && height.is_finite(),
        }
    }

    /// The kind's extent, when it has one — unbounded kinds (`Insert`,
    /// `XLine`, `Ray`) return `None`, as does a `Hatch` with no boundary.
    pub fn bounding_box(&self) -> Option<BoundingBox> {
        match self {
            EntityKind::Curve(c) => Some(c.bounding_box()),
            EntityKind::Point(p) => Some(BoundingBox::new(*p, *p)),
            EntityKind::Text {
                anchor,
                height,
                content,
                ..
            } => {
                let w = 0.6 * height * content.chars().count() as f64;
                let (ax, ay) = anchor.to_f64();
                Some(BoundingBox::from_corners(ax, ay, ax + w, ay + height))
            }
            EntityKind::Insert { .. } => None,
            EntityKind::XLine { .. } | EntityKind::Ray { .. } => None,
            EntityKind::Hatch { boundary, .. } => boundary
                .iter()
                .map(|c| c.bounding_box())
                .reduce(|a, b| a.union(&b)),
            EntityKind::Dimension { p1, p2, line, .. }
            | EntityKind::OrthoDim { p1, p2, line, .. } => {
                Some(bbox_of(&[p1.to_f64(), p2.to_f64(), line.to_f64()]))
            }
            EntityKind::AngularDim {
                center,
                p1,
                p2,
                line,
                ..
            } => Some(bbox_of(&[
                center.to_f64(),
                p1.to_f64(),
                p2.to_f64(),
                line.to_f64(),
            ])),
            EntityKind::RadialDim { center, edge, .. } => {
                Some(bbox_of(&[center.to_f64(), edge.to_f64()]))
            }
        }
    }
}

/// A recorded tangency: this entity is kept tangent to `target`, using `near`
/// to pick which tangent solution when there are several.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TangentRef {
    /// The entity this one is tangent to.
    pub target: EntityId,
    /// A hint point selecting among multiple tangent solutions.
    pub near: Point2d,
}

/// An entity as stored in the document: its geometry ([`EntityKind`]) plus all
/// the display attributes and bookkeeping attached to it.
#[derive(Clone, Debug)]
pub struct Entity {
    /// Stable identity of this entity.
    pub id: EntityId,
    /// The geometry/content payload.
    pub kind: EntityKind,
    /// Index of the layer this entity belongs to.
    pub layer: usize,
    /// Colour, or `ByLayer`/`ByBlock` inheritance.
    pub color: Color,
    /// Line type (dash pattern), or inheritance.
    pub line_type: LineTypeRef,
    /// Line weight (thickness), or inheritance.
    pub line_weight: LineWeight,
    /// Transparency in `0.0` (opaque) to `1.0` (invisible).
    pub transparency: f64,
    /// Arbitrary extended application data.
    pub xdata: XData,
    /// Tangency constraints recorded on this entity.
    pub tangents: Vec<TangentRef>,
}

impl Entity {
    /// A new entity of `kind` on `layer`, with all attributes defaulting to
    /// `ByLayer` inheritance.
    pub fn new(id: EntityId, kind: EntityKind, layer: usize) -> Self {
        Entity {
            id,
            kind,
            layer,
            color: Color::ByLayer,
            line_type: LineTypeRef::ByLayer,
            line_weight: LineWeight::ByLayer,
            transparency: 0.0,
            xdata: XData::default(),
            tangents: Vec::new(),
        }
    }

    /// The entity's extent (delegates to [`EntityKind::bounding_box`]).
    pub fn bounding_box(&self) -> Option<BoundingBox> {
        self.kind.bounding_box()
    }

    /// Applies an affine transform to the entity's geometry in place. A
    /// non-finite transform is rejected here, keeping the document finite.
    pub fn transform(&mut self, t: &Transform2d) {
        // The single geometry floor: a non-finite transform (NaN drag
        // delta, a mirror axis of two identical points) would poison every
        // coordinate it touched. Every transform application — the CAD
        // edit helpers, the UI tool-event loops — passes through here, so
        // one gate keeps the whole document finite. Callers keep only
        // their own UX concerns (a message, skipping a history snapshot).
        if !t.is_finite() {
            return;
        }
        self.tangents.clear();
        self.kind = match &self.kind {
            EntityKind::Curve(c) => EntityKind::Curve(t.apply_curve(c)),
            EntityKind::Point(p) => EntityKind::Point(t.apply_point(p)),
            EntityKind::Text {
                anchor,
                content,
                height,
                rotation,
                font,
            } => EntityKind::Text {
                anchor: t.apply_point(anchor),
                content: content.clone(),
                height: height * t.scale_factor(),
                rotation: rotation + t.rotation_angle(),
                font: font.clone(),
            },
            EntityKind::XLine { through, dir } => EntityKind::XLine {
                through: t.apply_point(through),
                dir: transform_dir(t, dir),
            },
            EntityKind::Ray { from, dir } => EntityKind::Ray {
                from: t.apply_point(from),
                dir: transform_dir(t, dir),
            },
            EntityKind::Insert { block, transform } => EntityKind::Insert {
                block: block.clone(),
                transform: t.compose(transform),
            },
            EntityKind::Hatch {
                boundary,
                holes,
                fill,
                pattern,
            } => EntityKind::Hatch {
                boundary: boundary.iter().map(|c| t.apply_curve(c)).collect(),
                holes: holes
                    .iter()
                    .map(|h| h.iter().map(|c| t.apply_curve(c)).collect())
                    .collect(),
                fill: *fill,
                pattern: transform_pattern(pattern, t),
            },
            EntityKind::Dimension {
                p1,
                p2,
                line,
                height,
                override_text,
            } => EntityKind::Dimension {
                p1: t.apply_point(p1),
                p2: t.apply_point(p2),
                line: t.apply_point(line),
                height: height * t.scale_factor(),
                override_text: override_text.clone(),
            },
            EntityKind::OrthoDim {
                p1,
                p2,
                line,
                vertical,
                height,
                override_text,
            } => EntityKind::OrthoDim {
                p1: t.apply_point(p1),
                p2: t.apply_point(p2),
                line: t.apply_point(line),
                vertical: *vertical,
                height: height * t.scale_factor(),
                override_text: override_text.clone(),
            },
            EntityKind::AngularDim {
                center,
                p1,
                p2,
                line,
                height,
                override_text,
            } => EntityKind::AngularDim {
                center: t.apply_point(center),
                p1: t.apply_point(p1),
                p2: t.apply_point(p2),
                line: t.apply_point(line),
                height: height * t.scale_factor(),
                override_text: override_text.clone(),
            },
            EntityKind::RadialDim {
                center,
                edge,
                diameter,
                height,
                override_text,
            } => EntityKind::RadialDim {
                center: t.apply_point(center),
                edge: t.apply_point(edge),
                diameter: *diameter,
                height: height * t.scale_factor(),
                override_text: override_text.clone(),
            },
        };
    }

    /// A transformed copy of the entity, leaving the original unchanged.
    pub fn transformed(&self, t: &Transform2d) -> Entity {
        let mut e = self.clone();
        e.transform(t);
        e
    }

    /// Borrows the inner [`Curve`] when this entity is a `Curve`, else `None`.
    pub fn as_curve(&self) -> Option<&Curve> {
        if let EntityKind::Curve(c) = &self.kind {
            Some(c)
        } else {
            None
        }
    }
}

fn bbox_of(pts: &[(f64, f64)]) -> BoundingBox {
    let (mut minx, mut miny) = pts[0];
    let (mut maxx, mut maxy) = pts[0];
    for &(x, y) in pts {
        minx = minx.min(x);
        miny = miny.min(y);
        maxx = maxx.max(x);
        maxy = maxy.max(y);
    }
    BoundingBox::from_corners(minx, miny, maxx, maxy)
}

fn transform_pattern(p: &HatchPattern, t: &Transform2d) -> HatchPattern {
    let s = t.scale_factor();
    let rot = t.rotation_angle().to_degrees();
    match *p {
        HatchPattern::Solid => HatchPattern::Solid,
        HatchPattern::Lines { angle_deg, spacing } => HatchPattern::Lines {
            angle_deg: angle_deg + rot,
            spacing: spacing * s,
        },
        HatchPattern::Cross { angle_deg, spacing } => HatchPattern::Cross {
            angle_deg: angle_deg + rot,
            spacing: spacing * s,
        },
        HatchPattern::Dots { spacing } => HatchPattern::Dots {
            spacing: spacing * s,
        },
    }
}

fn transform_dir(t: &Transform2d, dir: &(f64, f64)) -> (f64, f64) {
    t.apply_vector(dir.0, dir.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxidraft_geometry::LineSeg;

    fn pt(x: i64, y: i64) -> Point2d {
        Point2d::from_i64(x, y)
    }

    #[test]
    fn entity_bbox_for_line() {
        let line = Curve::Line(LineSeg::from_endpoints(pt(0, 0), pt(4, 3)));
        let e = Entity::new(EntityId(1), EntityKind::Curve(line), 0);
        let bb = e.bounding_box().unwrap();
        assert_eq!(bb.min, pt(0, 0));
        assert_eq!(bb.max, pt(4, 3));
    }

    #[test]
    fn move_entity_translates_geometry() {
        let line = Curve::Line(LineSeg::from_endpoints(pt(0, 0), pt(2, 0)));
        let mut e = Entity::new(EntityId(1), EntityKind::Curve(line), 0);
        e.transform(&Transform2d::translation(5.0, 3.0));
        let c = e.as_curve().unwrap();
        if let Curve::Line(l) = c {
            assert_eq!(l.p0, pt(5, 3));
            assert_eq!(l.p1, pt(7, 3));
        } else {
            panic!()
        }
    }

    #[test]
    fn transformed_keeps_original() {
        let line = Curve::Line(LineSeg::from_endpoints(pt(0, 0), pt(2, 0)));
        let e = Entity::new(EntityId(1), EntityKind::Curve(line), 0);
        let moved = e.transformed(&Transform2d::translation(10.0, 0.0));
        if let Curve::Line(l) = e.as_curve().unwrap() {
            assert_eq!(l.p0, pt(0, 0));
        }
        if let Curve::Line(l) = moved.as_curve().unwrap() {
            assert_eq!(l.p0, pt(10, 0));
        }
    }

    #[test]
    fn infinite_lines_have_no_bbox() {
        let e = Entity::new(
            EntityId(1),
            EntityKind::XLine {
                through: pt(0, 0),
                dir: (1.0, 0.0),
            },
            0,
        );
        assert!(e.bounding_box().is_none());
    }
}
