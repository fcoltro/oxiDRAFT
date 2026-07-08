use oxidraft_document::{Document, EntityId, EntityKind};
use oxidraft_geometry::{
    CircularArc, CubicBezier, Curve, EllipticalArc, LineSeg, Point2d, PolyCurve,
};

pub fn line(doc: &mut Document, a: Point2d, b: Point2d) -> EntityId {
    doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
        a, b,
    ))))
}

pub fn circle(doc: &mut Document, center: Point2d, radius: f64) -> EntityId {
    let arc = CircularArc::new(center, radius, 0.0, 2.0 * std::f64::consts::PI);
    doc.add(EntityKind::Curve(Curve::Arc(arc)))
}

pub fn circle_3p(doc: &mut Document, p1: &Point2d, p2: &Point2d, p3: &Point2d) -> Option<EntityId> {
    let arc = CircularArc::from_three_points(p1, p2, p3)?;
    let full = CircularArc::new(arc.center, arc.radius, 0.0, 2.0 * std::f64::consts::PI);
    Some(doc.add(EntityKind::Curve(Curve::Arc(full))))
}

pub fn arc(doc: &mut Document, center: Point2d, radius: f64, start: f64, end: f64) -> EntityId {
    doc.add(EntityKind::Curve(Curve::Arc(CircularArc::new(
        center, radius, start, end,
    ))))
}

pub fn ellipse(
    doc: &mut Document,
    center: Point2d,
    major: f64,
    minor: f64,
    rotation: f64,
) -> EntityId {
    let e = EllipticalArc::new(
        center,
        major,
        minor,
        rotation,
        0.0,
        2.0 * std::f64::consts::PI,
    );
    doc.add(EntityKind::Curve(Curve::Ellipse(e)))
}

pub fn rectangle(doc: &mut Document, c0: &Point2d, c1: &Point2d) -> Vec<EntityId> {
    let (x0, x1) = order(c0.x, c1.x);
    let (y0, y1) = order(c0.y, c1.y);
    let p = |x: f64, y: f64| Point2d::new(x, y);
    let corners = [p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)];
    (0..4)
        .map(|i| line(doc, corners[i], corners[(i + 1) % 4]))
        .collect()
}

pub fn polygon(
    doc: &mut Document,
    center: &Point2d,
    n: u32,
    radius: f64,
    inscribed: bool,
    start_angle: f64,
) -> Vec<EntityId> {
    // Side count and geometry come straight from user input: decline
    // instead of panicking on n < 3 or adding a billion entities on a
    // typo'd count, and don't let a non-finite center/radius/angle write
    // NaN vertices into the document.
    if !(3..=4096).contains(&n)
        || !center.is_finite()
        || !radius.is_finite()
        || !start_angle.is_finite()
    {
        return Vec::new();
    }
    let r = if inscribed {
        radius
    } else {
        radius / (std::f64::consts::PI / n as f64).cos()
    };
    let (cx, cy) = center.to_f64();
    let verts: Vec<Point2d> = (0..n)
        .map(|i| {
            let a = start_angle + 2.0 * std::f64::consts::PI * i as f64 / n as f64;
            Point2d::from_f64(cx + r * a.cos(), cy + r * a.sin())
        })
        .collect();
    (0..n as usize)
        .map(|i| line(doc, verts[i], verts[(i + 1) % n as usize]))
        .collect()
}

pub fn bezier(doc: &mut Document, p0: Point2d, p1: Point2d, p2: Point2d, p3: Point2d) -> EntityId {
    doc.add(EntityKind::Curve(Curve::Bezier(CubicBezier::new(
        p0, p1, p2, p3,
    ))))
}

pub fn polycurve(doc: &mut Document, segments: Vec<Curve>) -> EntityId {
    doc.add(EntityKind::Curve(Curve::Poly(Box::new(PolyCurve::new(
        segments,
    )))))
}

pub fn point(doc: &mut Document, p: Point2d) -> EntityId {
    doc.add(EntityKind::Point(p))
}

pub fn divide(doc: &mut Document, curve: &Curve, n: u32) -> Vec<EntityId> {
    use oxidraft_geometry::CurveSegment;
    // Matches AutoCAD's DIVIDE segment cap; a corrupt count would balloon
    // the document with millions of point entities.
    if n > 32_767 {
        return Vec::new();
    }
    let (t0, t1) = curve.domain();
    (1..n)
        .filter_map(|i| {
            let t = t0 + (t1 - t0) * i as f64 / n as f64;
            let (x, y) = curve.evaluate_f64(t);
            let p = Point2d::from_f64(x, y);
            // A poisoned curve evaluates to NaN; drop those division
            // points rather than storing them.
            p.is_finite().then(|| point(doc, p))
        })
        .collect()
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
    fn draw_line_and_circle() {
        let mut doc = Document::new();
        line(&mut doc, pt(0, 0), pt(5, 5));
        circle(&mut doc, pt(2, 2), 3.0);
        assert_eq!(doc.len(), 2);
    }

    #[test]
    fn rectangle_has_four_sides_and_correct_extents() {
        let mut doc = Document::new();
        let ids = rectangle(&mut doc, &pt(0, 0), &pt(4, 3));
        assert_eq!(ids.len(), 4);
        let bb = doc.extents().unwrap();
        assert_eq!(bb.min, pt(0, 0));
        assert_eq!(bb.max, pt(4, 3));
    }

    #[test]
    fn polygon_vertex_count() {
        let mut doc = Document::new();
        let edges = polygon(&mut doc, &pt(0, 0), 6, 5.0, true, 0.0);
        assert_eq!(edges.len(), 6);
    }

    #[test]
    fn inscribed_polygon_vertices_on_circle() {
        let mut doc = Document::new();
        polygon(&mut doc, &pt(0, 0), 4, 5.0, true, 0.0);
        for e in doc.iter() {
            if let Some(Curve::Line(l)) = e.as_curve() {
                let d = (l.p0.x.powi(2) + l.p0.y.powi(2)).sqrt();
                assert!((d - 5.0).abs() < 1e-6, "vertex not on circle: d={}", d);
            }
        }
    }

    #[test]
    fn divide_creates_n_minus_1_points() {
        let mut doc = Document::new();
        let c = Curve::Line(LineSeg::from_endpoints(pt(0, 0), pt(10, 0)));
        let pts = divide(&mut doc, &c, 5);
        assert_eq!(pts.len(), 4);
        let first = doc.get(pts[0]).unwrap();
        if let EntityKind::Point(p) = &first.kind {
            assert!((p.x - 2.0).abs() < 1e-6);
        } else {
            panic!()
        }
    }

    #[test]
    fn circle_3p_through_points() {
        let mut doc = Document::new();
        let id = circle_3p(
            &mut doc,
            &Point2d::from_f64(1.0, 0.0),
            &Point2d::from_f64(0.0, 1.0),
            &Point2d::from_f64(-1.0, 0.0),
        )
        .unwrap();
        if let Some(Curve::Arc(a)) = doc.get(id).unwrap().as_curve() {
            assert!(a.center.x.abs() < 1e-9 && a.center.y.abs() < 1e-9);
            assert!((a.radius - 1.0).abs() < 1e-6);
        } else {
            panic!()
        }
    }
}
