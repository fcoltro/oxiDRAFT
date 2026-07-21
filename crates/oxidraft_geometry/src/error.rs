//! Error type for fallible geometry constructors.
//!
//! The `*::new` constructors keep their `assert!` contracts for trusted internal
//! callers that have already validated their inputs. The `try_new` variants return
//! [`GeomError`] instead, so code that ingests untrusted/CAD data can reject bad
//! geometry without crashing across an FFI or UI boundary.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GeomError {
    /// A radius that was not strictly positive.
    NonPositiveRadius(f64),
    /// Control-point and weight arrays disagreed in length.
    LengthMismatch { points: usize, weights: usize },
    /// Too few control points for the requested curve.
    TooFewPoints { got: usize, need: usize },
    /// A weight that was not strictly positive.
    NonPositiveWeight(f64),
    /// Two points that had to be distinct coincided.
    CoincidentPoints,
    /// An ellipse semi-axis that was not strictly positive.
    NonPositiveAxis(f64),
    /// A coordinate or angle that was not finite.
    NonFiniteValue,
}

impl fmt::Display for GeomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GeomError::NonPositiveRadius(r) => write!(f, "radius must be positive, got {r}"),
            GeomError::LengthMismatch { points, weights } => {
                write!(
                    f,
                    "points ({points}) and weights ({weights}) must match in length"
                )
            }
            GeomError::TooFewPoints { got, need } => {
                write!(f, "need at least {need} control points, got {got}")
            }
            GeomError::NonPositiveWeight(w) => write!(f, "weights must be positive, got {w}"),
            GeomError::CoincidentPoints => write!(f, "points must be distinct"),
            GeomError::NonPositiveAxis(a) => write!(f, "semi-axes must be positive, got {a}"),
            GeomError::NonFiniteValue => write!(f, "coordinates and angles must be finite"),
        }
    }
}

impl std::error::Error for GeomError {}
