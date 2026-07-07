use oxidraft_document::EntityKind;
use oxidraft_geometry::{
    CircularArc, Curve, CurveSegment, EllipticalArc, LineSeg, NurbsCurve, Point2d, PolyCurve,
    RationalBezier,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GripRole {
    Endpoint(usize),
    Center,
    Radius,
    AngleStart,
    AngleEnd,
    Vertex(usize),
    PolyPoint(usize),
    Rotation,
    AxisMajor,
    AxisMinor,
}

#[derive(Clone, Copy, Debug)]
pub struct Grip {
    pub role: GripRole,
    pub world: Point2d,
}

impl Grip {
    fn new(role: GripRole, world: Point2d) -> Self {
        Grip { role, world }
    }
}

fn is_full_circle(arc: &CircularArc) -> bool {
    (arc.end_angle - arc.start_angle).abs() >= 2.0 * std::f64::consts::PI - 1e-9
}

pub fn grips_for(kind: &EntityKind) -> Vec<Grip> {
    match kind {
        EntityKind::Curve(Curve::Line(l)) => vec![
            Grip::new(GripRole::Endpoint(0), l.p0),
            Grip::new(GripRole::Endpoint(1), l.p1),
        ],
        EntityKind::Curve(Curve::Arc(arc)) => {
            if is_full_circle(arc) {
                let mut g = vec![Grip::new(GripRole::Center, arc.center)];
                for a in [
                    std::f64::consts::FRAC_PI_2,
                    0.0,
                    -std::f64::consts::FRAC_PI_2,
                    std::f64::consts::PI,
                ] {
                    g.push(Grip::new(GripRole::Radius, point_on_circle(arc, a)));
                }
                g
            } else {
                let mid = 0.5 * (arc.start_angle + arc.end_angle);
                vec![
                    Grip::new(GripRole::Center, arc.center),
                    Grip::new(GripRole::AngleStart, point_on_circle(arc, arc.start_angle)),
                    Grip::new(GripRole::AngleEnd, point_on_circle(arc, arc.end_angle)),
                    Grip::new(GripRole::Radius, point_on_circle(arc, mid)),
                ]
            }
        }
        EntityKind::Curve(Curve::Ellipse(el)) => {
            vec![
                Grip::new(GripRole::Center, el.center),
                Grip::new(GripRole::AxisMajor, ellipse_axis_point(el, true)),
                Grip::new(GripRole::AxisMinor, ellipse_axis_point(el, false)),
            ]
        }
        EntityKind::Curve(Curve::Poly(poly)) => match polyline_vertices(poly) {
            Some(vs) => vs
                .into_iter()
                .enumerate()
                .map(|(i, v)| Grip::new(GripRole::Vertex(i), v))
                .collect(),
            None => poly_edit_points(poly)
                .into_iter()
                .enumerate()
                .map(|(i, ep)| Grip::new(GripRole::PolyPoint(i), ep.pos))
                .collect(),
        },
        EntityKind::Curve(Curve::Bezier(b)) => vec![
            Grip::new(GripRole::Vertex(0), b.p0),
            Grip::new(GripRole::Vertex(1), b.p1),
            Grip::new(GripRole::Vertex(2), b.p2),
            Grip::new(GripRole::Vertex(3), b.p3),
        ],
        EntityKind::Curve(Curve::Nurbs(nc)) => nc
            .control
            .iter()
            .enumerate()
            .map(|(i, p)| Grip::new(GripRole::Vertex(i), *p))
            .collect(),
        EntityKind::Curve(Curve::Rational(rb)) => rb
            .points
            .iter()
            .enumerate()
            .map(|(i, p)| Grip::new(GripRole::Vertex(i), *p))
            .collect(),
        EntityKind::Point(p) => vec![Grip::new(GripRole::Endpoint(0), *p)],
        EntityKind::Text {
            anchor,
            height,
            rotation,
            ..
        } => {
            let h = height.max(1e-6) * 1.5;
            let rot =
                Point2d::from_f64(anchor.x + h * rotation.cos(), anchor.y + h * rotation.sin());
            vec![
                Grip::new(GripRole::Endpoint(0), *anchor),
                Grip::new(GripRole::Rotation, rot),
            ]
        }
        EntityKind::Dimension { p1, p2, line, .. } | EntityKind::OrthoDim { p1, p2, line, .. } => {
            vec![
                Grip::new(GripRole::Endpoint(0), *p1),
                Grip::new(GripRole::Endpoint(1), *p2),
                Grip::new(GripRole::Vertex(2), *line),
            ]
        }
        EntityKind::AngularDim {
            center,
            p1,
            p2,
            line,
            ..
        } => vec![
            Grip::new(GripRole::Vertex(0), *center),
            Grip::new(GripRole::Endpoint(1), *p1),
            Grip::new(GripRole::Endpoint(2), *p2),
            Grip::new(GripRole::Vertex(3), *line),
        ],
        EntityKind::RadialDim { center, edge, .. } => vec![
            Grip::new(GripRole::Vertex(0), *center),
            Grip::new(GripRole::Endpoint(1), *edge),
        ],
        _ => Vec::new(),
    }
}

fn ellipse_axis_point(el: &EllipticalArc, major: bool) -> Point2d {
    let (len, ang) = if major {
        (el.semi_major, el.rotation)
    } else {
        (el.semi_minor, el.rotation + std::f64::consts::FRAC_PI_2)
    };
    Point2d::from_f64(el.center.x + len * ang.cos(), el.center.y + len * ang.sin())
}

fn polyline_vertices(poly: &PolyCurve) -> Option<Vec<Point2d>> {
    if poly.segments.is_empty() {
        return None;
    }
    let mut vs = Vec::with_capacity(poly.segments.len() + 1);
    for (i, seg) in poly.segments.iter().enumerate() {
        let l = seg.as_line()?;
        if i == 0 {
            vs.push(l.p0);
        }
        vs.push(l.p1);
    }
    Some(vs)
}

fn point_on_circle(arc: &CircularArc, a: f64) -> Point2d {
    Point2d::from_f64(
        arc.center.x + arc.radius * a.cos(),
        arc.center.y + arc.radius * a.sin(),
    )
}

pub fn apply_grip(start: &EntityKind, grip: &Grip, to: Point2d) -> EntityKind {
    match (start, grip.role) {
        (EntityKind::Curve(Curve::Line(l)), GripRole::Endpoint(0)) => {
            EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(to, l.p1)))
        }
        (EntityKind::Curve(Curve::Line(l)), GripRole::Endpoint(1)) => {
            EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(l.p0, to)))
        }
        (EntityKind::Curve(Curve::Arc(arc)), GripRole::Center) => {
            EntityKind::Curve(Curve::Arc(CircularArc { center: to, ..*arc }))
        }
        (EntityKind::Curve(Curve::Arc(arc)), GripRole::Radius) => {
            let r = arc.center.dist_f64(&to).max(MIN_RADIUS);
            EntityKind::Curve(Curve::Arc(CircularArc { radius: r, ..*arc }))
        }
        (EntityKind::Curve(Curve::Arc(arc)), GripRole::AngleStart) => {
            let a = (to.y - arc.center.y).atan2(to.x - arc.center.x);
            EntityKind::Curve(Curve::Arc(with_angles(arc, a, arc.end_angle)))
        }
        (EntityKind::Curve(Curve::Arc(arc)), GripRole::AngleEnd) => {
            let a = (to.y - arc.center.y).atan2(to.x - arc.center.x);
            EntityKind::Curve(Curve::Arc(with_angles(arc, arc.start_angle, a)))
        }
        (EntityKind::Curve(Curve::Ellipse(el)), GripRole::Center) => {
            EntityKind::Curve(Curve::Ellipse(EllipticalArc { center: to, ..*el }))
        }
        (EntityKind::Curve(Curve::Ellipse(el)), GripRole::AxisMajor) => {
            let r = el.center.dist_f64(&to).max(MIN_RADIUS);
            EntityKind::Curve(Curve::Ellipse(EllipticalArc {
                semi_major: r,
                ..*el
            }))
        }
        (EntityKind::Curve(Curve::Ellipse(el)), GripRole::AxisMinor) => {
            let r = el.center.dist_f64(&to).max(MIN_RADIUS);
            EntityKind::Curve(Curve::Ellipse(EllipticalArc {
                semi_minor: r,
                ..*el
            }))
        }
        (EntityKind::Curve(Curve::Poly(poly)), GripRole::Vertex(k)) => {
            match move_polyline_vertex(poly, k, to) {
                Some(p) => EntityKind::Curve(Curve::Poly(Box::new(p))),
                None => start.clone(),
            }
        }
        (EntityKind::Curve(Curve::Poly(poly)), GripRole::PolyPoint(idx)) => {
            let pts = poly_edit_points(poly);
            match pts.get(idx) {
                Some(ep) => {
                    let mut segs = poly.segments.clone();
                    for &(s, ci) in &ep.writes {
                        if let Some(seg) = segs.get_mut(s) {
                            set_poly_ctrl(seg, ci, to);
                        }
                    }
                    EntityKind::Curve(Curve::Poly(Box::new(PolyCurve::new(segs))))
                }
                None => start.clone(),
            }
        }
        (EntityKind::Curve(Curve::Bezier(b)), GripRole::Vertex(k)) => {
            let mut nb = b.clone();
            match k {
                0 => nb.p0 = to,
                1 => nb.p1 = to,
                2 => nb.p2 = to,
                3 => nb.p3 = to,
                _ => {}
            }
            EntityKind::Curve(Curve::Bezier(nb))
        }
        (EntityKind::Curve(Curve::Nurbs(nc)), GripRole::Vertex(k)) => {
            let mut control = nc.control.clone();
            if k < control.len() {
                control[k] = to;
                EntityKind::Curve(Curve::Nurbs(NurbsCurve::new(control, nc.weights.clone())))
            } else {
                start.clone()
            }
        }
        (EntityKind::Curve(Curve::Rational(rb)), GripRole::Vertex(k)) => {
            let mut points = rb.points.clone();
            if k < points.len() {
                points[k] = to;
                EntityKind::Curve(Curve::Rational(RationalBezier {
                    points,
                    weights: rb.weights.clone(),
                }))
            } else {
                start.clone()
            }
        }

        (EntityKind::Point(_), GripRole::Endpoint(_)) => EntityKind::Point(to),

        (
            EntityKind::Text {
                content,
                height,
                rotation,
                font,
                ..
            },
            GripRole::Endpoint(0),
        ) => EntityKind::Text {
            anchor: to,
            content: content.clone(),
            height: *height,
            rotation: *rotation,
            font: font.clone(),
        },
        (
            EntityKind::Text {
                anchor,
                content,
                height,
                font,
                ..
            },
            GripRole::Rotation,
        ) => {
            let r = (to.y - anchor.y).atan2(to.x - anchor.x);
            EntityKind::Text {
                anchor: *anchor,
                content: content.clone(),
                height: *height,
                rotation: r,
                font: font.clone(),
            }
        }

        (
            EntityKind::Dimension {
                p2,
                line,
                height,
                override_text,
                ..
            },
            GripRole::Endpoint(0),
        ) => EntityKind::Dimension {
            p1: to,
            p2: *p2,
            line: *line,
            height: *height,
            override_text: override_text.clone(),
        },
        (
            EntityKind::Dimension {
                p1,
                line,
                height,
                override_text,
                ..
            },
            GripRole::Endpoint(1),
        ) => EntityKind::Dimension {
            p1: *p1,
            p2: to,
            line: *line,
            height: *height,
            override_text: override_text.clone(),
        },
        (
            EntityKind::Dimension {
                p1,
                p2,
                height,
                override_text,
                ..
            },
            GripRole::Vertex(2),
        ) => EntityKind::Dimension {
            p1: *p1,
            p2: *p2,
            line: to,
            height: *height,
            override_text: override_text.clone(),
        },

        (
            EntityKind::OrthoDim {
                p1,
                p2,
                line,
                vertical,
                height,
                override_text,
            },
            role,
        ) => {
            let (mut a, mut b, mut l) = (*p1, *p2, *line);
            match role {
                GripRole::Endpoint(0) => a = to,
                GripRole::Endpoint(1) => b = to,
                GripRole::Vertex(2) => l = to,
                _ => {}
            }
            EntityKind::OrthoDim {
                p1: a,
                p2: b,
                line: l,
                vertical: *vertical,
                height: *height,
                override_text: override_text.clone(),
            }
        }

        (
            EntityKind::AngularDim {
                center,
                p1,
                p2,
                line,
                height,
                override_text,
            },
            role,
        ) => {
            let (mut c, mut a, mut b, mut l) = (*center, *p1, *p2, *line);
            match role {
                GripRole::Vertex(0) => c = to,
                GripRole::Endpoint(1) => a = to,
                GripRole::Endpoint(2) => b = to,
                GripRole::Vertex(3) => l = to,
                _ => {}
            }
            EntityKind::AngularDim {
                center: c,
                p1: a,
                p2: b,
                line: l,
                height: *height,
                override_text: override_text.clone(),
            }
        }

        (
            EntityKind::RadialDim {
                center,
                edge,
                diameter,
                height,
                override_text,
            },
            role,
        ) => {
            let (mut c, mut e) = (*center, *edge);
            match role {
                GripRole::Vertex(0) => c = to,
                GripRole::Endpoint(1) => e = to,
                _ => {}
            }
            EntityKind::RadialDim {
                center: c,
                edge: e,
                diameter: *diameter,
                height: *height,
                override_text: override_text.clone(),
            }
        }

        _ => start.clone(),
    }
}

pub(crate) struct EditPoint {
    pub(crate) pos: Point2d,
    pub(crate) writes: Vec<(usize, u8)>,
}

pub(crate) fn poly_edit_points(poly: &PolyCurve) -> Vec<EditPoint> {
    let mut pts: Vec<EditPoint> = Vec::new();
    let mut last_node: Option<usize> = None;
    for (s, seg) in poly.segments.iter().enumerate() {
        let (start, end, end_ctrl, mids): (Point2d, Point2d, u8, Vec<(u8, Point2d)>) = match seg {
            Curve::Line(l) => (l.p0, l.p1, 1, vec![]),
            Curve::Bezier(b) => (b.p0, b.p3, 3, vec![(1, b.p1), (2, b.p2)]),
            other => {
                let (t0, t1) = other.domain();
                let a = other.evaluate_f64(t0);
                let c = other.evaluate_f64(t1);
                (
                    Point2d::from_f64(a.0, a.1),
                    Point2d::from_f64(c.0, c.1),
                    1,
                    vec![],
                )
            }
        };
        match last_node {
            Some(idx) => pts[idx].writes.push((s, 0)),
            None => pts.push(EditPoint {
                pos: start,
                writes: vec![(s, 0)],
            }),
        }
        for (ci, p) in mids {
            pts.push(EditPoint {
                pos: p,
                writes: vec![(s, ci)],
            });
        }
        pts.push(EditPoint {
            pos: end,
            writes: vec![(s, end_ctrl)],
        });
        last_node = Some(pts.len() - 1);
    }
    if pts.len() >= 2 {
        let last = pts.len() - 1;
        if pts[0].pos.dist_f64(&pts[last].pos) < 1e-6 {
            let writes = pts[last].writes.clone();
            pts[0].writes.extend(writes);
            pts.pop();
        }
    }
    pts
}

pub(crate) fn set_poly_ctrl(seg: &mut Curve, ci: u8, to: Point2d) {
    match seg {
        Curve::Line(l) => {
            if ci == 0 {
                l.p0 = to;
            } else {
                l.p1 = to;
            }
        }
        Curve::Bezier(b) => match ci {
            0 => b.p0 = to,
            1 => b.p1 = to,
            2 => b.p2 = to,
            3 => b.p3 = to,
            _ => {}
        },
        _ => {}
    }
}

fn move_polyline_vertex(poly: &PolyCurve, k: usize, to: Point2d) -> Option<PolyCurve> {
    let n = poly.segments.len();
    if n == 0 || k > n {
        return None;
    }
    let mut verts = polyline_vertices(poly)?;
    if k >= verts.len() {
        return None;
    }
    verts[k] = to;
    let segments = verts
        .windows(2)
        .map(|w| Curve::Line(LineSeg::from_endpoints(w[0], w[1])))
        .collect();
    Some(PolyCurve::new(segments))
}

const MIN_RADIUS: f64 = 1e-6;

pub fn apply_grip_value(
    start: &EntityKind,
    grip: &Grip,
    value: f64,
    cursor: Point2d,
) -> EntityKind {
    match (start, grip.role) {
        (EntityKind::Curve(Curve::Line(l)), GripRole::Endpoint(0)) => {
            let p = along(l.p1, cursor, l.p0, value);
            EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(p, l.p1)))
        }
        (EntityKind::Curve(Curve::Line(l)), GripRole::Endpoint(1)) => {
            let p = along(l.p0, cursor, l.p1, value);
            EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(l.p0, p)))
        }
        (EntityKind::Curve(Curve::Arc(arc)), GripRole::Radius) => {
            EntityKind::Curve(Curve::Arc(CircularArc {
                radius: value.max(MIN_RADIUS),
                ..*arc
            }))
        }
        (EntityKind::Curve(Curve::Arc(arc)), GripRole::Center) => {
            let c = along(arc.center, cursor, arc.center, value);
            EntityKind::Curve(Curve::Arc(CircularArc { center: c, ..*arc }))
        }
        (EntityKind::Curve(Curve::Arc(arc)), GripRole::AngleStart) => EntityKind::Curve(
            Curve::Arc(with_angles(arc, value.to_radians(), arc.end_angle)),
        ),
        (EntityKind::Curve(Curve::Arc(arc)), GripRole::AngleEnd) => EntityKind::Curve(Curve::Arc(
            with_angles(arc, arc.start_angle, value.to_radians()),
        )),

        (EntityKind::Curve(Curve::Ellipse(el)), GripRole::AxisMajor) => {
            EntityKind::Curve(Curve::Ellipse(EllipticalArc {
                semi_major: value.max(MIN_RADIUS),
                ..*el
            }))
        }
        (EntityKind::Curve(Curve::Ellipse(el)), GripRole::AxisMinor) => {
            EntityKind::Curve(Curve::Ellipse(EllipticalArc {
                semi_minor: value.max(MIN_RADIUS),
                ..*el
            }))
        }

        (
            EntityKind::Text {
                anchor,
                content,
                height,
                font,
                ..
            },
            GripRole::Rotation,
        ) => EntityKind::Text {
            anchor: *anchor,
            content: content.clone(),
            height: *height,
            rotation: value.to_radians(),
            font: font.clone(),
        },

        _ => start.clone(),
    }
}

pub fn grip_value_label(role: GripRole) -> &'static str {
    match role {
        GripRole::Radius => "R",
        GripRole::Endpoint(_) => "Len",
        GripRole::Center => "Dist",
        GripRole::AngleStart | GripRole::AngleEnd => "Ang°",
        GripRole::Vertex(_) => "Len",
        GripRole::PolyPoint(_) => "Len",
        GripRole::Rotation => "Ang°",
        GripRole::AxisMajor => "A",
        GripRole::AxisMinor => "B",
    }
}

fn along(base: Point2d, toward: Point2d, fallback: Point2d, dist: f64) -> Point2d {
    let (mut dx, mut dy) = (toward.x - base.x, toward.y - base.y);
    let mut len = (dx * dx + dy * dy).sqrt();
    if len < 1e-12 {
        dx = fallback.x - base.x;
        dy = fallback.y - base.y;
        len = (dx * dx + dy * dy).sqrt();
    }
    if len < 1e-12 {
        return Point2d::from_f64(base.x + dist, base.y);
    }
    Point2d::from_f64(base.x + dx / len * dist, base.y + dy / len * dist)
}

pub(crate) fn with_angles(arc: &CircularArc, start: f64, mut end: f64) -> CircularArc {
    let two_pi = 2.0 * std::f64::consts::PI;
    let start = start.rem_euclid(two_pi);
    while end <= start {
        end += two_pi;
    }
    while end > start + two_pi {
        end -= two_pi;
    }
    CircularArc {
        center: arc.center,
        radius: arc.radius,
        start_angle: start,
        end_angle: end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI};

    fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> EntityKind {
        EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(x0, y0),
            Point2d::from_f64(x1, y1),
        )))
    }
    fn circle(cx: f64, cy: f64, r: f64) -> EntityKind {
        EntityKind::Curve(Curve::Arc(CircularArc::new(
            Point2d::from_f64(cx, cy),
            r,
            0.0,
            2.0 * PI,
        )))
    }

    #[test]
    fn line_exposes_two_endpoint_grips() {
        let g = grips_for(&line(0.0, 0.0, 10.0, 0.0));
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].role, GripRole::Endpoint(0));
        assert_eq!(g[1].role, GripRole::Endpoint(1));
        assert_eq!(g[0].world, Point2d::from_f64(0.0, 0.0));
        assert_eq!(g[1].world, Point2d::from_f64(10.0, 0.0));
    }

    #[test]
    fn dragging_endpoint_moves_only_that_end() {
        let start = line(0.0, 0.0, 10.0, 0.0);
        let g = grips_for(&start);
        let edited = apply_grip(&start, &g[1], Point2d::from_f64(10.0, 5.0));
        if let EntityKind::Curve(Curve::Line(l)) = edited {
            assert_eq!(l.p0, Point2d::from_f64(0.0, 0.0));
            assert_eq!(l.p1, Point2d::from_f64(10.0, 5.0));
        } else {
            panic!("expected a line");
        }
    }

    #[test]
    fn circle_exposes_center_then_four_radius_grips() {
        let g = grips_for(&circle(0.0, 0.0, 5.0));
        assert_eq!(g.len(), 5);
        assert_eq!(g[0].role, GripRole::Center);
        assert!(g[1..].iter().all(|h| h.role == GripRole::Radius));
        for h in &g[1..] {
            assert!((h.world.dist_f64(&Point2d::from_f64(0.0, 0.0)) - 5.0).abs() < 1e-9);
        }
    }

    #[test]
    fn dragging_center_translates_circle() {
        let start = circle(0.0, 0.0, 5.0);
        let g = grips_for(&start);
        let edited = apply_grip(&start, &g[0], Point2d::from_f64(3.0, 4.0));
        if let EntityKind::Curve(Curve::Arc(a)) = edited {
            assert_eq!(a.center, Point2d::from_f64(3.0, 4.0));
            assert_eq!(a.radius, 5.0);
        } else {
            panic!("expected an arc");
        }
    }

    #[test]
    fn dragging_quadrant_changes_radius_not_center() {
        let start = circle(0.0, 0.0, 5.0);
        let g = grips_for(&start);
        let edited = apply_grip(&start, &g[1], Point2d::from_f64(8.0, 0.0));
        if let EntityKind::Curve(Curve::Arc(a)) = edited {
            assert_eq!(a.center, Point2d::from_f64(0.0, 0.0));
            assert!((a.radius - 8.0).abs() < 1e-9);
        } else {
            panic!("expected an arc");
        }
    }

    #[test]
    fn radius_drag_clamps_to_minimum() {
        let start = circle(0.0, 0.0, 5.0);
        let g = grips_for(&start);
        let edited = apply_grip(&start, &g[1], Point2d::from_f64(0.0, 0.0));
        if let EntityKind::Curve(Curve::Arc(a)) = edited {
            assert!(a.radius >= MIN_RADIUS);
        } else {
            panic!("expected an arc");
        }
    }

    #[test]
    fn partial_arc_exposes_center_endpoints_radius() {
        let arc = EntityKind::Curve(Curve::Arc(CircularArc::new(
            Point2d::from_f64(0.0, 0.0),
            5.0,
            0.0,
            FRAC_PI_2,
        )));
        let g = grips_for(&arc);
        assert_eq!(g.len(), 4);
        assert_eq!(g[0].role, GripRole::Center);
        assert_eq!(g[1].role, GripRole::AngleStart);
        assert_eq!(g[2].role, GripRole::AngleEnd);
        assert_eq!(g[3].role, GripRole::Radius);
    }

    #[test]
    fn arc_angle_grip_keeps_end_after_start() {
        let arc = EntityKind::Curve(Curve::Arc(CircularArc::new(
            Point2d::from_f64(0.0, 0.0),
            5.0,
            0.0,
            FRAC_PI_2,
        )));
        let g = grips_for(&arc);
        let edited = apply_grip(&arc, &g[2], Point2d::from_f64(0.0, -5.0));
        if let EntityKind::Curve(Curve::Arc(a)) = edited {
            assert!(a.end_angle > a.start_angle, "end must stay after start");
        } else {
            panic!("expected an arc");
        }
    }

    #[test]
    fn typed_value_sets_circle_radius() {
        let start = circle(0.0, 0.0, 5.0);
        let g = grips_for(&start);
        let edited = apply_grip_value(&start, &g[1], 12.0, Point2d::from_f64(0.0, 0.0));
        if let EntityKind::Curve(Curve::Arc(a)) = edited {
            assert_eq!(a.center, Point2d::from_f64(0.0, 0.0));
            assert!((a.radius - 12.0).abs() < 1e-9);
        } else {
            panic!("expected an arc");
        }
    }

    #[test]
    fn typed_value_sets_line_endpoint_length() {
        let start = line(0.0, 0.0, 10.0, 0.0);
        let g = grips_for(&start);
        let edited = apply_grip_value(&start, &g[1], 3.0, Point2d::from_f64(0.0, 7.0));
        if let EntityKind::Curve(Curve::Line(l)) = edited {
            assert_eq!(l.p0, Point2d::from_f64(0.0, 0.0));
            assert!((l.p1.dist_f64(&l.p0) - 3.0).abs() < 1e-9);
            assert!((l.p1.x - 0.0).abs() < 1e-9 && (l.p1.y - 3.0).abs() < 1e-9);
        } else {
            panic!("expected a line");
        }
    }

    #[test]
    fn point_grip_moves_the_point() {
        let start = EntityKind::Point(Point2d::from_f64(1.0, 2.0));
        let g = grips_for(&start);
        assert_eq!(g.len(), 1);
        let edited = apply_grip(&start, &g[0], Point2d::from_f64(7.0, 8.0));
        match edited {
            EntityKind::Point(p) => assert_eq!(p, Point2d::from_f64(7.0, 8.0)),
            _ => panic!("expected a point"),
        }
    }

    #[test]
    fn text_anchor_and_rotation_grips() {
        let start = EntityKind::Text {
            anchor: Point2d::from_f64(0.0, 0.0),
            content: "hi".into(),
            height: 2.0,
            rotation: 0.0,
            font: None,
        };
        let g = grips_for(&start);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].role, GripRole::Endpoint(0));
        assert_eq!(g[1].role, GripRole::Rotation);
        if let EntityKind::Text { anchor, .. } =
            apply_grip(&start, &g[0], Point2d::from_f64(5.0, 5.0))
        {
            assert_eq!(anchor, Point2d::from_f64(5.0, 5.0));
        } else {
            panic!("expected text");
        }
        if let EntityKind::Text { rotation, .. } =
            apply_grip(&start, &g[1], Point2d::from_f64(0.0, 3.0))
        {
            assert!((rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        } else {
            panic!("expected text");
        }
    }

    #[test]
    fn ellipse_center_and_axis_grips() {
        let start = EntityKind::Curve(Curve::Ellipse(EllipticalArc::new(
            Point2d::from_f64(0.0, 0.0),
            5.0,
            3.0,
            0.0,
            0.0,
            2.0 * PI,
        )));
        let g = grips_for(&start);
        assert_eq!(g.len(), 3);
        assert_eq!(g[0].role, GripRole::Center);
        assert_eq!(g[1].role, GripRole::AxisMajor);
        assert_eq!(g[2].role, GripRole::AxisMinor);
        assert!((g[1].world.dist_f64(&Point2d::from_f64(5.0, 0.0))).abs() < 1e-9);
        assert!((g[2].world.dist_f64(&Point2d::from_f64(0.0, 3.0))).abs() < 1e-9);
        if let EntityKind::Curve(Curve::Ellipse(e)) =
            apply_grip(&start, &g[1], Point2d::from_f64(9.0, 0.0))
        {
            assert!((e.semi_major - 9.0).abs() < 1e-9);
            assert!((e.semi_minor - 3.0).abs() < 1e-9);
            assert_eq!(e.center, Point2d::from_f64(0.0, 0.0));
        } else {
            panic!("expected ellipse");
        }
    }

    #[test]
    fn polyline_vertex_grips_move_one_vertex() {
        let poly = PolyCurve::new(vec![
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(0.0, 0.0),
                Point2d::from_f64(1.0, 0.0),
            )),
            Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(1.0, 0.0),
                Point2d::from_f64(2.0, 0.0),
            )),
        ]);
        let start = EntityKind::Curve(Curve::Poly(Box::new(poly)));
        let g = grips_for(&start);
        assert_eq!(g.len(), 3);
        assert!(matches!(g[1].role, GripRole::Vertex(1)));
        if let EntityKind::Curve(Curve::Poly(p)) =
            apply_grip(&start, &g[1], Point2d::from_f64(1.0, 5.0))
        {
            let l0 = p.segments[0].as_line().unwrap();
            let l1 = p.segments[1].as_line().unwrap();
            assert_eq!(l0.p1, Point2d::from_f64(1.0, 5.0));
            assert_eq!(l1.p0, Point2d::from_f64(1.0, 5.0));
            assert_eq!(l0.p0, Point2d::from_f64(0.0, 0.0));
            assert_eq!(l1.p1, Point2d::from_f64(2.0, 0.0));
        } else {
            panic!("expected polyline");
        }
    }

    #[test]
    fn spline_control_vertex_grips() {
        use oxidraft_geometry::NurbsCurve;
        let nc = NurbsCurve::uniform(vec![
            Point2d::from_f64(0.0, 0.0),
            Point2d::from_f64(1.0, 2.0),
            Point2d::from_f64(3.0, 0.0),
        ]);
        let start = EntityKind::Curve(Curve::Nurbs(nc));
        let g = grips_for(&start);
        assert_eq!(g.len(), 3);
        assert!(matches!(g[1].role, GripRole::Vertex(1)));
        if let EntityKind::Curve(Curve::Nurbs(n)) =
            apply_grip(&start, &g[1], Point2d::from_f64(1.0, 9.0))
        {
            assert_eq!(n.control[1], Point2d::from_f64(1.0, 9.0));
            assert_eq!(n.control[0], Point2d::from_f64(0.0, 0.0));
            assert_eq!(n.weights, vec![1.0, 1.0, 1.0]);
        } else {
            panic!("expected a spline");
        }
    }

    #[test]
    fn rational_bezier_control_vertex_grips() {
        // A G2/G3 blend curve is stored as Curve::Rational; it must be just as
        // draggable as a Curve::Bezier or Curve::Nurbs spline.
        use oxidraft_geometry::RationalBezier;
        let rb = RationalBezier::polynomial(vec![
            Point2d::from_f64(0.0, 0.0),
            Point2d::from_f64(1.0, 2.0),
            Point2d::from_f64(2.0, 3.0),
            Point2d::from_f64(3.0, 2.0),
            Point2d::from_f64(4.0, 0.0),
        ]);
        let start = EntityKind::Curve(Curve::Rational(rb));
        let g = grips_for(&start);
        assert_eq!(g.len(), 5, "one grip per control point");
        assert!(matches!(g[2].role, GripRole::Vertex(2)));
        if let EntityKind::Curve(Curve::Rational(r)) =
            apply_grip(&start, &g[2], Point2d::from_f64(2.0, 9.0))
        {
            assert_eq!(r.points[2], Point2d::from_f64(2.0, 9.0));
            assert_eq!(r.points[0], Point2d::from_f64(0.0, 0.0));
            assert_eq!(r.weights, vec![1.0; 5]);
        } else {
            panic!("expected a rational bezier");
        }
    }

    #[test]
    fn value_label_matches_role() {
        assert_eq!(grip_value_label(GripRole::Radius), "R");
        assert_eq!(grip_value_label(GripRole::Endpoint(0)), "Len");
        assert_eq!(grip_value_label(GripRole::AngleEnd), "Ang°");
    }

    #[test]
    fn unknown_role_returns_start_unchanged() {
        let start = line(0.0, 0.0, 10.0, 0.0);
        let bogus = Grip::new(GripRole::Rotation, Point2d::from_f64(0.0, 0.0));
        let edited = apply_grip(&start, &bogus, Point2d::from_f64(1.0, 1.0));
        if let (EntityKind::Curve(Curve::Line(a)), EntityKind::Curve(Curve::Line(b))) =
            (&start, &edited)
        {
            assert_eq!(a.p0, b.p0);
            assert_eq!(a.p1, b.p1);
        } else {
            panic!("expected lines");
        }
    }
}
