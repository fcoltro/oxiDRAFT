//! Interactive drawing and edit tools: [`Tool`] is the state machine for
//! whichever tool is active (accumulated clicks, live preview, commit), and
//! [`ToolEvent`] is what a click or commit produces for [`crate::state`] to
//! apply to the document.

use oxidraft_document::{ConstraintKind, EntityId, EntityKind};
use oxidraft_geometry::{
    CircularArc, Continuity, Curve, EllipticalArc, LineSeg, NurbsCurve, Point2d, Transform2d,
    cv_spline_segments,
};
/// What a pick step of a `ConPick` tool expects: a point anchor (endpoint,
/// midpoint, center, or point entity) or a whole curve entity of a kind.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConPickStep {
    /// An endpoint / midpoint / center / point-entity anchor.
    Point,
    /// A line entity (its infinite carrier or midpoint is the target).
    Line,
    /// A circle/arc entity (its rim is the target).
    Arc,
}

/// The ordered pick steps a pick-based constraint kind needs. Empty for
/// kinds that aren't pick-based (they never open a `ConPick` tool).
pub fn con_pick_plan(kind: ConstraintKind) -> &'static [ConPickStep] {
    use ConPickStep::*;
    match kind {
        ConstraintKind::Midpoint => &[Point, Line],
        ConstraintKind::PointOnLine => &[Point, Line],
        ConstraintKind::PointOnCircle => &[Point, Arc],
        ConstraintKind::Symmetric => &[Point, Point, Line],
        _ => &[],
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum Tool {
    Select,
    Point,
    Line {
        last: Option<Point2d>,
    },
    /// One circle tool for every construction: picks accumulate as
    /// [`Contribution`]s until the three degrees of freedom are gone, then it
    /// commits. See [`pick_readings`] for how a pick is read.
    Circle {
        /// What has been banked so far.
        parts: Vec<Contribution>,
        /// Which reading of the *next* pick is active, as an index into
        /// [`pick_readings`]. Tab advances it; it resets on every commit.
        choice: usize,
    },
    Arc3 {
        pts: Vec<Point2d>,
    },
    ArcStartCenterEnd {
        start: Option<Point2d>,
        center: Option<Point2d>,
    },
    ArcCenterStartEnd {
        center: Option<Point2d>,
        start: Option<Point2d>,
    },
    CircleTwoPoint {
        first: Option<Point2d>,
    },
    CircleThreePoint {
        pts: Vec<Point2d>,
    },
    CircleTtr {
        radius: f64,
        first: Option<EntityId>,
    },
    CircleTtt {
        picks: Vec<EntityId>,
    },
    TangentLine {
        first: Option<TanAnchor>,
    },
    Dimension {
        p1: Option<Point2d>,
        p2: Option<Point2d>,
    },
    DimAngularLines {
        a: Option<EntityId>,
        geom: Option<(Point2d, Point2d, Point2d)>,
    },
    DimRadial {
        diameter: bool,
        center: Option<Point2d>,
        radius: f64,
    },
    /// Smart dimensioning that adds *driving* constraints (not drafting
    /// annotations): click a line for a driving length, a circle/arc for a
    /// radius, two parallel lines for a width, or two crossing lines for an
    /// angle. `first` holds a line picked so far (it may still pair with a
    /// second line); `pending` holds fully picked geometry whose dimension
    /// preview follows the cursor until the placement click drops it.
    DimConstraint {
        first: Option<EntityId>,
        pending: Option<(EntityId, Option<EntityId>)>,
    },
    /// Pick-based coincident weld: click two points — a line endpoint or
    /// midpoint, an arc/circle center, or a point entity like the origin —
    /// and they are welded coincident. `first` holds the first pick as
    /// (entity, anchor index, anchor position) so the second pick can rubber-
    /// band from it.
    Weld {
        first: Option<(EntityId, u8, Point2d)>,
    },
    /// Pick-based application of one of the point-anchored relations
    /// (Midpoint, PointOnLine, PointOnCircle, Symmetric). `kind` chooses the
    /// relation; `picks` accumulates the anchors/entities picked so far,
    /// each as (entity, anchor index, world position). The number of picks
    /// the relation needs is fixed per kind — see `con_pick_plan`.
    ConPick {
        kind: ConstraintKind,
        picks: Vec<(EntityId, u8, Point2d)>,
    },
    Ellipse {
        center: Option<Point2d>,
        axis_end: Option<Point2d>,
    },
    Rectangle {
        first: Option<Point2d>,
    },
    /// Two-corner pick of the area to plot — feeds the Plot dialog's
    /// "Window" mode rather than creating geometry.
    PlotWindow {
        first: Option<Point2d>,
    },
    Move {
        base: Option<Point2d>,
        ids: Vec<EntityId>,
    },
    Copy {
        base: Option<Point2d>,
        ids: Vec<EntityId>,
    },
    Spline {
        pts: Vec<Point2d>,
    },
    Polyline {
        pts: Vec<Point2d>,
    },
    Polygon {
        center: Option<Point2d>,
        /// Set by the second click (radius/rotation); once set, the shape is
        /// spatially final and the side-count popup takes over — no more
        /// cursor-driven preview, just Apply/Cancel on whatever count is picked.
        radius_point: Option<Point2d>,
        sides: Option<usize>,
    },
    Text {
        anchor: Option<Point2d>,
        height: f64,
    },
    Rotate {
        base: Option<Point2d>,
        ids: Vec<EntityId>,
    },
    Scale {
        base: Option<Point2d>,
        reference: Option<f64>,
        ids: Vec<EntityId>,
    },
    Mirror {
        first: Option<Point2d>,
        ids: Vec<EntityId>,
    },
    Trim,
    Extend,
    Offset {
        dist: f64,
        source: Option<EntityId>,
    },
    Fillet {
        radius: f64,
        first: Option<EntityId>,
    },
    Chamfer {
        dist: f64,
        first: Option<EntityId>,
    },
    Blend {
        continuity: Continuity,
        tension: f64,
        first: Option<EntityId>,
        /// Set once both entities are picked: the blend is not committed yet,
        /// awaiting confirmation from the live-preview popup (Enter/Apply) or
        /// cancellation (Escape), so the user can tune continuity/tension first.
        second: Option<EntityId>,
    },
    Stretch {
        c1: Option<Point2d>,
        c2: Option<Point2d>,
        base: Option<Point2d>,
        ids: Vec<EntityId>,
    },
    Hatch,
}

/// What the tangent-line tool's first pick anchored to: a bare point, or a
/// circle/arc (whose tangent point is solved for at commit time).
#[derive(Clone, Debug)]
pub enum TanAnchor {
    Point(Point2d),
    Circle(EntityId, Point2d),
}

/// What a completed tool interaction asks the app to do to the document.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum ToolEvent {
    /// The tool hasn't finished its interaction yet; nothing to apply.
    Pending,
    /// Add these new entities to the document.
    Create(Vec<EntityKind>),
    /// Move/rotate/scale/mirror the given entities in place by `t`.
    Transform { ids: Vec<EntityId>, t: Transform2d },
    /// Duplicate the given entities, placing the copies at `t`.
    CopyOf { ids: Vec<EntityId>, t: Transform2d },
    /// Add these new entities, then record `relations` against the first of
    /// them — used when a tool knows not just *where* the geometry goes but
    /// *why*, and that reason is worth keeping.
    CreateConstrained {
        /// The entities to add.
        entities: Vec<EntityKind>,
        /// Relations to record against the first created entity.
        relations: Vec<PendingRelation>,
    },
    /// Both corners of the plot window were picked (raw, unsorted).
    PlotWindow(Point2d, Point2d),
}

/// A relation to record against an entity that does not exist yet.
///
/// A tool cannot build a `SketchConstraint` itself: the entity it is creating
/// has no [`EntityId`] until the document accepts it. So the tool names the
/// other side and the kind, and the caller finishes the job once the id exists.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingRelation {
    /// Which relation to record.
    pub kind: ConstraintKind,
    /// The already-existing entity on the other side of it.
    pub other: EntityId,
}

/// A click delivered to a tool: where it landed, and what it snapped to.
///
/// `snap` is `None` for a pick that resolved to nothing — empty space, or
/// snapping switched off. That absence is meaningful rather than missing data:
/// it is how a tool distinguishes "a point the user chose freely" from "a point
/// on something".
#[derive(Clone, Debug)]
pub struct Pick {
    /// Where the pick landed, after snapping.
    pub pos: Point2d,
    /// The snap that produced `pos`, if any.
    pub snap: Option<oxidraft_cad::SnapPoint>,
    /// The curve the snap landed on, cloned by the caller.
    ///
    /// A tool has no document, so it cannot look the entity up itself — but
    /// solving two tangents and a radius needs the actual geometry, not just
    /// an id. The caller has the document, so the caller supplies it.
    pub curve: Option<Curve>,
}

impl Pick {
    /// A pick with no snap behind it, as if the user clicked empty space.
    pub fn bare(pos: Point2d) -> Self {
        Pick {
            pos,
            snap: None,
            curve: None,
        }
    }
}

/// A circle has three degrees of freedom: two for the centre, one for the
/// radius.
pub const CIRCLE_DOF: u8 = 3;

/// One thing a pick contributed towards the circle being built.
///
/// The point is to stop enumerating constructions. `CIRCLE`, `CIRCLE 2P`,
/// `CIRCLE 3P`, `CIRCLE TTR` and `CIRCLE TTT` are five names for five ways of
/// removing the same three degrees of freedom; collecting contributions covers
/// all five, and covers combinations none of them named.
#[derive(Clone, Debug)]
pub enum Contribution {
    /// Centre pinned at a point the user chose freely.
    CenterAt(Point2d),
    /// Centre shared with another entity's centre.
    Concentric(EntityId, Point2d),
    /// The rim passes through this point.
    ThroughPoint(Point2d),
    /// Tangent to an entity. The point is where the tangency was picked, which
    /// is a point on the finished rim — so with the centre known it constrains
    /// the radius exactly the way a through-point does. The entity is carried
    /// for the constraint recorded on commit, and the curve so that two
    /// tangents plus a radius can be solved without reaching for the document.
    TangentTo(EntityId, Point2d, Box<Curve>),
    /// Radius driven to a typed value.
    RadiusIs(f64),
}

impl PartialEq for Contribution {
    /// Equal when two contributions say the same thing about the circle.
    ///
    /// The curve inside `TangentTo` is a snapshot of the entity it names, so
    /// comparing it would let a contribution stop equalling itself after an
    /// unrelated edit elsewhere in the document. Identity here is the entity
    /// and the point, which is what "the user already picked this" means.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Contribution::CenterAt(a), Contribution::CenterAt(b)) => a == b,
            (Contribution::Concentric(a, p), Contribution::Concentric(b, q)) => a == b && p == q,
            (Contribution::ThroughPoint(a), Contribution::ThroughPoint(b)) => a == b,
            (Contribution::TangentTo(a, p, _), Contribution::TangentTo(b, q, _)) => {
                a == b && p == q
            }
            (Contribution::RadiusIs(a), Contribution::RadiusIs(b)) => a == b,
            _ => false,
        }
    }
}

impl Contribution {
    /// How many degrees of freedom this removes.
    pub fn dof(&self) -> u8 {
        match self {
            Contribution::CenterAt(_) | Contribution::Concentric(..) => 2,
            _ => 1,
        }
    }

    /// The centre, if this contribution fixes one.
    pub fn center(&self) -> Option<Point2d> {
        match self {
            Contribution::CenterAt(p) | Contribution::Concentric(_, p) => Some(*p),
            _ => None,
        }
    }

    /// A point known to lie on the rim, if this contribution puts one there.
    pub fn rim_point(&self) -> Option<Point2d> {
        match self {
            Contribution::ThroughPoint(p) | Contribution::TangentTo(_, p, _) => Some(*p),
            _ => None,
        }
    }

    /// A short word for the chooser chip.
    pub fn label(&self) -> &'static str {
        match self {
            Contribution::CenterAt(_) => "Center",
            Contribution::Concentric(..) => "Concentric",
            Contribution::ThroughPoint(_) => "On rim",
            Contribution::TangentTo(..) => "Tangent",
            Contribution::RadiusIs(_) => "Radius",
        }
    }
}

/// Degrees of freedom already spoken for.
pub fn used_dof(parts: &[Contribution]) -> u8 {
    parts.iter().map(Contribution::dof).sum()
}

/// The ways this pick could be read, best first, filtered to those that can
/// still lead somewhere this tool knows how to finish.
///
/// The snap kind decides what a pick *means* — `Endpoint` is a rim point,
/// `Center` is concentric, a curve body is tangency — and that is already what
/// the snap resolver computes, so no geometry is re-derived here.
///
/// What is offered depends on everything banked so far, not just the degrees
/// of freedom left, because the two are not the same question. Three rim
/// points describe a circle; two rim points and a tangent describe one too,
/// but only via an Apollonius solve this does not do. Rather than let a user
/// build a state and then refuse to finish it, combinations that cannot be
/// completed are never offered:
///
/// - a centre makes everything else a free choice — anything 1-DOF finishes it
/// - rim points and tangents do not mix without a centre
/// - a typed radius pairs with a centre, or with two tangents, and nothing else
///
/// Every state reachable through these rules ends in a circle [`solve_circle`]
/// can build.
pub fn pick_readings(pick: &Pick, parts: &[Contribution]) -> Vec<Contribution> {
    use oxidraft_cad::SnapKind as K;
    let remaining = CIRCLE_DOF.saturating_sub(used_dof(parts));
    if remaining == 0 {
        return Vec::new();
    }
    let has_centre = parts.iter().any(|c| c.center().is_some());
    let has_radius = parts.iter().any(|c| matches!(c, Contribution::RadiusIs(_)));
    let tangents = parts
        .iter()
        .filter(|c| matches!(c, Contribution::TangentTo(..)))
        .count();
    let rims = parts
        .iter()
        .filter(|c| matches!(c, Contribution::ThroughPoint(_)))
        .count();

    let allow_centre = !has_centre && remaining >= 2;
    let allow_rim = has_centre || (tangents == 0 && !has_radius && rims < 3);
    let allow_tangent = has_centre || (rims == 0 && tangents < 2);

    let mut out: Vec<Contribution> = Vec::new();
    match pick.snap.as_ref() {
        // Empty space is a freely chosen point: centre by default, because
        // centre-and-radius is overwhelmingly the common circle.
        None => {
            if allow_centre {
                out.push(Contribution::CenterAt(pick.pos));
            }
            if allow_rim {
                out.push(Contribution::ThroughPoint(pick.pos));
            }
        }
        Some(sp) => {
            let p = Point2d::from_f64(sp.pos.0, sp.pos.1);
            match sp.kind {
                K::Center => {
                    if allow_centre {
                        out.push(Contribution::Concentric(sp.entity, p));
                    }
                    if allow_rim {
                        out.push(Contribution::ThroughPoint(p));
                    }
                }
                // Snapping onto the body of a curve reads as tangency, which
                // needs the curve itself — without it only the point is known,
                // and that is a rim point, not a tangency.
                K::Tangent | K::Nearest | K::Perpendicular => {
                    if allow_tangent && let Some(c) = pick.curve.as_ref() {
                        out.push(Contribution::TangentTo(sp.entity, p, Box::new(c.clone())));
                    }
                    if allow_rim {
                        out.push(Contribution::ThroughPoint(p));
                    }
                }
                // Endpoint, Midpoint, Node, Intersection, Quadrant, Insertion:
                // a specific point the user aimed at, so the rim goes through
                // it. Centre stays available as the alternative.
                _ => {
                    if allow_rim {
                        out.push(Contribution::ThroughPoint(p));
                    }
                    if allow_centre {
                        out.push(Contribution::CenterAt(p));
                    }
                }
            }
        }
    }
    out.retain(|c| c.dof() <= remaining);
    out.dedup();
    out
}

/// Banks one pick against the circle being built, committing when the three
/// degrees of freedom are gone.
///
/// Takes the fields rather than the `Tool` so both entry points — the bare
/// `on_point` and the snap-aware `on_pick` — share one implementation.
fn circle_pick(parts: &mut Vec<Contribution>, choice: &mut usize, pick: Pick) -> ToolEvent {
    let opts = pick_readings(&pick, parts);
    let Some(part) = opts.get(*choice % opts.len().max(1)).cloned() else {
        return ToolEvent::Pending;
    };
    // A pick that repeats one already banked adds nothing — most often a
    // double click on the same spot — so it must not spend a degree of
    // freedom.
    if parts.contains(&part) {
        return ToolEvent::Pending;
    }

    let mut next = parts.clone();
    next.push(part);
    if used_dof(&next) >= CIRCLE_DOF {
        // Determined. That is not the same as solvable: three collinear points
        // and a zero radius are both determined and both degenerate, and
        // `solve_circle` says so by returning None. Hold the pick rather than
        // commit something invalid or hand the kernel a singular system.
        return match solve_circle(&next, pick.pos) {
            Some(arc) => {
                let relations = pending_relations(&next);
                parts.clear();
                *choice = 0;
                let entities = vec![EntityKind::Curve(Curve::Arc(arc))];
                if relations.is_empty() {
                    ToolEvent::Create(entities)
                } else {
                    ToolEvent::CreateConstrained {
                        entities,
                        relations,
                    }
                }
            }
            None => ToolEvent::Pending,
        };
    }
    *parts = next;
    *choice = 0;
    ToolEvent::Pending
}

/// The relations worth keeping from a finished set of contributions.
///
/// Only the contributions that name an entity produce one. A point picked in
/// empty space, or a rim point that happened to land on a snap without binding
/// to it, describes where the circle went but not what it is *held* to — so
/// there is nothing to record, and inventing something would over-constrain the
/// sketch for no reason.
pub fn pending_relations(parts: &[Contribution]) -> Vec<PendingRelation> {
    parts
        .iter()
        .filter_map(|p| match p {
            Contribution::Concentric(other, _) => Some(PendingRelation {
                kind: ConstraintKind::Concentric,
                other: *other,
            }),
            Contribution::TangentTo(other, ..) => Some(PendingRelation {
                kind: ConstraintKind::Tangent,
                other: *other,
            }),
            _ => None,
        })
        .collect()
}

/// The circle these contributions determine, or `None` if they do not yet — or
/// if they determine something degenerate, such as three collinear points.
pub fn solve_circle(parts: &[Contribution], near: Point2d) -> Option<CircularArc> {
    if used_dof(parts) < CIRCLE_DOF {
        return None;
    }
    let full = std::f64::consts::TAU;
    let radius = parts.iter().find_map(|p| match p {
        Contribution::RadiusIs(r) => Some(*r),
        _ => None,
    });
    // Two tangents and a radius. Up to four circles satisfy that, so `near`
    // decides which — through the solver the old TTR tool already used, rather
    // than a second implementation of the same construction.
    let tangent_curves: Vec<&Curve> = parts
        .iter()
        .filter_map(|p| match p {
            Contribution::TangentTo(_, _, c) => Some(&**c),
            _ => None,
        })
        .collect();
    if let ([c1, c2], Some(r)) = (tangent_curves.as_slice(), radius) {
        return oxidraft_geometry::tangent_circle_ttr(c1, c2, r, near)
            .map(|(centre, rr)| CircularArc::new(centre, rr, 0.0, full));
    }
    if let Some(c) = parts.iter().find_map(Contribution::center) {
        // Centre plus one more thing. A tangency point sits on the rim, so it
        // fixes the radius exactly as a through-point does.
        let r = radius.or_else(|| {
            parts
                .iter()
                .find_map(|p| p.rim_point().map(|q| c.dist_f64(&q)))
        })?;
        return (r > 1e-9).then(|| CircularArc::new(c, r, 0.0, full));
    }
    let rim: Vec<Point2d> = parts.iter().filter_map(Contribution::rim_point).collect();
    if rim.len() == 3 {
        // Returns None for collinear points, which is the caller's cue to hold
        // the pick rather than commit something degenerate.
        return CircularArc::from_three_points(&rim[0], &rim[1], &rim[2]);
    }
    None
}

impl Tool {
    /// A fresh circle tool with nothing banked.
    pub fn circle() -> Tool {
        Tool::Circle {
            parts: Vec::new(),
            choice: 0,
        }
    }

    /// The tool's display name, as shown in the status bar (also the command
    /// verb for tools activated by typed command).
    pub fn name(&self) -> &'static str {
        match self {
            Tool::Select => "SELECT",
            Tool::Point => "POINT",
            Tool::Line { .. } => "LINE",
            Tool::Circle { .. } => "CIRCLE",
            Tool::Arc3 { .. } => "ARC",
            Tool::ArcStartCenterEnd { .. } => "ARC SCE",
            Tool::ArcCenterStartEnd { .. } => "ARC CSE",
            Tool::CircleTwoPoint { .. } => "CIRCLE 2P",
            Tool::CircleThreePoint { .. } => "CIRCLE 3P",
            Tool::CircleTtr { .. } => "CIRCLE TTR",
            Tool::CircleTtt { .. } => "CIRCLE TTT",
            Tool::TangentLine { .. } => "TANGENT",
            Tool::Dimension { .. } => "DIMENSION",
            Tool::DimAngularLines { .. } => "DIM ANGULAR (2 lines)",
            Tool::DimRadial { diameter: true, .. } => "DIM DIAMETER",
            Tool::DimRadial { .. } => "DIM RADIUS",
            Tool::DimConstraint { .. } => "SMART DIMENSION",
            Tool::Weld { .. } => "WELD",
            Tool::ConPick { .. } => "CONSTRAIN (pick)",
            Tool::Ellipse { .. } => "ELLIPSE",
            Tool::Rectangle { .. } => "RECTANGLE",
            Tool::PlotWindow { .. } => "PLOT WINDOW",
            Tool::Move { .. } => "MOVE",
            Tool::Copy { .. } => "COPY",
            Tool::Spline { .. } => "SPLINE",
            Tool::Polyline { .. } => "POLYLINE",
            Tool::Polygon { .. } => "POLYGON",
            Tool::Text { .. } => "TEXT",
            Tool::Rotate { .. } => "ROTATE",
            Tool::Scale { .. } => "SCALE",
            Tool::Mirror { .. } => "MIRROR",
            Tool::Trim => "TRIM",
            Tool::Extend => "EXTEND",
            Tool::Hatch => "HATCH",
            Tool::Offset { .. } => "OFFSET",
            Tool::Fillet { .. } => "FILLET",
            Tool::Chamfer { .. } => "CHAMFER",
            Tool::Blend { .. } => "BLEND",
            Tool::Stretch { .. } => "STRETCH",
        }
    }

    /// Whether the tool immediately starts its next segment after
    /// completing one, instead of returning to Select (currently just LINE).
    pub fn is_continuous(&self) -> bool {
        matches!(self, Tool::Line { .. })
    }

    /// Whether this tool's clicks pick existing entities rather than placing
    /// new points.
    pub fn picks_entities(&self) -> bool {
        matches!(
            self,
            Tool::Trim
                | Tool::Extend
                | Tool::Offset { .. }
                | Tool::Fillet { .. }
                | Tool::Chamfer { .. }
                | Tool::Blend { .. }
                | Tool::CircleTtr { .. }
                | Tool::CircleTtt { .. }
                | Tool::TangentLine { .. }
                | Tool::DimRadial { center: None, .. }
                | Tool::DimAngularLines { geom: None, .. }
                | Tool::DimConstraint { .. }
                | Tool::Weld { .. }
                | Tool::ConPick { .. }
        )
    }

    /// Whether the cursor should snap to geometry (endpoints, midpoints,
    /// intersections, …) while this tool is active.
    pub fn wants_point_snap(&self) -> bool {
        !matches!(
            self,
            Tool::Select
                | Tool::Trim
                | Tool::Extend
                | Tool::Hatch
                | Tool::Offset { .. }
                | Tool::Fillet { .. }
                | Tool::Chamfer { .. }
                | Tool::Blend { .. }
                | Tool::CircleTtr { .. }
                | Tool::CircleTtt { .. }
                | Tool::DimConstraint { .. }
        )
    }

    /// Feeds a pick to the tool — the position plus the snap that produced it.
    ///
    /// [`Self::on_point`] takes a bare coordinate, so by the time a tool sees a
    /// click it can no longer tell that the point was a *tangent point on
    /// entity 7* rather than an arbitrary spot: `AppState::pointer_moved`
    /// resolves the full [`Pick::snap`], uses it to place the crosshair, and
    /// drops it. A tool that wants to read intent from what was snapped needs
    /// it, so this is the channel that carries it.
    ///
    /// The default forwards to `on_point` and discards the snap, which is the
    /// right answer for every tool whose behaviour does not depend on it. Only
    /// tools that override this see any difference.
    pub fn on_pick(&mut self, pick: Pick) -> ToolEvent {
        if let Tool::Circle { parts, choice } = self {
            return circle_pick(parts, choice, pick);
        }
        self.on_point(pick.pos)
    }

    /// Supplies a typed radius, the one contribution that has no position.
    ///
    /// Returns `Pending` for tools this means nothing to. Typing a radius used
    /// to be faked as a click one radius to the right of the centre, which
    /// works only because "a point on the rim" and "a radius" happen to
    /// constrain the same thing when the centre is already known — and stops
    /// working the moment it is not, which is exactly the tangent-tangent-radius
    /// case.
    pub fn supply_radius(&mut self, r: f64) -> ToolEvent {
        let Tool::Circle { parts, choice } = self else {
            return ToolEvent::Pending;
        };
        if !(r.is_finite() && r > 1e-9) {
            return ToolEvent::Pending;
        }
        let remaining = CIRCLE_DOF.saturating_sub(used_dof(parts));
        if remaining == 0 || parts.iter().any(|p| matches!(p, Contribution::RadiusIs(_))) {
            return ToolEvent::Pending;
        }
        let mut next = parts.clone();
        next.push(Contribution::RadiusIs(r));
        if used_dof(&next) >= CIRCLE_DOF {
            // No cursor here — a typed radius has no position. The last thing
            // the user actually pointed at is the closest stand-in for where
            // they are looking, which is what picks among the four circles a
            // tangent-tangent-radius admits.
            let near = next
                .iter()
                .rev()
                .find_map(|c| c.rim_point().or_else(|| c.center()))
                .unwrap_or(Point2d::from_f64(0.0, 0.0));
            return match solve_circle(&next, near) {
                Some(arc) => {
                    let relations = pending_relations(&next);
                    parts.clear();
                    *choice = 0;
                    let entities = vec![EntityKind::Curve(Curve::Arc(arc))];
                    if relations.is_empty() {
                        ToolEvent::Create(entities)
                    } else {
                        ToolEvent::CreateConstrained {
                            entities,
                            relations,
                        }
                    }
                }
                None => ToolEvent::Pending,
            };
        }
        *parts = next;
        *choice = 0;
        ToolEvent::Pending
    }

    /// Advances which reading of the next pick is active — the Tab cycle.
    /// Does nothing for tools that have no alternatives to offer.
    pub fn cycle_reading(&mut self) {
        if let Tool::Circle { choice, .. } = self {
            *choice = choice.wrapping_add(1);
        }
    }

    /// Drops the last banked contribution, undoing one pick without leaving
    /// the tool. Returns whether anything was there to drop.
    pub fn drop_last_part(&mut self) -> bool {
        match self {
            Tool::Circle { parts, choice } => {
                *choice = 0;
                parts.pop().is_some()
            }
            _ => false,
        }
    }

    /// Feeds a clicked/typed point to the tool, advancing its internal state
    /// and returning what to do with the document (if anything yet).
    pub fn on_point(&mut self, p: Point2d) -> ToolEvent {
        match self {
            Tool::Select | Tool::Text { .. } => ToolEvent::Pending,

            Tool::Point => ToolEvent::Create(vec![EntityKind::Point(p)]),

            Tool::Line { last } => {
                let ev = match last.take() {
                    Some(prev) => ToolEvent::Create(vec![EntityKind::Curve(Curve::Line(
                        LineSeg::from_endpoints(prev, p),
                    ))]),
                    None => ToolEvent::Pending,
                };
                *last = Some(p);
                ev
            }

            // A bare coordinate carries no snap, so it can only ever read as a
            // freely chosen point. `on_pick` is the path that sees more.
            Tool::Circle { parts, choice } => circle_pick(parts, choice, Pick::bare(p)),

            Tool::Arc3 { pts } => {
                pts.push(p);
                if pts.len() == 3 {
                    let arc = CircularArc::from_three_points(&pts[0], &pts[1], &pts[2]);
                    *self = Tool::Arc3 { pts: vec![] };
                    match arc {
                        Some(a) => ToolEvent::Create(vec![EntityKind::Curve(Curve::Arc(a))]),
                        None => ToolEvent::Pending,
                    }
                } else {
                    ToolEvent::Pending
                }
            }

            Tool::ArcStartCenterEnd { start, center } => match (*start, *center) {
                (None, _) => {
                    *start = Some(p);
                    ToolEvent::Pending
                }
                (Some(_), None) => {
                    *center = Some(p);
                    ToolEvent::Pending
                }
                (Some(s), Some(c)) => match arc_start_center_end(&s, &c, &p) {
                    Some(a) => {
                        *self = Tool::ArcStartCenterEnd {
                            start: None,
                            center: None,
                        };
                        ToolEvent::Create(vec![EntityKind::Curve(Curve::Arc(a))])
                    }
                    None => ToolEvent::Pending,
                },
            },

            Tool::ArcCenterStartEnd { center, start } => match (*center, *start) {
                (None, _) => {
                    *center = Some(p);
                    ToolEvent::Pending
                }
                (Some(_), None) => {
                    *start = Some(p);
                    ToolEvent::Pending
                }
                (Some(c), Some(s)) => match arc_start_center_end(&s, &c, &p) {
                    Some(a) => {
                        *self = Tool::ArcCenterStartEnd {
                            center: None,
                            start: None,
                        };
                        ToolEvent::Create(vec![EntityKind::Curve(Curve::Arc(a))])
                    }
                    None => ToolEvent::Pending,
                },
            },

            Tool::CircleTwoPoint { first } => match first.take() {
                None => {
                    *first = Some(p);
                    ToolEvent::Pending
                }
                Some(a) => {
                    let d = a.dist_f64(&p);
                    if d < 1e-9 {
                        *first = Some(a);
                        ToolEvent::Pending
                    } else {
                        *self = Tool::CircleTwoPoint { first: None };
                        ToolEvent::Create(vec![EntityKind::Curve(Curve::Arc(CircularArc::new(
                            a.midpoint(&p),
                            d / 2.0,
                            0.0,
                            std::f64::consts::TAU,
                        )))])
                    }
                }
            },

            Tool::Dimension { p1, p2 } => match (*p1, *p2) {
                (None, _) => {
                    *p1 = Some(p);
                    ToolEvent::Pending
                }
                (Some(_), None) => {
                    *p2 = Some(p);
                    ToolEvent::Pending
                }
                (Some(a), Some(b)) => {
                    *self = Tool::Dimension { p1: None, p2: None };
                    let kind = match oxidraft_document::linear_orientation(a, b, p) {
                        None => EntityKind::Dimension {
                            p1: a,
                            p2: b,
                            line: p,
                            height: 2.5,
                            override_text: None,
                        },
                        Some(vertical) => EntityKind::OrthoDim {
                            p1: a,
                            p2: b,
                            line: p,
                            vertical,
                            height: 2.5,
                            override_text: None,
                        },
                    };
                    ToolEvent::Create(vec![kind])
                }
            },

            Tool::DimAngularLines { geom, .. } => match *geom {
                Some((center, a, b)) => {
                    *self = Tool::DimAngularLines {
                        a: None,
                        geom: None,
                    };
                    ToolEvent::Create(vec![EntityKind::AngularDim {
                        center,
                        p1: a,
                        p2: b,
                        line: p,
                        height: 2.5,
                        override_text: None,
                    }])
                }
                None => ToolEvent::Pending,
            },

            Tool::DimRadial {
                diameter,
                center,
                radius,
            } => {
                let snap = center.map(|c| (c, *radius, *diameter));
                match snap {
                    Some((c, r, dia)) => {
                        let (cx, cy) = c.to_f64();
                        let (dx, dy) = (p.x - cx, p.y - cy);
                        let len = (dx * dx + dy * dy).sqrt();
                        let edge = if len > 1e-9 {
                            Point2d::from_f64(cx + dx / len * r, cy + dy / len * r)
                        } else {
                            Point2d::from_f64(cx + r, cy)
                        };
                        *self = Tool::DimRadial {
                            diameter: dia,
                            center: None,
                            radius: 0.0,
                        };
                        ToolEvent::Create(vec![EntityKind::RadialDim {
                            center: c,
                            edge,
                            diameter: dia,
                            height: 2.5,
                            override_text: None,
                        }])
                    }
                    None => ToolEvent::Pending,
                }
            }

            Tool::CircleThreePoint { pts } => {
                pts.push(p);
                if pts.len() == 3 {
                    let res =
                        oxidraft_geometry::circle_through_three_points(pts[0], pts[1], pts[2]);
                    *self = Tool::CircleThreePoint { pts: vec![] };
                    match res {
                        Some((c, r)) => ToolEvent::Create(vec![EntityKind::Curve(Curve::Arc(
                            CircularArc::new(c, r, 0.0, std::f64::consts::TAU),
                        ))]),
                        None => ToolEvent::Pending,
                    }
                } else {
                    ToolEvent::Pending
                }
            }

            Tool::Ellipse { center, axis_end } => match (*center, *axis_end) {
                (None, _) => {
                    *center = Some(p);
                    ToolEvent::Pending
                }
                (Some(c), None) => {
                    if c.dist_f64(&p) < 1e-9 {
                        ToolEvent::Pending
                    } else {
                        *axis_end = Some(p);
                        ToolEvent::Pending
                    }
                }
                (Some(c), Some(a)) => match ellipse_from_axes(&c, &a, &p) {
                    Some(e) => {
                        *self = Tool::Ellipse {
                            center: None,
                            axis_end: None,
                        };
                        ToolEvent::Create(vec![EntityKind::Curve(Curve::Ellipse(e))])
                    }
                    None => ToolEvent::Pending,
                },
            },

            Tool::Rectangle { first } => match first.take() {
                None => {
                    *first = Some(p);
                    ToolEvent::Pending
                }
                Some(c0) => {
                    // A zero-area pick can't make a rectangle; keep waiting.
                    if c0.dist_f64(&p) < 1e-9 {
                        *first = Some(c0);
                        return ToolEvent::Pending;
                    }
                    *self = Tool::Rectangle { first: None };
                    // Four individual welded lines, not one PolyCurve — the
                    // constraint system only sees Line/Arc/Point entities, so
                    // this is what makes a rectangle's sides dimensionable.
                    // The corner welds are recorded post-create in
                    // `AppState::apply_tool_event`.
                    ToolEvent::Create(
                        rectangle_curves(&c0, &p)
                            .into_iter()
                            .map(EntityKind::Curve)
                            .collect(),
                    )
                }
            },

            Tool::PlotWindow { first } => match first.take() {
                None => {
                    *first = Some(p);
                    ToolEvent::Pending
                }
                Some(c0) => {
                    // One-shot: the pick hands back to the Plot dialog.
                    *self = Tool::Select;
                    ToolEvent::PlotWindow(c0, p)
                }
            },

            Tool::Move { base, ids } => match base.take() {
                None => {
                    *base = Some(p);
                    ToolEvent::Pending
                }
                Some(b) => {
                    let t = Transform2d::translation(p.x - b.x, p.y - b.y);
                    let ids = std::mem::take(ids);
                    ToolEvent::Transform { ids, t }
                }
            },

            Tool::Copy { base, ids } => match base.take() {
                None => {
                    *base = Some(p);
                    ToolEvent::Pending
                }
                Some(b) => {
                    let t = Transform2d::translation(p.x - b.x, p.y - b.y);
                    ToolEvent::CopyOf {
                        ids: ids.clone(),
                        t,
                    }
                }
            },

            Tool::Spline { pts } => {
                pts.push(p);
                ToolEvent::Pending
            }

            Tool::Polyline { pts } => {
                pts.push(p);
                ToolEvent::Pending
            }

            Tool::Polygon {
                center,
                radius_point,
                sides,
            } => match (*center, *radius_point) {
                (None, _) => {
                    // Side count defaults to 6 (or whatever was last used);
                    // it's adjustable via the popup shown after both clicks.
                    if sides.is_none() {
                        *sides = Some(6);
                    }
                    *center = Some(p);
                    ToolEvent::Pending
                }
                (Some(c), None) => {
                    // Second click fixes radius/rotation but does NOT commit:
                    // the side-count popup takes over from here, with Apply
                    // (or Enter) via `Tool::commit` finalizing the entity.
                    if c.dist_f64(&p) >= 1e-9 {
                        *radius_point = Some(p);
                    }
                    ToolEvent::Pending
                }
                (Some(_), Some(_)) => {
                    // Pending confirmation (popup showing); absorb further clicks.
                    ToolEvent::Pending
                }
            },

            Tool::Rotate { base, ids } => match base.take() {
                None => {
                    *base = Some(p);
                    ToolEvent::Pending
                }
                Some(b) => {
                    let angle = (p.y - b.y).atan2(p.x - b.x);
                    let t = Transform2d::rotation_about(&b, angle);
                    ToolEvent::Transform {
                        ids: std::mem::take(ids),
                        t,
                    }
                }
            },

            Tool::Scale {
                base,
                reference,
                ids,
            } => match *base {
                None => {
                    *base = Some(p);
                    ToolEvent::Pending
                }
                Some(b) => match *reference {
                    None => {
                        *reference = Some(b.dist_f64(&p).max(1e-9));
                        ToolEvent::Pending
                    }
                    Some(r1) => {
                        let factor = (b.dist_f64(&p) / r1).max(1e-9);
                        let s = factor;
                        let t = Transform2d::scale_about(&b, s, s);
                        ToolEvent::Transform {
                            ids: std::mem::take(ids),
                            t,
                        }
                    }
                },
            },

            Tool::Mirror { first, ids } => match first.take() {
                None => {
                    *first = Some(p);
                    ToolEvent::Pending
                }
                Some(f) => {
                    let t = Transform2d::mirror_line(&f, &p);
                    ToolEvent::Transform {
                        ids: std::mem::take(ids),
                        t,
                    }
                }
            },

            Tool::Trim
            | Tool::Extend
            | Tool::Hatch
            | Tool::Offset { .. }
            | Tool::Fillet { .. }
            | Tool::Chamfer { .. }
            | Tool::Blend { .. }
            | Tool::Stretch { .. }
            | Tool::CircleTtr { .. }
            | Tool::CircleTtt { .. }
            | Tool::DimConstraint { .. }
            | Tool::Weld { .. }
            | Tool::ConPick { .. }
            | Tool::TangentLine { .. } => ToolEvent::Pending,
        }
    }

    /// Clears the tool's accumulated clicks/picks, returning it to its
    /// initial state without changing which tool is active.
    pub fn reset(&mut self) {
        match self {
            Tool::Line { last } => *last = None,
            Tool::Circle { parts, choice } => {
                parts.clear();
                *choice = 0;
            }
            Tool::Arc3 { pts } => pts.clear(),
            Tool::ArcStartCenterEnd { start, center } => {
                *start = None;
                *center = None;
            }
            Tool::ArcCenterStartEnd { center, start } => {
                *center = None;
                *start = None;
            }
            Tool::CircleTwoPoint { first } => *first = None,
            Tool::CircleThreePoint { pts } => pts.clear(),
            Tool::CircleTtr { first, .. } => *first = None,
            Tool::CircleTtt { picks } => picks.clear(),
            Tool::TangentLine { first } => *first = None,
            Tool::Dimension { p1, p2 } => {
                *p1 = None;
                *p2 = None;
            }
            Tool::DimAngularLines { a, geom } => {
                *a = None;
                *geom = None;
            }
            Tool::DimRadial { center, radius, .. } => {
                *center = None;
                *radius = 0.0;
            }
            Tool::DimConstraint { first, pending } => {
                *first = None;
                *pending = None;
            }
            Tool::Weld { first } => *first = None,
            Tool::ConPick { picks, .. } => picks.clear(),
            Tool::Ellipse { center, axis_end } => {
                *center = None;
                *axis_end = None;
            }
            Tool::Rectangle { first } | Tool::PlotWindow { first } => *first = None,
            Tool::Move { base, .. } | Tool::Copy { base, .. } => *base = None,
            Tool::Spline { pts } => pts.clear(),
            Tool::Polyline { pts } => pts.clear(),
            Tool::Polygon {
                center,
                radius_point,
                ..
            } => {
                *center = None;
                *radius_point = None;
            }
            Tool::Rotate { base, .. } => *base = None,
            Tool::Scale {
                base, reference, ..
            } => {
                *base = None;
                *reference = None;
            }
            Tool::Mirror { first, .. } => *first = None,
            Tool::Offset { source, .. } => *source = None,
            Tool::Fillet { first, .. } => *first = None,
            Tool::Chamfer { first, .. } => *first = None,
            Tool::Blend { first, second, .. } => {
                *first = None;
                *second = None;
            }
            Tool::Stretch { c1, c2, base, .. } => {
                *c1 = None;
                *c2 = None;
                *base = None;
            }
            Tool::Text { anchor, .. } => *anchor = None,
            Tool::Trim | Tool::Extend | Tool::Hatch | Tool::Select | Tool::Point => {}
        }
    }

    /// Whether the tool has at least one click/pick accumulated (so Escape
    /// should reset it rather than deactivate it).
    pub fn has_pending_input(&self) -> bool {
        match self {
            Tool::Line { last } => last.is_some(),
            Tool::Circle { parts, .. } => !parts.is_empty(),
            Tool::Arc3 { pts } => !pts.is_empty(),
            Tool::ArcStartCenterEnd { start, .. } => start.is_some(),
            Tool::ArcCenterStartEnd { center, .. } => center.is_some(),
            Tool::CircleTwoPoint { first } => first.is_some(),
            Tool::CircleThreePoint { pts } => !pts.is_empty(),
            Tool::CircleTtr { first, .. } => first.is_some(),
            Tool::CircleTtt { picks } => !picks.is_empty(),
            Tool::TangentLine { first } => first.is_some(),
            Tool::Dimension { p1, .. } => p1.is_some(),
            Tool::DimAngularLines { a, geom } => a.is_some() || geom.is_some(),
            Tool::DimRadial { center, .. } => center.is_some(),
            Tool::DimConstraint { first, pending } => first.is_some() || pending.is_some(),
            Tool::Weld { first } => first.is_some(),
            Tool::ConPick { picks, .. } => !picks.is_empty(),
            Tool::Ellipse { center, .. } => center.is_some(),
            Tool::Rectangle { first } | Tool::PlotWindow { first } => first.is_some(),
            Tool::Move { base, .. } | Tool::Copy { base, .. } => base.is_some(),
            Tool::Spline { pts } => !pts.is_empty(),
            Tool::Polyline { pts } => !pts.is_empty(),
            Tool::Polygon { center, .. } => center.is_some(),
            Tool::Rotate { base, .. } => base.is_some(),
            Tool::Scale { base, .. } => base.is_some(),
            Tool::Mirror { first, .. } => first.is_some(),
            Tool::Offset { source, .. } => source.is_some(),
            Tool::Fillet { first, .. } => first.is_some(),
            Tool::Chamfer { first, .. } => first.is_some(),
            Tool::Blend { first, .. } => first.is_some(),
            Tool::Stretch { c1, .. } => c1.is_some(),
            Tool::Text { anchor, .. } => anchor.is_some(),
            Tool::Trim | Tool::Extend | Tool::Hatch | Tool::Select | Tool::Point => false,
        }
    }

    /// The live preview geometry for the tool's current in-progress shape,
    /// rubber-banding to `cursor`.
    pub fn preview(&self, cursor: &Point2d) -> Vec<Curve> {
        match self {
            Tool::Line { last: Some(p) } => vec![Curve::Line(LineSeg::from_endpoints(*p, *cursor))],
            // Preview the circle that would commit if the cursor were the
            // next pick, whatever combination has been banked so far.
            Tool::Circle { parts, .. } if !parts.is_empty() => {
                let mut trial = parts.clone();
                trial.push(Contribution::ThroughPoint(*cursor));
                match solve_circle(&trial, *cursor) {
                    Some(arc) => vec![Curve::Arc(arc)],
                    None => vec![],
                }
            }
            Tool::Rectangle { first: Some(c0) } | Tool::PlotWindow { first: Some(c0) } => {
                rectangle_curves(c0, cursor)
            }
            Tool::Ellipse {
                center: Some(c),
                axis_end: None,
            } => vec![Curve::Line(LineSeg::from_endpoints(*c, *cursor))],
            Tool::Ellipse {
                center: Some(c),
                axis_end: Some(a),
            } => match ellipse_from_axes(c, a, cursor) {
                Some(e) => vec![Curve::Ellipse(e)],
                None => vec![Curve::Line(LineSeg::from_endpoints(*c, *a))],
            },
            Tool::Arc3 { pts } if pts.len() == 1 => {
                vec![Curve::Line(LineSeg::from_endpoints(pts[0], *cursor))]
            }
            Tool::Arc3 { pts } if pts.len() == 2 => {
                match CircularArc::from_three_points(&pts[0], &pts[1], cursor) {
                    Some(a) => vec![Curve::Arc(a)],
                    None => vec![Curve::Line(LineSeg::from_endpoints(pts[1], *cursor))],
                }
            }
            Tool::ArcStartCenterEnd {
                start: Some(s),
                center: None,
            } => vec![Curve::Line(LineSeg::from_endpoints(*s, *cursor))],
            Tool::ArcStartCenterEnd {
                start: Some(s),
                center: Some(c),
            } => match arc_start_center_end(s, c, cursor) {
                Some(a) => vec![Curve::Arc(a)],
                None => vec![Curve::Line(LineSeg::from_endpoints(*c, *cursor))],
            },
            Tool::ArcCenterStartEnd {
                center: Some(c),
                start: None,
            } => vec![Curve::Line(LineSeg::from_endpoints(*c, *cursor))],
            Tool::ArcCenterStartEnd {
                center: Some(c),
                start: Some(s),
            } => match arc_start_center_end(s, c, cursor) {
                Some(a) => vec![Curve::Arc(a)],
                None => vec![Curve::Line(LineSeg::from_endpoints(*s, *cursor))],
            },
            Tool::Dimension {
                p1: Some(a),
                p2: None,
            } => vec![Curve::Line(LineSeg::from_endpoints(*a, *cursor))],
            Tool::CircleTwoPoint { first: Some(a) } => {
                let d = a.dist_f64(cursor);
                if d < 1e-9 {
                    vec![]
                } else {
                    vec![Curve::Arc(CircularArc::new(
                        a.midpoint(cursor),
                        d / 2.0,
                        0.0,
                        std::f64::consts::TAU,
                    ))]
                }
            }
            Tool::CircleThreePoint { pts } if pts.len() == 1 => {
                vec![Curve::Line(LineSeg::from_endpoints(pts[0], *cursor))]
            }
            Tool::CircleThreePoint { pts } if pts.len() == 2 => {
                match oxidraft_geometry::circle_through_three_points(pts[0], pts[1], *cursor) {
                    Some((c, r)) => vec![Curve::Arc(CircularArc::new(
                        c,
                        r,
                        0.0,
                        std::f64::consts::TAU,
                    ))],
                    None => vec![Curve::Line(LineSeg::from_endpoints(pts[1], *cursor))],
                }
            }
            Tool::Move { base: Some(b), .. }
            | Tool::Copy { base: Some(b), .. }
            | Tool::Rotate { base: Some(b), .. }
            | Tool::Scale { base: Some(b), .. }
            | Tool::Mirror { first: Some(b), .. }
            | Tool::Stretch { base: Some(b), .. } => {
                vec![Curve::Line(LineSeg::from_endpoints(*b, *cursor))]
            }
            Tool::Spline { pts } => {
                let mut cv = pts.clone();
                cv.push(*cursor);
                let mut out = line_chain(&cv);
                if pts.len() >= 3 {
                    out.extend(cv_spline_segments(pts).into_iter().map(Curve::Rational));
                }
                out
            }
            Tool::Polyline { pts } => {
                let mut curves = line_chain(pts);
                if let Some(last) = pts.last() {
                    curves.push(Curve::Line(LineSeg::from_endpoints(*last, *cursor)));
                }
                curves
            }
            Tool::Polygon {
                center: Some(c),
                radius_point,
                sides: Some(n),
            } => {
                // Before the radius click: follow the cursor. After it: the
                // shape is spatially final, only the side count popup can
                // still change it, so ignore the cursor and use the fixed point.
                let rp = radius_point.unwrap_or(*cursor);
                let cx = c.x;
                let cy = c.y;
                let dx = rp.x - cx;
                let dy = rp.y - cy;
                let r = (dx * dx + dy * dy).sqrt();
                let start_angle = dy.atan2(dx);
                let verts = polygon_vertices(cx, cy, r, start_angle, *n);
                closed_chain(&verts)
            }
            _ => vec![],
        }
    }

    /// The most recent point the tool has anchored to, used as the origin
    /// for relative/polar coordinate entry (`@dx,dy`) at the command line.
    pub fn reference_point(&self) -> Option<Point2d> {
        match self {
            Tool::Line { last } => *last,
            Tool::Circle { parts, .. } => parts
                .iter()
                .find_map(Contribution::center)
                .or_else(|| parts.iter().rev().find_map(Contribution::rim_point)),
            Tool::Rectangle { first } | Tool::PlotWindow { first } => *first,
            Tool::Arc3 { pts } => pts.last().cloned(),
            Tool::ArcStartCenterEnd { start, center } => (*center).or(*start),
            Tool::ArcCenterStartEnd { center, start } => (*start).or(*center),
            Tool::CircleTwoPoint { first } => *first,
            Tool::CircleThreePoint { pts } => pts.last().cloned(),
            Tool::Ellipse { center, axis_end } => (*axis_end).or(*center),
            Tool::Move { base, .. } => *base,
            Tool::Copy { base, .. } => *base,
            Tool::Spline { pts } => pts.last().cloned(),
            Tool::Polyline { pts } => pts.last().cloned(),
            Tool::Polygon { center, .. } => *center,
            Tool::Rotate { base, .. } => *base,
            Tool::Scale { base, .. } => *base,
            Tool::Mirror { first, .. } => *first,
            Tool::Stretch { base, c1, .. } => (*base).or(*c1),
            Tool::Text { .. }
            | Tool::Trim
            | Tool::Extend
            | Tool::Hatch
            | Tool::Offset { .. }
            | Tool::Fillet { .. }
            | Tool::Chamfer { .. }
            | Tool::Blend { .. }
            | Tool::CircleTtr { .. }
            | Tool::CircleTtt { .. }
            | Tool::DimConstraint { .. } => None,
            Tool::Weld { first } => first.map(|(_, _, p)| p),
            Tool::ConPick { picks, .. } => picks.last().map(|(_, _, p)| *p),
            Tool::TangentLine { first } => match first {
                Some(TanAnchor::Point(p)) => Some(*p),
                _ => None,
            },
            Tool::Dimension { p1, p2 } => (*p2).or(*p1),
            Tool::DimAngularLines { geom, .. } => geom.map(|(v, _, _)| v),
            Tool::DimRadial { center, .. } => *center,
            Tool::Select | Tool::Point => None,
        }
    }

    /// Every point placed so far in a multi-point tool (polyline, spline,
    /// arc-by-3-points, …), for drawing the in-progress vertex markers.
    pub fn in_progress_points(&self) -> Vec<Point2d> {
        match self {
            Tool::Polyline { pts } | Tool::Spline { pts } => pts.clone(),
            Tool::Arc3 { pts } | Tool::CircleThreePoint { pts } => pts.clone(),
            Tool::Line { last: Some(p) } => vec![*p],
            Tool::Rectangle { first: Some(p) } | Tool::PlotWindow { first: Some(p) } => vec![*p],
            Tool::Polygon {
                center: Some(c), ..
            } => vec![*c],
            _ => Vec::new(),
        }
    }

    /// Finalizes an open-ended multi-point tool (polyline, spline) on an
    /// explicit "done" action (e.g. Enter), creating an open chain from the
    /// points placed so far.
    pub fn commit(&mut self) -> ToolEvent {
        match self {
            Tool::Polyline { pts } => {
                // Individual welded lines, not one PolyCurve, so every
                // segment can carry constraints (welds recorded post-create
                // in `AppState::apply_tool_event`); JOIN reassembles a
                // single outline entity when one is wanted.
                if pts.len() >= 2 {
                    let lines = line_chain(pts);
                    *self = Tool::Polyline { pts: Vec::new() };
                    ToolEvent::Create(lines.into_iter().map(EntityKind::Curve).collect())
                } else {
                    *self = Tool::Polyline { pts: Vec::new() };
                    ToolEvent::Pending
                }
            }
            Tool::Spline { pts } => {
                let ev = spline_event(pts);
                *self = Tool::Spline { pts: Vec::new() };
                ev
            }
            Tool::Polygon {
                center,
                radius_point,
                sides,
            } => {
                let (Some(c), Some(rp), Some(n)) = (*center, *radius_point, *sides) else {
                    return ToolEvent::Pending;
                };
                let dx = rp.x - c.x;
                let dy = rp.y - c.y;
                let r = (dx * dx + dy * dy).sqrt();
                *center = None;
                *radius_point = None;
                if r < 1e-9 || n < 3 {
                    return ToolEvent::Pending;
                }
                let start_angle = dy.atan2(dx);
                let verts = polygon_vertices(c.x, c.y, r, start_angle, n);
                // n individual welded lines, not one PolyCurve, so each side
                // can carry constraints (welds recorded post-create in
                // `AppState::apply_tool_event`).
                ToolEvent::Create(
                    closed_chain(&verts)
                        .into_iter()
                        .map(EntityKind::Curve)
                        .collect(),
                )
            }
            _ => ToolEvent::Pending,
        }
    }

    /// Like [`commit`](Self::commit), but closes the chain back to its first
    /// point (polyline's "Close" action).
    pub fn close_and_commit(&mut self) -> ToolEvent {
        match self {
            Tool::Polyline { pts } => {
                if pts.len() >= 2 {
                    let mut segments = line_chain(pts);
                    segments.push(Curve::Line(LineSeg::from_endpoints(
                        *pts.last().unwrap(),
                        pts[0],
                    )));
                    *self = Tool::Polyline { pts: Vec::new() };
                    // Welded lines, closing corner included — see `commit`.
                    ToolEvent::Create(segments.into_iter().map(EntityKind::Curve).collect())
                } else {
                    *self = Tool::Polyline { pts: Vec::new() };
                    ToolEvent::Pending
                }
            }
            Tool::Spline { pts } => {
                let mut cv = pts.clone();
                if cv.len() >= 3 {
                    cv.push(cv[0]);
                }
                let ev = spline_event(&cv);
                *self = Tool::Spline { pts: Vec::new() };
                ev
            }
            _ => ToolEvent::Pending,
        }
    }
}
fn spline_event(cv: &[Point2d]) -> ToolEvent {
    match cv.len() {
        0 | 1 => ToolEvent::Pending,
        2 => ToolEvent::Create(vec![EntityKind::Curve(Curve::Line(
            LineSeg::from_endpoints(cv[0], cv[1]),
        ))]),
        _ => ToolEvent::Create(vec![EntityKind::Curve(Curve::Nurbs(NurbsCurve::uniform(
            cv.to_vec(),
        )))]),
    }
}
fn ellipse_from_axes(center: &Point2d, axis_end: &Point2d, p3: &Point2d) -> Option<EllipticalArc> {
    let dx = axis_end.x - center.x;
    let dy = axis_end.y - center.y;
    let semi_major = (dx * dx + dy * dy).sqrt();
    if semi_major < 1e-9 {
        return None;
    }
    let rotation = dy.atan2(dx);
    let (nx, ny) = (-rotation.sin(), rotation.cos());
    let semi_minor = ((p3.x - center.x) * nx + (p3.y - center.y) * ny).abs();
    if semi_minor < 1e-9 {
        return None;
    }
    Some(EllipticalArc::new(
        *center,
        semi_major,
        semi_minor,
        rotation,
        0.0,
        std::f64::consts::TAU,
    ))
}

fn rectangle_curves(c0: &Point2d, c1: &Point2d) -> Vec<Curve> {
    let (x0, x1) = order(c0.x, c1.x);
    let (y0, y1) = order(c0.y, c1.y);
    let p = |x: f64, y: f64| Point2d::new(x, y);
    let corners = [p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)];
    closed_chain(&corners)
}
/// Line segments joining each consecutive pair of points (open chain).
fn line_chain(pts: &[Point2d]) -> Vec<Curve> {
    pts.windows(2)
        .map(|w| Curve::Line(LineSeg::from_endpoints(w[0], w[1])))
        .collect()
}

/// Line segments around a closed loop, including the segment back to the start.
fn closed_chain(pts: &[Point2d]) -> Vec<Curve> {
    let n = pts.len();
    (0..n)
        .map(|i| Curve::Line(LineSeg::from_endpoints(pts[i], pts[(i + 1) % n])))
        .collect()
}

/// Vertices of a regular `n`-gon centred at `(cx, cy)` with circumradius `r`,
/// starting from `start_angle`.
fn polygon_vertices(cx: f64, cy: f64, r: f64, start_angle: f64, n: usize) -> Vec<Point2d> {
    (0..n)
        .map(|i| {
            let a = start_angle + (i as f64) * std::f64::consts::TAU / (n as f64);
            Point2d::from_f64(cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

fn arc_start_center_end(start: &Point2d, center: &Point2d, end: &Point2d) -> Option<CircularArc> {
    let r = center.dist_f64(start);
    if r < 1e-9 {
        return None;
    }
    let sa = (start.y - center.y).atan2(start.x - center.x);
    let mut ea = (end.y - center.y).atan2(end.x - center.x);
    while ea <= sa {
        ea += std::f64::consts::TAU;
    }
    Some(CircularArc::new(*center, r, sa, ea))
}

fn order(a: f64, b: f64) -> (f64, f64) {
    if a <= b { (a, b) } else { (b, a) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: i64, y: i64) -> Point2d {
        Point2d::from_i64(x, y)
    }

    #[test]
    fn line_tool_chains_segments() {
        let mut t = Tool::Line { last: None };
        assert!(matches!(t.on_point(pt(0, 0)), ToolEvent::Pending));
        match t.on_point(pt(5, 0)) {
            ToolEvent::Create(es) => assert_eq!(es.len(), 1),
            o => panic!("{:?}", o),
        }
        assert!(matches!(t.on_point(pt(5, 5)), ToolEvent::Create(_)));
        assert!(t.is_continuous());
    }

    #[test]
    fn a_plain_circle_is_still_two_clicks_with_no_stop() {
        // The acceptance test for the whole redesign. Centre-and-radius in
        // empty space is the circle people draw hundreds of times, and it was
        // two clicks before contributions existed. If collecting constraints
        // ever costs it a third click, or a confirmation, the redesign has
        // made the common case worse than the five tools it replaced — which
        // is the one outcome that is not worth any amount of inference.
        let mut t = Tool::circle();
        assert!(
            matches!(t.on_pick(Pick::bare(pt(0, 0))), ToolEvent::Pending),
            "the first click must simply bank the centre"
        );
        let arc = committed_arc(t.on_pick(Pick::bare(pt(0, 6))));
        assert!((arc.radius - 6.0).abs() < 1e-6, "r {}", arc.radius);
        assert!(
            matches!(&t, Tool::Circle { parts, .. } if parts.is_empty()),
            "and the tool must be ready for the next circle straight away"
        );
    }

    #[test]
    fn two_tangents_and_a_radius_build_the_ttr_circle() {
        // The construction that used to need its own tool and its own menu
        // entry, reached by picking two edges and typing a number. Two axes
        // and radius 2 put the centre at (2,2) — one of the four solutions,
        // chosen because it is the one nearest the picks.
        use oxidraft_cad::SnapKind;
        let x_axis = Curve::Line(LineSeg::from_endpoints(pt(-10, 0), pt(10, 0)));
        let y_axis = Curve::Line(LineSeg::from_endpoints(pt(0, -10), pt(0, 10)));

        let mut t = Tool::circle();
        assert!(matches!(
            t.on_pick(snapped_on(SnapKind::Nearest, 1, pt(4, 0), x_axis)),
            ToolEvent::Pending
        ));
        assert!(matches!(
            t.on_pick(snapped_on(SnapKind::Nearest, 2, pt(0, 4), y_axis)),
            ToolEvent::Pending
        ));
        let ev = t.supply_radius(2.0);
        let arc = committed_arc(ev.clone());
        let (cx, cy) = arc.center.to_f64();
        assert!(
            (cx.abs() - 2.0).abs() < 1e-6 && (cy.abs() - 2.0).abs() < 1e-6,
            "centre should sit one radius off each axis, got ({cx},{cy})"
        );
        assert!((arc.radius - 2.0).abs() < 1e-6, "r {}", arc.radius);

        // And it commits knowing what it is tangent to, not just where it is.
        match ev {
            ToolEvent::CreateConstrained { relations, .. } => {
                assert_eq!(relations.len(), 2, "both tangencies: {relations:?}");
                assert!(
                    relations.iter().all(|r| r.kind == ConstraintKind::Tangent),
                    "{relations:?}"
                );
            }
            o => panic!("expected the tangencies to be recorded, got {o:?}"),
        }
    }

    #[test]
    fn a_rim_point_and_a_tangent_are_never_mixed_without_a_centre() {
        // Two rim points and a tangent is a real construction, but an
        // Apollonius one this does not solve. Offering it would let a user
        // bank three picks and then be told no, so it is never offered: after
        // a rim point a curve snap reads as another rim point, and after a
        // tangent no rim point is offered at all.
        use oxidraft_cad::SnapKind;
        let after_rim = [Contribution::ThroughPoint(pt(0, 0))];
        let readings = pick_readings(&snapped(SnapKind::Nearest, 7, pt(5, 5)), &after_rim);
        assert!(
            !readings
                .iter()
                .any(|c| matches!(c, Contribution::TangentTo(..))),
            "tangency must not be offered after a rim point: {readings:?}"
        );

        let after_tangent = [Contribution::TangentTo(
            EntityId(7),
            pt(5, 5),
            Box::new(Curve::Line(LineSeg::from_endpoints(pt(0, 5), pt(9, 5)))),
        )];
        let readings = pick_readings(&snapped(SnapKind::Endpoint, 8, pt(1, 1)), &after_tangent);
        assert!(
            !readings
                .iter()
                .any(|c| matches!(c, Contribution::ThroughPoint(_))),
            "a rim point must not be offered after a tangency: {readings:?}"
        );
    }

    #[test]
    fn a_typed_radius_is_recorded_as_a_radius() {
        // It used to be applied as a click one radius east of the centre,
        // which lands the same circle but says the wrong thing — and only
        // works at all because the centre is already known.
        let mut t = Tool::circle();
        t.on_pick(Pick::bare(pt(3, 3)));
        let arc = committed_arc(t.supply_radius(2.5));
        let (cx, cy) = arc.center.to_f64();
        assert!(
            (cx - 3.0).abs() < 1e-9 && (cy - 3.0).abs() < 1e-9,
            "the centre must not move: ({cx},{cy})"
        );
        assert!((arc.radius - 2.5).abs() < 1e-9, "r {}", arc.radius);
    }

    #[test]
    fn a_typed_radius_is_ignored_when_it_cannot_apply() {
        // No centre yet, a nonsense value, or a radius already given: all are
        // refused rather than banked, so the tool cannot be walked into a
        // state it will not solve.
        let mut fresh = Tool::circle();
        assert!(matches!(fresh.supply_radius(0.0), ToolEvent::Pending));
        assert!(matches!(fresh.supply_radius(f64::NAN), ToolEvent::Pending));
        assert!(
            matches!(&fresh, Tool::Circle { parts, .. } if parts.is_empty()),
            "nothing should have been banked"
        );

        let mut other = Tool::circle();
        other.on_pick(Pick::bare(pt(0, 0)));
        assert!(matches!(other.supply_radius(-1.0), ToolEvent::Pending));
        assert!(
            matches!(&other, Tool::Circle { parts, .. } if parts.len() == 1),
            "a negative radius must not be banked either"
        );
    }

    /// A pick that snapped to `kind` on entity `id`, at `p`, carrying a
    /// horizontal line as the snapped curve — enough for tangency to read.
    fn snapped(kind: oxidraft_cad::SnapKind, id: u64, p: Point2d) -> Pick {
        snapped_on(
            kind,
            id,
            p,
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(p.x - 10.0, p.y),
                Point2d::from_f64(p.x + 10.0, p.y),
            )),
        )
    }

    /// A snapped pick carrying an explicit curve.
    fn snapped_on(kind: oxidraft_cad::SnapKind, id: u64, p: Point2d, curve: Curve) -> Pick {
        Pick {
            curve: Some(curve),
            pos: p,
            snap: Some(oxidraft_cad::SnapPoint {
                kind,
                pos: p.to_f64(),
                entity: EntityId(id),
            }),
        }
    }

    /// The arc a commit produced, whichever way it committed.
    ///
    /// A circle built only from free points commits as `Create`; one bound to
    /// an entity commits as `CreateConstrained` so the relation can be
    /// recorded. Both are commits, and a test about geometry should not care
    /// which — the relation itself is asserted separately.
    fn committed_arc(ev: ToolEvent) -> CircularArc {
        let entities = match &ev {
            ToolEvent::Create(es) => es,
            ToolEvent::CreateConstrained { entities, .. } => entities,
            o => panic!("expected a commit, got {o:?}"),
        };
        match &entities[0] {
            EntityKind::Curve(Curve::Arc(a)) => *a,
            o => panic!("expected an arc, got {o:?}"),
        }
    }

    #[test]
    fn three_snapped_points_make_a_circle_through_them() {
        // No centre is ever picked. Three endpoints each read as "the rim
        // passes through here", which is the 3-point construction — reached
        // without the user naming it.
        use oxidraft_cad::SnapKind;
        let mut t = Tool::circle();
        assert!(matches!(
            t.on_pick(snapped(SnapKind::Endpoint, 1, pt(-5, 0))),
            ToolEvent::Pending
        ));
        assert!(matches!(
            t.on_pick(snapped(SnapKind::Endpoint, 2, pt(5, 0))),
            ToolEvent::Pending
        ));
        let arc = committed_arc(t.on_pick(snapped(SnapKind::Midpoint, 3, pt(0, 5))));
        let (cx, cy) = arc.center.to_f64();
        assert!(
            cx.abs() < 1e-6 && cy.abs() < 1e-6 && (arc.radius - 5.0).abs() < 1e-6,
            "expected the circle through (-5,0),(5,0),(0,5): centre ({cx},{cy}) r {}",
            arc.radius
        );
    }

    #[test]
    fn a_center_snap_reads_as_concentric_not_as_a_rim_point() {
        // Snapping the centre of an existing circle should place *this*
        // circle's centre there — two degrees of freedom — so one more pick
        // finishes it. If it were read as a rim point it would take three.
        use oxidraft_cad::SnapKind;
        let mut t = Tool::circle();
        assert!(matches!(
            t.on_pick(snapped(SnapKind::Center, 9, pt(2, 2))),
            ToolEvent::Pending
        ));
        // Assert the *contribution*, not just the resulting circle. A plain
        // `CenterAt` at the same spot produces identical geometry, so checking
        // only the centre and radius would pass even if the snap never
        // reached the tool.
        let Tool::Circle { parts, .. } = &t else {
            panic!("expected the circle tool")
        };
        assert!(
            matches!(parts.as_slice(), [Contribution::Concentric(EntityId(9), _)]),
            "expected a concentric contribution naming entity 9, got {parts:?}"
        );

        let arc = committed_arc(t.on_pick(Pick::bare(pt(2, 7))));
        let (cx, cy) = arc.center.to_f64();
        assert!(
            (cx - 2.0).abs() < 1e-6 && (cy - 2.0).abs() < 1e-6 && (arc.radius - 5.0).abs() < 1e-6,
            "centre ({cx},{cy}) r {}",
            arc.radius
        );
    }

    #[test]
    fn a_curve_snap_reads_as_tangent_once_the_centre_is_pinned() {
        // The tangency point lies on the finished rim, so it fixes the radius.
        // The entity is carried on the contribution for the constraint that
        // will be recorded later.
        use oxidraft_cad::SnapKind;
        let mut t = Tool::circle();
        t.on_point(pt(0, 0));
        let Tool::Circle { parts, .. } = &t else {
            panic!("expected the circle tool")
        };
        assert_eq!(parts.len(), 1, "the centre should be banked");

        let readings = pick_readings(&snapped(SnapKind::Tangent, 4, pt(0, 3)), parts);
        assert!(
            matches!(
                readings.first(),
                Some(Contribution::TangentTo(EntityId(4), ..))
            ),
            "a curve snap with the centre already pinned should read as \
             tangent first, got {readings:?}"
        );

        let arc = committed_arc(t.on_pick(snapped(SnapKind::Tangent, 4, pt(0, 3))));
        assert!((arc.radius - 3.0).abs() < 1e-6, "r {}", arc.radius);
    }

    #[test]
    fn centre_stops_being_offered_once_it_cannot_fit() {
        // "Centre here" costs two of the three degrees of freedom. After two
        // rim points only one is left, so offering it would overshoot — this
        // is what stops a pick spending more than the circle has.
        let all = pick_readings(&Pick::bare(pt(1, 1)), &[]);
        assert!(
            all.iter().any(|c| matches!(c, Contribution::CenterAt(_))),
            "with everything free, centre should be the default: {all:?}"
        );
        assert_eq!(all.first().map(Contribution::label), Some("Center"));

        let two_rim = [
            Contribution::ThroughPoint(pt(0, 0)),
            Contribution::ThroughPoint(pt(4, 0)),
        ];
        let tight = pick_readings(&Pick::bare(pt(1, 1)), &two_rim);
        assert!(
            !tight.iter().any(|c| matches!(c, Contribution::CenterAt(_))),
            "centre costs 2 and only 1 was left, so it must not be offered: {tight:?}"
        );
        assert!(
            tight
                .iter()
                .any(|c| matches!(c, Contribution::ThroughPoint(_)))
        );
    }

    #[test]
    fn three_collinear_points_are_held_rather_than_committed() {
        // Determined is not the same as solvable. The third pick completes the
        // degrees of freedom but describes no circle, so it must not commit —
        // and must not hand the kernel a singular system either.
        use oxidraft_cad::SnapKind;
        let mut t = Tool::circle();
        t.on_pick(snapped(SnapKind::Endpoint, 1, pt(0, 0)));
        t.on_pick(snapped(SnapKind::Endpoint, 2, pt(5, 0)));
        let ev = t.on_pick(snapped(SnapKind::Endpoint, 3, pt(10, 0)));
        assert!(
            matches!(ev, ToolEvent::Pending),
            "collinear points should be held, got {ev:?}"
        );
        let Tool::Circle { parts, .. } = &t else {
            panic!("expected the circle tool")
        };
        assert_eq!(
            parts.len(),
            2,
            "the refused pick must not be banked, or the tool deadlocks"
        );
    }

    #[test]
    fn tab_cycles_to_the_other_reading_of_the_same_pick() {
        // The override path: an endpoint reads as a rim point by default, but
        // Tab should reach "centre here" without leaving the tool.
        use oxidraft_cad::SnapKind;
        let mut t = Tool::circle();
        t.cycle_reading();
        let arc = committed_arc({
            t.on_pick(snapped(SnapKind::Endpoint, 1, pt(0, 0)));
            t.on_pick(Pick::bare(pt(0, 4)))
        });
        let (cx, cy) = arc.center.to_f64();
        assert!(
            cx.abs() < 1e-6 && cy.abs() < 1e-6 && (arc.radius - 4.0).abs() < 1e-6,
            "after Tab the endpoint should have been the centre: ({cx},{cy}) r {}",
            arc.radius
        );
    }

    #[test]
    fn a_repeated_pick_does_not_spend_a_degree_of_freedom() {
        let mut t = Tool::circle();
        t.on_point(pt(0, 0));
        assert!(matches!(t.on_point(pt(0, 0)), ToolEvent::Pending));
        let Tool::Circle { parts, .. } = &t else {
            panic!("expected the circle tool")
        };
        assert_eq!(
            parts.len(),
            1,
            "a double click on one spot is still one pick"
        );
    }

    #[test]
    fn circle_tool_center_radius() {
        let mut t = Tool::circle();
        assert!(matches!(t.on_point(pt(0, 0)), ToolEvent::Pending));
        match t.on_point(pt(3, 4)) {
            ToolEvent::Create(es) => {
                assert_eq!(es.len(), 1);
                if let EntityKind::Curve(Curve::Arc(a)) = &es[0] {
                    assert!((a.radius - 5.0).abs() < 1e-6);
                } else {
                    panic!()
                }
            }
            o => panic!("{:?}", o),
        }
    }

    #[test]
    fn ellipse_tool_center_axis_minor() {
        let mut t = Tool::Ellipse {
            center: None,
            axis_end: None,
        };
        assert!(matches!(t.on_point(pt(0, 0)), ToolEvent::Pending));
        assert!(matches!(t.on_point(pt(10, 0)), ToolEvent::Pending));
        match t.on_point(pt(0, 4)) {
            ToolEvent::Create(es) => {
                assert_eq!(es.len(), 1);
                if let EntityKind::Curve(Curve::Ellipse(e)) = &es[0] {
                    assert!((e.semi_major - 10.0).abs() < 1e-6);
                    assert!((e.semi_minor - 4.0).abs() < 1e-6);
                    assert!(e.rotation.abs() < 1e-9);
                } else {
                    panic!("expected an ellipse, got {:?}", es[0])
                }
            }
            o => panic!("{:?}", o),
        }
        assert!(matches!(
            t,
            Tool::Ellipse {
                center: None,
                axis_end: None
            }
        ));
    }

    #[test]
    fn plot_window_tool_hands_both_corners_to_the_dialog() {
        let mut t = Tool::PlotWindow { first: None };
        assert!(matches!(t.on_point(pt(8, 6)), ToolEvent::Pending));
        assert!(t.has_pending_input(), "rubber-band preview while picking");
        match t.on_point(pt(2, 1)) {
            ToolEvent::PlotWindow(a, b) => {
                assert_eq!((a.x, a.y, b.x, b.y), (8.0, 6.0, 2.0, 1.0));
            }
            o => panic!("{o:?}"),
        }
        assert!(matches!(t, Tool::Select), "one-shot pick returns to Select");
    }

    #[test]
    fn rectangle_tool_makes_four_individual_lines() {
        let mut t = Tool::Rectangle { first: None };
        assert!(matches!(t.on_point(pt(0, 0)), ToolEvent::Pending));
        match t.on_point(pt(4, 3)) {
            ToolEvent::Create(es) => {
                // Four Line entities (welded post-create), not one PolyCurve —
                // that's what lets each side carry constraints.
                assert_eq!(es.len(), 4, "four individual sides");
                assert!(
                    es.iter()
                        .all(|k| matches!(k, EntityKind::Curve(Curve::Line(_)))),
                    "every side is a Line entity"
                );
            }
            o => panic!("{:?}", o),
        }
    }

    #[test]
    fn rectangle_tool_ignores_a_zero_area_second_corner() {
        let mut t = Tool::Rectangle { first: None };
        t.on_point(pt(2, 2));
        assert!(matches!(t.on_point(pt(2, 2)), ToolEvent::Pending));
        assert!(t.has_pending_input(), "still waiting for a real corner");
    }

    #[test]
    fn move_tool_emits_translation() {
        let ids = vec![EntityId(1), EntityId(2)];
        let mut t = Tool::Move {
            base: None,
            ids: ids.clone(),
        };
        assert!(matches!(t.on_point(pt(0, 0)), ToolEvent::Pending));
        match t.on_point(pt(10, 5)) {
            ToolEvent::Transform { ids: got, t } => {
                assert_eq!(got, ids);
                assert_eq!(t.apply_point(&pt(0, 0)), pt(10, 5));
            }
            o => panic!("{:?}", o),
        }
    }

    #[test]
    fn copy_tool_emits_copy() {
        let mut t = Tool::Copy {
            base: None,
            ids: vec![EntityId(7)],
        };
        t.on_point(pt(1, 1));
        assert!(matches!(t.on_point(pt(4, 1)), ToolEvent::CopyOf { .. }));
    }

    #[test]
    fn arc3_needs_three_points() {
        let mut t = Tool::Arc3 { pts: vec![] };
        assert!(matches!(t.on_point(pt(1, 0)), ToolEvent::Pending));
        assert!(matches!(t.on_point(pt(0, 1)), ToolEvent::Pending));
        assert!(matches!(t.on_point(pt(-1, 0)), ToolEvent::Create(_)));
    }

    #[test]
    fn arc3_preview_matches_commit() {
        let start = pt(1, 0);
        let mid = pt(0, 1);
        let end = pt(-1, 0);

        let prev = Tool::Arc3 {
            pts: vec![start, mid],
        };
        let preview = prev.preview(&end);
        let pa = match preview.as_slice() {
            [Curve::Arc(a)] => *a,
            other => panic!("expected one arc in preview, got {:?}", other),
        };

        let mut t = Tool::Arc3 { pts: vec![] };
        t.on_point(start);
        t.on_point(mid);
        let committed = match t.on_point(end) {
            ToolEvent::Create(es) => match es.as_slice() {
                [EntityKind::Curve(Curve::Arc(a))] => *a,
                other => panic!("expected one arc, got {:?}", other),
            },
            o => panic!("{:?}", o),
        };

        assert!((pa.center.to_f64().0 - committed.center.to_f64().0).abs() < 1e-9);
        assert!((pa.center.to_f64().1 - committed.center.to_f64().1).abs() < 1e-9);
        assert!((pa.start_angle - committed.start_angle).abs() < 1e-9);
        assert!((pa.end_angle - committed.end_angle).abs() < 1e-9);
        assert!(
            (pa.included_angle() - std::f64::consts::PI).abs() < 1e-6,
            "expected a 180° arc, got {}",
            pa.included_angle()
        );
    }

    #[test]
    fn reset_clears_partial() {
        let mut t = Tool::Line { last: None };
        t.on_point(pt(0, 0));
        assert!(t.has_pending_input());
        t.reset();
        assert!(!t.has_pending_input());
    }

    #[test]
    fn polygon_creates_regular_polygon() {
        // Center click, then radius click: the radius click no longer
        // commits directly — it stages the shape (see `Tool::preview`, which
        // switches from cursor-driven to this fixed point) and leaves it for
        // the side-count popup's Apply (`Tool::commit`) to finalize.
        let mut t = Tool::Polygon {
            center: None,
            radius_point: None,
            sides: Some(5),
        };
        assert!(matches!(t.on_point(pt(0, 0)), ToolEvent::Pending));
        assert!(matches!(t.on_point(pt(10, 0)), ToolEvent::Pending));
        assert!(matches!(
            t,
            Tool::Polygon {
                center: Some(_),
                radius_point: Some(_),
                sides: Some(5)
            }
        ));

        match t.commit() {
            ToolEvent::Create(es) => {
                // Five individual side lines (welded post-create), so each
                // side is a constraint target.
                assert_eq!(es.len(), 5, "five individual sides");
                if let EntityKind::Curve(Curve::Line(l)) = &es[0] {
                    assert!(
                        (l.p0.x - 10.0).abs() < 1e-6 && l.p0.y.abs() < 1e-6,
                        "first vertex on the cursor ray"
                    );
                } else {
                    panic!("expected a Line, got {:?}", es[0]);
                }
                assert!(
                    es.iter()
                        .all(|k| matches!(k, EntityKind::Curve(Curve::Line(_)))),
                    "every side is a Line entity"
                );
            }
            o => panic!("{:?}", o),
        }
        assert!(
            matches!(
                t,
                Tool::Polygon {
                    center: None,
                    radius_point: None,
                    sides: Some(5)
                }
            ),
            "commit resets center/radius but keeps the side count for next time"
        );
    }

    #[test]
    fn polygon_center_click_works_before_sides_are_chosen() {
        // No cursor-following "pick sides first" gate anymore: the first
        // click always places the center, defaulting sides to 6 so the tool
        // is immediately in a valid state for the side-count popup and the
        // live radius preview to take over from there.
        let mut t = Tool::Polygon {
            center: None,
            radius_point: None,
            sides: None,
        };
        assert!(matches!(t.on_point(pt(0, 0)), ToolEvent::Pending));
        assert!(matches!(
            t,
            Tool::Polygon {
                center: Some(_),
                radius_point: None,
                sides: Some(6)
            }
        ));
    }

    #[test]
    fn polygon_center_click_preserves_previously_chosen_sides() {
        let mut t = Tool::Polygon {
            center: None,
            radius_point: None,
            sides: Some(8),
        };
        t.on_point(pt(0, 0));
        assert!(matches!(
            t,
            Tool::Polygon {
                center: Some(_),
                radius_point: None,
                sides: Some(8)
            }
        ));
    }

    #[test]
    fn polygon_radius_click_stages_without_committing() {
        let mut t = Tool::Polygon {
            center: None,
            radius_point: None,
            sides: Some(6),
        };
        t.on_point(pt(0, 0));
        assert!(matches!(t.on_point(pt(10, 0)), ToolEvent::Pending));
        assert!(matches!(
            t,
            Tool::Polygon {
                center: Some(_),
                radius_point: Some(_),
                ..
            }
        ));
        // A third click while pending must be absorbed, not re-picked.
        assert!(matches!(t.on_point(pt(99, 99)), ToolEvent::Pending));
        assert!(matches!(
            t,
            Tool::Polygon {
                radius_point: Some(p),
                ..
            } if (p.x - 10.0).abs() < 1e-9 && p.y.abs() < 1e-9
        ));
    }

    #[test]
    fn cv_spline_commits_to_editable_nurbs() {
        let mut t = Tool::Spline { pts: vec![] };
        let cvs = [
            pt(0, 0),
            pt(5, 5),
            pt(10, -5),
            pt(15, 0),
            pt(20, 6),
            pt(25, 0),
        ];
        for p in cvs {
            assert!(matches!(t.on_point(p), ToolEvent::Pending));
        }
        match t.commit() {
            ToolEvent::Create(es) => match &es[0] {
                EntityKind::Curve(Curve::Nurbs(nc)) => {
                    assert_eq!(nc.control().len(), cvs.len());
                    assert_eq!(nc.control()[0], cvs[0]);
                    assert!(nc.weights().iter().all(|&w| w == 1.0));
                }
                o => panic!("expected a Nurbs curve, got {:?}", o),
            },
            o => panic!("{:?}", o),
        }
        assert!(matches!(t, Tool::Spline { ref pts } if pts.is_empty()));
    }

    #[test]
    fn spline_preview_matches_commit_geometry() {
        let pts = vec![pt(0, 0), pt(5, 8), pt(12, 2)];
        let t = Tool::Spline { pts: pts.clone() };
        let commit_segs = oxidraft_geometry::cv_spline_segments(&pts);
        let cursor = Point2d::from_i64(99, -40);
        let preview_segs: Vec<_> = t
            .preview(&cursor)
            .into_iter()
            .filter_map(|c| match c {
                Curve::Rational(rb) => Some(rb),
                _ => None,
            })
            .collect();

        assert_eq!(
            preview_segs, commit_segs,
            "preview spline must match the committed geometry (cursor excluded)"
        );
    }

    #[test]
    fn cv_spline_two_points_is_a_line() {
        let mut t = Tool::Spline { pts: vec![] };
        t.on_point(pt(0, 0));
        t.on_point(pt(4, 2));
        assert!(matches!(t.commit(),
            ToolEvent::Create(es) if matches!(es[0], EntityKind::Curve(Curve::Line(_)))));
    }

    #[test]
    fn polyline_accumulates_and_commits_individual_lines() {
        let mut t = Tool::Polyline { pts: vec![] };
        assert!(matches!(t.on_point(pt(0, 0)), ToolEvent::Pending));
        assert!(matches!(t.on_point(pt(5, 5)), ToolEvent::Pending));
        assert!(matches!(t.on_point(pt(10, 0)), ToolEvent::Pending));

        match t.commit() {
            ToolEvent::Create(es) => {
                // Two individual welded lines, not one PolyCurve — JOIN
                // reassembles a single entity when one is wanted.
                assert_eq!(es.len(), 2);
                assert!(
                    es.iter()
                        .all(|k| matches!(k, EntityKind::Curve(Curve::Line(_)))),
                    "every segment is a Line entity"
                );
            }
            o => panic!("{:?}", o),
        }
    }

    #[test]
    fn polyline_closes_into_individual_lines_with_the_closing_segment() {
        let mut t = Tool::Polyline { pts: vec![] };
        assert!(matches!(t.on_point(pt(0, 0)), ToolEvent::Pending));
        assert!(matches!(t.on_point(pt(5, 5)), ToolEvent::Pending));
        assert!(matches!(t.on_point(pt(10, 0)), ToolEvent::Pending));

        match t.close_and_commit() {
            ToolEvent::Create(es) => {
                assert_eq!(es.len(), 3, "two drawn segments plus the closer");
                let EntityKind::Curve(Curve::Line(last)) = &es[2] else {
                    panic!("expected a Line, got {:?}", es[2]);
                };
                assert_eq!(
                    (last.p1.x, last.p1.y),
                    (0.0, 0.0),
                    "closing segment lands back on the start"
                );
            }
            o => panic!("{:?}", o),
        }
    }
}
