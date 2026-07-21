//! Inference guides shown while drawing: alignment lines the cursor snaps to,
//! such as the extension of the segment being drawn.

/// The kind of inference guide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideKind {
    /// The extension line of a segment beyond its endpoint.
    Extension,
}

impl GuideKind {
    /// A short human-readable name for the guide.
    pub fn label(self) -> &'static str {
        match self {
            GuideKind::Extension => "Extension",
        }
    }
}

/// An inference guide line: an infinite alignment line the cursor can snap to.
#[derive(Clone, Copy, Debug)]
pub struct Guide {
    /// What kind of guide this is.
    pub kind: GuideKind,
    /// A point the guide line passes through.
    pub origin: (f64, f64),
    /// The guide's unit direction.
    pub dir: (f64, f64),
}

/// The outcome of inference: the (possibly snapped) point and the guides that
/// influenced it.
#[derive(Clone, Debug)]
pub struct InferResult {
    /// The resolved point after snapping to any guide.
    pub point: (f64, f64),
    /// The active guide lines.
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

/// Given a segment `p0→p1` and the current `cursor`, returns a snapped point
/// and the alignment guide when the cursor is within `tol` of the segment's
/// extension line, else `None`.
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
