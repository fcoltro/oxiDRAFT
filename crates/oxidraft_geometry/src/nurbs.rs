use crate::curve::{Curve, CurveSegment};
use crate::error::GeomError;
use crate::point::{BoundingBox, Point2d};

/// Shared validation for control-point/weight pairs of rational curves.
fn validate_rational(points: usize, weights: &[f64]) -> Result<(), GeomError> {
    if points != weights.len() {
        return Err(GeomError::LengthMismatch {
            points,
            weights: weights.len(),
        });
    }
    if points < 2 {
        return Err(GeomError::TooFewPoints {
            got: points,
            need: 2,
        });
    }
    if let Some(&w) = weights.iter().find(|&&w| w.is_nan() || w <= 0.0) {
        return Err(GeomError::NonPositiveWeight(w));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub struct RationalBezier {
    pub points: Vec<Point2d>,
    pub weights: Vec<f64>,
}

impl RationalBezier {
    /// Trusted-caller constructor; panics on invalid input. Use
    /// [`RationalBezier::try_new`] for untrusted data.
    pub fn new(points: Vec<Point2d>, weights: Vec<f64>) -> Self {
        Self::try_new(points, weights).expect("invalid rational Bézier")
    }

    /// Fallible constructor: returns a [`GeomError`] instead of panicking.
    pub fn try_new(points: Vec<Point2d>, weights: Vec<f64>) -> Result<Self, GeomError> {
        validate_rational(points.len(), &weights)?;
        Ok(RationalBezier { points, weights })
    }

    pub fn polynomial(points: Vec<Point2d>) -> Self {
        let weights = vec![1.0; points.len()];
        RationalBezier::new(points, weights)
    }

    pub fn degree(&self) -> usize {
        self.points.len() - 1
    }

    fn homogeneous(&self) -> Vec<[f64; 3]> {
        self.points
            .iter()
            .zip(&self.weights)
            .map(|(p, &w)| [w * p.x, w * p.y, w])
            .collect()
    }

    fn from_homogeneous(h: &[[f64; 3]]) -> RationalBezier {
        let points = h
            .iter()
            .map(|c| Point2d::new(c[0] / c[2], c[1] / c[2]))
            .collect();
        let weights = h.iter().map(|c| c[2]).collect();
        RationalBezier { points, weights }
    }

    pub fn evaluate(&self, t: f64) -> Point2d {
        let [x, y, w] = de_casteljau(&self.homogeneous(), t);
        Point2d::new(x / w, y / w)
    }

    pub fn tangent(&self, t: f64) -> (f64, f64) {
        let h = self.homogeneous();
        let [hx, hy, hw] = de_casteljau(&h, t);
        let n = self.degree() as f64;
        let d: Vec<[f64; 3]> = h
            .windows(2)
            .map(|w| {
                [
                    n * (w[1][0] - w[0][0]),
                    n * (w[1][1] - w[0][1]),
                    n * (w[1][2] - w[0][2]),
                ]
            })
            .collect();
        let [dx, dy, dw] = if d.is_empty() {
            [0.0, 0.0, 0.0]
        } else {
            de_casteljau(&d, t)
        };
        let inv = 1.0 / (hw * hw);
        ((dx * hw - hx * dw) * inv, (dy * hw - hy * dw) * inv)
    }

    pub fn split(&self, t: f64) -> (RationalBezier, RationalBezier) {
        let mut level = self.homogeneous();
        let mut left = vec![level[0]];
        let mut right = vec![*level.last().unwrap()];
        while level.len() > 1 {
            let next: Vec<[f64; 3]> = level
                .windows(2)
                .map(|w| {
                    [
                        (1.0 - t) * w[0][0] + t * w[1][0],
                        (1.0 - t) * w[0][1] + t * w[1][1],
                        (1.0 - t) * w[0][2] + t * w[1][2],
                    ]
                })
                .collect();
            left.push(next[0]);
            right.push(*next.last().unwrap());
            level = next;
        }
        right.reverse();
        (
            RationalBezier::from_homogeneous(&left),
            RationalBezier::from_homogeneous(&right),
        )
    }

    pub fn reverse(&self) -> RationalBezier {
        let mut points = self.points.clone();
        let mut weights = self.weights.clone();
        points.reverse();
        weights.reverse();
        RationalBezier { points, weights }
    }

    pub fn bounding_box(&self) -> BoundingBox {
        let mut xmin = f64::INFINITY;
        let mut xmax = f64::NEG_INFINITY;
        let mut ymin = f64::INFINITY;
        let mut ymax = f64::NEG_INFINITY;
        for p in &self.points {
            xmin = xmin.min(p.x);
            xmax = xmax.max(p.x);
            ymin = ymin.min(p.y);
            ymax = ymax.max(p.y);
        }
        BoundingBox::from_corners(xmin, ymin, xmax, ymax)
    }

    pub fn arc_length(&self) -> f64 {
        const NODES: [f64; 5] = [0.046910077, 0.230765346, 0.5, 0.769234654, 0.953089923];
        const WEIGHTS: [f64; 5] = [
            0.118463442,
            0.239314335,
            0.284444444,
            0.239314335,
            0.118463442,
        ];
        NODES.iter().zip(WEIGHTS.iter()).fold(0.0, |acc, (&t, &w)| {
            let (dx, dy) = self.tangent(t);
            acc + w * (dx * dx + dy * dy).sqrt()
        })
    }

    pub fn to_polyline(&self, tol: f64) -> Vec<Point2d> {
        let mut out = vec![self.evaluate(0.0)];
        self.flatten_into(0.0, 1.0, tol, 0, &mut out);
        out
    }

    fn flatten_into(&self, t0: f64, t1: f64, tol: f64, depth: u32, out: &mut Vec<Point2d>) {
        let a = self.evaluate(t0);
        let b = self.evaluate(t1);
        let tm = 0.5 * (t0 + t1);
        let m = self.evaluate(tm);
        let cmx = 0.5 * (a.x + b.x);
        let cmy = 0.5 * (a.y + b.y);
        let dev = ((m.x - cmx).powi(2) + (m.y - cmy).powi(2)).sqrt();
        if dev <= tol || depth >= 24 {
            out.push(b);
        } else {
            self.flatten_into(t0, tm, tol, depth + 1, out);
            self.flatten_into(tm, t1, tol, depth + 1, out);
        }
    }
}

impl CurveSegment for RationalBezier {
    fn domain(&self) -> (f64, f64) {
        (0.0, 1.0)
    }
    fn evaluate_f64(&self, t: f64) -> (f64, f64) {
        let p = self.evaluate(t);
        (p.x, p.y)
    }
    fn bounding_box(&self) -> BoundingBox {
        self.bounding_box()
    }
    fn tangent_f64(&self, t: f64) -> (f64, f64) {
        self.tangent(t)
    }
    fn arc_length(&self) -> f64 {
        self.arc_length()
    }
}

fn de_casteljau(control: &[[f64; 3]], t: f64) -> [f64; 3] {
    // Real curves here are low degree (lines=2, arcs=3, cubics=4), so collapse on a
    // stack buffer with no allocation; fall back to the heap only for exotic degrees.
    const STACK: usize = 8;
    let n = control.len();
    if n <= STACK {
        let mut buf = [[0.0; 3]; STACK];
        buf[..n].copy_from_slice(control);
        de_casteljau_inplace(&mut buf[..n], t)
    } else {
        let mut buf = control.to_vec();
        de_casteljau_inplace(&mut buf, t)
    }
}

fn de_casteljau_inplace(h: &mut [[f64; 3]], t: f64) -> [f64; 3] {
    let n = h.len();
    for r in 1..n {
        for i in 0..n - r {
            let (a, b) = (h[i], h[i + 1]);
            h[i] = [
                (1.0 - t) * a[0] + t * b[0],
                (1.0 - t) * a[1] + t * b[1],
                (1.0 - t) * a[2] + t * b[2],
            ];
        }
    }
    h[0]
}

pub fn lower(curve: &Curve) -> Vec<RationalBezier> {
    match curve {
        Curve::Line(l) => vec![RationalBezier::polynomial(vec![l.p0, l.p1])],
        Curve::Bezier(b) => vec![RationalBezier::polynomial(vec![b.p0, b.p1, b.p2, b.p3])],
        Curve::Arc(a) => {
            let (cx, cy, r) = (a.center.x, a.center.y, a.radius);
            unit_arc_segments(a.start_angle, a.end_angle)
                .into_iter()
                .map(|(cps, w)| {
                    let map = |p: [f64; 2]| Point2d::new(cx + r * p[0], cy + r * p[1]);
                    RationalBezier::new(
                        vec![map(cps[0]), map(cps[1]), map(cps[2])],
                        vec![1.0, w, 1.0],
                    )
                })
                .collect()
        }
        Curve::Ellipse(e) => {
            let (sin_phi, cos_phi) = e.rotation.sin_cos();
            let (cx, cy, sa, sb) = (e.center.x, e.center.y, e.semi_major, e.semi_minor);
            let map = |p: [f64; 2]| {
                let (u, v) = (sa * p[0], sb * p[1]);
                Point2d::new(
                    cx + u * cos_phi - v * sin_phi,
                    cy + u * sin_phi + v * cos_phi,
                )
            };
            unit_arc_segments(e.start_angle, e.end_angle)
                .into_iter()
                .map(|(cps, w)| {
                    RationalBezier::new(
                        vec![map(cps[0]), map(cps[1]), map(cps[2])],
                        vec![1.0, w, 1.0],
                    )
                })
                .collect()
        }
        Curve::Poly(pc) => pc.segments.iter().flat_map(lower).collect(),
        Curve::Rational(rb) => vec![rb.clone()],
        Curve::Nurbs(nc) => nc.segments(),
    }
}

pub fn tessellate_curve(curve: &Curve, tol: f64) -> Vec<Point2d> {
    // Circular arcs have a closed-form optimal flattening (uniform angle
    // steps sized by the exact sagitta bound), which produces the minimum
    // vertex count for the tolerance; recursive bisection can only land on
    // power-of-two counts, up to 2x more vertices (Levien & Uguray 2024 make
    // the same observation for GPU flattening).
    if let Curve::Arc(a) = curve {
        return flatten_arc_optimal(a, tol);
    }
    if let Curve::Poly(pc) = curve {
        let mut out: Vec<Point2d> = Vec::new();
        for (i, seg) in pc.segments.iter().enumerate() {
            let poly = tessellate_curve(seg, tol);
            if i == 0 {
                out.extend(poly);
            } else {
                out.extend(poly.into_iter().skip(1));
            }
        }
        return out;
    }
    let mut out: Vec<Point2d> = Vec::new();
    for (i, seg) in lower(curve).iter().enumerate() {
        let poly = seg.to_polyline(tol);
        if i == 0 {
            out.extend(poly);
        } else {
            out.extend(poly.into_iter().skip(1));
        }
    }
    out
}

/// Uniform-angle arc flattening at the exact sagitta limit: a chord spanning
/// `dθ` on radius `r` deviates by `r·(1 − cos(dθ/2))`, so the largest
/// tolerable step is `2·acos(1 − tol/r)`.
fn flatten_arc_optimal(a: &crate::primitives::CircularArc, tol: f64) -> Vec<Point2d> {
    let r = a.radius.abs();
    let sweep = a.end_angle - a.start_angle;
    let max_step = if r <= tol {
        std::f64::consts::PI
    } else {
        2.0 * (1.0 - tol / r).clamp(-1.0, 1.0).acos()
    };
    let n = if max_step <= 0.0 {
        65_536
    } else {
        ((sweep.abs() / max_step).ceil() as usize).clamp(1, 65_536)
    };
    let (cx, cy) = a.center.to_f64();
    (0..=n)
        .map(|i| {
            let ang = a.start_angle + sweep * i as f64 / n as f64;
            Point2d::from_f64(cx + r * ang.cos(), cy + r * ang.sin())
        })
        .collect()
}

fn unit_arc_segments(a0: f64, a1: f64) -> Vec<([[f64; 2]; 3], f64)> {
    let sweep = a1 - a0;
    let n = ((sweep.abs() / std::f64::consts::FRAC_PI_2).ceil() as usize).max(1);
    let step = sweep / n as f64;
    (0..n)
        .map(|i| {
            let b0 = a0 + step * i as f64;
            let b1 = b0 + step;
            let half = 0.5 * (b1 - b0);
            let mid = 0.5 * (b0 + b1);
            let w = half.cos();
            let p0 = [b0.cos(), b0.sin()];
            let p2 = [b1.cos(), b1.sin()];
            let p1 = [mid.cos() / w, mid.sin() / w];
            ([p0, p1, p2], w)
        })
        .collect()
}

pub struct NurbsCurve {
    pub control: Vec<Point2d>,
    pub weights: Vec<f64>,
    // Cached Bézier decomposition, keyed by a content hash of control/weights.
    // Evaluation used to re-run the full knot-insertion decomposition on every
    // call, which dominated tessellation and projection of splines. The hash
    // key makes the cache self-validating: code that mutates `control` or
    // `weights` in place (grip drags do) just triggers a recompute — a stale
    // decomposition is never served.
    seg_cache: std::sync::RwLock<Option<(u64, std::sync::Arc<Vec<RationalBezier>>)>>,
}

impl Clone for NurbsCurve {
    fn clone(&self) -> Self {
        // Carry the warm cache over (the Arc clone is cheap).
        let cache = lock_read(&self.seg_cache).clone();
        NurbsCurve {
            control: self.control.clone(),
            weights: self.weights.clone(),
            seg_cache: std::sync::RwLock::new(cache),
        }
    }
}

impl std::fmt::Debug for NurbsCurve {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NurbsCurve")
            .field("control", &self.control)
            .field("weights", &self.weights)
            .finish()
    }
}

impl PartialEq for NurbsCurve {
    fn eq(&self, other: &Self) -> bool {
        self.control == other.control && self.weights == other.weights
    }
}

fn lock_read<T>(l: &std::sync::RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    l.read().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl NurbsCurve {
    /// Trusted-caller constructor; panics on invalid input. Use
    /// [`NurbsCurve::try_new`] for untrusted data.
    pub fn new(control: Vec<Point2d>, weights: Vec<f64>) -> Self {
        Self::try_new(control, weights).expect("invalid NURBS curve")
    }

    /// Fallible constructor: returns a [`GeomError`] instead of panicking.
    pub fn try_new(control: Vec<Point2d>, weights: Vec<f64>) -> Result<Self, GeomError> {
        validate_rational(control.len(), &weights)?;
        Ok(NurbsCurve {
            control,
            weights,
            seg_cache: std::sync::RwLock::new(None),
        })
    }

    pub fn uniform(control: Vec<Point2d>) -> Self {
        let weights = vec![1.0; control.len()];
        NurbsCurve::new(control, weights)
    }

    pub fn segments(&self) -> Vec<RationalBezier> {
        self.segments_arc().as_ref().clone()
    }

    /// Cached Bézier decomposition. FNV over the raw f64 bits: a collision
    /// would serve a stale decomposition, the same accepted 2⁻⁶⁴ trade-off as
    /// the UI geometry caches.
    fn content_hash(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        let mut feed = |v: u64| {
            h ^= v;
            h = h.wrapping_mul(0x100000001b3);
        };
        for p in &self.control {
            feed(p.x.to_bits());
            feed(p.y.to_bits());
        }
        for w in &self.weights {
            feed(w.to_bits());
        }
        h
    }

    pub(crate) fn segments_arc(&self) -> std::sync::Arc<Vec<RationalBezier>> {
        let key = self.content_hash();
        if let Some((k, segs)) = lock_read(&self.seg_cache).as_ref()
            && *k == key
        {
            return segs.clone();
        }
        let segs = std::sync::Arc::new(cv_spline_segments_weighted(&self.control, &self.weights));
        *self
            .seg_cache
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((key, segs.clone()));
        segs
    }
}

impl CurveSegment for NurbsCurve {
    fn domain(&self) -> (f64, f64) {
        (0.0, 1.0)
    }
    fn evaluate_f64(&self, t: f64) -> (f64, f64) {
        let segs = self.segments_arc();
        if segs.is_empty() {
            return (0.0, 0.0);
        }
        let (i, lt) = seg_param(segs.len(), t);
        segs[i].evaluate_f64(lt)
    }
    fn tangent_f64(&self, t: f64) -> (f64, f64) {
        let segs = self.segments_arc();
        if segs.is_empty() {
            return (0.0, 0.0);
        }
        let (i, lt) = seg_param(segs.len(), t);
        segs[i].tangent_f64(lt)
    }
    fn bounding_box(&self) -> BoundingBox {
        let mut xmin = f64::INFINITY;
        let mut xmax = f64::NEG_INFINITY;
        let mut ymin = f64::INFINITY;
        let mut ymax = f64::NEG_INFINITY;
        for p in &self.control {
            xmin = xmin.min(p.x);
            xmax = xmax.max(p.x);
            ymin = ymin.min(p.y);
            ymax = ymax.max(p.y);
        }
        BoundingBox::from_corners(xmin, ymin, xmax, ymax)
    }
    fn arc_length(&self) -> f64 {
        self.segments_arc().iter().map(|s| s.arc_length()).sum()
    }
}
pub(crate) fn seg_param(n: usize, t: f64) -> (usize, f64) {
    let scaled = t.clamp(0.0, 1.0) * n as f64;
    let i = (scaled.floor() as usize).min(n - 1);
    (i, scaled - i as f64)
}

/// All `n` rational basis values `R_i(t)` of the curve family produced by
/// [`cv_spline_segments_weighted`]: rational Bernstein for `n ≤ 4` control
/// points, clamped uniform cubic B-spline otherwise. Evaluating the basis
/// directly replaces the old probe trick (building a `NurbsCurve` per basis
/// function and running the full Bézier decomposition to read off one basis
/// value), which made interpolation O(n³) with enormous constants.
pub(crate) fn rational_basis_all(n: usize, weights: &[f64], t: f64) -> Vec<f64> {
    let mut basis = bspline_basis_all(n, t);
    let denom: f64 = basis
        .iter()
        .zip(weights)
        .map(|(b, w)| b * w)
        .sum::<f64>()
        .max(1e-300);
    for (b, w) in basis.iter_mut().zip(weights) {
        *b = *b * w / denom;
    }
    basis
}

fn bspline_basis_all(n: usize, t: f64) -> Vec<f64> {
    let t = t.clamp(0.0, 1.0);
    if n <= 4 {
        // Bernstein basis of degree n-1.
        let d = n - 1;
        let u = 1.0 - t;
        return match d {
            0 => vec![1.0],
            1 => vec![u, t],
            2 => vec![u * u, 2.0 * u * t, t * t],
            _ => vec![u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t],
        };
    }
    // Clamped cubic B-spline over knots [0,0,0,0, 1, 2, …, n-4, m,m,m,m] with
    // m = n-3 — the same layout `clamped_cubic_bspline_homog` decomposes, so
    // the parameterization matches `NurbsCurve::evaluate_f64` exactly.
    const P: usize = 3;
    let interior = n - 4;
    let m = (interior + 1) as f64;
    let u = t * m;
    let knot = |i: usize| -> f64 {
        if i < P + 1 {
            0.0
        } else if i < n {
            (i - P) as f64
        } else {
            m
        }
    };
    let span = (u.floor() as usize).min(interior) + P;
    // Cox–de Boor (The NURBS Book, algorithm A2.2): the P+1 basis functions
    // that are non-zero on this span.
    let mut vals = [1.0f64, 0.0, 0.0, 0.0];
    let mut left = [0.0f64; P + 1];
    let mut right = [0.0f64; P + 1];
    for j in 1..=P {
        left[j] = u - knot(span + 1 - j);
        right[j] = knot(span + j) - u;
        let mut saved = 0.0;
        for r in 0..j {
            let temp = vals[r] / (right[r + 1] + left[j - r]);
            vals[r] = saved + right[r + 1] * temp;
            saved = left[j - r] * temp;
        }
        vals[j] = saved;
    }
    let mut out = vec![0.0; n];
    for (r, &v) in vals.iter().enumerate() {
        out[span - P + r] = v;
    }
    out
}

pub fn cv_spline_segments(cvs: &[Point2d]) -> Vec<RationalBezier> {
    cv_spline_segments_weighted(cvs, &vec![1.0; cvs.len()])
}

pub fn cv_spline_segments_weighted(cvs: &[Point2d], weights: &[f64]) -> Vec<RationalBezier> {
    match cvs.len() {
        0 | 1 => vec![],
        2..=4 => vec![RationalBezier::new(cvs.to_vec(), weights.to_vec())],
        _ => {
            let h: Vec<[f64; 3]> = cvs
                .iter()
                .zip(weights)
                .map(|(p, &w)| [w * p.x, w * p.y, w])
                .collect();
            clamped_cubic_bspline_homog(&h)
        }
    }
}

fn clamped_cubic_bspline_homog(h: &[[f64; 3]]) -> Vec<RationalBezier> {
    const P: usize = 3;
    let n = h.len() - 1;
    let interior = n - P;

    let mut knots: Vec<f64> = vec![0.0; P + 1];
    for i in 1..=interior {
        knots.push(i as f64);
    }
    knots.extend(std::iter::repeat_n((interior + 1) as f64, P + 1));

    let mut pts = h.to_vec();
    for k in 1..=interior {
        let val = k as f64;
        let mult = knots.iter().filter(|&&x| (x - val).abs() < 1e-9).count();
        for _ in mult..P {
            knot_insert_homog(&mut knots, &mut pts, val, P);
        }
    }

    (0..=interior)
        .map(|s| {
            let b = s * P;
            RationalBezier::from_homogeneous(&pts[b..b + 4])
        })
        .collect()
}

fn knot_insert_homog(knots: &mut Vec<f64>, pts: &mut Vec<[f64; 3]>, val: f64, p: usize) {
    let mut k = p;
    while k + 1 < knots.len() && !(knots[k] <= val && val < knots[k + 1]) {
        k += 1;
    }

    let mut out: Vec<[f64; 3]> = Vec::with_capacity(pts.len() + 1);
    out.extend_from_slice(&pts[..=k - p]);
    for i in (k - p + 1)..=k {
        let denom = knots[i + p] - knots[i];
        let a = if denom.abs() < 1e-12 {
            0.0
        } else {
            (val - knots[i]) / denom
        };
        let q = pts[i - 1];
        let r = pts[i];
        out.push([
            (1.0 - a) * q[0] + a * r[0],
            (1.0 - a) * q[1] + a * r[1],
            (1.0 - a) * q[2] + a * r[2],
        ]);
    }
    out.extend_from_slice(&pts[k..]);
    *pts = out;
    knots.insert(k + 1, val);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::CurveSegment;
    use crate::primitives::{CircularArc, CubicBezier, EllipticalArc, LineSeg};

    fn pt(x: f64, y: f64) -> Point2d {
        Point2d::from_f64(x, y)
    }

    #[test]
    fn line_lowers_to_degree_1() {
        let l = Curve::Line(LineSeg::from_endpoints(pt(1.0, 2.0), pt(5.0, 8.0)));
        let segs = lower(&l);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].degree(), 1);
        let m = segs[0].evaluate(0.25);
        assert!((m.x - 2.0).abs() < 1e-12 && (m.y - 3.5).abs() < 1e-12);
    }

    #[test]
    fn cubic_lowers_and_matches_evaluation() {
        let b = CubicBezier::new(pt(0.0, 0.0), pt(1.0, 3.0), pt(3.0, 3.0), pt(4.0, 0.0));
        let c = Curve::Bezier(b.clone());
        let segs = lower(&c);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].degree(), 3);
        for i in 0..=10 {
            let t = i as f64 / 10.0;
            let (ex, ey) = b.evaluate_f64(t);
            let m = segs[0].evaluate(t);
            assert!(
                (m.x - ex).abs() < 1e-12 && (m.y - ey).abs() < 1e-12,
                "t={}",
                t
            );
        }
    }

    #[test]
    fn arc_lowers_to_exact_circle() {
        let a = CircularArc::new(pt(3.0, 4.0), 5.0, 0.0, 1.5 * std::f64::consts::PI);
        let segs = lower(&Curve::Arc(a));
        assert_eq!(
            segs.len(),
            3,
            "270° splits into three ≤90° rational quadratics"
        );
        for seg in &segs {
            assert_eq!(seg.degree(), 2);
            for i in 0..=16 {
                let p = seg.evaluate(i as f64 / 16.0);
                let d = ((p.x - 3.0).powi(2) + (p.y - 4.0).powi(2)).sqrt();
                assert!((d - 5.0).abs() < 1e-9, "off circle: d={}", d);
            }
        }
        let start = segs.first().unwrap().evaluate(0.0);
        let end = segs.last().unwrap().evaluate(1.0);
        assert!((start.x - 8.0).abs() < 1e-9 && (start.y - 4.0).abs() < 1e-9);
        assert!((end.x - 3.0).abs() < 1e-9 && (end.y - (4.0 - 5.0)).abs() < 1e-9);
    }

    #[test]
    fn quarter_arc_is_single_segment() {
        let a = CircularArc::new(pt(0.0, 0.0), 1.0, 0.0, std::f64::consts::FRAC_PI_2);
        let segs = lower(&Curve::Arc(a));
        assert_eq!(segs.len(), 1);
        let m = segs[0].evaluate(0.5);
        let inv = 1.0 / 2f64.sqrt();
        assert!(
            (m.x - inv).abs() < 1e-12 && (m.y - inv).abs() < 1e-12,
            "got {:?}",
            m
        );
    }

    #[test]
    fn ellipse_lowers_to_exact_ellipse() {
        let e = EllipticalArc::axis_aligned(pt(0.0, 0.0), 3.0, 2.0, 0.0, std::f64::consts::TAU);
        let segs = lower(&Curve::Ellipse(e));
        assert_eq!(segs.len(), 4);
        for seg in &segs {
            for i in 0..=16 {
                let p = seg.evaluate(i as f64 / 16.0);
                let f = (p.x / 3.0).powi(2) + (p.y / 2.0).powi(2);
                assert!((f - 1.0).abs() < 1e-9, "off ellipse: f={}", f);
            }
        }
    }

    #[test]
    fn rotated_ellipse_lowers_exactly() {
        let phi = 0.5;
        let (a, b) = (4.0_f64, 1.5_f64);
        let e = EllipticalArc::new(pt(1.0, -2.0), a, b, phi, 0.0, std::f64::consts::TAU);
        let segs = lower(&Curve::Ellipse(e));
        let (sin, cos) = phi.sin_cos();
        for seg in &segs {
            for i in 0..=12 {
                let p = seg.evaluate(i as f64 / 12.0);
                let (dx, dy) = (p.x - 1.0, p.y + 2.0);
                let u = dx * cos + dy * sin;
                let v = -dx * sin + dy * cos;
                let f = (u / a).powi(2) + (v / b).powi(2);
                assert!((f - 1.0).abs() < 1e-9, "off rotated ellipse: f={}", f);
            }
        }
    }

    #[test]
    fn split_reconstructs_curve() {
        let a = CircularArc::new(pt(0.0, 0.0), 2.0, 0.0, std::f64::consts::FRAC_PI_2);
        let seg = lower(&Curve::Arc(a)).remove(0);
        let (left, right) = seg.split(0.5);
        let j0 = left.evaluate(1.0);
        let j1 = right.evaluate(0.0);
        assert!((j0.x - j1.x).abs() < 1e-12 && (j0.y - j1.y).abs() < 1e-12);
        for (c, s) in [(&left, 0.3), (&right, 0.7)] {
            let p = c.evaluate(s);
            let d = (p.x * p.x + p.y * p.y).sqrt();
            assert!(
                (d - 2.0).abs() < 1e-9,
                "split half left the circle: d={}",
                d
            );
        }
    }

    #[test]
    fn tangent_matches_finite_difference() {
        let b = CubicBezier::new(pt(0.0, 0.0), pt(1.0, 3.0), pt(3.0, -1.0), pt(4.0, 0.0));
        let seg = lower(&Curve::Bezier(b)).remove(0);
        let h = 1e-6;
        for &t in &[0.2, 0.5, 0.8] {
            let (tx, ty) = seg.tangent(t);
            let a = seg.evaluate(t - h);
            let c = seg.evaluate(t + h);
            let (fx, fy) = ((c.x - a.x) / (2.0 * h), (c.y - a.y) / (2.0 * h));
            assert!((tx - fx).abs() < 1e-4 && (ty - fy).abs() < 1e-4, "t={}", t);
        }
    }

    #[test]
    fn cv_spline_four_points_is_single_cubic() {
        let cvs = vec![pt(0.0, 0.0), pt(1.0, 2.0), pt(3.0, 2.0), pt(4.0, 0.0)];
        let segs = cv_spline_segments(&cvs);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].degree(), 3);
        assert_eq!(segs[0].points, cvs, "4 CVs = the cubic Bézier through them");
    }

    #[test]
    fn cv_spline_clamped_cubic_bspline_properties() {
        let cvs = vec![
            pt(0.0, 0.0),
            pt(1.0, 3.0),
            pt(3.0, 3.0),
            pt(5.0, -1.0),
            pt(7.0, 2.0),
            pt(9.0, 0.0),
        ];
        let segs = cv_spline_segments(&cvs);
        assert_eq!(segs.len(), cvs.len() - 3, "6 CVs → 3 cubic spans");
        for s in &segs {
            assert_eq!(s.degree(), 3);
        }

        let start = segs.first().unwrap().evaluate(0.0);
        let end = segs.last().unwrap().evaluate(1.0);
        assert!(
            (start.x - 0.0).abs() < 1e-9 && (start.y - 0.0).abs() < 1e-9,
            "start {start:?}"
        );
        assert!(
            (end.x - 9.0).abs() < 1e-9 && (end.y - 0.0).abs() < 1e-9,
            "end {end:?}"
        );

        for w in segs.windows(2) {
            let a = w[0].evaluate(1.0);
            let b = w[1].evaluate(0.0);
            assert!(
                (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9,
                "join gap"
            );
            let (t0x, t0y) = w[0].tangent(1.0);
            let (t1x, t1y) = w[1].tangent(0.0);
            let cross = t0x * t1y - t0y * t1x;
            let dot = t0x * t1x + t0y * t1y;
            let mag = t0x.hypot(t0y) * t1x.hypot(t1y);
            assert!(
                cross.abs() < 1e-6 * mag.max(1.0) && dot > 0.0,
                "G1 break at joint"
            );
        }

        let (mut xmn, mut xmx, mut ymn, mut ymx) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for c in &cvs {
            xmn = xmn.min(c.x);
            xmx = xmx.max(c.x);
            ymn = ymn.min(c.y);
            ymx = ymx.max(c.y);
        }
        for s in &segs {
            for i in 0..=10 {
                let q = s.evaluate(i as f64 / 10.0);
                assert!(
                    q.x >= xmn - 1e-9
                        && q.x <= xmx + 1e-9
                        && q.y >= ymn - 1e-9
                        && q.y <= ymx + 1e-9,
                    "sample {q:?} outside control hull"
                );
            }
        }
    }

    #[test]
    fn nurbs_curve_clamped_with_uniform_weights() {
        let cvs = vec![
            pt(0.0, 0.0),
            pt(2.0, 4.0),
            pt(6.0, 4.0),
            pt(8.0, 0.0),
            pt(10.0, 4.0),
        ];
        let nc = NurbsCurve::uniform(cvs.clone());
        let s = nc.evaluate_f64(0.0);
        let e = nc.evaluate_f64(1.0);
        assert!(
            (s.0 - 0.0).abs() < 1e-9 && (s.1 - 0.0).abs() < 1e-9,
            "start {s:?}"
        );
        assert!(
            (e.0 - 10.0).abs() < 1e-9 && (e.1 - 4.0).abs() < 1e-9,
            "end {e:?}"
        );
        let bb = nc.bounding_box();
        assert!((bb.min.x - 0.0).abs() < 1e-9 && (bb.max.x - 10.0).abs() < 1e-9);
        assert_eq!(nc.segments(), cv_spline_segments(&cvs));
        assert!(nc.arc_length() > 0.0);
    }

    #[test]
    fn nurbs_weight_pulls_curve_toward_vertex() {
        let cvs = vec![
            pt(0.0, 0.0),
            pt(2.0, 4.0),
            pt(6.0, 4.0),
            pt(8.0, 0.0),
            pt(10.0, 4.0),
        ];
        let target = (6.0, 4.0);
        let min_dist = |nc: &NurbsCurve| {
            (0..=40)
                .map(|i| {
                    let p = nc.evaluate_f64(i as f64 / 40.0);
                    ((p.0 - target.0).powi(2) + (p.1 - target.1).powi(2)).sqrt()
                })
                .fold(f64::MAX, f64::min)
        };

        let uniform = NurbsCurve::uniform(cvs.clone());
        let mut w = vec![1.0; cvs.len()];
        w[2] = 8.0;
        let heavy = NurbsCurve::new(cvs.clone(), w);
        assert!(
            min_dist(&heavy) < min_dist(&uniform),
            "raising weight[2] should pull the curve closer to cvs[2]"
        );
    }

    #[test]
    fn rational_is_a_first_class_curve() {
        let arc = CircularArc::new(pt(0.0, 0.0), 2.0, 0.0, std::f64::consts::FRAC_PI_2);
        let c = Curve::Rational(lower(&Curve::Arc(arc)).remove(0));

        let (x0, y0) = c.evaluate_f64(0.0);
        let (x1, y1) = c.evaluate_f64(1.0);
        assert!((x0 - 2.0).abs() < 1e-9 && y0.abs() < 1e-9);
        assert!(x1.abs() < 1e-9 && (y1 - 2.0).abs() < 1e-9);
        assert!(c.arc_length() > 0.0);

        assert_eq!(lower(&c).len(), 1);

        let (l, r) = crate::split_curve(&c, 0.5);
        for half in [&l, &r] {
            let (x, y) = half.evaluate_f64(0.5);
            assert!(
                (x.hypot(y) - 2.0).abs() < 1e-9,
                "split half left the circle"
            );
        }

        let (rx, ry) = crate::reverse_curve(&c).evaluate_f64(0.0);
        assert!(rx.abs() < 1e-9 && (ry - 2.0).abs() < 1e-9);

        let moved = crate::Transform2d::translation(10.0, 0.0).apply_curve(&c);
        assert!(matches!(moved, Curve::Rational(_)));
        let (mx, my) = moved.evaluate_f64(0.0);
        assert!((mx - 12.0).abs() < 1e-9 && my.abs() < 1e-9);
    }

    #[test]
    fn rational_basis_matches_probe_evaluation() {
        // The old interpolate_nurbs built each matrix column with a "probe"
        // curve (control x_i = 1, rest 0) whose evaluation reads off the
        // rational basis value. The direct basis evaluation must agree.
        for n in 2..=9usize {
            let weights: Vec<f64> = (0..n).map(|i| 0.5 + 0.35 * i as f64).collect();
            for k in 0..=12 {
                let t = k as f64 / 12.0;
                let basis = rational_basis_all(n, &weights, t);
                let sum: f64 = basis.iter().sum();
                assert!(
                    (sum - 1.0).abs() < 1e-9,
                    "n={n} t={t}: rational basis must sum to 1, got {sum}"
                );
                for i in 0..n {
                    let mut ctrl = vec![pt(0.0, 0.0); n];
                    ctrl[i] = pt(1.0, 0.0);
                    let probe = NurbsCurve::new(ctrl, weights.clone());
                    let want = probe.evaluate_f64(t).0;
                    assert!(
                        (basis[i] - want).abs() < 1e-9,
                        "n={n} i={i} t={t}: basis {} vs probe {want}",
                        basis[i]
                    );
                }
            }
        }
    }

    #[test]
    fn tessellate_circle_stays_on_circle() {
        let a = CircularArc::new(pt(0.0, 0.0), 10.0, 0.0, std::f64::consts::TAU);
        let poly = tessellate_curve(&Curve::Arc(a), 0.05);
        assert!(
            poly.len() > 8,
            "expected a refined polyline, got {}",
            poly.len()
        );
        for p in &poly {
            let d = (p.x * p.x + p.y * p.y).sqrt();
            assert!(
                (d - 10.0).abs() < 1e-9,
                "tessellation vertex off circle: {}",
                d
            );
        }
    }

    #[test]
    fn arc_flattening_is_optimal_and_within_tolerance() {
        let r = 5.0;
        let tol = 0.01;
        let a = CircularArc::new(pt(1.0, 2.0), r, 0.0, std::f64::consts::TAU);
        let poly = tessellate_curve(&Curve::Arc(a), tol);

        // The closed-form segment count: ceil(2π / (2·acos(1 − tol/r))).
        let optimal = (std::f64::consts::TAU / (2.0 * (1.0 - tol / r).acos())).ceil() as usize;
        assert_eq!(poly.len(), optimal + 1, "must hit the sagitta minimum");

        // Every chord midpoint stays within the sagitta tolerance.
        for w in poly.windows(2) {
            let mx = 0.5 * (w[0].x + w[1].x) - 1.0;
            let my = 0.5 * (w[0].y + w[1].y) - 2.0;
            let sag = r - (mx * mx + my * my).sqrt();
            assert!(sag <= tol * 1.0001, "chord sagitta {sag} exceeds {tol}");
        }

        // Endpoints are exact.
        let first = poly.first().unwrap();
        let last = poly.last().unwrap();
        assert!((first.x - 6.0).abs() < 1e-12 && (first.y - 2.0).abs() < 1e-12);
        assert!((last.x - 6.0).abs() < 1e-9 && (last.y - 2.0).abs() < 1e-9);
    }

    #[test]
    fn reversed_arc_flattens_in_curve_direction() {
        // reverse_curve produces arcs with end < start; flattening must walk
        // the sweep in the arc's own direction.
        let a = CircularArc::new(pt(0.0, 0.0), 2.0, std::f64::consts::FRAC_PI_2, 0.0);
        let poly = tessellate_curve(&Curve::Arc(a), 0.01);
        let first = poly.first().unwrap();
        let last = poly.last().unwrap();
        assert!(
            first.x.abs() < 1e-12 && (first.y - 2.0).abs() < 1e-12,
            "starts at the π/2 end"
        );
        assert!(
            (last.x - 2.0).abs() < 1e-9 && last.y.abs() < 1e-9,
            "ends at the 0 end"
        );
    }
}
