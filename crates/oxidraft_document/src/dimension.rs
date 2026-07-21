//! Dimension geometry and text: the measured value of a dimension entity, its
//! orientation, and the formatted label — shared by the renderer and exporters
//! so on-screen and exported dimensions read identically.

use crate::{DimStyle, EntityKind, Units};
use oxidraft_geometry::{Point2d, wrap_pi};

/// The arc an angular dimension spans: where it starts, how far it sweeps
/// (signed), and at what radius the dimension arc is drawn.
#[derive(Clone, Copy, Debug)]
pub struct AngularSweep {
    /// Start angle in radians.
    pub start: f64,
    /// Signed sweep in radians, chosen to include the dimension-line point.
    pub sweep: f64,
    /// Radius at which the dimension arc sits.
    pub radius: f64,
}

/// Computes the [`AngularSweep`] for an angular dimension, picking the sweep
/// direction/side that contains the dimension-line point `line`.
pub fn angular_sweep(center: Point2d, p1: Point2d, p2: Point2d, line: Point2d) -> AngularSweep {
    let (cx, cy) = center.to_f64();
    let ang = |p: Point2d| {
        let (x, y) = p.to_f64();
        (y - cy).atan2(x - cx)
    };
    let start = ang(p1);
    let mut sweep = wrap_pi(ang(p2) - start);
    let d = wrap_pi(ang(line) - start);
    let within =
        (sweep >= 0.0 && (0.0..=sweep).contains(&d)) || (sweep < 0.0 && (sweep..=0.0).contains(&d));
    if !within {
        sweep = if sweep >= 0.0 {
            sweep - std::f64::consts::TAU
        } else {
            sweep + std::f64::consts::TAU
        };
    }
    let radius = line.dist_f64(&center).max(1e-6);
    AngularSweep {
        start,
        sweep,
        radius,
    }
}

/// Whether a linear dimension reads horizontally (`Some(true)`), vertically
/// (`Some(false)`), or is ambiguous (`None`), inferred from where the
/// dimension line sits relative to the measured segment's midpoint.
pub fn linear_orientation(p1: Point2d, p2: Point2d, line: Point2d) -> Option<bool> {
    let mid = p1.midpoint(&p2);
    let (mx, my) = mid.to_f64();
    let (lx, ly) = line.to_f64();
    let (ox, oy) = ((lx - mx).abs(), (ly - my).abs());
    if ox > 2.0 * oy {
        Some(true)
    } else if oy > 2.0 * ox {
        Some(false)
    } else {
        None
    }
}

/// The raw measured value of a dimension entity (a length, angle, or radius),
/// or `None` for non-dimension kinds.
pub fn measured_value(kind: &EntityKind) -> Option<f64> {
    Some(match kind {
        EntityKind::Dimension { p1, p2, .. } => p1.dist_f64(p2),
        EntityKind::OrthoDim {
            p1, p2, vertical, ..
        } => {
            let (a, b) = (p1.to_f64(), p2.to_f64());
            if *vertical {
                (b.1 - a.1).abs()
            } else {
                (b.0 - a.0).abs()
            }
        }
        EntityKind::AngularDim {
            center,
            p1,
            p2,
            line,
            ..
        } => angular_sweep(*center, *p1, *p2, *line)
            .sweep
            .abs()
            .to_degrees(),
        EntityKind::RadialDim {
            center,
            edge,
            diameter,
            ..
        } => {
            let r = center.dist_f64(edge);
            if *diameter { 2.0 * r } else { r }
        }
        _ => return None,
    })
}

/// The text shown on a dimension: the manual `override_text` if set, otherwise
/// the measured value formatted per `style` and `units` (with the appropriate
/// unit/angle suffix). `None` for non-dimension kinds.
pub fn label_text(kind: &EntityKind, style: &DimStyle, units: Units) -> Option<String> {
    let ovr = override_text(kind);
    if let Some(t) = ovr {
        return Some(t.to_string());
    }
    let value = measured_value(kind)?;
    Some(match kind {
        EntityKind::AngularDim { .. } => format!("{value:.*}\u{00b0}", style.precision),
        EntityKind::RadialDim { diameter, .. } => {
            let prefix = if *diameter { "\u{00d8}" } else { "R" };
            format!("{prefix}{}", units.format_measure(value, style.precision))
        }
        _ => units.format_measure(value, style.precision),
    })
}

pub fn override_text(kind: &EntityKind) -> Option<&str> {
    match kind {
        EntityKind::Dimension { override_text, .. }
        | EntityKind::OrthoDim { override_text, .. }
        | EntityKind::AngularDim { override_text, .. }
        | EntityKind::RadialDim { override_text, .. } => override_text.as_deref(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point2d {
        Point2d::from_f64(x, y)
    }

    #[test]
    fn right_angle_sweep_is_90_degrees() {
        let s = angular_sweep(p(0.0, 0.0), p(10.0, 0.0), p(0.0, 10.0), p(3.0, 3.0));
        assert!((s.sweep.abs().to_degrees() - 90.0).abs() < 1e-6);
    }

    #[test]
    fn reflex_side_selects_270() {
        let s = angular_sweep(p(0.0, 0.0), p(10.0, 0.0), p(0.0, 10.0), p(-3.0, -3.0));
        assert!((s.sweep.abs().to_degrees() - 270.0).abs() < 1e-6);
    }

    #[test]
    fn label_prefers_override() {
        let kind = EntityKind::RadialDim {
            center: p(0.0, 0.0),
            edge: p(5.0, 0.0),
            diameter: true,
            height: 2.5,
            override_text: Some("custom".into()),
        };
        assert_eq!(
            label_text(&kind, &DimStyle::default(), Units::Millimeters).as_deref(),
            Some("custom")
        );
    }

    #[test]
    fn radial_label_has_prefix_and_units() {
        let kind = EntityKind::RadialDim {
            center: p(0.0, 0.0),
            edge: p(5.0, 0.0),
            diameter: false,
            height: 2.5,
            override_text: None,
        };
        let label = label_text(&kind, &DimStyle::default(), Units::Millimeters).unwrap();
        assert!(label.starts_with('R') && label.contains("mm"));
    }
}
