use oxidraft_document::{DimStyle, EntityKind, Units};
use oxidraft_geometry::Point2d;

pub(crate) struct DimText {
    pub content: String,
    pub anchor: Point2d,
    pub height: f64,
    pub rotation_deg: f64,
}

pub(crate) struct DimPrimitives {
    pub segs: Vec<(Point2d, Point2d)>,
    pub text: Option<DimText>,
}

pub(crate) fn dimension_primitives(
    kind: &EntityKind,
    style: &DimStyle,
    units: Units,
) -> Option<DimPrimitives> {
    let label = oxidraft_document::label_text(kind, style, units)?;
    match kind {
        EntityKind::Dimension { p1, p2, line, .. } => Some(linear(
            p1.to_f64(),
            p2.to_f64(),
            line.to_f64(),
            None,
            style,
            label,
        )),
        EntityKind::OrthoDim {
            p1,
            p2,
            line,
            vertical,
            ..
        } => Some(linear(
            p1.to_f64(),
            p2.to_f64(),
            line.to_f64(),
            Some(*vertical),
            style,
            label,
        )),
        EntityKind::AngularDim {
            center,
            p1,
            p2,
            line,
            ..
        } => Some(angular(
            center.to_f64(),
            p1.to_f64(),
            p2.to_f64(),
            line.to_f64(),
            style,
            label,
        )),
        EntityKind::RadialDim {
            center,
            edge,
            diameter,
            ..
        } => Some(radial(
            center.to_f64(),
            edge.to_f64(),
            *diameter,
            style,
            label,
        )),
        _ => None,
    }
}

type P = (f64, f64);

fn pt(p: P) -> Point2d {
    Point2d::from_f64(p.0, p.1)
}

fn arrow(tip: P, from: P, size: f64, out: &mut Vec<(Point2d, Point2d)>) {
    let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let back = (tip.0 - ux * size, tip.1 - uy * size);
    let (px, py) = (-uy * size * 0.35, ux * size * 0.35);
    out.push((pt(tip), pt((back.0 + px, back.1 + py))));
    out.push((pt(tip), pt((back.0 - px, back.1 - py))));
}

fn linear(
    p1: P,
    p2: P,
    line: P,
    ortho: Option<bool>,
    style: &DimStyle,
    label: String,
) -> DimPrimitives {
    let mut segs = Vec::new();
    let (d1, d2, dir): (P, P, P) = match ortho {
        Some(true) => ((line.0, p1.1), (line.0, p2.1), (0.0, 1.0)),
        Some(false) => ((p1.0, line.1), (p2.0, line.1), (1.0, 0.0)),
        None => {
            let (dx, dy) = (p2.0 - p1.0, p2.1 - p1.1);
            let len = (dx * dx + dy * dy).sqrt().max(1e-12);
            let u = (dx / len, dy / len);
            let t1 = (p1.0 - line.0) * u.0 + (p1.1 - line.1) * u.1;
            let t2 = (p2.0 - line.0) * u.0 + (p2.1 - line.1) * u.1;
            (
                (line.0 + t1 * u.0, line.1 + t1 * u.1),
                (line.0 + t2 * u.0, line.1 + t2 * u.1),
                u,
            )
        }
    };
    segs.push((pt(p1), pt(d1)));
    segs.push((pt(p2), pt(d2)));
    segs.push((pt(d1), pt(d2)));
    let asz = style.arrow_size.max(1e-6);
    arrow(d1, d2, asz, &mut segs);
    arrow(d2, d1, asz, &mut segs);

    let mut rot = dir.1.atan2(dir.0);
    use std::f64::consts::FRAC_PI_2;
    if !(-FRAC_PI_2..=FRAC_PI_2).contains(&rot) {
        rot += std::f64::consts::PI;
    }
    let mid = ((d1.0 + d2.0) * 0.5, (d1.1 + d2.1) * 0.5);
    let text = DimText {
        content: label,
        anchor: pt(mid),
        height: style.text_height,
        rotation_deg: rot.to_degrees(),
    };
    DimPrimitives {
        segs,
        text: Some(text),
    }
}

fn angular(center: P, p1: P, p2: P, line: P, style: &DimStyle, label: String) -> DimPrimitives {
    let (cx, cy) = center;
    let sw = oxidraft_document::angular_sweep(pt(center), pt(p1), pt(p2), pt(line));
    let (start, sweep, r) = (sw.start, sw.sweep, sw.radius);

    let mut segs = Vec::new();
    let arc_pt = |ang: f64| (cx + r * ang.cos(), cy + r * ang.sin());
    let e1 = arc_pt(start);
    let e2 = arc_pt(start + sweep);
    segs.push((pt(p1), pt(e1)));
    segs.push((pt(p2), pt(e2)));
    let steps = 32.max((sweep.abs() / 0.1) as usize).min(256);
    let mut prev = e1;
    for i in 1..=steps {
        let a = start + sweep * (i as f64 / steps as f64);
        let cur = arc_pt(a);
        segs.push((pt(prev), pt(cur)));
        prev = cur;
    }
    let asz = style.arrow_size.max(1e-6);
    let near_start = arc_pt(start + sweep.signum() * 0.05);
    let near_end = arc_pt(start + sweep - sweep.signum() * 0.05);
    arrow(e1, near_start, asz, &mut segs);
    arrow(e2, near_end, asz, &mut segs);

    let mid_a = start + sweep * 0.5;
    let label_pt = (
        cx + (r + style.text_height) * mid_a.cos(),
        cy + (r + style.text_height) * mid_a.sin(),
    );
    let text = DimText {
        content: label,
        anchor: pt(label_pt),
        height: style.text_height,
        rotation_deg: 0.0,
    };
    DimPrimitives {
        segs,
        text: Some(text),
    }
}

fn radial(center: P, edge: P, diameter: bool, style: &DimStyle, label: String) -> DimPrimitives {
    let (cx, cy) = center;
    let (ex, ey) = edge;
    let r = ((ex - cx).powi(2) + (ey - cy).powi(2)).sqrt();
    let mut segs = Vec::new();
    if r < 1e-9 {
        return DimPrimitives { segs, text: None };
    }
    let (ux, uy) = ((ex - cx) / r, (ey - cy) / r);
    let near = if diameter {
        (cx - ux * r, cy - uy * r)
    } else {
        center
    };
    segs.push((pt(near), pt(edge)));
    let asz = style.arrow_size.max(1e-6);
    arrow(edge, near, asz, &mut segs);
    if diameter {
        arrow(near, edge, asz, &mut segs);
    }
    let label_pt = (
        ex + ux * style.text_height * 0.4,
        ey + uy * style.text_height * 0.4,
    );
    let text = DimText {
        content: label,
        anchor: pt(label_pt),
        height: style.text_height,
        rotation_deg: 0.0,
    };
    DimPrimitives {
        segs,
        text: Some(text),
    }
}
