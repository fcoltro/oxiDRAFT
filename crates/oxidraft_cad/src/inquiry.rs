use oxidraft_document::{Document, EntityId, EntityKind, HatchPattern};
use oxidraft_geometry::{Curve, CurveSegment, Point2d, curve_to_curve_distance};

fn hatch_pattern_name(p: &HatchPattern) -> &'static str {
    match p {
        HatchPattern::Solid => "solid",
        HatchPattern::Lines { .. } => "lines",
        HatchPattern::Cross { .. } => "cross",
        HatchPattern::Dots { .. } => "dots",
    }
}

pub fn distance_points(a: &Point2d, b: &Point2d) -> (f64, f64) {
    let dsq = a.dist_sq(b);
    (dsq, dsq.sqrt())
}

pub fn distance_entities(doc: &Document, a: EntityId, b: EntityId) -> Option<f64> {
    let ca = doc.get(a)?.as_curve()?;
    let cb = doc.get(b)?.as_curve()?;
    Some(curve_to_curve_distance(ca, cb))
}

pub fn area_of_loop(doc: &Document, ids: &[EntityId]) -> f64 {
    let mut area = 0.0;
    let steps = 32usize;
    for &id in ids {
        if let Some(c) = doc.get(id).and_then(|e| e.as_curve()) {
            let (t0, t1) = c.domain();
            let dt = (t1 - t0) / steps as f64;
            for i in 0..steps {
                let (x1, y1) = c.evaluate_f64(t0 + dt * i as f64);
                let (x2, y2) = c.evaluate_f64(t0 + dt * (i + 1) as f64);
                area += (x1 + x2) * (y2 - y1);
            }
        }
    }
    (area / 2.0).abs()
}

pub fn total_length(doc: &Document, ids: &[EntityId]) -> f64 {
    ids.iter()
        .filter_map(|&id| doc.get(id).and_then(|e| e.as_curve()))
        .map(|c| c.arc_length())
        .sum()
}

pub fn list_entity(doc: &Document, id: EntityId) -> Option<String> {
    let e = doc.get(id)?;
    let layer_name = doc
        .layers
        .get(e.layer)
        .map(|l| l.name.as_str())
        .unwrap_or("?");
    let geom = match &e.kind {
        EntityKind::Curve(Curve::Line(l)) => format!(
            "LINE  ({},{}) → ({},{})  len={:.4}",
            l.p0.x,
            l.p0.y,
            l.p1.x,
            l.p1.y,
            l.length_f64()
        ),
        EntityKind::Curve(Curve::Arc(a)) => {
            let span = (a.end_angle - a.start_angle).abs();
            let kind = if (span - 2.0 * std::f64::consts::PI).abs() < 1e-9 {
                "CIRCLE"
            } else {
                "ARC"
            };
            format!(
                "{}  center=({},{})  r={}",
                kind, a.center.x, a.center.y, a.radius
            )
        }
        EntityKind::Curve(Curve::Ellipse(el)) => format!(
            "ELLIPSE  center=({},{})  a={} b={}",
            el.center.x, el.center.y, el.semi_major, el.semi_minor
        ),
        EntityKind::Curve(Curve::Bezier(_)) => "SPLINE (cubic Bézier)".to_string(),
        EntityKind::Curve(Curve::Rational(rb)) => format!(
            "SPLINE (rational Bézier, degree {}, {} control points)",
            rb.degree(),
            rb.points.len()
        ),
        EntityKind::Curve(Curve::Nurbs(nc)) => {
            format!("NURBS SPLINE ({} control vertices)", nc.control.len())
        }
        EntityKind::Curve(Curve::Poly(p)) => format!("POLYLINE  {} segments", p.segments.len()),
        EntityKind::Point(p) => format!("POINT  ({},{})", p.x, p.y),
        EntityKind::Text { content, .. } => format!("TEXT  \"{}\"", content),
        EntityKind::XLine { .. } => "XLINE (construction)".to_string(),
        EntityKind::Ray { .. } => "RAY (construction)".to_string(),
        EntityKind::Insert { block, .. } => format!("INSERT  block=\"{}\"", block),
        EntityKind::Hatch {
            boundary,
            holes,
            fill,
            pattern,
        } => format!(
            "HATCH  {} boundary segs, {} island(s)  fill=#{:02x}{:02x}{:02x}  {}",
            boundary.len(),
            holes.len(),
            fill.0,
            fill.1,
            fill.2,
            hatch_pattern_name(pattern)
        ),
        EntityKind::Dimension { p1, p2, .. } => {
            format!("DIMENSION  length={:.4}", p1.dist_f64(p2))
        }
        EntityKind::OrthoDim {
            p1, p2, vertical, ..
        } => {
            let (a, b) = (p1.to_f64(), p2.to_f64());
            let d = if *vertical {
                (b.1 - a.1).abs()
            } else {
                (b.0 - a.0).abs()
            };
            let axis = if *vertical { "vertical" } else { "horizontal" };
            format!("DIMENSION ({axis})  length={d:.4}")
        }
        EntityKind::AngularDim { center, p1, p2, .. } => {
            let (cx, cy) = center.to_f64();
            let (a1x, a1y) = p1.to_f64();
            let (a2x, a2y) = p2.to_f64();
            let a = oxidraft_geometry::wrap_deg360(
                ((a2y - cy).atan2(a2x - cx) - (a1y - cy).atan2(a1x - cx)).to_degrees(),
            );
            format!("ANGULAR DIM  angle={a:.3}°")
        }
        EntityKind::RadialDim {
            center,
            edge,
            diameter,
            ..
        } => {
            let r = center.dist_f64(edge);
            if *diameter {
                format!("DIAMETER DIM  diameter={:.4}", 2.0 * r)
            } else {
                format!("RADIUS DIM  radius={r:.4}")
            }
        }
    };
    Some(format!("[{}] layer={}  {}", id.0, layer_name, geom))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw;
    use oxidraft_geometry::LineSeg;

    fn pt(x: i64, y: i64) -> Point2d {
        Point2d::from_i64(x, y)
    }

    #[test]
    fn distance_3_4_5() {
        let (dsq, d) = distance_points(&pt(0, 0), &pt(3, 4));
        assert!((dsq - 25.0).abs() < 1e-9);
        assert!((d - 5.0).abs() < 1e-9);
    }

    #[test]
    fn area_of_square() {
        let mut doc = Document::new();
        let ids = draw::rectangle(&mut doc, &pt(0, 0), &pt(4, 4));
        let a = area_of_loop(&doc, &ids);
        assert!((a - 16.0).abs() < 1e-6, "area={}", a);
    }

    #[test]
    fn perimeter_of_rectangle() {
        let mut doc = Document::new();
        let ids = draw::rectangle(&mut doc, &pt(0, 0), &pt(3, 2));
        let p = total_length(&doc, &ids);
        assert!((p - 10.0).abs() < 1e-9);
    }

    #[test]
    fn distance_between_parallel_lines() {
        let mut doc = Document::new();
        let a = draw::line(&mut doc, pt(0, 0), pt(10, 0));
        let b = draw::line(&mut doc, pt(0, 3), pt(10, 3));
        let d = distance_entities(&doc, a, b).unwrap();
        assert!((d - 3.0).abs() < 1e-6, "d={}", d);
    }

    #[test]
    fn list_describes_line() {
        let mut doc = Document::new();
        let id = doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(0, 0),
            pt(3, 4),
        ))));
        let s = list_entity(&doc, id).unwrap();
        assert!(s.contains("LINE"));
        assert!(s.contains("len=5"));
    }
}
