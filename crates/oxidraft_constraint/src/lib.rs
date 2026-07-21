//! Numeric geometric constraint solving for 2D sketches.
//!
//! A sketch is a set of point variables plus residual-based constraints
//! (coincident, horizontal, distance, parallel, …). Solving minimises the
//! stacked residual vector with damped Gauss–Newton (Levenberg–Marquardt),
//! which converges quadratically near a solution and — because the damping
//! term regularises a rank-deficient normal matrix — handles the
//! under-constrained sketches real users draw: unconstrained degrees of
//! freedom simply stay near where the user put them.
//!
//! This is the numeric core that constraint-based CAD builds on; a
//! graph-constructive decomposition layer (splitting the sketch into
//! minimally-sized solvable subsystems) can be added above it later without
//! changing the constraint vocabulary.
//!
//! Beyond solving, [`Sketch::analyze`] and [`Sketch::diagnose_conflict`]
//! answer the questions a trustworthy CAD solver must: how many degrees of
//! freedom remain, which constraints are redundant, and — when a solve
//! fails — which constraint(s) are the likely conflict.

mod dual;
use dual::Dual;

/// Handle to one 2D point variable in a [`Sketch`]. Stores the base index
/// of its x coordinate in the flat variable vector (y follows at +1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointVar(usize);

/// Handle to one scalar variable (a radius, typically) in a [`Sketch`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScalarVar(usize);

#[derive(Clone, Debug)]
pub enum Constraint {
    /// The two points coincide.
    Coincident(PointVar, PointVar),
    /// The midpoint of segment a0→a1 coincides with the midpoint of segment
    /// b0→b1. Passing the same variable twice on a side degenerates that
    /// side's "midpoint" to the point itself, so one variant covers welding
    /// a point to a midpoint, an endpoint to a midpoint, and two midpoints.
    MidpointsCoincident(PointVar, PointVar, PointVar, PointVar),
    /// The segment a→b is horizontal.
    Horizontal(PointVar, PointVar),
    /// The segment a→b is vertical.
    Vertical(PointVar, PointVar),
    /// The distance |ab| equals the given value.
    Distance(PointVar, PointVar, f64),
    /// The horizontal separation |bx − ax| equals the given value
    /// (unsigned, so the points may sit in either order).
    HorizontalDistance(PointVar, PointVar, f64),
    /// The vertical separation |by − ay| equals the given value (unsigned).
    VerticalDistance(PointVar, PointVar, f64),
    /// Segment a→b is parallel to segment c→d.
    Parallel(PointVar, PointVar, PointVar, PointVar),
    /// Segment a→b is perpendicular to segment c→d.
    Perpendicular(PointVar, PointVar, PointVar, PointVar),
    /// Segment a→b meets segment c→d at the given angle (radians). Lines
    /// are undirected, so the residual is zero at θ and θ+π alike;
    /// Parallel (θ=0) and Perpendicular (θ=π/2) are its special cases.
    Angle(PointVar, PointVar, PointVar, PointVar, f64),
    /// The point is pinned to fixed coordinates.
    Fixed(PointVar, f64, f64),
    /// Point p lies on the infinite line through a and b.
    PointOnLine(PointVar, PointVar, PointVar),
    /// The midpoint of segment p1→p2 lies on the infinite line through a
    /// and b. With Perpendicular(p1, p2, a, b) this makes p1 and p2
    /// symmetric about the line.
    MidpointOnLine(PointVar, PointVar, PointVar, PointVar),
    /// The perpendicular distance from point p to the infinite line through
    /// a and b equals the given value (unsigned — the point may sit on
    /// either side). Two of these on a segment's endpoints hold two lines
    /// parallel at a driving width.
    PointLineDistance(PointVar, PointVar, PointVar, f64),
    /// |ab| equals |cd|.
    EqualLength(PointVar, PointVar, PointVar, PointVar),
    /// The scalar is pinned to a fixed value.
    FixedScalar(ScalarVar, f64),
    /// The two scalars are equal (equal radii, typically).
    EqualScalar(ScalarVar, ScalarVar),
    /// Point p lies on the circle with the given center and radius.
    PointOnCircle(PointVar, PointVar, ScalarVar),
    /// The infinite line through a→b is tangent to the circle with the
    /// given center and radius (unsigned distance center→line equals r).
    TangentLineCircle(PointVar, PointVar, PointVar, ScalarVar),
    /// Two circles are tangent. `internal: false` keeps them touching from
    /// outside (center distance = r1 + r2); `internal: true` nests one in
    /// the other (center distance = |r1 − r2|).
    TangentCircleCircle {
        c1: PointVar,
        r1: ScalarVar,
        c2: PointVar,
        r2: ScalarVar,
        internal: bool,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct SolveResult {
    pub converged: bool,
    /// Infinity norm of the residual vector at exit.
    pub residual: f64,
    pub iterations: u32,
}

#[derive(Clone, Default)]
pub struct Sketch {
    vars: Vec<f64>,
    constraints: Vec<Constraint>,
    /// Base indices of point variables (x at base, y at base+1) and indices
    /// of scalar variables — kept so [`Sketch::feature_scale`] can measure
    /// the sketch's coordinate *spread* per axis rather than its distance
    /// from the origin.
    point_bases: Vec<usize>,
    scalar_idx: Vec<usize>,
}

impl Sketch {
    pub fn new() -> Self {
        Sketch::default()
    }

    pub fn add_point(&mut self, x: f64, y: f64) -> PointVar {
        let idx = self.vars.len();
        self.vars.push(x);
        self.vars.push(y);
        self.point_bases.push(idx);
        PointVar(idx)
    }

    pub fn point(&self, p: PointVar) -> (f64, f64) {
        (self.vars[p.0], self.vars[p.0 + 1])
    }

    pub fn set_point(&mut self, p: PointVar, x: f64, y: f64) {
        self.vars[p.0] = x;
        self.vars[p.0 + 1] = y;
    }

    pub fn add_scalar(&mut self, v: f64) -> ScalarVar {
        let idx = self.vars.len();
        self.vars.push(v);
        self.scalar_idx.push(idx);
        ScalarVar(idx)
    }

    /// Characteristic feature size of the sketch: the larger of the points'
    /// per-axis coordinate spread and the largest scalar magnitude (radii).
    /// Per-axis spread, not coordinate magnitude, so a small part drawn far
    /// from the origin still reads as small.
    fn feature_scale(&self) -> f64 {
        let mut spread = 0.0f64;
        for axis in 0..2 {
            let vals = self.point_bases.iter().map(|&b| self.vars[b + axis]);
            let (min, max) = vals.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                (lo.min(v), hi.max(v))
            });
            spread = spread.max(max - min);
        }
        let scalar_mag = self
            .scalar_idx
            .iter()
            .map(|&i| self.vars[i].abs())
            .fold(0.0f64, f64::max);
        spread.max(scalar_mag)
    }

    pub fn scalar(&self, s: ScalarVar) -> f64 {
        self.vars[s.0]
    }

    pub fn set_scalar(&mut self, s: ScalarVar, v: f64) {
        self.vars[s.0] = v;
    }

    pub fn constrain(&mut self, c: Constraint) {
        self.constraints.push(c);
    }

    /// Number of constraints added so far. Lets callers that lower one
    /// document constraint into several solver constraints (e.g. pinning
    /// every degree of freedom of an entity) count how many rows they added.
    pub fn constraint_count(&self) -> usize {
        self.constraints.len()
    }

    /// Residuals of every constraint at the given variable assignment. Each
    /// residual is scaled to *length* units so mixed constraint types weigh
    /// comparably in the least-squares objective.
    fn residuals(&self, vars: &[f64], out: &mut Vec<f64>) {
        out.clear();
        let p = |v: PointVar| (vars[v.0], vars[v.0 + 1]);
        for c in &self.constraints {
            match *c {
                Constraint::Coincident(a, b) => {
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    out.push(ax - bx);
                    out.push(ay - by);
                }
                Constraint::MidpointsCoincident(a0, a1, b0, b1) => {
                    let (ax0, ay0) = p(a0);
                    let (ax1, ay1) = p(a1);
                    let (bx0, by0) = p(b0);
                    let (bx1, by1) = p(b1);
                    out.push((ax0 + ax1 - bx0 - bx1) * 0.5);
                    out.push((ay0 + ay1 - by0 - by1) * 0.5);
                }
                Constraint::Horizontal(a, b) => {
                    let (_, ay) = p(a);
                    let (_, by) = p(b);
                    out.push(ay - by);
                }
                Constraint::Vertical(a, b) => {
                    let (ax, _) = p(a);
                    let (bx, _) = p(b);
                    out.push(ax - bx);
                }
                Constraint::Distance(a, b, d) => {
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    out.push((bx - ax).hypot(by - ay) - d);
                }
                Constraint::HorizontalDistance(a, b, d) => {
                    let (ax, _) = p(a);
                    let (bx, _) = p(b);
                    out.push((bx - ax).abs() - d);
                }
                Constraint::VerticalDistance(a, b, d) => {
                    let (_, ay) = p(a);
                    let (_, by) = p(b);
                    out.push((by - ay).abs() - d);
                }
                Constraint::Parallel(a, b, c2, d) => {
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    let (cx, cy) = p(c2);
                    let (dx, dy) = p(d);
                    let (ux, uy) = (bx - ax, by - ay);
                    let (vx, vy) = (dx - cx, dy - cy);
                    // Cross product normalised by one factor keeps length units.
                    let n = ux.hypot(uy).max(1e-12);
                    out.push((ux * vy - uy * vx) / n);
                }
                Constraint::Perpendicular(a, b, c2, d) => {
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    let (cx, cy) = p(c2);
                    let (dx, dy) = p(d);
                    let (ux, uy) = (bx - ax, by - ay);
                    let (vx, vy) = (dx - cx, dy - cy);
                    let n = ux.hypot(uy).max(1e-12);
                    out.push((ux * vx + uy * vy) / n);
                }
                Constraint::Angle(a, b, c2, d, theta) => {
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    let (cx, cy) = p(c2);
                    let (dx, dy) = p(d);
                    let (ux, uy) = (bx - ax, by - ay);
                    let (vx, vy) = (dx - cx, dy - cy);
                    let n = ux.hypot(uy).max(1e-12);
                    // cross·cosθ − dot·sinθ = |u||v|·sin(angle − θ), the
                    // same normalised-by-one-factor scheme as Parallel and
                    // Perpendicular above.
                    let cross = ux * vy - uy * vx;
                    let dot = ux * vx + uy * vy;
                    out.push((cross * theta.cos() - dot * theta.sin()) / n);
                }
                Constraint::Fixed(a, x, y) => {
                    let (ax, ay) = p(a);
                    out.push(ax - x);
                    out.push(ay - y);
                }
                Constraint::PointOnLine(q, a, b) => {
                    let (qx, qy) = p(q);
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    let (ux, uy) = (bx - ax, by - ay);
                    let n = ux.hypot(uy).max(1e-12);
                    out.push((ux * (qy - ay) - uy * (qx - ax)) / n);
                }
                Constraint::MidpointOnLine(p1, p2, a, b) => {
                    let (x1, y1) = p(p1);
                    let (x2, y2) = p(p2);
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    let (mx, my) = ((x1 + x2) * 0.5, (y1 + y2) * 0.5);
                    let (ux, uy) = (bx - ax, by - ay);
                    let n = ux.hypot(uy).max(1e-12);
                    out.push((ux * (my - ay) - uy * (mx - ax)) / n);
                }
                Constraint::PointLineDistance(q, a, b, dist) => {
                    let (qx, qy) = p(q);
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    let (ux, uy) = (bx - ax, by - ay);
                    let n = ux.hypot(uy).max(1e-12);
                    let d = (ux * (qy - ay) - uy * (qx - ax)) / n;
                    out.push(d.abs() - dist);
                }
                Constraint::EqualLength(a, b, c2, d) => {
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    let (cx, cy) = p(c2);
                    let (dx, dy) = p(d);
                    out.push((bx - ax).hypot(by - ay) - (dx - cx).hypot(dy - cy));
                }
                Constraint::FixedScalar(s, v) => {
                    out.push(vars[s.0] - v);
                }
                Constraint::EqualScalar(s1, s2) => {
                    out.push(vars[s1.0] - vars[s2.0]);
                }
                Constraint::PointOnCircle(q, c2, r) => {
                    let (qx, qy) = p(q);
                    let (cx, cy) = p(c2);
                    out.push((qx - cx).hypot(qy - cy) - vars[r.0]);
                }
                Constraint::TangentLineCircle(a, b, c2, r) => {
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    let (cx, cy) = p(c2);
                    let (ux, uy) = (bx - ax, by - ay);
                    let n = ux.hypot(uy).max(1e-12);
                    let d = (ux * (cy - ay) - uy * (cx - ax)) / n;
                    out.push(d.abs() - vars[r.0]);
                }
                Constraint::TangentCircleCircle {
                    c1,
                    r1,
                    c2,
                    r2,
                    internal,
                } => {
                    let (x1, y1) = p(c1);
                    let (x2, y2) = p(c2);
                    let d = (x2 - x1).hypot(y2 - y1);
                    let target = if internal {
                        (vars[r1.0] - vars[r2.0]).abs()
                    } else {
                        vars[r1.0] + vars[r2.0]
                    };
                    out.push(d - target);
                }
            }
        }
    }

    /// Residual values, their exact gradient with respect to every variable
    /// (via forward-mode dual numbers — see [`dual`]), and which constraint
    /// index owns each row (most constraints emit one row; `Coincident` and
    /// `Fixed` emit two). Mirrors [`Sketch::residuals`] arm-for-arm; a
    /// dedicated regression test cross-checks the two against finite
    /// differences so a transcription slip here is caught immediately.
    fn residuals_and_jacobian(&self, vars: &[f64]) -> (Vec<f64>, Vec<Vec<f64>>, Vec<usize>) {
        let nv = vars.len();
        let mut values = Vec::new();
        let mut jac = Vec::new();
        let mut owner = Vec::new();
        let p = |v: PointVar| {
            (
                Dual::var(vars[v.0], v.0, nv),
                Dual::var(vars[v.0 + 1], v.0 + 1, nv),
            )
        };
        // Floors a near-zero length denominator to 1e-12 (matching the f64
        // `residuals()` path) without disturbing its gradient — the floor is
        // only a division-by-exact-zero safety valve in an already-singular
        // configuration, so the small gradient inaccuracy there is moot.
        let floor = |n: &Dual| Dual {
            val: n.val.max(1e-12),
            d: n.d.clone(),
        };
        let mut push = |ci: usize, r: Dual| {
            values.push(r.val);
            jac.push(r.d);
            owner.push(ci);
        };
        for (ci, c) in self.constraints.iter().enumerate() {
            match *c {
                Constraint::Coincident(a, b) => {
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    push(ci, &ax - &bx);
                    push(ci, &ay - &by);
                }
                Constraint::MidpointsCoincident(a0, a1, b0, b1) => {
                    let (ax0, ay0) = p(a0);
                    let (ax1, ay1) = p(a1);
                    let (bx0, by0) = p(b0);
                    let (bx1, by1) = p(b1);
                    let half = Dual::constant(0.5, nv);
                    push(ci, &(&(&(&ax0 + &ax1) - &bx0) - &bx1) * &half);
                    push(ci, &(&(&(&ay0 + &ay1) - &by0) - &by1) * &half);
                }
                Constraint::Horizontal(a, b) => {
                    let (_, ay) = p(a);
                    let (_, by) = p(b);
                    push(ci, &ay - &by);
                }
                Constraint::Vertical(a, b) => {
                    let (ax, _) = p(a);
                    let (bx, _) = p(b);
                    push(ci, &ax - &bx);
                }
                Constraint::Distance(a, b, d) => {
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    let len = dual::hypot(&(&bx - &ax), &(&by - &ay));
                    push(ci, &len - &Dual::constant(d, nv));
                }
                Constraint::HorizontalDistance(a, b, d) => {
                    let (ax, _) = p(a);
                    let (bx, _) = p(b);
                    push(ci, &(&bx - &ax).abs() - &Dual::constant(d, nv));
                }
                Constraint::VerticalDistance(a, b, d) => {
                    let (_, ay) = p(a);
                    let (_, by) = p(b);
                    push(ci, &(&by - &ay).abs() - &Dual::constant(d, nv));
                }
                Constraint::Parallel(a, b, c2, d) => {
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    let (cx, cy) = p(c2);
                    let (dx, dy) = p(d);
                    let (ux, uy) = (&bx - &ax, &by - &ay);
                    let (vx, vy) = (&dx - &cx, &dy - &cy);
                    let n = dual::hypot(&ux, &uy);
                    let n = floor(&n);
                    let cross = &(&ux * &vy) - &(&uy * &vx);
                    push(ci, &cross / &n);
                }
                Constraint::Perpendicular(a, b, c2, d) => {
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    let (cx, cy) = p(c2);
                    let (dx, dy) = p(d);
                    let (ux, uy) = (&bx - &ax, &by - &ay);
                    let (vx, vy) = (&dx - &cx, &dy - &cy);
                    let n = dual::hypot(&ux, &uy);
                    let n = floor(&n);
                    let dot = &(&ux * &vx) + &(&uy * &vy);
                    push(ci, &dot / &n);
                }
                Constraint::Angle(a, b, c2, d, theta) => {
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    let (cx, cy) = p(c2);
                    let (dx, dy) = p(d);
                    let (ux, uy) = (&bx - &ax, &by - &ay);
                    let (vx, vy) = (&dx - &cx, &dy - &cy);
                    let n = dual::hypot(&ux, &uy);
                    let n = floor(&n);
                    let cross = &(&ux * &vy) - &(&uy * &vx);
                    let dot = &(&ux * &vx) + &(&uy * &vy);
                    let ct = Dual::constant(theta.cos(), nv);
                    let st = Dual::constant(theta.sin(), nv);
                    let r = &(&cross * &ct) - &(&dot * &st);
                    push(ci, &r / &n);
                }
                Constraint::Fixed(a, x, y) => {
                    let (ax, ay) = p(a);
                    push(ci, &ax - &Dual::constant(x, nv));
                    push(ci, &ay - &Dual::constant(y, nv));
                }
                Constraint::PointOnLine(q, a, b) => {
                    let (qx, qy) = p(q);
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    let (ux, uy) = (&bx - &ax, &by - &ay);
                    let n = dual::hypot(&ux, &uy);
                    let n = floor(&n);
                    let cross = &(&ux * &(&qy - &ay)) - &(&uy * &(&qx - &ax));
                    push(ci, &cross / &n);
                }
                Constraint::MidpointOnLine(p1, p2, a, b) => {
                    let (x1, y1) = p(p1);
                    let (x2, y2) = p(p2);
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    let half = Dual::constant(0.5, nv);
                    let mx = &(&x1 + &x2) * &half;
                    let my = &(&y1 + &y2) * &half;
                    let (ux, uy) = (&bx - &ax, &by - &ay);
                    let n = dual::hypot(&ux, &uy);
                    let n = floor(&n);
                    let cross = &(&ux * &(&my - &ay)) - &(&uy * &(&mx - &ax));
                    push(ci, &cross / &n);
                }
                Constraint::PointLineDistance(q, a, b, dist) => {
                    let (qx, qy) = p(q);
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    let (ux, uy) = (&bx - &ax, &by - &ay);
                    let n = dual::hypot(&ux, &uy);
                    let n = floor(&n);
                    let cross = &(&ux * &(&qy - &ay)) - &(&uy * &(&qx - &ax));
                    let d = &cross / &n;
                    push(ci, &d.abs() - &Dual::constant(dist, nv));
                }
                Constraint::EqualLength(a, b, c2, d) => {
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    let (cx, cy) = p(c2);
                    let (dx, dy) = p(d);
                    let lab = dual::hypot(&(&bx - &ax), &(&by - &ay));
                    let lcd = dual::hypot(&(&dx - &cx), &(&dy - &cy));
                    push(ci, &lab - &lcd);
                }
                Constraint::FixedScalar(s, v) => {
                    let sv = Dual::var(vars[s.0], s.0, nv);
                    push(ci, &sv - &Dual::constant(v, nv));
                }
                Constraint::EqualScalar(s1, s2) => {
                    let v1 = Dual::var(vars[s1.0], s1.0, nv);
                    let v2 = Dual::var(vars[s2.0], s2.0, nv);
                    push(ci, &v1 - &v2);
                }
                Constraint::PointOnCircle(q, c2, r) => {
                    let (qx, qy) = p(q);
                    let (cx, cy) = p(c2);
                    let rv = Dual::var(vars[r.0], r.0, nv);
                    let d = dual::hypot(&(&qx - &cx), &(&qy - &cy));
                    push(ci, &d - &rv);
                }
                Constraint::TangentLineCircle(a, b, c2, r) => {
                    let (ax, ay) = p(a);
                    let (bx, by) = p(b);
                    let (cx, cy) = p(c2);
                    let rv = Dual::var(vars[r.0], r.0, nv);
                    let (ux, uy) = (&bx - &ax, &by - &ay);
                    let n = dual::hypot(&ux, &uy);
                    let n = floor(&n);
                    let cross = &(&ux * &(&cy - &ay)) - &(&uy * &(&cx - &ax));
                    let d = &cross / &n;
                    push(ci, &d.abs() - &rv);
                }
                Constraint::TangentCircleCircle {
                    c1,
                    r1,
                    c2,
                    r2,
                    internal,
                } => {
                    let (x1, y1) = p(c1);
                    let (x2, y2) = p(c2);
                    let rv1 = Dual::var(vars[r1.0], r1.0, nv);
                    let rv2 = Dual::var(vars[r2.0], r2.0, nv);
                    let d = dual::hypot(&(&x2 - &x1), &(&y2 - &y1));
                    let target = if internal {
                        (&rv1 - &rv2).abs()
                    } else {
                        &rv1 + &rv2
                    };
                    push(ci, &d - &target);
                }
            }
        }
        (values, jac, owner)
    }

    /// Levenberg–Marquardt over all point variables with a numeric Jacobian.
    /// Sketch systems are small (tens of points), so the dense normal-matrix
    /// solve is far from being the bottleneck.
    #[allow(clippy::needless_range_loop)]
    pub fn solve(&mut self) -> SolveResult {
        const MAX_ITER: u32 = 200;
        /// Convergence tolerance per unit of feature size. Residuals are in
        /// length units, so an absolute cutoff can't serve both a 4-unit part
        /// and a 4-billion-unit site plan: at 4e9 an f64 ULP is already
        /// ~9e-7, making the old absolute 1e-7 unreachable and every such
        /// solve report failure. Scaling by [`Sketch::feature_scale`]
        /// (floored at unit scale so sub-unit parts keep an absolute cutoff)
        /// asks every sketch for the same *relative* accuracy instead. 1e-8
        /// reproduces the old cutoff at part scale (~10 units) — which sat
        /// deliberately above the ~κ²·ε precision floor that stiff systems
        /// (e.g. a tangent-welded slot) plateau at, itself proportional to
        /// feature size — and still sits ~4e7 ULPs above f64 resolution at
        /// any scale.
        const TOL: f64 = 1e-8;

        let nv = self.vars.len();
        let mut r = Vec::new();
        self.residuals(&self.vars, &mut r);
        if r.is_empty() || nv == 0 {
            return SolveResult {
                converged: true,
                residual: 0.0,
                iterations: 0,
            };
        }
        let mut lambda = 1e-3;
        let mut cost: f64 = r.iter().map(|v| v * v).sum();
        let mut r_probe = Vec::new();

        for iter in 0..MAX_ITER {
            let inf: f64 = r.iter().fold(0.0, |m, v| m.max(v.abs()));
            // Recomputed from the *current* variables, not the entry guess,
            // so convergence is a pure predicate of the state being tested: a
            // sketch that just converged re-converges in zero iterations even
            // when solving shrank its spread (and with it the tolerance).
            let tol = TOL * self.feature_scale().max(1.0);
            if inf < tol {
                return SolveResult {
                    converged: true,
                    residual: inf,
                    iterations: iter,
                };
            }

            // Exact Jacobian via forward-mode dual numbers (see
            // `residuals_and_jacobian`) — no finite-difference truncation
            // noise, which matters both for convergence and for the
            // rank-based DOF/redundancy analysis built on the same rows.
            let nr = r.len();
            let (_, jac, _) = self.residuals_and_jacobian(&self.vars);

            // Normal equations JᵀJ + λ·diag, RHS −Jᵀr.
            let mut jtj = vec![vec![0.0f64; nv]; nv];
            let mut jtr = vec![0.0f64; nv];
            for row in 0..nr {
                for i in 0..nv {
                    let jri = jac[row][i];
                    if jri == 0.0 {
                        continue;
                    }
                    jtr[i] -= jri * r[row];
                    for j in i..nv {
                        jtj[i][j] += jri * jac[row][j];
                    }
                }
            }
            for i in 0..nv {
                for j in 0..i {
                    jtj[i][j] = jtj[j][i];
                }
            }

            // Try steps with increasing damping until the cost drops.
            let mut accepted = false;
            for _ in 0..8 {
                let mut m = jtj.clone();
                // Floor the damping diagonal at identity scale: directions the
                // constraints barely touch would otherwise get a near-zero
                // damping term, and numeric-Jacobian noise in the gradient
                // launches the step along them.
                for (i, row) in m.iter_mut().enumerate() {
                    row[i] += lambda * (jtj[i][i].max(1.0));
                }
                let Some(step) = solve_dense(&mut m, jtr.clone()) else {
                    lambda *= 10.0;
                    continue;
                };
                let trial: Vec<f64> = self.vars.iter().zip(&step).map(|(v, s)| v + s).collect();
                self.residuals(&trial, &mut r_probe);
                let trial_cost: f64 = r_probe.iter().map(|v| v * v).sum();
                if trial_cost < cost {
                    self.vars = trial;
                    std::mem::swap(&mut r, &mut r_probe);
                    cost = trial_cost;
                    lambda = (lambda / 3.0).max(1e-12);
                    accepted = true;
                    break;
                }
                lambda *= 4.0;
            }
            if !accepted {
                // No damping level improves the cost: local minimum (possibly
                // an inconsistent constraint set). Vars are unchanged since
                // the loop-start check, so this recomputes the same tol.
                let inf: f64 = r.iter().fold(0.0, |m, v| m.max(v.abs()));
                return SolveResult {
                    converged: inf < TOL * self.feature_scale().max(1.0),
                    residual: inf,
                    iterations: iter,
                };
            }
        }
        let inf: f64 = r.iter().fold(0.0, |m, v| m.max(v.abs()));
        SolveResult {
            converged: inf < TOL * self.feature_scale().max(1.0),
            residual: inf,
            iterations: MAX_ITER,
        }
    }

    /// Degrees of freedom remaining, and which constraints are numerically
    /// redundant, at the sketch's current variable assignment (call after a
    /// successful [`Sketch::solve`]). Rank is computed by sequential
    /// modified Gram-Schmidt over the Jacobian's rows in constraint
    /// insertion order: a row that lies (within tolerance) in the span of
    /// the rows already kept contributes no new information and is dropped.
    /// Insertion order — rather than a max-norm pivot search, as classic
    /// rank-revealing QR would use — is deliberate: it blames the
    /// *later*-added constraint when two are equivalent, matching how
    /// `Document::add_constraint`'s own dedup already behaves. A constraint
    /// that emits more than one row (`Coincident`, `Fixed`) is only reported
    /// redundant when *all* of its rows were dropped — one surviving row
    /// still carries information the others don't.
    pub fn analyze(&self) -> DofReport {
        let nv = self.vars.len();
        if nv == 0 {
            return DofReport {
                dof: 0,
                redundant: Vec::new(),
            };
        }
        let (_, jac, owner) = self.residuals_and_jacobian(&self.vars);
        let (rank, kept) = independent_rows(&jac);
        let nc = self.constraints.len();
        let mut total = vec![0usize; nc];
        let mut kept_count = vec![0usize; nc];
        for (row, &ci) in owner.iter().enumerate() {
            total[ci] += 1;
            if kept[row] {
                kept_count[ci] += 1;
            }
        }
        let redundant = (0..nc)
            .filter(|&ci| total[ci] > 0 && kept_count[ci] == 0)
            .collect();
        DofReport {
            dof: nv.saturating_sub(rank),
            redundant,
        }
    }

    /// Captures the current variable assignment so a later
    /// [`Sketch::diagnose_conflict`] call can replay the same starting
    /// point a failed [`Sketch::solve`] began from.
    pub fn snapshot(&self) -> Vec<f64> {
        self.vars.clone()
    }

    /// Solves like [`Sketch::solve`], but retries once from a slightly
    /// perturbed starting point if the first attempt doesn't converge.
    /// Many pure-angle residuals (Horizontal, Parallel, Perpendicular, ...)
    /// have an exact saddle 90° from the target; a starting configuration
    /// that happens to sit exactly there needs a nudge to break the
    /// symmetry and descend (the same trick `perturb_line`, in the CAD
    /// layer above, applies for a single line about its own midpoint — this
    /// is the generic, entity-agnostic version for an arbitrary system).
    pub fn solve_robust(&mut self) -> SolveResult {
        let initial = self.vars.clone();
        let res = self.solve();
        if res.converged {
            return res;
        }
        self.vars = perturb(&initial);
        self.solve()
    }

    /// Only meaningful after `solve()`/`solve_robust()` (started from
    /// `initial`) returned `converged: false`. Re-solves the system once
    /// per constraint with that one constraint removed, from the same
    /// starting point (via `solve_robust`, so a saddle in the trial itself
    /// doesn't wrongly hide a constraint that genuinely IS solvable once
    /// its neighbor is removed); any constraint whose absence alone lets
    /// the rest converge is a leading conflict suspect. O(k) extra solves
    /// (k = constraint count) — cheap at this crate's scale, and only run
    /// on the already-exceptional failure path.
    pub fn diagnose_conflict(&self, initial: &[f64]) -> ConflictReport {
        let mut culprits = Vec::new();
        for i in 0..self.constraints.len() {
            let constraints = self
                .constraints
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .map(|(_, c)| c.clone())
                .collect();
            let mut trial = Sketch {
                vars: initial.to_vec(),
                constraints,
                point_bases: self.point_bases.clone(),
                scalar_idx: self.scalar_idx.clone(),
            };
            if trial.solve_robust().converged {
                culprits.push(i);
            }
        }
        ConflictReport { culprits }
    }
}

/// A small deterministic per-variable nudge that breaks exact symmetry
/// (e.g. a line sitting exactly on a 90° saddle) without moving far from
/// `vars`, scaled to each variable's own magnitude so it's meaningful
/// across unit scales.
fn perturb(vars: &[f64]) -> Vec<f64> {
    vars.iter()
        .enumerate()
        .map(|(k, v)| v + 1e-3 * (k as f64 * 0.7 + 1.0).sin() * v.abs().max(1.0))
        .collect()
}

#[derive(Clone, Debug)]
pub struct DofReport {
    pub dof: usize,
    /// Constraint indices (in this sketch's insertion order) that are
    /// numerically redundant given the others.
    pub redundant: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct ConflictReport {
    /// Constraint indices whose removal alone lets the rest of the system
    /// converge — the leading suspects in a contradictory constraint set.
    pub culprits: Vec<usize>,
}

/// Sequential modified Gram-Schmidt over `rows` (each of length `nv`).
/// Returns the rank and, per row, whether it was kept in the basis (`true`)
/// or found linearly dependent on the rows already kept (`false`).
fn independent_rows(rows: &[Vec<f64>]) -> (usize, Vec<bool>) {
    let dot = |a: &[f64], b: &[f64]| -> f64 { a.iter().zip(b).map(|(x, y)| x * y).sum() };
    let max_norm = rows
        .iter()
        .map(|r| dot(r, r).sqrt())
        .fold(0.0_f64, f64::max);
    let tol = 1e-9 * max_norm.max(1.0);
    let mut basis: Vec<Vec<f64>> = Vec::new();
    let mut kept = vec![false; rows.len()];
    for (i, row) in rows.iter().enumerate() {
        let mut v = row.clone();
        for b in &basis {
            let proj = dot(&v, b);
            for (vi, bi) in v.iter_mut().zip(b) {
                *vi -= proj * bi;
            }
        }
        let norm = dot(&v, &v).sqrt();
        if norm > tol {
            for x in &mut v {
                *x /= norm;
            }
            basis.push(v);
            kept[i] = true;
        }
    }
    (basis.len(), kept)
}

/// Dense Gaussian elimination with partial pivoting; `None` when singular
/// beyond the damping's ability to regularise.
#[allow(clippy::needless_range_loop)]
fn solve_dense(a: &mut [Vec<f64>], mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = a.len();
    for col in 0..n {
        let mut piv = col;
        let mut best = a[col][col].abs();
        for r in (col + 1)..n {
            if a[r][col].abs() > best {
                best = a[r][col].abs();
                piv = r;
            }
        }
        if best < 1e-300 {
            return None;
        }
        a.swap(col, piv);
        b.swap(col, piv);
        for r in (col + 1)..n {
            let f = a[r][col] / a[col][col];
            if f == 0.0 {
                continue;
            }
            for c in col..n {
                a[r][c] -= f * a[col][c];
            }
            b[r] -= f * b[col];
        }
    }
    for col in (0..n).rev() {
        let mut s = b[col];
        for c in (col + 1)..n {
            s -= a[col][c] * b[c];
        }
        b[col] = s / a[col][col];
    }
    Some(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
        (a.0 - b.0).hypot(a.1 - b.1)
    }

    #[test]
    fn sloppy_quad_becomes_exact_rectangle() {
        let mut s = Sketch::new();
        // Four roughly-rectangular corners, drawn sloppily.
        let a = s.add_point(0.1, -0.2);
        let b = s.add_point(4.3, 0.15);
        let c = s.add_point(3.9, 3.2);
        let d = s.add_point(-0.2, 2.8);
        s.constrain(Constraint::Fixed(a, 0.0, 0.0));
        s.constrain(Constraint::Horizontal(a, b));
        s.constrain(Constraint::Vertical(b, c));
        s.constrain(Constraint::Horizontal(c, d));
        s.constrain(Constraint::Vertical(d, a));
        s.constrain(Constraint::Distance(a, b, 4.0));
        s.constrain(Constraint::Distance(b, c, 3.0));
        let res = s.solve();
        assert!(res.converged, "must converge, residual {}", res.residual);
        assert!(dist(s.point(a), (0.0, 0.0)) < 1e-6);
        assert!(dist(s.point(b), (4.0, 0.0)) < 1e-6);
        assert!(dist(s.point(c), (4.0, 3.0)) < 1e-6);
        assert!(dist(s.point(d), (0.0, 3.0)) < 1e-6);
    }

    #[test]
    fn triangle_from_three_side_lengths() {
        let mut s = Sketch::new();
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(5.2, 0.3);
        let c = s.add_point(2.0, 3.5);
        s.constrain(Constraint::Fixed(a, 0.0, 0.0));
        s.constrain(Constraint::Horizontal(a, b));
        s.constrain(Constraint::Distance(a, b, 5.0));
        s.constrain(Constraint::Distance(b, c, 4.0));
        s.constrain(Constraint::Distance(a, c, 3.0));
        let res = s.solve();
        assert!(res.converged, "residual {}", res.residual);
        // 3-4-5 triangle: the angle at c must be right.
        let (cx, cy) = s.point(c);
        let (bx, by) = s.point(b);
        let dot = (0.0 - cx) * (bx - cx) + (0.0 - cy) * (by - cy);
        assert!(dot.abs() < 1e-5, "3-4-5 triangle has a right angle at C");
    }

    #[test]
    fn under_constrained_sketch_still_solves_and_stays_close() {
        // One distance constraint on two free points: infinitely many
        // solutions; LM damping must pick one near the initial guess.
        let mut s = Sketch::new();
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(2.5, 0.1);
        s.constrain(Constraint::Distance(a, b, 3.0));
        let res = s.solve();
        assert!(res.converged, "residual {}", res.residual);
        assert!((dist(s.point(a), s.point(b)) - 3.0).abs() < 1e-6);
        assert!(dist(s.point(a), (0.0, 0.0)) < 0.5, "a stayed close");
    }

    #[test]
    fn redundant_consistent_constraints_converge() {
        let mut s = Sketch::new();
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(3.9, 0.2);
        s.constrain(Constraint::Fixed(a, 0.0, 0.0));
        s.constrain(Constraint::Horizontal(a, b));
        s.constrain(Constraint::Horizontal(a, b));
        s.constrain(Constraint::Distance(a, b, 4.0));
        let res = s.solve();
        assert!(res.converged, "residual {}", res.residual);
        assert!(dist(s.point(b), (4.0, 0.0)) < 1e-6);
    }

    #[test]
    fn perpendicular_and_parallel_pairs() {
        let mut s = Sketch::new();
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(4.0, 0.4);
        let c = s.add_point(0.1, 0.1);
        let d = s.add_point(0.4, 3.0);
        let e = s.add_point(1.0, 1.2);
        let f = s.add_point(5.2, 1.4);
        s.constrain(Constraint::Fixed(a, 0.0, 0.0));
        s.constrain(Constraint::Fixed(b, 4.0, 0.0));
        s.constrain(Constraint::Coincident(a, c));
        s.constrain(Constraint::Perpendicular(a, b, c, d));
        s.constrain(Constraint::Parallel(a, b, e, f));
        let res = s.solve();
        assert!(res.converged, "residual {}", res.residual);
        let (dx, dy) = s.point(d);
        let (cx, cy) = s.point(c);
        assert!((dx - cx).abs() < 1e-7, "cd is vertical, got dx {}", dx - cx);
        assert!((dy - cy).abs() > 1.0, "d kept its extent");
        let (ex, ey) = s.point(e);
        let (fx, fy) = s.point(f);
        assert!((fy - ey).abs() < 1e-7, "ef is horizontal");
        assert!((fx - ex).abs() > 1.0, "ef kept its extent");
    }

    #[test]
    fn point_line_distance_holds_two_lines_at_a_width() {
        // Line a is pinned; line c→d starts roughly parallel 1.5 above it.
        // Driving both of its endpoints to distance 4 from line a must slide
        // it out to a clean parallel at the target width.
        let mut s = Sketch::new();
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(6.0, 0.0);
        let c = s.add_point(0.2, 1.4);
        let d = s.add_point(5.8, 1.6);
        s.constrain(Constraint::Fixed(a, 0.0, 0.0));
        s.constrain(Constraint::Fixed(b, 6.0, 0.0));
        s.constrain(Constraint::PointLineDistance(c, a, b, 4.0));
        s.constrain(Constraint::PointLineDistance(d, a, b, 4.0));
        let res = s.solve();
        assert!(res.converged, "residual {}", res.residual);
        let (_, cy) = s.point(c);
        let (_, dy) = s.point(d);
        assert!((cy - 4.0).abs() < 1e-6, "c settled at the width: {cy}");
        assert!((dy - 4.0).abs() < 1e-6, "d settled at the width: {dy}");
    }

    #[test]
    fn midpoints_coincident_drags_a_line_by_its_midpoint() {
        // The origin scenario: a fixed point at (0,0), a line floating
        // nearby. Welding the line's midpoint to the point (same-var
        // degenerate side) must translate the line so its middle lands
        // exactly on the point.
        let mut s = Sketch::new();
        let o = s.add_point(0.0, 0.0);
        let a = s.add_point(3.0, 1.0);
        let b = s.add_point(7.0, 3.0);
        s.constrain(Constraint::Fixed(o, 0.0, 0.0));
        s.constrain(Constraint::Distance(a, b, 20.0f64.sqrt()));
        s.constrain(Constraint::MidpointsCoincident(o, o, a, b));
        let res = s.solve();
        assert!(res.converged, "residual {}", res.residual);
        let (ax, ay) = s.point(a);
        let (bx, by) = s.point(b);
        assert!(
            ((ax + bx) * 0.5).abs() < 1e-6 && ((ay + by) * 0.5).abs() < 1e-6,
            "midpoint landed on the origin: mid=({}, {})",
            (ax + bx) * 0.5,
            (ay + by) * 0.5
        );
        assert!(
            ((bx - ax).hypot(by - ay) - 20.0f64.sqrt()).abs() < 1e-6,
            "the line kept its length"
        );
    }

    #[test]
    fn midpoints_coincident_joins_two_line_middles() {
        let mut s = Sketch::new();
        let a0 = s.add_point(0.0, 0.0);
        let a1 = s.add_point(4.0, 0.0);
        let b0 = s.add_point(10.0, 5.0);
        let b1 = s.add_point(14.0, 7.0);
        s.constrain(Constraint::Fixed(a0, 0.0, 0.0));
        s.constrain(Constraint::Fixed(a1, 4.0, 0.0));
        s.constrain(Constraint::MidpointsCoincident(a0, a1, b0, b1));
        let res = s.solve();
        assert!(res.converged, "residual {}", res.residual);
        let (bx0, by0) = s.point(b0);
        let (bx1, by1) = s.point(b1);
        assert!(
            ((bx0 + bx1) * 0.5 - 2.0).abs() < 1e-6 && ((by0 + by1) * 0.5).abs() < 1e-6,
            "b's midpoint landed on a's midpoint (2, 0): ({}, {})",
            (bx0 + bx1) * 0.5,
            (by0 + by1) * 0.5
        );
    }

    #[test]
    fn point_on_circle_pulls_point_to_rim() {
        let mut s = Sketch::new();
        let c = s.add_point(0.0, 0.0);
        let r = s.add_scalar(2.0);
        let q = s.add_point(2.6, 0.3);
        s.constrain(Constraint::Fixed(c, 0.0, 0.0));
        s.constrain(Constraint::FixedScalar(r, 2.0));
        s.constrain(Constraint::PointOnCircle(q, c, r));
        let res = s.solve();
        assert!(res.converged, "residual {}", res.residual);
        assert!((dist(s.point(q), (0.0, 0.0)) - 2.0).abs() < 1e-6);
        assert!(s.point(q).0 > 1.5, "q stayed near its start");
    }

    #[test]
    fn free_line_becomes_tangent_to_fixed_circle() {
        let mut s = Sketch::new();
        let c = s.add_point(0.0, 0.0);
        let r = s.add_scalar(1.0);
        let a = s.add_point(-3.0, 1.4);
        let b = s.add_point(3.0, 1.5);
        s.constrain(Constraint::Fixed(c, 0.0, 0.0));
        s.constrain(Constraint::FixedScalar(r, 1.0));
        s.constrain(Constraint::TangentLineCircle(a, b, c, r));
        let res = s.solve();
        assert!(res.converged, "residual {}", res.residual);
        let (ax, ay) = s.point(a);
        let (bx, by) = s.point(b);
        let n = (bx - ax).hypot(by - ay);
        let d = ((bx - ax) * (0.0 - ay) - (by - ay) * (0.0 - ax)) / n;
        assert!((d.abs() - 1.0).abs() < 1e-6, "line touches the rim, d={d}");
        assert!(ay > 0.5, "line approached from above, not through");
    }

    #[test]
    fn circles_touch_externally_and_internally() {
        // External: free circle pulled until it touches the fixed one.
        let mut s = Sketch::new();
        let c1 = s.add_point(0.0, 0.0);
        let r1 = s.add_scalar(2.0);
        let c2 = s.add_point(5.5, 0.0);
        let r2 = s.add_scalar(1.0);
        s.constrain(Constraint::Fixed(c1, 0.0, 0.0));
        s.constrain(Constraint::FixedScalar(r1, 2.0));
        s.constrain(Constraint::FixedScalar(r2, 1.0));
        s.constrain(Constraint::TangentCircleCircle {
            c1,
            r1,
            c2,
            r2,
            internal: false,
        });
        let res = s.solve();
        assert!(res.converged, "external residual {}", res.residual);
        assert!((dist(s.point(c2), (0.0, 0.0)) - 3.0).abs() < 1e-6);

        // Internal: small circle nests inside the big one.
        let mut s = Sketch::new();
        let c1 = s.add_point(0.0, 0.0);
        let r1 = s.add_scalar(3.0);
        let c2 = s.add_point(1.2, 0.4);
        let r2 = s.add_scalar(1.0);
        s.constrain(Constraint::Fixed(c1, 0.0, 0.0));
        s.constrain(Constraint::FixedScalar(r1, 3.0));
        s.constrain(Constraint::FixedScalar(r2, 1.0));
        s.constrain(Constraint::TangentCircleCircle {
            c1,
            r1,
            c2,
            r2,
            internal: true,
        });
        let res = s.solve();
        assert!(res.converged, "internal residual {}", res.residual);
        assert!((dist(s.point(c2), (0.0, 0.0)) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn fillet_system_follows_a_dragged_line() {
        // Horizontal line into a quarter-fillet into a vertical line; drag
        // the vertical line's far end sideways and the whole corner must
        // re-solve with the arc still tangent to both lines.
        let mut s = Sketch::new();
        let a0 = s.add_point(0.0, 0.0);
        let a1 = s.add_point(3.0, 0.0);
        let c = s.add_point(3.0, 1.0);
        let r = s.add_scalar(1.0);
        let ps = s.add_point(3.0, 0.0);
        let pe = s.add_point(4.0, 1.0);
        let b0 = s.add_point(4.0, 1.0);
        let b1 = s.add_point(4.0, 4.0);
        s.constrain(Constraint::Coincident(a1, ps));
        s.constrain(Constraint::Coincident(b0, pe));
        s.constrain(Constraint::PointOnCircle(ps, c, r));
        s.constrain(Constraint::PointOnCircle(pe, c, r));
        s.constrain(Constraint::TangentLineCircle(a0, a1, c, r));
        s.constrain(Constraint::TangentLineCircle(b0, b1, c, r));
        s.constrain(Constraint::FixedScalar(r, 1.0));
        s.constrain(Constraint::Fixed(a0, 0.0, 0.0));
        // Drag: the vertical line's far end moves right and the line tilts.
        s.constrain(Constraint::Fixed(b1, 5.0, 4.0));
        let res = s.solve();
        assert!(res.converged, "residual {}", res.residual);
        let (cx, cy) = s.point(c);
        for (p0, p1) in [(a0, a1), (b0, b1)] {
            let (x0, y0) = s.point(p0);
            let (x1, y1) = s.point(p1);
            let n = (x1 - x0).hypot(y1 - y0);
            let d = (((x1 - x0) * (cy - y0)) - ((y1 - y0) * (cx - x0))) / n;
            assert!(
                (d.abs() - 1.0).abs() < 1e-7,
                "line still tangent, d={d} for {p0:?}"
            );
        }
        for q in [ps, pe] {
            assert!(
                (dist(s.point(q), (cx, cy)) - 1.0).abs() < 1e-7,
                "arc endpoint on rim"
            );
        }
        assert!(
            dist(s.point(a1), s.point(ps)) < 1e-7 && dist(s.point(b0), s.point(pe)) < 1e-7,
            "joints stayed welded"
        );
    }

    #[test]
    fn point_on_line_and_equal_length() {
        let mut s = Sketch::new();
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(6.0, 0.0);
        let q = s.add_point(2.7, 1.4);
        let c = s.add_point(0.0, 2.0);
        let d = s.add_point(2.2, 2.1);
        s.constrain(Constraint::Fixed(a, 0.0, 0.0));
        s.constrain(Constraint::Fixed(b, 6.0, 0.0));
        s.constrain(Constraint::PointOnLine(q, a, b));
        s.constrain(Constraint::Fixed(c, 0.0, 2.0));
        s.constrain(Constraint::EqualLength(a, b, c, d));
        let res = s.solve();
        assert!(res.converged, "residual {}", res.residual);
        assert!(s.point(q).1.abs() < 1e-6, "q dropped onto the line");
        assert!(
            (dist(s.point(c), s.point(d)) - 6.0).abs() < 1e-7,
            "cd stretched to |ab|"
        );
    }

    #[test]
    fn angle_constraint_rotates_the_mover_to_the_target() {
        let mut s = Sketch::new();
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(4.0, 0.0);
        let c = s.add_point(0.0, 1.0);
        let d = s.add_point(3.0, 1.4);
        s.constrain(Constraint::Fixed(a, 0.0, 0.0));
        s.constrain(Constraint::Fixed(b, 4.0, 0.0));
        s.constrain(Constraint::Fixed(c, 0.0, 1.0));
        let theta = 60f64.to_radians();
        s.constrain(Constraint::Angle(a, b, c, d, theta));
        let res = s.solve_robust();
        assert!(res.converged, "residual {}", res.residual);
        let (cx, cy) = s.point(c);
        let (dx, dy) = s.point(d);
        let got = (dy - cy).atan2(dx - cx);
        // Undirected lines: θ and θ±π are both correct.
        let diff = (got - theta).rem_euclid(std::f64::consts::PI);
        let diff = diff.min(std::f64::consts::PI - diff);
        assert!(
            diff < 1e-6,
            "cd settled {}° off the target",
            diff.to_degrees()
        );
    }

    /// One of every constraint kind, at non-degenerate values (no zero-length
    /// segments, so the near-zero-denominator floor never engages and can't
    /// mask a transcription error). Cross-checks `residuals_and_jacobian`'s
    /// exact gradient against finite differences of the plain `residuals()`
    /// path, arm for arm — the regression guard for the `Dual` port.
    #[test]
    fn analytical_jacobian_matches_finite_differences() {
        let mut s = Sketch::new();
        let a = s.add_point(0.3, 0.2);
        let b = s.add_point(4.1, 0.6);
        let c = s.add_point(1.1, 3.2);
        let d = s.add_point(3.4, 2.1);
        let q = s.add_point(2.0, 1.0);
        let center = s.add_point(0.5, 0.5);
        let r = s.add_scalar(1.7);
        let center2 = s.add_point(6.0, 0.4);
        let r2 = s.add_scalar(0.9);

        s.constrain(Constraint::Coincident(a, b));
        s.constrain(Constraint::MidpointsCoincident(a, b, c, d));
        s.constrain(Constraint::MidpointsCoincident(q, q, a, b));
        s.constrain(Constraint::Horizontal(a, b));
        s.constrain(Constraint::Vertical(c, d));
        s.constrain(Constraint::Distance(a, b, 2.5));
        s.constrain(Constraint::HorizontalDistance(a, b, 1.2));
        s.constrain(Constraint::VerticalDistance(c, d, 0.9));
        s.constrain(Constraint::Parallel(a, b, c, d));
        s.constrain(Constraint::Perpendicular(a, b, c, d));
        s.constrain(Constraint::Angle(a, b, c, d, 0.6));
        s.constrain(Constraint::Fixed(a, 0.3, 0.2));
        s.constrain(Constraint::PointOnLine(q, a, b));
        s.constrain(Constraint::MidpointOnLine(c, d, a, b));
        s.constrain(Constraint::PointLineDistance(q, a, b, 0.8));
        s.constrain(Constraint::EqualLength(a, b, c, d));
        s.constrain(Constraint::FixedScalar(r, 1.7));
        s.constrain(Constraint::EqualScalar(r, r2));
        s.constrain(Constraint::PointOnCircle(q, center, r));
        s.constrain(Constraint::TangentLineCircle(a, b, center, r));
        s.constrain(Constraint::TangentCircleCircle {
            c1: center,
            r1: r,
            c2: center2,
            r2,
            internal: false,
        });

        let vars = s.vars.clone();
        let (vals, jac, _owner) = s.residuals_and_jacobian(&vars);
        let mut r0 = Vec::new();
        s.residuals(&vars, &mut r0);
        assert_eq!(r0, vals, "value pass must match the plain residuals() path");

        let nv = vars.len();
        let mut probe = vars.clone();
        for col in 0..nv {
            let h = 1e-6 * vars[col].abs().max(1.0);
            probe[col] = vars[col] + h;
            let mut plus = Vec::new();
            s.residuals(&probe, &mut plus);
            probe[col] = vars[col] - h;
            let mut minus = Vec::new();
            s.residuals(&probe, &mut minus);
            probe[col] = vars[col];
            for row in 0..vals.len() {
                let fd = (plus[row] - minus[row]) / (2.0 * h);
                assert!(
                    (jac[row][col] - fd).abs() < 1e-4 * fd.abs().max(1.0),
                    "row {row} col {col}: analytical {} vs finite-diff {fd}",
                    jac[row][col]
                );
            }
        }
    }

    #[test]
    fn equal_scalar_evens_out_two_radii() {
        let mut s = Sketch::new();
        let c1 = s.add_point(0.0, 0.0);
        let r1 = s.add_scalar(2.0);
        let c2 = s.add_point(6.0, 0.0);
        let r2 = s.add_scalar(0.8);
        s.constrain(Constraint::Fixed(c1, 0.0, 0.0));
        s.constrain(Constraint::Fixed(c2, 6.0, 0.0));
        s.constrain(Constraint::FixedScalar(r1, 2.0));
        s.constrain(Constraint::EqualScalar(r1, r2));
        let res = s.solve();
        assert!(res.converged, "residual {}", res.residual);
        assert!((s.scalar(r2) - 2.0).abs() < 1e-6, "r2 grew to match r1");
    }

    #[test]
    fn midpoint_on_line_with_perpendicular_makes_symmetry() {
        // Mirror line = the x axis (pinned). Point p1 pinned above it; p2
        // starts somewhere sloppy and must settle at p1's reflection.
        let mut s = Sketch::new();
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(10.0, 0.0);
        let p1 = s.add_point(3.0, 2.0);
        let p2 = s.add_point(4.1, -1.2);
        s.constrain(Constraint::Fixed(a, 0.0, 0.0));
        s.constrain(Constraint::Fixed(b, 10.0, 0.0));
        s.constrain(Constraint::Fixed(p1, 3.0, 2.0));
        s.constrain(Constraint::MidpointOnLine(p1, p2, a, b));
        s.constrain(Constraint::Perpendicular(p1, p2, a, b));
        let res = s.solve_robust();
        assert!(res.converged, "residual {}", res.residual);
        assert!(
            dist(s.point(p2), (3.0, -2.0)) < 1e-6,
            "p2 landed on the reflection: {:?}",
            s.point(p2)
        );
    }

    #[test]
    fn horizontal_and_vertical_distance_drive_separations() {
        let mut s = Sketch::new();
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(2.6, 1.2);
        s.constrain(Constraint::Fixed(a, 0.0, 0.0));
        s.constrain(Constraint::HorizontalDistance(a, b, 4.0));
        s.constrain(Constraint::VerticalDistance(a, b, 3.0));
        let res = s.solve();
        assert!(res.converged, "residual {}", res.residual);
        let (bx, by) = s.point(b);
        assert!((bx.abs() - 4.0).abs() < 1e-6, "dx settled at 4: {bx}");
        assert!((by.abs() - 3.0).abs() < 1e-6, "dy settled at 3: {by}");
        assert!(bx > 0.0 && by > 0.0, "b stayed on its own side");
    }

    #[test]
    fn billion_unit_sketch_converges_with_scale_relative_tolerance() {
        // At coordinates ~4e9 an f64 ULP is ~9e-7, so the old absolute 1e-7
        // cutoff was unreachable and this solve reported failure forever.
        const S: f64 = 1e9;
        let mut s = Sketch::new();
        let a = s.add_point(0.1 * S, -0.2 * S);
        let b = s.add_point(4.3 * S, 0.15 * S);
        let c = s.add_point(3.9 * S, 3.2 * S);
        let d = s.add_point(-0.2 * S, 2.8 * S);
        s.constrain(Constraint::Fixed(a, 0.0, 0.0));
        s.constrain(Constraint::Horizontal(a, b));
        s.constrain(Constraint::Vertical(b, c));
        s.constrain(Constraint::Horizontal(c, d));
        s.constrain(Constraint::Vertical(d, a));
        s.constrain(Constraint::Distance(a, b, 4.0 * S));
        s.constrain(Constraint::Distance(b, c, 3.0 * S));
        let res = s.solve();
        assert!(res.converged, "must converge, residual {}", res.residual);
        // Same relative accuracy the unit-scale rectangle test demands.
        assert!(dist(s.point(b), (4.0 * S, 0.0)) < 1e-6 * S);
        assert!(dist(s.point(c), (4.0 * S, 3.0 * S)) < 1e-6 * S);
    }

    #[test]
    fn small_part_far_from_origin_keeps_tight_tolerance() {
        // feature_scale measures per-axis spread, not coordinate magnitude:
        // a 4x3 part drawn a million units out must still be solved to the
        // unit-scale tolerance, not to 1e6 * 1e-7 = 0.1 units of slop.
        const OFF: f64 = 1e6;
        let mut s = Sketch::new();
        let a = s.add_point(OFF + 0.1, OFF - 0.2);
        let b = s.add_point(OFF + 4.3, OFF + 0.15);
        let c = s.add_point(OFF + 3.9, OFF + 3.2);
        s.constrain(Constraint::Fixed(a, OFF, OFF));
        s.constrain(Constraint::Horizontal(a, b));
        s.constrain(Constraint::Vertical(b, c));
        s.constrain(Constraint::Distance(a, b, 4.0));
        s.constrain(Constraint::Distance(b, c, 3.0));
        let res = s.solve();
        assert!(res.converged, "must converge, residual {}", res.residual);
        assert!(res.residual < 1e-6, "no origin-distance slop crept in");
        assert!(dist(s.point(b), (OFF + 4.0, OFF)) < 1e-5);
        assert!(dist(s.point(c), (OFF + 4.0, OFF + 3.0)) < 1e-5);
    }

    #[test]
    fn resolve_after_spread_shrinking_solve_is_a_no_op() {
        // Proptest-found boundary case: the noisy start has a larger spread
        // (hence looser tolerance) than the solved rectangle, and the first
        // solve exited with a residual between the two. With the tolerance
        // computed from the entry state a re-solve then burned an iteration;
        // computed from the *current* state, convergence is a pure predicate
        // and the re-solve must be an immediate no-op.
        let (w, h) = (1.441022932807858, 9.118437328976492);
        let noise = [
            -0.2500194697696756,
            -0.21159623907899536,
            0.009096108683423611,
            0.38408440784426223,
            -0.12954499680755016,
            0.0,
            0.0,
            0.17901450506973884,
        ];
        let mut s = Sketch::new();
        let a = s.add_point(noise[0], noise[1]);
        let b = s.add_point(w + noise[2], noise[3]);
        let c = s.add_point(w + noise[4], h + noise[5]);
        let d = s.add_point(noise[6], h + noise[7]);
        s.constrain(Constraint::Fixed(a, 0.0, 0.0));
        s.constrain(Constraint::Horizontal(a, b));
        s.constrain(Constraint::Vertical(b, c));
        s.constrain(Constraint::Horizontal(c, d));
        s.constrain(Constraint::Vertical(d, a));
        s.constrain(Constraint::Distance(a, b, w));
        s.constrain(Constraint::Distance(b, c, h));
        assert!(s.solve().converged);
        let again = s.solve();
        assert!(again.converged);
        assert_eq!(again.iterations, 0, "re-solve must be a no-op");
    }

    #[test]
    fn dof_counts_free_and_pinned_points() {
        let mut s = Sketch::new();
        let free = s.add_point(1.0, 2.0);
        assert_eq!(s.analyze().dof, 2, "one free point is 2 DOF");

        let a = s.add_point(0.0, 0.0);
        s.constrain(Constraint::Fixed(a, 0.0, 0.0));
        let report = s.analyze();
        assert_eq!(report.dof, 2, "the free point's 2 DOF remain");
        assert!(report.redundant.is_empty());
        let _ = free;
    }

    #[test]
    fn redundant_duplicate_horizontal_is_flagged() {
        let mut s = Sketch::new();
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(3.9, 0.2);
        s.constrain(Constraint::Fixed(a, 0.0, 0.0));
        s.constrain(Constraint::Horizontal(a, b));
        s.constrain(Constraint::Horizontal(a, b));
        s.constrain(Constraint::Distance(a, b, 4.0));
        assert!(s.solve().converged);
        let report = s.analyze();
        assert_eq!(report.dof, 0, "fully constrained rectangle side");
        assert_eq!(
            report.redundant,
            vec![2],
            "the second, later-added Horizontal is blamed"
        );
    }

    #[test]
    fn one_surviving_row_keeps_a_two_row_constraint_off_the_redundant_list() {
        // Fixed pins both x and y; nothing else touches this point, so
        // neither of Fixed's two rows can be redundant.
        let mut s = Sketch::new();
        let a = s.add_point(1.0, 2.0);
        s.constrain(Constraint::Fixed(a, 1.0, 2.0));
        let report = s.analyze();
        assert_eq!(report.dof, 0);
        assert!(report.redundant.is_empty());
    }

    #[test]
    fn conflicting_distance_constraints_are_both_culprits() {
        // 5 and 3 on the same segment can never both hold: no solve
        // converges, and removing either one alone fixes it.
        let mut s = Sketch::new();
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(5.0, 0.0);
        s.constrain(Constraint::Fixed(a, 0.0, 0.0));
        s.constrain(Constraint::Distance(a, b, 5.0));
        s.constrain(Constraint::Distance(a, b, 3.0));
        let initial = s.snapshot();
        assert!(!s.solve().converged);
        let report = s.diagnose_conflict(&initial);
        assert_eq!(report.culprits, vec![1, 2]);
    }
}
