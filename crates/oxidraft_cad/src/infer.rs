#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideKind {
    Extension,
}

impl GuideKind {
    pub fn label(self) -> &'static str {
        match self {
            GuideKind::Extension => "Extension",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Guide {
    pub kind: GuideKind,
    pub origin: (f64, f64),
    pub dir: (f64, f64),
}

#[derive(Clone, Debug)]
pub struct InferResult {
    pub point: (f64, f64),
    pub guides: Vec<Guide>,
}

type P = (f64, f64);

fn norm((x, y): P) -> Option<P> {
    let l = (x * x + y * y).sqrt();
    (l > 1e-9).then(|| (x / l, y / l))
}

fn dist(a: P, b: P) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn line_dist(g: &Guide, p: P) -> f64 {
    ((p.0 - g.origin.0) * -g.dir.1 + (p.1 - g.origin.1) * g.dir.0).abs()
}

fn project_on(g: &Guide, p: P) -> P {
    let t = (p.0 - g.origin.0) * g.dir.0 + (p.1 - g.origin.1) * g.dir.1;
    (g.origin.0 + g.dir.0 * t, g.origin.1 + g.dir.1 * t)
}

pub fn infer_axis(p0: P, p1: P, cursor: P, tol: f64) -> Option<InferResult> {
    if tol <= 0.0 {
        return None;
    }
    let dir = norm((p1.0 - p0.0, p1.1 - p0.1))?;
    let origin = if dist(p0, cursor) >= dist(p1, cursor) {
        p0
    } else {
        p1
    };
    let g = Guide {
        kind: GuideKind::Extension,
        origin,
        dir,
    };
    if line_dist(&g, cursor) > tol {
        return None;
    }
    Some(InferResult {
        point: project_on(&g, cursor),
        guides: vec![g],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_lock_keeps_an_endpoint_colinear() {
        let res = infer_axis((0.0, 0.0), (10.0, 10.0), (6.2, 5.8), 0.5).expect("axis lock");
        assert!(
            (res.point.0 - res.point.1).abs() < 1e-9,
            "snaps onto y=x, got {:?}",
            res.point
        );
        assert_eq!(res.guides.len(), 1);
        assert_eq!(res.guides[0].kind, GuideKind::Extension);
    }

    #[test]
    fn axis_no_lock_when_far_off() {
        assert!(infer_axis((0.0, 0.0), (10.0, 0.0), (5.0, 4.0), 0.5).is_none());
    }
}
