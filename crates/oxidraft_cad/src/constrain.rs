//! Geometric constraints on line entities. Applying a constraint solves it
//! once — the numeric solver in `oxidraft_constraint` moves endpoints the
//! *minimum* amount that satisfies the requested relation, so a nearly
//! horizontal line snaps flat about its own midpoint instead of jumping —
//! and records it on the document so later edits can re-satisfy it via
//! [`resolve_after_edit`] / [`resolve_after_transform`]. Reference geometry
//! (the first pick of a pair) is pinned during the initial solve and never
//! moves.

use oxidraft_constraint::{Constraint, PointVar, ScalarVar, Sketch};
use oxidraft_document::{ConstraintKind, Document, EntityId, EntityKind, SketchConstraint};
use oxidraft_geometry::{CircularArc, Curve, LineSeg, Point2d};
use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

fn line_of(doc: &Document, id: EntityId) -> Option<LineSeg> {
    match &doc.get(id)?.kind {
        EntityKind::Curve(Curve::Line(l)) => Some(l.clone()),
        _ => None,
    }
}

fn arc_of(doc: &Document, id: EntityId) -> Option<CircularArc> {
    match &doc.get(id)?.kind {
        EntityKind::Curve(Curve::Arc(a)) => Some(*a),
        _ => None,
    }
}

fn point_of(doc: &Document, id: EntityId) -> Option<Point2d> {
    match &doc.get(id)?.kind {
        EntityKind::Point(p) => Some(*p),
        _ => None,
    }
}

fn arc_is_full(a: &CircularArc) -> bool {
    (a.end_angle - a.start_angle).abs() >= TAU - 1e-9
}

fn arc_end_pos(a: &CircularArc, i: u8) -> (f64, f64) {
    let th = if i == 0 { a.start_angle } else { a.end_angle };
    (
        a.center.x + a.radius * th.cos(),
        a.center.y + a.radius * th.sin(),
    )
}

fn len(l: &LineSeg) -> f64 {
    (l.p1.x - l.p0.x).hypot(l.p1.y - l.p0.y)
}

fn endpoint(l: &LineSeg, i: u8) -> (f64, f64) {
    if i == 0 {
        (l.p0.x, l.p0.y)
    } else {
        (l.p1.x, l.p1.y)
    }
}

/// Rotates the sketch's initial guess for the b-line slightly about its
/// midpoint. The parallel/perpendicular/horizontal residuals have a saddle
/// when the line starts exactly 90° from the target orientation; a small
/// rotation breaks the symmetry so LM can descend.
fn perturb_line(s: &mut Sketch, b0: PointVar, b1: PointVar, l: &LineSeg) {
    let (mx, my) = ((l.p0.x + l.p1.x) * 0.5, (l.p0.y + l.p1.y) * 0.5);
    let th = 0.05f64;
    let (c, sn) = (th.cos(), th.sin());
    let rot = |x: f64, y: f64| {
        (
            mx + c * (x - mx) - sn * (y - my),
            my + sn * (x - mx) + c * (y - my),
        )
    };
    let (x0, y0) = rot(l.p0.x, l.p0.y);
    let (x1, y1) = rot(l.p1.x, l.p1.y);
    s.set_point(b0, x0, y0);
    s.set_point(b1, x1, y1);
}

/// Applies the constraint to the selected line entities and records it on
/// the document. Single-line kinds (horizontal/vertical) accept any number
/// of lines; pair kinds require exactly two, with the first selected line
/// acting as the fixed reference. Coincident joins the nearest endpoints of
/// the two lines.
pub fn constrain_lines(
    doc: &mut Document,
    selection: &[EntityId],
    kind: ConstraintKind,
) -> Result<String, String> {
    let lines: Vec<(EntityId, LineSeg)> = selection
        .iter()
        .filter_map(|&id| line_of(doc, id).map(|l| (id, l)))
        .collect();

    match kind {
        ConstraintKind::Fixed => {
            Err("Fixed is set automatically on structural anchors, not a selectable command".into())
        }
        ConstraintKind::Tangent => constrain_tangent(doc, selection),
        ConstraintKind::Radius => constrain_radius(doc, selection, None),
        ConstraintKind::Distance => constrain_distance(doc, selection, None),
        ConstraintKind::Horizontal | ConstraintKind::Vertical => {
            if lines.is_empty() {
                return Err(format!("Select at least one line to make {}", kind.label()));
            }
            let mut count = 0;
            for (id, l) in &lines {
                // Record first and validate against the FULL connected
                // component (component_sketch pulls in whatever's already
                // recorded on this entity) before touching geometry — this
                // is what catches e.g. a line already Horizontal being
                // asked to go Vertical, instead of silently leaving both
                // (mutually exclusive) records on it while only the new
                // one's geometry actually holds.
                let candidate = SketchConstraint::single(kind, *id);
                let added = doc.add_constraint(candidate);
                if added {
                    let CompSketch {
                        mut s,
                        vars,
                        constraint_doc_idx,
                    } = component_sketch(doc, &[*id]);
                    anchor_line_lengths(&mut s, doc, &vars);
                    let initial = s.snapshot();
                    if !s.solve_robust().converged {
                        let touched = doc
                            .constraints
                            .iter()
                            .position(|c| c.same_relation(&candidate));
                        let culprits: Vec<usize> = s
                            .diagnose_conflict(&initial)
                            .culprits
                            .into_iter()
                            .filter_map(|i| constraint_doc_idx.get(i).copied())
                            .collect();
                        let conflict = describe_conflict(doc, &culprits, touched);
                        doc.constraints.retain(|c| !c.same_relation(&candidate));
                        return Err(format!(
                            "Could not make the line {} against its existing constraints{conflict}",
                            kind.label()
                        ));
                    }
                }
                let mut s = Sketch::new();
                let a = s.add_point(l.p0.x, l.p0.y);
                let b = s.add_point(l.p1.x, l.p1.y);
                s.constrain(match kind {
                    ConstraintKind::Horizontal => Constraint::Horizontal(a, b),
                    _ => Constraint::Vertical(a, b),
                });
                s.constrain(Constraint::Distance(a, b, len(l)));
                let mut res = s.solve();
                if !res.converged {
                    perturb_line(&mut s, a, b, l);
                    res = s.solve();
                }
                if !res.converged {
                    return Err(format!(
                        "Could not make the line {} (residual {:.2e})",
                        kind.label(),
                        res.residual
                    ));
                }
                write_line(doc, *id, s.point(a), s.point(b));
                // The solve above only ever looks at `id` in isolation, so a
                // coincident (or otherwise linked) neighbour elsewhere in the
                // component is still sitting at the pre-solve position —
                // drag it back into place the same way a live edit would.
                resolve_after_transform(doc, &[*id]);
                count += 1;
            }
            Ok(format!("Made {count} line(s) {}", kind.label()))
        }
        ConstraintKind::Parallel
        | ConstraintKind::Perpendicular
        | ConstraintKind::EqualLength
        | ConstraintKind::Coincident => {
            if lines.len() != 2 {
                return Err(format!(
                    "Select exactly two lines to make them {} (got {})",
                    kind.label(),
                    lines.len()
                ));
            }
            let (ref_id, ref_line) = lines[0].clone();
            let (mov_id, mov_line) = lines[1].clone();

            // Join the endpoint pair that is already closest — computed up
            // front since Coincident's candidate record needs it either way.
            let endpoints = (kind == ConstraintKind::Coincident).then(|| {
                let mut best = (f64::INFINITY, 0u8, 0u8);
                for ea in 0..2u8 {
                    for eb in 0..2u8 {
                        let (ax, ay) = endpoint(&ref_line, ea);
                        let (bx, by) = endpoint(&mov_line, eb);
                        let d = (ax - bx).hypot(ay - by);
                        if d < best.0 {
                            best = (d, ea, eb);
                        }
                    }
                }
                (best.1, best.2)
            });
            let candidate = match endpoints {
                Some((ea, eb)) => SketchConstraint::coincident(ref_id, ea, mov_id, eb),
                None => SketchConstraint::pair(kind, ref_id, mov_id),
            };

            // Record first and validate against the FULL connected
            // component (whatever's already recorded on either line) before
            // touching geometry — this is what catches e.g. the mover
            // already being Perpendicular to a third line that this new
            // Parallel relation can't also hold, instead of silently
            // leaving both records with only the new one geometrically true.
            let added = doc.add_constraint(candidate);
            if added {
                let CompSketch {
                    mut s,
                    vars,
                    constraint_doc_idx,
                } = component_sketch(doc, &[ref_id, mov_id]);
                // Parallel/Perpendicular need the same length-collapse
                // safeguard Horizontal/Vertical do (their real solve below
                // also anchors the mover's length); EqualLength's whole
                // point is to change a length, so it must NOT be anchored,
                // and Coincident doesn't involve length at all.
                if matches!(
                    kind,
                    ConstraintKind::Parallel | ConstraintKind::Perpendicular
                ) {
                    anchor_line_lengths(&mut s, doc, &vars);
                }
                let initial = s.snapshot();
                if !s.solve_robust().converged {
                    let touched = doc
                        .constraints
                        .iter()
                        .position(|c| c.same_relation(&candidate));
                    let culprits: Vec<usize> = s
                        .diagnose_conflict(&initial)
                        .culprits
                        .into_iter()
                        .filter_map(|i| constraint_doc_idx.get(i).copied())
                        .collect();
                    let conflict = describe_conflict(doc, &culprits, touched);
                    doc.constraints.retain(|c| !c.same_relation(&candidate));
                    return Err(format!(
                        "Could not make the lines {} against their existing constraints{conflict}",
                        kind.label()
                    ));
                }
            }

            let mut s = Sketch::new();
            let a0 = s.add_point(ref_line.p0.x, ref_line.p0.y);
            let a1 = s.add_point(ref_line.p1.x, ref_line.p1.y);
            let b0 = s.add_point(mov_line.p0.x, mov_line.p0.y);
            let b1 = s.add_point(mov_line.p1.x, mov_line.p1.y);
            s.constrain(Constraint::Fixed(a0, ref_line.p0.x, ref_line.p0.y));
            s.constrain(Constraint::Fixed(a1, ref_line.p1.x, ref_line.p1.y));
            match kind {
                ConstraintKind::Parallel => {
                    s.constrain(Constraint::Parallel(a0, a1, b0, b1));
                    s.constrain(Constraint::Distance(b0, b1, len(&mov_line)));
                }
                ConstraintKind::Perpendicular => {
                    s.constrain(Constraint::Perpendicular(a0, a1, b0, b1));
                    s.constrain(Constraint::Distance(b0, b1, len(&mov_line)));
                }
                ConstraintKind::EqualLength => {
                    s.constrain(Constraint::EqualLength(a0, a1, b0, b1));
                }
                ConstraintKind::Coincident => {
                    let (ea, eb) = endpoints.expect("computed above for Coincident");
                    let pa = if ea == 0 { a0 } else { a1 };
                    let pb = if eb == 0 { b0 } else { b1 };
                    s.constrain(Constraint::Coincident(pa, pb));
                }
                _ => unreachable!(),
            }
            let mut res = s.solve();
            if !res.converged {
                s.set_point(a0, ref_line.p0.x, ref_line.p0.y);
                s.set_point(a1, ref_line.p1.x, ref_line.p1.y);
                perturb_line(&mut s, b0, b1, &mov_line);
                res = s.solve();
            }
            if !res.converged {
                return Err(format!(
                    "Could not make the lines {} (residual {:.2e})",
                    kind.label(),
                    res.residual
                ));
            }
            write_line(doc, mov_id, s.point(b0), s.point(b1));
            // Same as above: this solve only ever looked at ref_id/mov_id,
            // so a third entity coincident (or otherwise linked) to mov_id
            // is still sitting at mov_id's pre-solve position — drag it
            // back into place the same way a live edit would.
            resolve_after_transform(doc, &[mov_id]);
            Ok(format!(
                "Made the second line {} to the first",
                kind.label()
            ))
        }
    }
}

/// Makes a line and a circular arc tangent. The first-selected entity is
/// the fixed reference: tangent-to-line slides the circle onto the line,
/// tangent-to-circle rotates/translates the line (length kept) until it
/// touches. Records the relation for re-solving on later edits.
fn constrain_tangent(doc: &mut Document, selection: &[EntityId]) -> Result<String, String> {
    if selection.len() != 2 {
        return Err(format!(
            "Select one line and one arc/circle to make them tangent (got {})",
            selection.len()
        ));
    }
    let (first, second) = (selection[0], selection[1]);
    let (line_id, arc_id) = match (
        line_of(doc, first).is_some(),
        arc_of(doc, first).is_some(),
        line_of(doc, second).is_some(),
        arc_of(doc, second).is_some(),
    ) {
        (_, true, _, true) => return constrain_tangent_circles(doc, first, second),
        (true, _, _, true) => (first, second),
        (_, true, true, _) => (second, first),
        _ => return Err("Tangent needs a line and an arc, or two arcs".into()),
    };
    let l = line_of(doc, line_id).expect("classified as line");
    let a = arc_of(doc, arc_id).expect("classified as arc");
    let mut s = Sketch::new();
    let l0 = s.add_point(l.p0.x, l.p0.y);
    let l1 = s.add_point(l.p1.x, l.p1.y);
    let av = add_arc_vars(&mut s, &a);
    let (c, r) = av.circle().expect("arc vars carry a circle");
    if first == line_id {
        for p in [l0, l1] {
            let (x, y) = s.point(p);
            s.constrain(Constraint::Fixed(p, x, y));
        }
    } else {
        pin_shape(&mut s, &av);
        // The line is the mover: keep its length so it slides and rotates
        // rather than collapsing onto the rim.
        s.constrain(Constraint::Distance(l0, l1, len(&l)));
    }
    s.constrain(Constraint::TangentLineCircle(l0, l1, c, r));
    let res = s.solve();
    if !res.converged {
        return Err(format!(
            "Could not make the entities tangent (residual {:.2e})",
            res.residual
        ));
    }
    // Write back only the mover: the pinned reference is a least-squares
    // residual, within ~1e-10 of its coordinates but not bit-exact.
    let mut vars = HashMap::new();
    if first == line_id {
        vars.insert(arc_id, av);
    } else {
        vars.insert(line_id, ShapeVars::Line(l0, l1));
    }
    write_back(doc, &s, &vars);
    doc.add_constraint(SketchConstraint::pair(
        ConstraintKind::Tangent,
        first,
        second,
    ));
    Ok("Made the second entity tangent to the first".into())
}

/// Makes two circular arcs tangent, first pick pinned. The mover keeps its
/// radius and translates into contact; internal vs external tangency is
/// whichever the current drawing is closer to.
fn constrain_tangent_circles(
    doc: &mut Document,
    first: EntityId,
    second: EntityId,
) -> Result<String, String> {
    let a = arc_of(doc, first).expect("classified as arc");
    let b = arc_of(doc, second).expect("classified as arc");
    let mut s = Sketch::new();
    let va = add_arc_vars(&mut s, &a);
    let vb = add_arc_vars(&mut s, &b);
    pin_shape(&mut s, &va);
    let (c1, r1) = va.circle().expect("arc vars carry a circle");
    let (c2, r2) = vb.circle().expect("arc vars carry a circle");
    s.constrain(Constraint::FixedScalar(r2, b.radius));
    s.constrain(Constraint::TangentCircleCircle {
        c1,
        r1,
        c2,
        r2,
        internal: tangency_is_internal(&a, &b),
    });
    let res = s.solve();
    if !res.converged {
        return Err(format!(
            "Could not make the circles tangent (residual {:.2e})",
            res.residual
        ));
    }
    let mut vars = HashMap::new();
    vars.insert(second, vb);
    write_back(doc, &s, &vars);
    doc.add_constraint(SketchConstraint::pair(
        ConstraintKind::Tangent,
        first,
        second,
    ));
    Ok("Made the second circle tangent to the first".into())
}

/// Drives the radius of the selected circles/arcs, recording it so later
/// edits re-satisfy it. `value: None` locks each arc's current radius in
/// place. The whole constraint component of each arc re-solves with the
/// new radius as the only added target, so welded and tangent neighbours
/// follow the resize.
pub fn constrain_radius(
    doc: &mut Document,
    selection: &[EntityId],
    value: Option<f64>,
) -> Result<String, String> {
    let arcs: Vec<EntityId> = selection
        .iter()
        .copied()
        .filter(|&id| arc_of(doc, id).is_some())
        .collect();
    if arcs.is_empty() {
        return Err("Select at least one circle or arc to constrain its radius".into());
    }
    if let Some(v) = value
        && (!v.is_finite() || v <= 0.0)
    {
        return Err("Radius must be a positive number".into());
    }
    let mut count = 0;
    for id in arcs {
        let target = match value {
            Some(v) => v,
            None => arc_of(doc, id).expect("classified as arc").radius,
        };
        // Record (or retarget) first so the component lowering itself
        // carries the new value — adding a second scalar target for the
        // same radius would fight the old record.
        let prev = doc
            .constraints
            .iter()
            .find(|c| c.kind == ConstraintKind::Radius && c.a == id)
            .copied();
        doc.add_constraint(SketchConstraint::radius(id, target));
        let CompSketch { mut s, vars, .. } = component_sketch(doc, &[id]);
        let res = s.solve();
        if !res.converged {
            let touched = doc
                .constraints
                .iter()
                .position(|c| c.kind == ConstraintKind::Radius && c.a == id);
            let conflict = describe_conflict(doc, &diagnose_conflict(doc, &[id]), touched);
            match prev {
                Some(p) => {
                    doc.add_constraint(p);
                }
                None => doc
                    .constraints
                    .retain(|c| !(c.kind == ConstraintKind::Radius && c.a == id)),
            }
            return Err(format!(
                "Could not solve radius {target} against the existing constraints (residual {:.2e}){conflict}",
                res.residual
            ));
        }
        write_back(doc, &s, &vars);
        count += 1;
    }
    Ok(format!(
        "Constrained the radius of {count} circle(s)/arc(s)"
    ))
}

/// Drives the length of the selected lines, recording it so later edits
/// re-satisfy it. `value: None` locks each line's current length in place.
/// The whole constraint component of each line re-solves with the new
/// length as the only added target, so the line scales about its midpoint
/// and coincident/parallel neighbours follow.
pub fn constrain_distance(
    doc: &mut Document,
    selection: &[EntityId],
    value: Option<f64>,
) -> Result<String, String> {
    let lines: Vec<EntityId> = selection
        .iter()
        .copied()
        .filter(|&id| line_of(doc, id).is_some())
        .collect();
    if lines.is_empty() {
        return Err("Select at least one line to constrain its length".into());
    }
    if let Some(v) = value
        && (!v.is_finite() || v <= 0.0)
    {
        return Err("Length must be a positive number".into());
    }
    let mut count = 0;
    for id in lines {
        let target = match value {
            Some(v) => v,
            None => len(&line_of(doc, id).expect("classified as line")),
        };
        // Record (or retarget) first so the component lowering itself
        // carries the new value — adding a second distance target for the
        // same line would fight the old record.
        let prev = doc
            .constraints
            .iter()
            .find(|c| c.kind == ConstraintKind::Distance && c.a == id)
            .copied();
        doc.add_constraint(SketchConstraint::distance(id, target));
        let CompSketch { mut s, vars, .. } = component_sketch(doc, &[id]);
        let res = s.solve();
        if !res.converged {
            let touched = doc
                .constraints
                .iter()
                .position(|c| c.kind == ConstraintKind::Distance && c.a == id);
            let conflict = describe_conflict(doc, &diagnose_conflict(doc, &[id]), touched);
            match prev {
                Some(p) => {
                    doc.add_constraint(p);
                }
                None => doc
                    .constraints
                    .retain(|c| !(c.kind == ConstraintKind::Distance && c.a == id)),
            }
            return Err(format!(
                "Could not solve length {target} against the existing constraints (residual {:.2e}){conflict}",
                res.residual
            ));
        }
        write_back(doc, &s, &vars);
        count += 1;
    }
    Ok(format!("Constrained the length of {count} line(s)"))
}

/// Solver variables for one entity: a line's two endpoints, or an arc's
/// center + radius (+ endpoint points kept on the rim for partial arcs).
enum ShapeVars {
    Line(PointVar, PointVar),
    Point(PointVar),
    Arc {
        c: PointVar,
        r: ScalarVar,
        ends: Option<(PointVar, PointVar)>,
        orig: CircularArc,
    },
}

impl ShapeVars {
    fn line(&self) -> Option<(PointVar, PointVar)> {
        match self {
            ShapeVars::Line(a, b) => Some((*a, *b)),
            _ => None,
        }
    }

    fn endpoint(&self, i: u8) -> Option<PointVar> {
        match self {
            ShapeVars::Line(a, b) => Some(if i == 0 { *a } else { *b }),
            ShapeVars::Point(p) => Some(*p),
            ShapeVars::Arc {
                ends: Some((ps, pe)),
                ..
            } => Some(if i == 0 { *ps } else { *pe }),
            _ => None,
        }
    }

    fn circle(&self) -> Option<(PointVar, ScalarVar)> {
        match self {
            ShapeVars::Arc { c, r, .. } => Some((*c, *r)),
            _ => None,
        }
    }

    fn arc_orig(&self) -> Option<&CircularArc> {
        match self {
            ShapeVars::Arc { orig, .. } => Some(orig),
            _ => None,
        }
    }
}

/// Whether two circles, as currently drawn, are closer to internal (nested)
/// than external tangency — the mode the solver should maintain.
fn tangency_is_internal(a: &CircularArc, b: &CircularArc) -> bool {
    let d = (a.center.x - b.center.x).hypot(a.center.y - b.center.y);
    (d - (a.radius - b.radius).abs()).abs() < (d - (a.radius + b.radius)).abs()
}

fn add_arc_vars(s: &mut Sketch, a: &CircularArc) -> ShapeVars {
    let c = s.add_point(a.center.x, a.center.y);
    let r = s.add_scalar(a.radius);
    let ends = (!arc_is_full(a)).then(|| {
        let (sx, sy) = arc_end_pos(a, 0);
        let (ex, ey) = arc_end_pos(a, 1);
        let ps = s.add_point(sx, sy);
        let pe = s.add_point(ex, ey);
        s.constrain(Constraint::PointOnCircle(ps, c, r));
        s.constrain(Constraint::PointOnCircle(pe, c, r));
        (ps, pe)
    });
    ShapeVars::Arc {
        c,
        r,
        ends,
        orig: *a,
    }
}

/// Pins every degree of freedom of the entity where it currently sits.
fn pin_shape(s: &mut Sketch, sv: &ShapeVars) {
    match sv {
        ShapeVars::Line(p0, p1) => {
            for p in [*p0, *p1] {
                let (x, y) = s.point(p);
                s.constrain(Constraint::Fixed(p, x, y));
            }
        }
        ShapeVars::Point(p) => {
            let (x, y) = s.point(*p);
            s.constrain(Constraint::Fixed(*p, x, y));
        }
        ShapeVars::Arc { c, r, ends, .. } => {
            let (cx, cy) = s.point(*c);
            s.constrain(Constraint::Fixed(*c, cx, cy));
            let rv = s.scalar(*r);
            s.constrain(Constraint::FixedScalar(*r, rv));
            if let Some((ps, pe)) = ends {
                for p in [*ps, *pe] {
                    let (x, y) = s.point(p);
                    s.constrain(Constraint::Fixed(p, x, y));
                }
            }
        }
    }
}

/// Anchors every line entity's CURRENT length in `s`, using its
/// `component_sketch` vars. A validation solve checking whether a new
/// pure-angle relation (Horizontal/Vertical/Parallel/Perpendicular) is
/// consistent with everything already recorded must not be allowed to
/// "succeed" merely by collapsing some line in the component to a
/// zero-length point — a numeric solver otherwise treats that as a
/// perfectly valid solution (a degenerate one, but the residuals really
/// are zero), silently hiding a genuine conflict a real geometric solution
/// never has this escape from. Not used for EqualLength, whose whole point
/// is to change a length.
fn anchor_line_lengths(s: &mut Sketch, doc: &Document, vars: &HashMap<EntityId, ShapeVars>) {
    for (&id, sv) in vars {
        if let (Some((a, b)), Some(l)) = (sv.line(), line_of(doc, id)) {
            s.constrain(Constraint::Distance(a, b, len(&l)));
        }
    }
}

struct CompSketch {
    s: Sketch,
    vars: HashMap<EntityId, ShapeVars>,
    /// Parallel to `s`'s constraint insertion order: `constraint_doc_idx[i]`
    /// is the index into `doc.constraints` that solver constraint `i` came
    /// from. Lets diagnostics computed on `s` (redundant rows, conflict
    /// culprits) name the document constraint responsible.
    constraint_doc_idx: Vec<usize>,
}

/// Builds one sketch covering the connected component(s) of the constraint
/// graph reachable from the seeds, with every recorded constraint on those
/// entities lowered into solver form.
fn component_sketch(doc: &Document, seeds: &[EntityId]) -> CompSketch {
    let mut comp: Vec<EntityId> = Vec::new();
    for &id in seeds {
        if !comp.contains(&id) {
            comp.push(id);
        }
    }
    let mut grew = true;
    while grew {
        grew = false;
        for c in &doc.constraints {
            let Some(b) = c.b else { continue };
            let has_a = comp.contains(&c.a);
            let has_b = comp.contains(&b);
            if has_a != has_b {
                comp.push(if has_a { b } else { c.a });
                grew = true;
            }
        }
    }

    let mut s = Sketch::new();
    let mut vars: HashMap<EntityId, ShapeVars> = HashMap::new();
    for &id in &comp {
        if let Some(l) = line_of(doc, id) {
            let p0 = s.add_point(l.p0.x, l.p0.y);
            let p1 = s.add_point(l.p1.x, l.p1.y);
            vars.insert(id, ShapeVars::Line(p0, p1));
        } else if let Some(a) = arc_of(doc, id) {
            let sv = add_arc_vars(&mut s, &a);
            vars.insert(id, sv);
        } else if let Some(p) = point_of(doc, id) {
            let pv = s.add_point(p.x, p.y);
            vars.insert(id, ShapeVars::Point(pv));
        }
    }

    let mut constraint_doc_idx = Vec::new();
    for (doc_idx, c) in doc.constraints.iter().enumerate() {
        let Some(sa) = vars.get(&c.a) else {
            continue;
        };
        let sb = c.b.and_then(|b| vars.get(&b));
        match c.kind {
            ConstraintKind::Fixed => {
                let Some(p) = sa.endpoint(0) else { continue };
                let (x, y) = s.point(p);
                s.constrain(Constraint::Fixed(p, x, y));
                constraint_doc_idx.push(doc_idx);
            }
            ConstraintKind::Horizontal | ConstraintKind::Vertical => {
                let Some((a0, a1)) = sa.line() else { continue };
                s.constrain(match c.kind {
                    ConstraintKind::Horizontal => Constraint::Horizontal(a0, a1),
                    _ => Constraint::Vertical(a0, a1),
                });
                constraint_doc_idx.push(doc_idx);
            }
            ConstraintKind::Parallel
            | ConstraintKind::Perpendicular
            | ConstraintKind::EqualLength => {
                let (Some((a0, a1)), Some((b0, b1))) = (sa.line(), sb.and_then(|v| v.line()))
                else {
                    continue;
                };
                s.constrain(match c.kind {
                    ConstraintKind::Parallel => Constraint::Parallel(a0, a1, b0, b1),
                    ConstraintKind::Perpendicular => Constraint::Perpendicular(a0, a1, b0, b1),
                    _ => Constraint::EqualLength(a0, a1, b0, b1),
                });
                constraint_doc_idx.push(doc_idx);
            }
            ConstraintKind::Coincident => {
                let Some((ea, eb)) = c.pts else { continue };
                let (Some(pa), Some(pb)) = (sa.endpoint(ea), sb.and_then(|v| v.endpoint(eb)))
                else {
                    continue;
                };
                s.constrain(Constraint::Coincident(pa, pb));
                constraint_doc_idx.push(doc_idx);
            }
            ConstraintKind::Radius => {
                let (Some((_, r)), Some(v)) = (sa.circle(), c.val) else {
                    continue;
                };
                s.constrain(Constraint::FixedScalar(r, v));
                constraint_doc_idx.push(doc_idx);
            }
            ConstraintKind::Distance => {
                let (Some((a0, a1)), Some(v)) = (sa.line(), c.val) else {
                    continue;
                };
                s.constrain(Constraint::Distance(a0, a1, v));
                constraint_doc_idx.push(doc_idx);
            }
            ConstraintKind::Tangent => {
                let Some(sb) = sb else { continue };
                match (sa.line(), sb.line(), sa.circle(), sb.circle()) {
                    (Some(l), _, _, Some(k)) | (_, Some(l), Some(k), _) => {
                        s.constrain(Constraint::TangentLineCircle(l.0, l.1, k.0, k.1));
                    }
                    (_, _, Some(k1), Some(k2)) => {
                        let (Some(oa), Some(ob)) = (sa.arc_orig(), sb.arc_orig()) else {
                            continue;
                        };
                        s.constrain(Constraint::TangentCircleCircle {
                            c1: k1.0,
                            r1: k1.1,
                            c2: k2.0,
                            r2: k2.1,
                            internal: tangency_is_internal(oa, ob),
                        });
                    }
                    _ => continue,
                }
                constraint_doc_idx.push(doc_idx);
            }
        }
    }
    CompSketch {
        s,
        vars,
        constraint_doc_idx,
    }
}

/// Degrees of freedom remaining, and which recorded constraints are
/// numerically redundant, for the connected constraint component containing
/// `seeds`. Indices in `redundant` are into `doc.constraints`.
pub struct DofSummary {
    pub dof: usize,
    pub redundant: Vec<usize>,
}

/// Reports the DOF/redundancy state of the constraint component reachable
/// from `seeds` — e.g. for a selection, "how much freedom does this shape
/// still have, and is anything on it redundant."
pub fn dof_report(doc: &Document, seeds: &[EntityId]) -> DofSummary {
    let CompSketch {
        s,
        constraint_doc_idx,
        ..
    } = component_sketch(doc, seeds);
    let report = s.analyze();
    let redundant = report
        .redundant
        .into_iter()
        .filter_map(|i| constraint_doc_idx.get(i).copied())
        .collect();
    DofSummary {
        dof: report.dof,
        redundant,
    }
}

/// Only useful right after an action on `seeds` failed to solve (before any
/// rollback restores prior geometry — this rebuilds the component from
/// whatever `doc` holds right now and replays the same starting point).
/// Returns the `doc.constraints` indices whose removal alone would let the
/// rest converge: the leading suspects in a contradictory constraint set.
/// Empty when the component isn't actually failing (nothing to diagnose).
pub fn diagnose_conflict(doc: &Document, seeds: &[EntityId]) -> Vec<usize> {
    let CompSketch {
        mut s,
        constraint_doc_idx,
        ..
    } = component_sketch(doc, seeds);
    let initial = s.snapshot();
    if s.solve().converged {
        return Vec::new();
    }
    s.diagnose_conflict(&initial)
        .culprits
        .into_iter()
        .filter_map(|i| constraint_doc_idx.get(i).copied())
        .collect()
}

/// Turns `diagnose_conflict`'s culprit indices into a short clause to append
/// to an error message, e.g. "; conflicts with its existing perpendicular
/// constraint". `exclude` drops the constraint whose own addition triggered
/// the diagnosis (it always shows up as a trivial culprit — removing what
/// you just tried to add naturally "fixes" the solve — and isn't useful to
/// name). Empty once that's excluded, and there's nothing else to blame.
fn describe_conflict(doc: &Document, culprits: &[usize], exclude: Option<usize>) -> String {
    let mut labels: Vec<&str> = culprits
        .iter()
        .filter(|&&i| Some(i) != exclude)
        .filter_map(|&i| doc.constraints.get(i))
        .map(|c| c.kind.label())
        .collect();
    labels.sort_unstable();
    labels.dedup();
    if labels.is_empty() {
        return String::new();
    }
    format!(
        "; conflicts with its existing {} constraint{}",
        labels.join("/"),
        if labels.len() > 1 { "s" } else { "" }
    )
}

/// Re-satisfies the recorded constraints after `moved` was edited, solving
/// the connected component of the constraint graph that contains it. The
/// dragged endpoint (or the whole entity when `pinned_endpoint` is `None`,
/// and always for arcs) is pinned where the user put it; everything else
/// moves minimally.
///
/// Returns `false` when the component could not be solved, in which case no
/// geometry beyond the caller's original edit is touched.
pub fn resolve_after_edit(
    doc: &mut Document,
    moved: EntityId,
    pinned_endpoint: Option<usize>,
) -> bool {
    if doc.constraints_on(moved).next().is_none() {
        return true;
    }
    let CompSketch { mut s, vars, .. } = component_sketch(doc, &[moved]);
    let Some(sv) = vars.get(&moved) else {
        return true;
    };
    match (sv, pinned_endpoint) {
        (ShapeVars::Line(m0, m1), Some(i)) => {
            let pv = if i == 1 { *m1 } else { *m0 };
            let (x, y) = s.point(pv);
            s.constrain(Constraint::Fixed(pv, x, y));
        }
        _ => pin_shape(&mut s, sv),
    }
    if !s.solve().converged {
        return false;
    }
    write_back(doc, &s, &vars);
    true
}

/// Re-satisfies constraints after a whole-entity transform (move/rotate/
/// scale) of `moved`. Every moved entity is pinned where the user put it;
/// constrained neighbours outside the moved set follow. Returns `false`
/// when the constraints cannot be satisfied with the moved geometry pinned
/// (for example rotating a horizontal-constrained line); the transform is
/// kept as the user made it.
pub fn resolve_after_transform(doc: &mut Document, moved: &[EntityId]) -> bool {
    let seeds: Vec<EntityId> = moved
        .iter()
        .copied()
        .filter(|&id| doc.constraints_on(id).next().is_some())
        .collect();
    if seeds.is_empty() {
        return true;
    }
    let CompSketch { mut s, vars, .. } = component_sketch(doc, &seeds);
    for id in moved {
        if let Some(sv) = vars.get(id) {
            pin_shape(&mut s, sv);
        }
    }
    if !s.solve().converged {
        return false;
    }
    write_back(doc, &s, &vars);
    true
}

fn write_back(doc: &mut Document, s: &Sketch, vars: &HashMap<EntityId, ShapeVars>) {
    for (&id, sv) in vars {
        match sv {
            ShapeVars::Line(p0, p1) => write_line(doc, id, s.point(*p0), s.point(*p1)),
            ShapeVars::Point(p) => {
                let (x, y) = s.point(*p);
                if let Some(e) = doc.get_mut(id) {
                    e.kind = EntityKind::Point(Point2d::from_f64(x, y));
                }
            }
            ShapeVars::Arc { c, r, ends, orig } => {
                let (cx, cy) = s.point(*c);
                let radius = s.scalar(*r);
                if radius <= 1e-9 {
                    continue;
                }
                let center = Point2d::from_f64(cx, cy);
                let arc = match ends {
                    None => CircularArc::new(center, radius, orig.start_angle, orig.end_angle),
                    Some((ps, pe)) => {
                        // Rebuild the angular span from the solved endpoints,
                        // keeping the original sweep direction and winding.
                        let (sx, sy) = s.point(*ps);
                        let (ex, ey) = s.point(*pe);
                        let th_s = (sy - cy).atan2(sx - cx);
                        let sweep_old = orig.end_angle - orig.start_angle;
                        let mut sweep = (ey - cy).atan2(ex - cx) - th_s;
                        while sweep - sweep_old > PI {
                            sweep -= TAU;
                        }
                        while sweep_old - sweep > PI {
                            sweep += TAU;
                        }
                        CircularArc::new(center, radius, th_s, th_s + sweep)
                    }
                };
                if let Some(e) = doc.get_mut(id) {
                    e.kind = EntityKind::Curve(Curve::Arc(arc));
                }
            }
        }
    }
}

fn write_line(doc: &mut Document, id: EntityId, p0: (f64, f64), p1: (f64, f64)) {
    if let Some(e) = doc.get_mut(id) {
        e.kind = EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(p0.0, p0.1),
            Point2d::from_f64(p1.0, p1.1),
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add_line(doc: &mut Document, x0: f64, y0: f64, x1: f64, y1: f64) -> EntityId {
        doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(x0, y0),
            Point2d::from_f64(x1, y1),
        ))))
    }

    fn set_line(doc: &mut Document, id: EntityId, x0: f64, y0: f64, x1: f64, y1: f64) {
        write_line(doc, id, (x0, y0), (x1, y1));
    }

    #[test]
    fn horizontal_levels_a_sloped_line_about_its_midpoint() {
        let mut doc = Document::new();
        let id = add_line(&mut doc, 0.0, 0.0, 4.0, 0.6);
        constrain_lines(&mut doc, &[id], ConstraintKind::Horizontal).expect("must solve");
        let l = line_of(&doc, id).unwrap();
        assert!((l.p0.y - l.p1.y).abs() < 1e-8, "level: {:?}", l);
        assert!((len(&l) - (4.0f64.hypot(0.6))).abs() < 1e-7, "length kept");
        // Minimal motion: both endpoints moved, in opposite directions.
        assert!(l.p0.y > 0.0 && l.p1.y < 0.6, "levelled about the middle");
    }

    #[test]
    fn perpendicular_rotates_only_the_second_line() {
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 5.0, 0.0);
        let b = add_line(&mut doc, 2.0, 1.0, 4.5, 3.5);
        constrain_lines(&mut doc, &[a, b], ConstraintKind::Perpendicular).expect("must solve");
        let la = line_of(&doc, a).unwrap();
        let lb = line_of(&doc, b).unwrap();
        assert!(
            (la.p0.x, la.p0.y, la.p1.x, la.p1.y) == (0.0, 0.0, 5.0, 0.0),
            "reference untouched"
        );
        let dot =
            (la.p1.x - la.p0.x) * (lb.p1.x - lb.p0.x) + (la.p1.y - la.p0.y) * (lb.p1.y - lb.p0.y);
        assert!(dot.abs() < 1e-6, "perpendicular, dot={dot}");
    }

    #[test]
    fn parallel_preserves_the_moved_lines_length() {
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 6.0, 0.0);
        let b = add_line(&mut doc, 1.0, 2.0, 3.0, 4.5);
        let before = len(&line_of(&doc, b).unwrap());
        constrain_lines(&mut doc, &[a, b], ConstraintKind::Parallel).expect("must solve");
        let la = line_of(&doc, a).unwrap();
        let lb = line_of(&doc, b).unwrap();
        let cross =
            (la.p1.x - la.p0.x) * (lb.p1.y - lb.p0.y) - (la.p1.y - la.p0.y) * (lb.p1.x - lb.p0.x);
        assert!(cross.abs() < 1e-6, "parallel, cross={cross}");
        assert!((len(&lb) - before).abs() < 1e-7, "length preserved");
    }

    #[test]
    fn parallel_works_from_an_exactly_perpendicular_start() {
        // 90° apart is a saddle of the cross-product residual; the
        // perturbation retry must get past it.
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 5.0, 0.0);
        let b = add_line(&mut doc, 1.0, 1.0, 1.0, 4.0);
        constrain_lines(&mut doc, &[a, b], ConstraintKind::Parallel).expect("must solve");
        let lb = line_of(&doc, b).unwrap();
        assert!((lb.p0.y - lb.p1.y).abs() < 1e-6, "b is horizontal: {lb:?}");
        assert!((len(&lb) - 3.0).abs() < 1e-6, "length preserved");
    }

    #[test]
    fn vertical_works_on_an_exactly_horizontal_line() {
        let mut doc = Document::new();
        let id = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
        constrain_lines(&mut doc, &[id], ConstraintKind::Vertical).expect("must solve");
        let l = line_of(&doc, id).unwrap();
        assert!((l.p0.x - l.p1.x).abs() < 1e-6, "vertical: {l:?}");
        assert!((len(&l) - 4.0).abs() < 1e-6, "length preserved");
    }

    #[test]
    fn equal_length_stretches_the_second_line() {
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 8.0, 0.0);
        let b = add_line(&mut doc, 1.0, 2.0, 4.0, 2.0);
        constrain_lines(&mut doc, &[a, b], ConstraintKind::EqualLength).expect("must solve");
        let lb = line_of(&doc, b).unwrap();
        assert!(
            (len(&lb) - 8.0).abs() < 1e-7,
            "stretched to 8, got {}",
            len(&lb)
        );
    }

    #[test]
    fn coincident_joins_the_nearest_endpoints() {
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
        let b = add_line(&mut doc, 4.3, 0.2, 8.0, 3.0);
        constrain_lines(&mut doc, &[a, b], ConstraintKind::Coincident).expect("must solve");
        let la = line_of(&doc, a).unwrap();
        let lb = line_of(&doc, b).unwrap();
        assert!(
            (la.p1.x - lb.p0.x).abs() < 1e-8 && (la.p1.y - lb.p0.y).abs() < 1e-8,
            "endpoints joined: {la:?} {lb:?}"
        );
        assert_eq!(
            (la.p0.x, la.p0.y, la.p1.x, la.p1.y),
            (0.0, 0.0, 4.0, 0.0),
            "reference untouched"
        );
        assert_eq!(doc.constraints.len(), 1);
        assert_eq!(doc.constraints[0].pts, Some((1, 0)));
    }

    #[test]
    fn pair_kinds_reject_wrong_selection_counts() {
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 5.0, 0.0);
        assert!(constrain_lines(&mut doc, &[a], ConstraintKind::Parallel).is_err());
        assert!(constrain_lines(&mut doc, &[], ConstraintKind::Horizontal).is_err());
    }

    #[test]
    fn applying_a_constraint_records_it_once() {
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 5.0, 0.0);
        let b = add_line(&mut doc, 1.0, 1.0, 1.2, 4.0);
        constrain_lines(&mut doc, &[a, b], ConstraintKind::Perpendicular).unwrap();
        constrain_lines(&mut doc, &[a, b], ConstraintKind::Perpendicular).unwrap();
        constrain_lines(&mut doc, &[b, a], ConstraintKind::Perpendicular).unwrap();
        assert_eq!(doc.constraints.len(), 1, "symmetric duplicates collapse");
        constrain_lines(&mut doc, &[a], ConstraintKind::Horizontal).unwrap();
        assert_eq!(doc.constraints.len(), 2);
    }

    #[test]
    fn vertical_conflicts_with_an_existing_horizontal_and_is_rejected() {
        // Horizontal and Vertical on the same line force it to a single
        // point — the only way to reproduce a bug where this silently
        // "succeeded" (rotated the line vertical) while leaving BOTH
        // records behind, the Horizontal one now lying about the geometry.
        let mut doc = Document::new();
        let id = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
        constrain_lines(&mut doc, &[id], ConstraintKind::Horizontal).unwrap();
        let before = line_of(&doc, id).unwrap();
        let err = constrain_lines(&mut doc, &[id], ConstraintKind::Vertical).unwrap_err();
        assert!(
            err.contains("conflicts with its existing horizontal constraint"),
            "names the conflict: {err}"
        );
        assert_eq!(
            doc.constraints,
            vec![SketchConstraint::single(ConstraintKind::Horizontal, id)],
            "the rejected Vertical must not be left recorded alongside Horizontal"
        );
        let after = line_of(&doc, id).unwrap();
        assert_eq!(
            (before.p0, before.p1),
            (after.p0, after.p1),
            "geometry untouched by the rejected attempt"
        );
    }

    #[test]
    fn parallel_conflicts_with_an_existing_parallel_to_a_vertical_line_and_is_rejected() {
        // a is Horizontal (an absolute angle anchor), b is Parallel to a
        // (so b is horizontal too), c is Vertical (another absolute
        // anchor). Pure orientation relations among free-floating lines are
        // never truly contradictory on their own — angle is relative, so
        // the group can just rotate together — genuine conflict needs an
        // absolute anchor like Horizontal/Vertical pinning two DIFFERENT
        // angles into the same component. Asking b to ALSO be parallel to
        // c is exactly that: horizontal can't be parallel to vertical.
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 5.0, 0.0);
        let b = add_line(&mut doc, 0.0, 1.0, 5.0, 1.2);
        let c = add_line(&mut doc, 2.0, 2.0, 2.3, 6.0);
        constrain_lines(&mut doc, &[a], ConstraintKind::Horizontal).unwrap();
        constrain_lines(&mut doc, &[a, b], ConstraintKind::Parallel).unwrap();
        constrain_lines(&mut doc, &[c], ConstraintKind::Vertical).unwrap();
        let before = line_of(&doc, b).unwrap();
        let count_before = doc.constraints.len();
        let err = constrain_lines(&mut doc, &[c, b], ConstraintKind::Parallel).unwrap_err();
        assert!(
            err.contains("conflicts with its existing"),
            "names a conflict: {err}"
        );
        assert_eq!(
            doc.constraints.len(),
            count_before,
            "the rejected Parallel must not be left recorded"
        );
        let after = line_of(&doc, b).unwrap();
        assert_eq!(
            (before.p0, before.p1),
            (after.p0, after.p1),
            "geometry untouched by the rejected attempt"
        );
    }

    #[test]
    fn deleting_an_entity_prunes_its_constraints() {
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 5.0, 0.0);
        let b = add_line(&mut doc, 1.0, 1.0, 4.0, 2.0);
        constrain_lines(&mut doc, &[a, b], ConstraintKind::Parallel).unwrap();
        constrain_lines(&mut doc, &[a], ConstraintKind::Horizontal).unwrap();
        assert_eq!(doc.constraints.len(), 2);
        doc.remove(b);
        assert_eq!(doc.constraints.len(), 1, "only the pair is pruned");
        doc.remove(a);
        assert!(doc.constraints.is_empty());
    }

    #[test]
    fn drag_keeps_a_horizontal_line_horizontal() {
        let mut doc = Document::new();
        let id = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
        constrain_lines(&mut doc, &[id], ConstraintKind::Horizontal).unwrap();
        // Simulate a grip drag of endpoint 1 off-axis.
        set_line(&mut doc, id, 0.0, 0.0, 5.0, 2.0);
        assert!(resolve_after_edit(&mut doc, id, Some(1)));
        let l = line_of(&doc, id).unwrap();
        assert!(
            (l.p1.x - 5.0).abs() < 1e-8 && (l.p1.y - 2.0).abs() < 1e-8,
            "drag wins: {l:?}"
        );
        assert!((l.p0.y - l.p1.y).abs() < 1e-8, "still horizontal: {l:?}");
        assert!(l.p0.x.abs() < 1e-6, "free endpoint x stays near start");
    }

    #[test]
    fn drag_on_a_perpendicular_pair_rotates_the_partner() {
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 5.0, 0.0);
        let b = add_line(&mut doc, 0.0, 0.0, 0.0, 3.0);
        constrain_lines(&mut doc, &[a, b], ConstraintKind::Perpendicular).unwrap();
        // Rotate line a by dragging its far endpoint upward.
        set_line(&mut doc, a, 0.0, 0.0, 4.0, 3.0);
        assert!(resolve_after_edit(&mut doc, a, Some(1)));
        let la = line_of(&doc, a).unwrap();
        let lb = line_of(&doc, b).unwrap();
        assert!((la.p1.x - 4.0).abs() < 1e-8 && (la.p1.y - 3.0).abs() < 1e-8);
        let dot =
            (la.p1.x - la.p0.x) * (lb.p1.x - lb.p0.x) + (la.p1.y - la.p0.y) * (lb.p1.y - lb.p0.y);
        assert!(dot.abs() < 1e-6, "partner stayed perpendicular, dot={dot}");
    }

    #[test]
    fn drag_reaches_through_a_constraint_chain() {
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
        let b = add_line(&mut doc, 0.0, 1.0, 4.0, 1.0);
        let c = add_line(&mut doc, 0.0, 2.0, 4.0, 2.0);
        constrain_lines(&mut doc, &[a, b], ConstraintKind::Parallel).unwrap();
        constrain_lines(&mut doc, &[b, c], ConstraintKind::Parallel).unwrap();
        set_line(&mut doc, a, 0.0, 0.0, 4.0, 2.0);
        assert!(resolve_after_edit(&mut doc, a, Some(1)));
        let la = line_of(&doc, a).unwrap();
        let lc = line_of(&doc, c).unwrap();
        let cross =
            (la.p1.x - la.p0.x) * (lc.p1.y - lc.p0.y) - (la.p1.y - la.p0.y) * (lc.p1.x - lc.p0.x);
        assert!(cross.abs() < 1e-6, "c follows a through b, cross={cross}");
    }

    #[test]
    fn drag_pulls_a_coincident_corner_along() {
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
        let b = add_line(&mut doc, 4.0, 0.0, 4.0, 3.0);
        doc.add_constraint(SketchConstraint::coincident(a, 1, b, 0));
        // Drag a's shared endpoint away.
        set_line(&mut doc, a, 0.0, 0.0, 5.0, 1.0);
        assert!(resolve_after_edit(&mut doc, a, Some(1)));
        let lb = line_of(&doc, b).unwrap();
        assert!(
            (lb.p0.x - 5.0).abs() < 1e-8 && (lb.p0.y - 1.0).abs() < 1e-8,
            "b's corner followed: {lb:?}"
        );
        assert!(
            (lb.p1.x - 4.0).abs() < 0.5 && (lb.p1.y - 3.0).abs() < 0.5,
            "b's far end stayed near where it was: {lb:?}"
        );
    }

    #[test]
    fn fixed_point_anchors_a_coincident_line_end_under_a_drag() {
        // Mirrors how the origin is wired up in oxidraft_ui::add_origin_point:
        // a Point entity carrying a `Fixed` constraint, welded to a line's
        // endpoint. Dragging the line's far end must re-solve the near end
        // back onto the fixed point, not let it drift with the drag.
        let mut doc = Document::new();
        let origin = doc.add(EntityKind::Point(Point2d::from_f64(0.0, 0.0)));
        doc.add_constraint(SketchConstraint::fixed(origin));
        let a = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
        doc.add_constraint(SketchConstraint::coincident(origin, 0, a, 0));

        set_line(&mut doc, a, 0.2, 0.3, 9.0, 2.0);
        assert!(resolve_after_edit(&mut doc, a, Some(1)));
        let la = line_of(&doc, a).unwrap();
        assert!(
            (la.p0.x).abs() < 1e-6 && (la.p0.y).abs() < 1e-6,
            "near end pulled back onto the fixed origin: {la:?}"
        );
        assert!(
            (la.p1.x - 9.0).abs() < 1e-6 && (la.p1.y - 2.0).abs() < 1e-6,
            "dragged end kept the user's placement: {la:?}"
        );
        if let Some(EntityKind::Point(p)) = doc.get(origin).map(|e| &e.kind) {
            let (ox, oy) = p.to_f64();
            assert!(
                ox.abs() < 1e-6 && oy.abs() < 1e-6,
                "the origin itself never moved: ({ox}, {oy})"
            );
        } else {
            panic!("expected the origin to still be a point");
        }
    }

    #[test]
    fn moving_a_line_drags_coincident_neighbours() {
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
        let b = add_line(&mut doc, 4.0, 0.0, 4.0, 3.0);
        doc.add_constraint(SketchConstraint::coincident(a, 1, b, 0));
        // Translate the whole of line a.
        set_line(&mut doc, a, 1.0, 2.0, 5.0, 2.0);
        assert!(resolve_after_transform(&mut doc, &[a]));
        let la = line_of(&doc, a).unwrap();
        let lb = line_of(&doc, b).unwrap();
        for (got, want) in [
            (la.p0.x, 1.0),
            (la.p0.y, 2.0),
            (la.p1.x, 5.0),
            (la.p1.y, 2.0),
        ] {
            assert!((got - want).abs() < 1e-8, "moved line pinned: {la:?}");
        }
        assert!(
            (lb.p0.x - 5.0).abs() < 1e-8 && (lb.p0.y - 2.0).abs() < 1e-8,
            "neighbour corner reattached: {lb:?}"
        );
    }

    #[test]
    fn moving_both_members_of_a_pair_keeps_them_put() {
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
        let b = add_line(&mut doc, 0.0, 1.0, 4.0, 1.0);
        constrain_lines(&mut doc, &[a, b], ConstraintKind::Parallel).unwrap();
        // Translate both together: the relation still holds, nothing moves.
        set_line(&mut doc, a, 10.0, 0.0, 14.0, 0.0);
        set_line(&mut doc, b, 10.0, 1.0, 14.0, 1.0);
        assert!(resolve_after_transform(&mut doc, &[a, b]));
        let la = line_of(&doc, a).unwrap();
        let lb = line_of(&doc, b).unwrap();
        assert_eq!((la.p0.x, la.p0.y), (10.0, 0.0));
        assert_eq!((lb.p0.x, lb.p0.y), (10.0, 1.0));
    }

    #[test]
    fn transform_that_breaks_a_constraint_reports_failure() {
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
        constrain_lines(&mut doc, &[a], ConstraintKind::Horizontal).unwrap();
        // "Rotate" the line 30°: pinned endpoints conflict with Horizontal.
        set_line(&mut doc, a, 0.0, 0.0, 3.46, 2.0);
        assert!(!resolve_after_transform(&mut doc, &[a]));
        let la = line_of(&doc, a).unwrap();
        assert!(
            (la.p1.y - 2.0).abs() < 1e-9,
            "user's transform left alone: {la:?}"
        );
    }

    #[test]
    fn perpendicular_on_a_coincident_pair_keeps_the_joint_welded() {
        // a-b share a corner (Coincident). Squaring b up to a third line c
        // rotates b about that corner; the corner must follow, not gap.
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
        let b = add_line(&mut doc, 4.0, 0.0, 6.0, 1.5);
        let c = add_line(&mut doc, -3.0, -3.0, -3.0, 3.0);
        constrain_lines(&mut doc, &[a, b], ConstraintKind::Coincident).unwrap();
        constrain_lines(&mut doc, &[c, b], ConstraintKind::Perpendicular).unwrap();
        let la = line_of(&doc, a).unwrap();
        let lb = line_of(&doc, b).unwrap();
        assert!(
            (la.p1.x - lb.p0.x).abs() < 1e-6 && (la.p1.y - lb.p0.y).abs() < 1e-6,
            "corner stayed welded after b rotated: a={la:?} b={lb:?}"
        );
    }

    #[test]
    fn perpendicular_still_solves_on_a_tight_slot_after_a_small_nudge() {
        // A "slot" — two vertical legs tangent to a shared bottom arc, a
        // horizontal top, and welded corners — is a numerically stiff
        // component: normal-equations LM's achievable precision is roughly
        // the *square* of the system's condition number times machine
        // epsilon, and this combination of tangency + welds + H/V anchors
        // is ill-conditioned enough to plateau around 1e-8, just above the
        // solver's old 1e-10 tolerance. That made a perfectly legitimate,
        // barely-off-vertical `right` (as a small grip drag would leave it)
        // get rejected as "conflicts with its existing constraints" even
        // though a solution a few nanometres away clearly exists.
        use oxidraft_document::SketchConstraint;
        let mut doc = Document::new();
        let top = add_line(&mut doc, 0.0, 5.0, 4.0, 5.0);
        let left = add_line(&mut doc, 0.0, 5.0, 0.0, 0.0);
        let right = add_line(&mut doc, 4.0, 5.0, 4.0, 0.0);
        let arc = doc.add(EntityKind::Curve(Curve::Arc(CircularArc::new(
            Point2d::from_f64(2.0, 0.0),
            2.0,
            PI,
            TAU,
        ))));
        constrain_lines(&mut doc, &[top], ConstraintKind::Horizontal).expect("top horizontal");
        constrain_lines(&mut doc, &[left], ConstraintKind::Vertical).expect("left vertical");
        constrain_tangent(&mut doc, &[arc, left]).expect("left tangent");
        constrain_tangent(&mut doc, &[arc, right]).expect("right tangent");
        doc.add_constraint(SketchConstraint::coincident(top, 0, left, 0));
        doc.add_constraint(SketchConstraint::coincident(top, 1, right, 0));
        doc.add_constraint(SketchConstraint::coincident(left, 1, arc, 0));
        doc.add_constraint(SketchConstraint::coincident(right, 1, arc, 1));

        // Nudge right's top endpoint sideways a little, as a small manual
        // drag would, breaking exact verticality/tangency slightly.
        let r = line_of(&doc, right).unwrap();
        set_line(&mut doc, right, 4.1, 5.0, r.p1.x, r.p1.y);

        constrain_lines(&mut doc, &[top, right], ConstraintKind::Perpendicular)
            .expect("a barely-off-vertical leg must still solve, not be rejected as conflicting");
        let lr = line_of(&doc, right).unwrap();
        assert!(
            (lr.p0.x - lr.p1.x).abs() < 1e-6,
            "right ended up vertical: {lr:?}"
        );
    }

    fn add_circle(doc: &mut Document, cx: f64, cy: f64, r: f64) -> EntityId {
        doc.add(EntityKind::Curve(Curve::Arc(CircularArc::new(
            Point2d::from_f64(cx, cy),
            r,
            0.0,
            TAU,
        ))))
    }

    fn line_circle_gap(l: &LineSeg, a: &CircularArc) -> f64 {
        let (ux, uy) = (l.p1.x - l.p0.x, l.p1.y - l.p0.y);
        let n = ux.hypot(uy);
        let d = (ux * (a.center.y - l.p0.y) - uy * (a.center.x - l.p0.x)) / n;
        d.abs() - a.radius
    }

    #[test]
    fn tangent_slides_the_line_onto_a_pinned_circle() {
        let mut doc = Document::new();
        let circle = add_circle(&mut doc, 0.0, 0.0, 2.0);
        let line = add_line(&mut doc, -3.0, 3.0, 3.0, 3.4);
        let before = len(&line_of(&doc, line).unwrap());
        constrain_lines(&mut doc, &[circle, line], ConstraintKind::Tangent).expect("must solve");
        let l = line_of(&doc, line).unwrap();
        let a = arc_of(&doc, circle).unwrap();
        assert!(
            (a.center.x, a.center.y) == (0.0, 0.0) && a.radius == 2.0,
            "reference pinned"
        );
        assert!(line_circle_gap(&l, &a).abs() < 1e-7, "line touches the rim");
        assert!((len(&l) - before).abs() < 1e-7, "line length kept");
        assert_eq!(doc.constraints.len(), 1);
        assert_eq!(doc.constraints[0].kind, ConstraintKind::Tangent);
    }

    #[test]
    fn tangent_pulls_the_circle_onto_a_pinned_line() {
        let mut doc = Document::new();
        let line = add_line(&mut doc, -4.0, 0.0, 4.0, 0.0);
        let circle = add_circle(&mut doc, 0.5, 3.1, 2.0);
        constrain_lines(&mut doc, &[line, circle], ConstraintKind::Tangent).expect("must solve");
        let l = line_of(&doc, line).unwrap();
        let a = arc_of(&doc, circle).unwrap();
        assert_eq!((l.p0.x, l.p0.y, l.p1.x, l.p1.y), (-4.0, 0.0, 4.0, 0.0));
        assert!(
            line_circle_gap(&l, &a).abs() < 1e-7,
            "circle touches the line"
        );
        assert!(a.center.y > 1.0, "circle stayed on its side");
    }

    #[test]
    fn dragging_a_tangent_line_pulls_the_circle_along() {
        let mut doc = Document::new();
        let circle = add_circle(&mut doc, 0.0, 0.0, 2.0);
        let line = add_line(&mut doc, -3.0, 2.0, 3.0, 2.0);
        constrain_lines(&mut doc, &[circle, line], ConstraintKind::Tangent).unwrap();
        // Tilt the line by dragging its right end up.
        set_line(&mut doc, line, -3.0, 2.0, 3.0, 4.0);
        assert!(resolve_after_edit(&mut doc, line, Some(1)));
        let l = line_of(&doc, line).unwrap();
        let a = arc_of(&doc, circle).unwrap();
        assert!(
            (l.p1.x - 3.0).abs() < 1e-8 && (l.p1.y - 4.0).abs() < 1e-8,
            "drag wins"
        );
        assert!(
            line_circle_gap(&l, &a).abs() < 1e-7,
            "circle re-attached: {a:?}"
        );
    }

    #[test]
    fn radius_resizes_a_circle_and_keeps_its_tangent_line() {
        let mut doc = Document::new();
        let circle = add_circle(&mut doc, 0.0, 0.0, 2.0);
        let line = add_line(&mut doc, -3.0, 2.0, 3.0, 2.0);
        constrain_lines(&mut doc, &[circle, line], ConstraintKind::Tangent).unwrap();
        constrain_radius(&mut doc, &[circle], Some(3.0)).expect("must solve");
        let a = arc_of(&doc, circle).unwrap();
        let l = line_of(&doc, line).unwrap();
        assert!((a.radius - 3.0).abs() < 1e-6, "resized: {}", a.radius);
        assert!(line_circle_gap(&l, &a).abs() < 1e-6, "still tangent");
        assert!(
            doc.constraints
                .iter()
                .any(|c| c.kind == ConstraintKind::Radius && c.a == circle && c.val == Some(3.0)),
            "radius recorded"
        );
    }

    #[test]
    fn radius_reapplied_retargets_the_same_constraint() {
        let mut doc = Document::new();
        let circle = add_circle(&mut doc, 0.0, 0.0, 2.0);
        constrain_radius(&mut doc, &[circle], Some(3.0)).unwrap();
        constrain_radius(&mut doc, &[circle], Some(4.0)).unwrap();
        assert_eq!(doc.constraints.len(), 1, "one constraint, updated in place");
        assert_eq!(doc.constraints[0].val, Some(4.0));
        assert!((arc_of(&doc, circle).unwrap().radius - 4.0).abs() < 1e-6);
    }

    #[test]
    fn bare_radius_locks_the_current_value() {
        let mut doc = Document::new();
        let circle = add_circle(&mut doc, 1.0, 1.0, 2.5);
        constrain_radius(&mut doc, &[circle], None).unwrap();
        let a = arc_of(&doc, circle).unwrap();
        assert!((a.radius - 2.5).abs() < 1e-9, "geometry untouched");
        assert_eq!(doc.constraints[0].val, Some(2.5));
    }

    #[test]
    fn dragging_a_tangent_line_respects_a_driven_radius() {
        let mut doc = Document::new();
        let circle = add_circle(&mut doc, 0.0, 0.0, 2.0);
        let line = add_line(&mut doc, -3.0, 2.0, 3.0, 2.0);
        constrain_lines(&mut doc, &[circle, line], ConstraintKind::Tangent).unwrap();
        constrain_radius(&mut doc, &[circle], Some(2.0)).unwrap();
        // Tilt the line by dragging its right end up; the circle must
        // follow without resizing.
        set_line(&mut doc, line, -3.0, 2.0, 3.0, 4.0);
        assert!(resolve_after_edit(&mut doc, line, Some(1)));
        let a = arc_of(&doc, circle).unwrap();
        let l = line_of(&doc, line).unwrap();
        assert!((a.radius - 2.0).abs() < 1e-6, "radius held: {}", a.radius);
        assert!(line_circle_gap(&l, &a).abs() < 1e-6, "still tangent");
    }

    #[test]
    fn radius_rejects_nonsense() {
        let mut doc = Document::new();
        let circle = add_circle(&mut doc, 0.0, 0.0, 2.0);
        assert!(constrain_radius(&mut doc, &[circle], Some(-1.0)).is_err());
        let l = add_line(&mut doc, 0.0, 0.0, 1.0, 0.0);
        assert!(
            constrain_radius(&mut doc, &[l], Some(1.0)).is_err(),
            "lines have no radius"
        );
        assert!(doc.constraints.is_empty());
    }

    #[test]
    fn distance_resizes_a_line_about_its_midpoint() {
        let mut doc = Document::new();
        let id = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
        constrain_distance(&mut doc, &[id], Some(6.0)).expect("must solve");
        let l = line_of(&doc, id).unwrap();
        assert!((len(&l) - 6.0).abs() < 1e-6, "resized: {}", len(&l));
        // Minimal motion scales about the midpoint (x = 2), symmetrically.
        assert!(
            (l.p0.x - -1.0).abs() < 1e-6 && (l.p1.x - 5.0).abs() < 1e-6,
            "{l:?}"
        );
        assert!(
            doc.constraints
                .iter()
                .any(|c| c.kind == ConstraintKind::Distance && c.a == id && c.val == Some(6.0)),
            "length recorded"
        );
    }

    #[test]
    fn distance_reapplied_retargets_the_same_constraint() {
        let mut doc = Document::new();
        let id = add_line(&mut doc, 0.0, 0.0, 3.0, 0.0);
        constrain_distance(&mut doc, &[id], Some(5.0)).unwrap();
        constrain_distance(&mut doc, &[id], Some(7.0)).unwrap();
        assert_eq!(doc.constraints.len(), 1, "one constraint, updated in place");
        assert_eq!(doc.constraints[0].val, Some(7.0));
        assert!((len(&line_of(&doc, id).unwrap()) - 7.0).abs() < 1e-6);
    }

    #[test]
    fn bare_distance_locks_the_current_length() {
        let mut doc = Document::new();
        let id = add_line(&mut doc, 1.0, 1.0, 4.0, 5.0);
        constrain_distance(&mut doc, &[id], None).unwrap();
        let l = line_of(&doc, id).unwrap();
        assert_eq!(
            (l.p0.x, l.p0.y, l.p1.x, l.p1.y),
            (1.0, 1.0, 4.0, 5.0),
            "geometry untouched"
        );
        assert_eq!(doc.constraints[0].val, Some(5.0), "3-4-5 length locked");
    }

    #[test]
    fn distance_drags_a_coincident_neighbour_when_driven() {
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
        let b = add_line(&mut doc, 4.0, 0.0, 4.0, 3.0);
        doc.add_constraint(SketchConstraint::coincident(a, 1, b, 0));
        // Lengthening a about its midpoint pushes the shared corner out, and
        // b's joined endpoint must follow.
        constrain_distance(&mut doc, &[a], Some(6.0)).expect("must solve");
        let la = line_of(&doc, a).unwrap();
        let lb = line_of(&doc, b).unwrap();
        assert!((len(&la) - 6.0).abs() < 1e-6, "a resized: {}", len(&la));
        assert!(
            (lb.p0.x - la.p1.x).abs() < 1e-6 && (lb.p0.y - la.p1.y).abs() < 1e-6,
            "corner stayed welded: {la:?} {lb:?}"
        );
    }

    #[test]
    fn distance_rejects_nonsense() {
        let mut doc = Document::new();
        let l = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
        assert!(constrain_distance(&mut doc, &[l], Some(-1.0)).is_err());
        let circle = add_circle(&mut doc, 0.0, 0.0, 2.0);
        assert!(
            constrain_distance(&mut doc, &[circle], Some(1.0)).is_err(),
            "arcs have no length"
        );
        assert!(doc.constraints.is_empty());
    }

    #[test]
    fn moving_a_circle_drags_its_tangent_line() {
        let mut doc = Document::new();
        let line = add_line(&mut doc, -4.0, 0.0, 4.0, 0.0);
        let circle = add_circle(&mut doc, 0.0, 2.0, 2.0);
        constrain_lines(&mut doc, &[line, circle], ConstraintKind::Tangent).unwrap();
        // Translate the circle up; the line must follow to stay tangent.
        if let Some(e) = doc.get_mut(circle) {
            e.kind = EntityKind::Curve(Curve::Arc(CircularArc::new(
                Point2d::from_f64(0.0, 3.0),
                2.0,
                0.0,
                TAU,
            )));
        }
        assert!(resolve_after_transform(&mut doc, &[circle]));
        let l = line_of(&doc, line).unwrap();
        let a = arc_of(&doc, circle).unwrap();
        assert!((a.center.y - 3.0).abs() < 1e-8, "moved circle pinned");
        assert!(line_circle_gap(&l, &a).abs() < 1e-7, "line followed");
    }

    #[test]
    fn tangent_circles_touch_and_follow_each_other() {
        let mut doc = Document::new();
        let a = add_circle(&mut doc, 0.0, 0.0, 2.0);
        let b = add_circle(&mut doc, 5.5, 0.0, 1.0);
        constrain_lines(&mut doc, &[a, b], ConstraintKind::Tangent).expect("must solve");
        let ca = arc_of(&doc, a).unwrap();
        let cb = arc_of(&doc, b).unwrap();
        assert_eq!((ca.center.x, ca.center.y, ca.radius), (0.0, 0.0, 2.0));
        let d = (cb.center.x - ca.center.x).hypot(cb.center.y - ca.center.y);
        assert!((d - 3.0).abs() < 1e-7, "externally tangent, d={d}");
        assert!((cb.radius - 1.0).abs() < 1e-6, "mover kept its radius");

        // Move the reference circle; the partner must re-attach.
        if let Some(e) = doc.get_mut(a) {
            e.kind = EntityKind::Curve(Curve::Arc(CircularArc::new(
                Point2d::from_f64(0.0, 2.0),
                2.0,
                0.0,
                TAU,
            )));
        }
        assert!(resolve_after_transform(&mut doc, &[a]));
        let ca = arc_of(&doc, a).unwrap();
        let cb = arc_of(&doc, b).unwrap();
        assert!((ca.center.y - 2.0).abs() < 1e-8, "moved circle pinned");
        let d = (cb.center.x - ca.center.x).hypot(cb.center.y - ca.center.y);
        assert!(
            (d - (ca.radius + cb.radius)).abs() < 1e-7,
            "still tangent after move, d={d}"
        );
    }

    #[test]
    fn welded_tangent_fillet_survives_dragging_a_leg() {
        // Horizontal leg into a quarter fillet into a vertical leg, welded
        // and tangent like the fillet tool records. Dragging the far end of
        // the vertical leg must keep the corner smooth.
        let mut doc = Document::new();
        let leg_a = add_line(&mut doc, 0.0, 0.0, 3.0, 0.0);
        let leg_b = add_line(&mut doc, 4.0, 1.0, 4.0, 4.0);
        let arc = doc.add(EntityKind::Curve(Curve::Arc(CircularArc::new(
            Point2d::from_f64(3.0, 1.0),
            1.0,
            -std::f64::consts::FRAC_PI_2,
            0.0,
        ))));
        doc.add_constraint(SketchConstraint::coincident(leg_a, 1, arc, 0));
        doc.add_constraint(SketchConstraint::coincident(leg_b, 0, arc, 1));
        doc.add_constraint(SketchConstraint::pair(ConstraintKind::Tangent, leg_a, arc));
        doc.add_constraint(SketchConstraint::pair(ConstraintKind::Tangent, leg_b, arc));
        set_line(&mut doc, leg_b, 4.0, 1.0, 5.5, 4.0);
        assert!(resolve_after_edit(&mut doc, leg_b, Some(1)));
        let la = line_of(&doc, leg_a).unwrap();
        let lb = line_of(&doc, leg_b).unwrap();
        let a = arc_of(&doc, arc).unwrap();
        assert!(
            (lb.p1.x - 5.5).abs() < 1e-8 && (lb.p1.y - 4.0).abs() < 1e-8,
            "drag wins"
        );
        assert!(line_circle_gap(&la, &a).abs() < 1e-6, "leg a still tangent");
        assert!(line_circle_gap(&lb, &a).abs() < 1e-6, "leg b still tangent");
        let (s0, s1) = (arc_end_pos(&a, 0), arc_end_pos(&a, 1));
        assert!(
            (la.p1.x - s0.0).abs() < 1e-6 && (la.p1.y - s0.1).abs() < 1e-6,
            "arc start welded to leg a: {s0:?} vs {la:?}"
        );
        assert!(
            (lb.p0.x - s1.0).abs() < 1e-6 && (lb.p0.y - s1.1).abs() < 1e-6,
            "arc end welded to leg b: {s1:?} vs {lb:?}"
        );
    }

    #[test]
    fn resolve_ignores_unconstrained_entities() {
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
        let free = add_line(&mut doc, 9.0, 9.0, 10.0, 10.0);
        constrain_lines(&mut doc, &[a], ConstraintKind::Horizontal).unwrap();
        set_line(&mut doc, free, 9.0, 9.0, 11.0, 12.0);
        assert!(resolve_after_edit(&mut doc, free, Some(1)));
        assert!(resolve_after_transform(&mut doc, &[free]));
        let l = line_of(&doc, free).unwrap();
        assert!(
            (l.p1.x - 11.0).abs() < 1e-12 && (l.p1.y - 12.0).abs() < 1e-12,
            "untouched"
        );
    }

    #[test]
    fn dof_report_counts_a_plain_and_a_horizontal_line() {
        let mut doc = Document::new();
        let id = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
        assert_eq!(
            dof_report(&doc, &[id]).dof,
            4,
            "two free endpoints, unconstrained"
        );
        constrain_lines(&mut doc, &[id], ConstraintKind::Horizontal).unwrap();
        let report = dof_report(&doc, &[id]);
        assert_eq!(report.dof, 3, "one row pinned by Horizontal");
        assert!(report.redundant.is_empty());
    }

    #[test]
    fn dof_report_flags_a_transitively_redundant_parallel() {
        // Three lines: a‖b and b‖c already force a‖c — recording it too
        // (a real thing a user might do, e.g. via three separate PAR
        // commands) is redundant, not a new relation.
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 4.0, 0.0);
        let b = add_line(&mut doc, 0.0, 1.0, 4.0, 1.3);
        let c = add_line(&mut doc, 0.0, 2.0, 4.0, 2.2);
        constrain_lines(&mut doc, &[a, b], ConstraintKind::Parallel).unwrap();
        constrain_lines(&mut doc, &[b, c], ConstraintKind::Parallel).unwrap();
        constrain_lines(&mut doc, &[a, c], ConstraintKind::Parallel).unwrap();
        let report = dof_report(&doc, &[a]);
        assert_eq!(
            report.redundant.len(),
            1,
            "exactly one of the three Parallel records is redundant: {:?}",
            report.redundant
        );
        assert_eq!(
            doc.constraints[report.redundant[0]].kind,
            ConstraintKind::Parallel
        );
    }

    #[test]
    fn distance_conflict_is_named_in_the_error_and_left_unrecorded() {
        // A closed triangle with side lengths 1, 1, 10 violates the
        // triangle inequality — no real triangle has those sides, so the
        // third length lock can never solve against the other two.
        let mut doc = Document::new();
        let a = add_line(&mut doc, 0.0, 0.0, 1.0, 0.0);
        let b = add_line(&mut doc, 1.0, 0.0, 1.0, 1.0);
        let c = add_line(&mut doc, 1.0, 1.0, 0.0, 0.0);
        doc.add_constraint(SketchConstraint::coincident(a, 1, b, 0));
        doc.add_constraint(SketchConstraint::coincident(b, 1, c, 0));
        doc.add_constraint(SketchConstraint::coincident(c, 1, a, 0));
        constrain_distance(&mut doc, &[a], Some(1.0)).unwrap();
        constrain_distance(&mut doc, &[b], Some(1.0)).unwrap();
        let before = doc.constraints.len();
        let err = constrain_distance(&mut doc, &[c], Some(10.0)).unwrap_err();
        // Both the other length locks AND either weld are legitimate
        // leave-one-out culprits (breaking the loop "solves" it too), so
        // the message names both kinds.
        assert!(
            err.contains("conflicts with its existing") && err.contains("length"),
            "names the conflicting kind(s): {err}"
        );
        assert_eq!(
            doc.constraints.len(),
            before,
            "the impossible length is not left recorded"
        );
    }
}
