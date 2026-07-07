//! Blend curve: a spline that joins the ends of two entities (line, arc, or spline)
//! with a chosen order of geometric continuity.
//!
//! The blend is a single polynomial Bézier of degree `2n+1`, where `n` is the
//! continuity order. Its endpoint derivatives are set so that, at each join, the
//! blend shares the source curve's position (G0), unit tangent (G1), curvature
//! vector (G2) and rate of change of curvature (G3). Matching the geometric
//! invariants up to order `n` at both ends yields exactly `Gn` continuity.
//!
//! A tension factor per side scales the endpoint speed `α = tension · chord`,
//! controlling how tightly the blend hugs each source curve (the classic Hermite
//! handle length is `chord/3`, recovered here with the default tension of 1.0).

use crate::curve::{Curve, CurveSegment};
use crate::nurbs::RationalBezier;
use crate::ops::curvature::curvature_at;
use crate::point::Point2d;
use crate::primitives::{CubicBezier, LineSeg};

/// Order of geometric continuity requested at both joins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Continuity {
    /// Positional only — a straight chord between the two join points.
    G0,
    /// Tangent-continuous (cubic).
    G1,
    /// Curvature-continuous (quintic).
    G2,
    /// Curvature-rate-continuous (septic).
    G3,
}

impl Continuity {
    /// The highest derivative order matched at each end (0..=3).
    pub fn order(self) -> usize {
        match self {
            Continuity::G0 => 0,
            Continuity::G1 => 1,
            Continuity::G2 => 2,
            Continuity::G3 => 3,
        }
    }

    /// Degree of the resulting Bézier, `2·order + 1`.
    pub fn degree(self) -> usize {
        2 * self.order() + 1
    }
}

/// Blends the two curves with the requested continuity.
///
/// `a_at_end` / `b_at_end` select which extremity of each curve is joined: `true`
/// for the curve's end (parameter `t1`), `false` for its start (`t0`). The blend
/// runs from curve `a`'s join to curve `b`'s join. `tension_a` / `tension_b` scale
/// the endpoint handle length on each side (`1.0` is a sensible default).
///
/// Returns `None` if the two join points coincide (no chord to span).
pub fn blend_curves(
    a: &Curve,
    a_at_end: bool,
    b: &Curve,
    b_at_end: bool,
    continuity: Continuity,
    tension_a: f64,
    tension_b: f64,
) -> Option<Curve> {
    let n = continuity.order();
    let deg = continuity.degree();

    let (ta_lo, ta_hi) = a.domain();
    let (tb_lo, tb_hi) = b.domain();
    let ta_star = if a_at_end { ta_hi } else { ta_lo };
    let tb_star = if b_at_end { tb_hi } else { tb_lo };

    let pa = pt(a.evaluate_f64(ta_star));
    let pb = pt(b.evaluate_f64(tb_star));
    let chord = pa.dist_f64(&pb);
    if chord < 1e-12 {
        return None;
    }

    // G0 is just the chord; no derivative data needed.
    if n == 0 {
        return Some(Curve::Line(LineSeg::from_endpoints(pa, pb)));
    }

    let alpha_a = tension_a * chord;
    let alpha_b = tension_b * chord;

    // Travel direction at each join, expressed as a sign on the source forward
    // tangent. Leaving curve `a`: +forward if joined at its end, else -forward.
    // Arriving into curve `b`: +forward if joined at its start, else -forward.
    let sigma_a = if a_at_end { 1.0 } else { -1.0 };
    let sigma_b = if b_at_end { -1.0 } else { 1.0 };

    let d_start = blend_derivs(a, ta_star, sigma_a, alpha_a, n);
    let d_end = blend_derivs(b, tb_star, sigma_b, alpha_b, n);

    let control = assemble_bezier(deg, &d_start, &d_end);
    Some(wrap_bezier(control))
}

/// Parametric derivatives `B^(k)(join)` of a degree-`deg` blend that match the
/// source curve's geometry to order `n`, for `k = 0..=n`.
///
/// With a constant-speed reparameterization `α`, the blend's k-th parametric
/// derivative is `αᵏ` times the source's unit-speed (arc-length) derivative:
/// `r0 = P`, `r1 = T`, `r2 = κ·N` (the curvature vector), and
/// `r3 = -κ²·T + κ'·N` (derivative of the curvature vector along arc length).
fn blend_derivs(curve: &Curve, t_star: f64, sigma: f64, alpha: f64, n: usize) -> Vec<Point2d> {
    let p = pt(curve.evaluate_f64(t_star));
    let (gx, gy) = curve.tangent_f64(t_star);
    let speed = (gx * gx + gy * gy).sqrt().max(1e-12);
    // Unit tangent in the travel direction, and its +90° (CCW) normal.
    let tx = sigma * gx / speed;
    let ty = sigma * gy / speed;
    let nx = -ty;
    let ny = tx;

    let mut out = Vec::with_capacity(n + 1);
    out.push(p); // r0 · α⁰
    if n >= 1 {
        out.push(Point2d::new(alpha * tx, alpha * ty)); // α·T
    }
    if n >= 2 {
        // Signed curvature in the travel frame: reversing travel negates it.
        let kappa = sigma * curvature_at(curve, t_star).unwrap_or(0.0);
        let a2 = alpha * alpha;
        out.push(Point2d::new(a2 * kappa * nx, a2 * kappa * ny)); // α²·κ·N
    }
    if n >= 3 {
        let kappa = sigma * curvature_at(curve, t_star).unwrap_or(0.0);
        let kprime = dkappa_ds(curve, t_star, speed); // dκ/ds, travel-direction invariant
        let a3 = alpha * alpha * alpha;
        // r3 = -κ²·T + κ'·N
        let cx = -kappa * kappa * tx + kprime * nx;
        let cy = -kappa * kappa * ty + kprime * ny;
        out.push(Point2d::new(a3 * cx, a3 * cy));
    }
    out
}

/// `dκ/ds` (curvature rate w.r.t. arc length) at `t_star`. This equals
/// `(dκ/dt) / speed` and is independent of travel direction, so it is computed
/// directly from the source's signed curvature by finite difference in `t`.
fn dkappa_ds(curve: &Curve, t_star: f64, speed: f64) -> f64 {
    let (lo, hi) = {
        let (a, b) = curve.domain();
        (a.min(b), a.max(b))
    };
    let h = ((hi - lo) * 1e-3).max(1e-9);
    // Stay inside the domain; use a one-sided step at an endpoint.
    let t_hi = (t_star + h).min(hi);
    let t_lo = (t_star - h).max(lo);
    let dt = t_hi - t_lo;
    if dt < 1e-12 {
        return 0.0;
    }
    let k_hi = curvature_at(curve, t_hi).unwrap_or(0.0);
    let k_lo = curvature_at(curve, t_lo).unwrap_or(0.0);
    (k_hi - k_lo) / dt / speed
}

/// Builds the `deg+1` Bézier control points from the start-end derivative data.
/// The first `n+1` points are fixed by the start derivatives (forward
/// differences), the last `n+1` by the end derivatives (backward differences);
/// since `deg = 2n+1` the two halves tile the control polygon exactly.
fn assemble_bezier(deg: usize, d_start: &[Point2d], d_end: &[Point2d]) -> Vec<Point2d> {
    let n = deg; // degree
    let order = d_start.len() - 1;
    let mut p = vec![Point2d::new(0.0, 0.0); n + 1];

    // Start side: ΔᵏP₀ = D_k · (n-k)!/n!  ⇒  P_k = ΔᵏP₀ − Σ_{i<k} (-1)^{k-i} C(k,i) P_i
    for k in 0..=order {
        let scale = fact(n - k) / fact(n);
        let mut acc = scl(d_start[k], scale);
        for (i, &pi) in p.iter().take(k).enumerate() {
            let c = (-1.0_f64).powi((k - i) as i32) * binom(k, i);
            acc = sub(acc, scl(pi, c));
        }
        p[k] = acc;
    }

    // End side: ∇ᵏP_n = E_k · (n-k)!/n!  ⇒
    //   P_{n-k} = (-1)^k [ ∇ᵏP_n − Σ_{i<k} (-1)^i C(k,i) P_{n-i} ]
    for k in 0..=order {
        let scale = fact(n - k) / fact(n);
        let mut acc = scl(d_end[k], scale);
        for i in 0..k {
            let c = (-1.0_f64).powi(i as i32) * binom(k, i);
            acc = sub(acc, scl(p[n - i], c));
        }
        p[n - k] = scl(acc, (-1.0_f64).powi(k as i32));
    }

    p
}

/// Wraps a control polygon in the most natural `Curve` variant for its degree.
fn wrap_bezier(control: Vec<Point2d>) -> Curve {
    match control.len() {
        2 => Curve::Line(LineSeg::from_endpoints(control[0], control[1])),
        4 => Curve::Bezier(CubicBezier::new(
            control[0], control[1], control[2], control[3],
        )),
        _ => Curve::Rational(RationalBezier::polynomial(control)),
    }
}

#[inline]
fn pt((x, y): (f64, f64)) -> Point2d {
    Point2d::new(x, y)
}
#[inline]
fn scl(p: Point2d, s: f64) -> Point2d {
    Point2d::new(p.x * s, p.y * s)
}
#[inline]
fn sub(a: Point2d, b: Point2d) -> Point2d {
    Point2d::new(a.x - b.x, a.y - b.y)
}

fn fact(n: usize) -> f64 {
    (1..=n).map(|i| i as f64).product::<f64>().max(1.0)
}

fn binom(n: usize, k: usize) -> f64 {
    fact(n) / (fact(k) * fact(n - k))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::CircularArc;

    fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Curve {
        Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(x0, y0),
            Point2d::from_f64(x1, y1),
        ))
    }

    fn unit(v: (f64, f64)) -> (f64, f64) {
        let m = (v.0 * v.0 + v.1 * v.1).sqrt();
        (v.0 / m, v.1 / m)
    }

    #[test]
    fn degrees_match_continuity() {
        assert_eq!(Continuity::G0.degree(), 1);
        assert_eq!(Continuity::G1.degree(), 3);
        assert_eq!(Continuity::G2.degree(), 5);
        assert_eq!(Continuity::G3.degree(), 7);
    }

    #[test]
    fn g0_is_the_chord() {
        let a = line(0.0, 0.0, 1.0, 0.0);
        let b = line(4.0, 3.0, 5.0, 3.0);
        let blend = blend_curves(&a, true, &b, false, Continuity::G0, 1.0, 1.0).unwrap();
        assert!(matches!(blend, Curve::Line(_)));
        let s = blend.evaluate_f64(0.0);
        let e = blend.evaluate_f64(1.0);
        assert!((s.0 - 1.0).abs() < 1e-12 && (s.1 - 0.0).abs() < 1e-12);
        assert!((e.0 - 4.0).abs() < 1e-12 && (e.1 - 3.0).abs() < 1e-12);
    }

    #[test]
    fn endpoints_land_on_the_joins_for_all_orders() {
        let a = Curve::Arc(CircularArc::new(
            Point2d::from_f64(0.0, 0.0),
            2.0,
            0.0,
            std::f64::consts::FRAC_PI_2,
        ));
        let b = line(5.0, 5.0, 8.0, 5.0);
        for c in [Continuity::G1, Continuity::G2, Continuity::G3] {
            let blend = blend_curves(&a, true, &b, false, c, 1.0, 1.0).unwrap();
            let pa = a.evaluate_f64(std::f64::consts::FRAC_PI_2);
            let pb = b.evaluate_f64(0.0);
            let s = blend.evaluate_f64(0.0);
            let e = blend.evaluate_f64(1.0);
            assert!(
                (s.0 - pa.0).abs() < 1e-9 && (s.1 - pa.1).abs() < 1e-9,
                "{c:?} start {s:?} vs {pa:?}"
            );
            assert!(
                (e.0 - pb.0).abs() < 1e-9 && (e.1 - pb.1).abs() < 1e-9,
                "{c:?} end {e:?} vs {pb:?}"
            );
        }
    }

    #[test]
    fn g1_matches_tangent_direction_at_both_joins() {
        // Curve a is a horizontal line ending at (2,0); curve b a vertical line
        // starting at (4,2). The blend must leave horizontally and arrive vertically.
        let a = line(0.0, 0.0, 2.0, 0.0);
        let b = line(4.0, 2.0, 4.0, 5.0);
        let blend = blend_curves(&a, true, &b, false, Continuity::G1, 1.0, 1.0).unwrap();

        let t0 = unit(blend.tangent_f64(0.0));
        let t1 = unit(blend.tangent_f64(1.0));
        // leaves along +x (away from a's body)
        assert!((t0.0 - 1.0).abs() < 1e-6 && t0.1.abs() < 1e-6, "t0={t0:?}");
        // arrives along +y (into b's body)
        assert!(t1.0.abs() < 1e-6 && (t1.1 - 1.0).abs() < 1e-6, "t1={t1:?}");
    }

    #[test]
    fn g2_matches_curvature_at_the_arc_join() {
        // Blend the end of a radius-2 arc into a line. At the blend's start the
        // curvature must equal the arc's curvature, 1/2.
        let a = Curve::Arc(CircularArc::new(
            Point2d::from_f64(0.0, 0.0),
            2.0,
            0.0,
            std::f64::consts::FRAC_PI_2,
        ));
        let b = line(6.0, 0.0, 9.0, 0.0);
        let blend = blend_curves(&a, true, &b, false, Continuity::G2, 1.0, 1.0).unwrap();

        // Tolerance reflects the finite-difference `curvature_at` probe, not the
        // blend, whose start curvature is 1/2 by construction.
        let k_blend = curvature_at(&blend, 0.0).unwrap();
        assert!(
            (k_blend.abs() - 0.5).abs() < 1e-3,
            "blend start curvature {k_blend}, expected ±0.5"
        );
        // The line end (curve b, κ=0) must leave the blend ~straight.
        let k_end = curvature_at(&blend, 1.0).unwrap();
        assert!(
            k_end.abs() < 1e-3,
            "blend end curvature {k_end}, expected ~0"
        );
    }

    /// Exact k-th derivative of a Bézier at t=0 via forward differences of the
    /// control polygon (algebraic, no finite-difference truncation error).
    fn deriv_at_0(control: &[Point2d], k: usize) -> Point2d {
        let n = control.len() - 1;
        let mut level: Vec<Point2d> = control.to_vec();
        for _ in 0..k {
            level = level
                .windows(2)
                .map(|w| Point2d::new(w[1].x - w[0].x, w[1].y - w[0].y))
                .collect();
        }
        let scale = fact(n) / fact(n - k);
        scl(level[0], scale)
    }

    /// Exact k-th derivative of a Bézier at t=1 via backward differences.
    fn deriv_at_1(control: &[Point2d], k: usize) -> Point2d {
        let mut rev = control.to_vec();
        rev.reverse();
        let d = deriv_at_0(&rev, k);
        // B^(k)(1) on the original = (-1)^k * (reversed curve)^(k)(0)
        scl(d, (-1.0_f64).powi(k as i32))
    }

    fn extract_control(c: &Curve) -> Vec<Point2d> {
        match c {
            Curve::Line(l) => vec![l.p0, l.p1],
            Curve::Bezier(b) => vec![b.p0, b.p1, b.p2, b.p3],
            Curve::Rational(rb) => rb.points.clone(),
            other => panic!("unexpected blend curve variant: {other:?}"),
        }
    }

    #[test]
    fn assembled_control_polygon_exactly_matches_requested_derivatives() {
        // Exercise all four join-side combinations (at_end x2) for every
        // continuity order, with two arcs of different radius/handedness, and
        // verify the *algebraic* derivatives of the assembled Bezier/Rational
        // control polygon equal the requested d_start/d_end exactly (no FD).
        let a = Curve::Arc(CircularArc::new(
            Point2d::from_f64(0.0, 0.0),
            2.0,
            0.0,
            std::f64::consts::FRAC_PI_2,
        ));
        let b = Curve::Arc(CircularArc::new(
            Point2d::from_f64(10.0, 1.0),
            1.3,
            std::f64::consts::PI,
            1.5 * std::f64::consts::PI,
        ));
        for &a_end in &[true, false] {
            for &b_end in &[true, false] {
                for c in [Continuity::G1, Continuity::G2, Continuity::G3] {
                    let n = c.order();
                    let (ta_lo, ta_hi) = a.domain();
                    let (tb_lo, tb_hi) = b.domain();
                    let ta_star = if a_end { ta_hi } else { ta_lo };
                    let tb_star = if b_end { tb_hi } else { tb_lo };
                    let pa = pt(a.evaluate_f64(ta_star));
                    let pb = pt(b.evaluate_f64(tb_star));
                    let chord = pa.dist_f64(&pb);
                    if chord < 1e-9 {
                        continue;
                    }
                    let sigma_a = if a_end { 1.0 } else { -1.0 };
                    let sigma_b = if b_end { -1.0 } else { 1.0 };
                    let d_start = blend_derivs(&a, ta_star, sigma_a, chord, n);
                    let d_end = blend_derivs(&b, tb_star, sigma_b, chord, n);

                    let blend = blend_curves(&a, a_end, &b, b_end, c, 1.0, 1.0).expect("chord > 0");
                    let control = extract_control(&blend);

                    for k in 0..=n {
                        let got0 = deriv_at_0(&control, k);
                        let got1 = deriv_at_1(&control, k);
                        let want0 = d_start[k];
                        let want1 = d_end[k];
                        assert!(
                            (got0.x - want0.x).abs() < 1e-6 && (got0.y - want0.y).abs() < 1e-6,
                            "{c:?} a_end={a_end} b_end={b_end} k={k} start: got {got0:?} want {want0:?}"
                        );
                        assert!(
                            (got1.x - want1.x).abs() < 1e-6 && (got1.y - want1.y).abs() < 1e-6,
                            "{c:?} a_end={a_end} b_end={b_end} k={k} end: got {got1:?} want {want1:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn coincident_joins_return_none() {
        let a = line(0.0, 0.0, 1.0, 1.0);
        let b = line(1.0, 1.0, 2.0, 2.0);
        // a's end and b's start are the same point.
        assert!(blend_curves(&a, true, &b, false, Continuity::G2, 1.0, 1.0).is_none());
    }

    #[test]
    fn g3_curvature_continuous_between_two_arcs() {
        // Two arcs of different radii; a G3 blend should match curvature at both
        // ends (a sufficient sanity check that the septic assembly is correct).
        let a = Curve::Arc(CircularArc::new(
            Point2d::from_f64(0.0, 0.0),
            3.0,
            0.0,
            std::f64::consts::FRAC_PI_2,
        ));
        let b = Curve::Arc(CircularArc::new(
            Point2d::from_f64(10.0, 0.0),
            1.5,
            std::f64::consts::PI,
            1.5 * std::f64::consts::PI,
        ));
        let blend = blend_curves(&a, true, &b, false, Continuity::G3, 1.0, 1.0).unwrap();
        let ks = curvature_at(&blend, 0.0).unwrap().abs();
        let ke = curvature_at(&blend, 1.0).unwrap().abs();
        assert!((ks - 1.0 / 3.0).abs() < 2e-3, "start κ {ks}, want 1/3");
        assert!((ke - 1.0 / 1.5).abs() < 2e-3, "end κ {ke}, want 1/1.5");
    }
}
