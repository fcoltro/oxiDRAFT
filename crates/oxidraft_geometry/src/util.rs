const TAU: f64 = std::f64::consts::TAU;
const PI: f64 = std::f64::consts::PI;

pub fn wrap_pi(mut a: f64) -> f64 {
    while a <= -PI {
        a += TAU;
    }
    while a > PI {
        a -= TAU;
    }
    a
}

pub fn wrap_tau(mut a: f64) -> f64 {
    while a < 0.0 {
        a += TAU;
    }
    while a >= TAU {
        a -= TAU;
    }
    a
}

pub fn wrap_deg360(mut a: f64) -> f64 {
    while a < 0.0 {
        a += 360.0;
    }
    while a >= 360.0 {
        a -= 360.0;
    }
    a
}

pub fn wrap_from(a: f64, start: f64) -> f64 {
    start + wrap_tau(a - start)
}

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
    fn point_segment_distance_basics() {
        assert!((point_segment_dist((1.0, 1.0), (0.0, 0.0), (2.0, 0.0)) - 1.0).abs() < 1e-12);
        assert!((point_segment_dist((3.0, 0.0), (0.0, 0.0), (2.0, 0.0)) - 1.0).abs() < 1e-12);
        assert!((point_segment_dist((3.0, 4.0), (0.0, 0.0), (0.0, 0.0)) - 5.0).abs() < 1e-12);
    }
}
