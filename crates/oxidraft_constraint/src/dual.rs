//! Minimal forward-mode automatic differentiation for the constraint
//! residuals. Each [`Dual`] carries its value plus the exact partial
//! derivative with respect to every sketch variable (full-width gradient —
//! sketches are small, tens of variables, so there's no need for the
//! local/global index bookkeeping a sparse representation would add).
//! Exact derivatives (vs. the finite-difference approximation this replaces)
//! remove the truncation/rounding noise that would otherwise pollute
//! rank-based DOF and redundancy detection.

use std::ops::{Add, Div, Mul, Neg, Sub};

/// A dual number: a value paired with its gradient (one partial derivative per
/// sketch variable), propagated through arithmetic to differentiate residuals
/// exactly.
#[derive(Clone, Debug)]
pub struct Dual {
    /// The value.
    pub val: f64,
    /// Partial derivative with respect to each sketch variable.
    pub d: Vec<f64>,
}

impl Dual {
    /// A constant: zero derivative with respect to every variable.
    pub fn constant(val: f64, nv: usize) -> Self {
        Dual {
            val,
            d: vec![0.0; nv],
        }
    }

    /// The `idx`-th sketch variable itself: derivative 1 with respect to
    /// itself, 0 with respect to everything else.
    pub fn var(val: f64, idx: usize, nv: usize) -> Self {
        let mut d = vec![0.0; nv];
        d[idx] = 1.0;
        Dual { val, d }
    }

    /// Absolute value, with the derivative carried through the active branch
    /// (the sign of `val`).
    pub fn abs(&self) -> Dual {
        let s = if self.val >= 0.0 { 1.0 } else { -1.0 };
        Dual {
            val: self.val.abs(),
            d: self.d.iter().map(|v| v * s).collect(),
        }
    }
}

/// `sqrt(dx^2 + dy^2)`, differentiated exactly. Matches the `.max(1e-12)`
/// floor the rest of the crate uses to avoid dividing by an exact zero
/// length at a degenerate (coincident-endpoint) configuration.
pub fn hypot(dx: &Dual, dy: &Dual) -> Dual {
    let val = dx.val.hypot(dy.val);
    let denom = val.max(1e-12);
    let d =
        dx.d.iter()
            .zip(&dy.d)
            .map(|(a, b)| (dx.val * a + dy.val * b) / denom)
            .collect();
    Dual { val, d }
}

impl Add for &Dual {
    type Output = Dual;
    fn add(self, rhs: &Dual) -> Dual {
        Dual {
            val: self.val + rhs.val,
            d: self.d.iter().zip(&rhs.d).map(|(a, b)| a + b).collect(),
        }
    }
}

impl Sub for &Dual {
    type Output = Dual;
    fn sub(self, rhs: &Dual) -> Dual {
        Dual {
            val: self.val - rhs.val,
            d: self.d.iter().zip(&rhs.d).map(|(a, b)| a - b).collect(),
        }
    }
}

impl Mul for &Dual {
    type Output = Dual;
    // The product rule genuinely mixes `*` and `+`; this isn't a slip.
    #[allow(clippy::suspicious_arithmetic_impl)]
    fn mul(self, rhs: &Dual) -> Dual {
        Dual {
            val: self.val * rhs.val,
            d: self
                .d
                .iter()
                .zip(&rhs.d)
                .map(|(a, b)| a * rhs.val + self.val * b)
                .collect(),
        }
    }
}

impl Div for &Dual {
    type Output = Dual;
    fn div(self, rhs: &Dual) -> Dual {
        let denom = rhs.val * rhs.val;
        Dual {
            val: self.val / rhs.val,
            d: self
                .d
                .iter()
                .zip(&rhs.d)
                .map(|(a, b)| (a * rhs.val - self.val * b) / denom)
                .collect(),
        }
    }
}

impl Neg for &Dual {
    type Output = Dual;
    fn neg(self) -> Dual {
        Dual {
            val: -self.val,
            d: self.d.iter().map(|v| -v).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_matches_hand_derivatives() {
        // f(x, y) = x*y - x/y at (2, 3): df/dx = y - 1/y, df/dy = x + x/y^2
        let x = Dual::var(2.0, 0, 2);
        let y = Dual::var(3.0, 1, 2);
        let f = &(&x * &y) - &(&x / &y);
        assert!((f.val - (6.0 - 2.0 / 3.0)).abs() < 1e-12);
        assert!((f.d[0] - (3.0 - 1.0 / 3.0)).abs() < 1e-12);
        assert!((f.d[1] - (2.0 + 2.0 / 9.0)).abs() < 1e-12);
    }

    #[test]
    fn hypot_matches_hand_derivative() {
        // g(x, y) = hypot(x, y) at (3, 4): dg/dx = x/g, dg/dy = y/g
        let x = Dual::var(3.0, 0, 2);
        let y = Dual::var(4.0, 1, 2);
        let g = hypot(&x, &y);
        assert!((g.val - 5.0).abs() < 1e-12);
        assert!((g.d[0] - 0.6).abs() < 1e-12);
        assert!((g.d[1] - 0.8).abs() < 1e-12);
    }

    #[test]
    fn abs_flips_sign_of_gradient() {
        let x = Dual::var(-2.0, 0, 1);
        let a = x.abs();
        assert_eq!(a.val, 2.0);
        assert_eq!(a.d[0], -1.0);
    }
}
