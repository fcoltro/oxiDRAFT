//! Small numeric helpers shared across the kernel: total angle-wrapping
//! functions (safe even for NaN/±∞ from corrupt input), point-to-segment
//! distance, and a tiny "keep the smallest" accumulator.

const TAU: f64 = std::f64::consts::TAU;
const PI: f64 = std::f64::consts::PI;

// The wrap functions are total: any input terminates in O(1), with non-finite
// angles propagating as NaN. The τ-stepping loops they replace never terminated
// for ±inf and took one iteration per turn for large-magnitude angles, so a
// single corrupt coordinate could hang the whole app.

/// Wraps an angle into (-π, π].
pub fn wrap_pi(a: f64) -> f64 {
    let w = wrap_tau(a);
    if w > PI { w - TAU } else { w }
}

/// Wraps an angle into [0, τ).
pub fn wrap_tau(a: f64) -> f64 {
    let r = a.rem_euclid(TAU);
    // rem_euclid can round up to exactly τ for tiny negative inputs.
    if r >= TAU { 0.0 } else { r }
}

/// Wraps an angle in degrees into [0, 360).
pub fn wrap_deg360(a: f64) -> f64 {
    let r = a.rem_euclid(360.0);
    if r >= 360.0 { 0.0 } else { r }
}

/// Normalizes an angular sweep into (0, τ]: zero and negative sweeps wrap into
/// the positive turn, while already-positive values (including sweeps beyond a
/// full turn) pass through unchanged.
pub fn positive_sweep(a: f64) -> f64 {
    if a > 0.0 {
        return a;
    }
    let r = wrap_tau(a);
    if r == 0.0 { TAU } else { r }
}

/// Wraps `a` into the half-open turn `[start, start + τ)` — the representative
/// of `a` that is ≥ `start`. Used to test whether an angle lies on an arc.
pub fn wrap_from(a: f64, start: f64) -> f64 {
    start + wrap_tau(a - start)
}

/// Squared distance from point `p` to the segment `a→b`, clamped to the
/// segment's endpoints (a degenerate zero-length segment measures to `a`).
pub fn point_segment_dist_sq(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len_sq = dx * dx + dy * dy;
    let t = if len_sq < 1e-20 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len_sq).clamp(0.0, 1.0)
    };
    let (fx, fy) = (a.0 + t * dx, a.1 + t * dy);
    (p.0 - fx).powi(2) + (p.1 - fy).powi(2)
}

/// Distance from point `p` to the segment `a→b` (the square root of
/// [`point_segment_dist_sq`]).
pub fn point_segment_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    point_segment_dist_sq(p, a, b).sqrt()
}

/// Accumulates the value with the smallest `f64` score seen so far.
///
/// Replaces the hand-rolled `best.as_ref().map(|(b, _)| x < *b).unwrap_or(true)`
/// idiom: callers `offer` candidates inside a loop and read the winner with
/// [`MinTracker::value`]. Ties keep the first candidate offered.
pub struct MinTracker<T> {
    best: Option<(f64, T)>,
}

impl<T> Default for MinTracker<T> {
    fn default() -> Self {
        Self { best: None }
    }
}

impl<T> MinTracker<T> {
    /// A fresh tracker holding no candidate yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Keeps `value` when `score` is strictly smaller than the current best.
    pub fn offer(&mut self, score: f64, value: T) {
        if self.best.as_ref().is_none_or(|(b, _)| score < *b) {
            self.best = Some((score, value));
        }
    }

    /// The value with the smallest score, or `None` if nothing was offered.
    pub fn value(self) -> Option<T> {
        self.best.map(|(_, v)| v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_pi_brings_into_range() {
        assert!((wrap_pi(3.0 * PI) - PI).abs() < 1e-12);
        assert!((wrap_pi(-3.0 * PI) - PI).abs() < 1e-12);
        assert!((wrap_pi(0.0)).abs() < 1e-12);
        for &a in &[-10.0, -1.0, 0.0, 1.0, 7.0, 100.0] {
            let w = wrap_pi(a);
            assert!(w > -PI - 1e-12 && w <= PI + 1e-12);
        }
    }

    #[test]
    fn wrap_tau_and_deg_are_nonnegative_in_range() {
        assert!((wrap_tau(-0.1) - (TAU - 0.1)).abs() < 1e-12);
        assert!((wrap_tau(TAU + 0.2) - 0.2).abs() < 1e-12);
        assert!((wrap_deg360(-90.0) - 270.0).abs() < 1e-9);
        assert!((wrap_deg360(450.0) - 90.0).abs() < 1e-9);
    }

    #[test]
    fn wrap_from_lands_in_start_turn() {
        let s = 1.5;
        let w = wrap_from(s - 0.3, s);
        assert!(w >= s - 1e-12 && w < s + TAU);
        assert!((w - (s + TAU - 0.3)).abs() < 1e-9);
    }

    #[test]
    fn wrap_is_total_for_hostile_angles() {
        // Formerly τ-stepping loops: ±inf never terminated and 1e300 took
        // ~1e299 iterations. Corrupt files can carry such values, so these
        // must return (NaN propagates; huge finite values wrap) instantly.
        for f in [wrap_pi, wrap_tau, wrap_deg360, positive_sweep] {
            assert!(f(f64::NAN).is_nan());
            assert!(f(f64::INFINITY).is_nan() || f(f64::INFINITY) == f64::INFINITY);
            assert!(f(f64::NEG_INFINITY).is_nan());
            assert!(f(-1e300).is_finite());
            assert!(f(1e300).is_finite() || f(1e300) == 1e300);
        }
        assert!(wrap_from(f64::NEG_INFINITY, 1.0).is_nan());
    }

    #[test]
    fn positive_sweep_matches_old_loop_semantics() {
        assert!(
            (positive_sweep(0.0) - TAU).abs() < 1e-12,
            "zero sweep is a full turn"
        );
        assert!((positive_sweep(-TAU) - TAU).abs() < 1e-12);
        assert!((positive_sweep(-PI) - PI).abs() < 1e-12);
        assert_eq!(
            positive_sweep(3.0 * TAU),
            3.0 * TAU,
            "positive sweeps pass through"
        );
        assert_eq!(positive_sweep(0.5), 0.5);
    }

    #[test]
    fn wrap_tau_tiny_negative_rounds_to_zero_not_tau() {
        let w = wrap_tau(-1e-20);
        assert!((0.0..TAU).contains(&w), "must stay in [0, τ): {w}");
    }

    #[test]
    fn point_segment_distance_basics() {
        assert!((point_segment_dist((1.0, 1.0), (0.0, 0.0), (2.0, 0.0)) - 1.0).abs() < 1e-12);
        assert!((point_segment_dist((3.0, 0.0), (0.0, 0.0), (2.0, 0.0)) - 1.0).abs() < 1e-12);
        assert!((point_segment_dist((3.0, 4.0), (0.0, 0.0), (0.0, 0.0)) - 5.0).abs() < 1e-12);
    }
}
