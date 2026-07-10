use crate::tools::Tool;
use oxidraft_cad::ConstraintKind;

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum Command {
    Activate(Tool),
    ZoomExtents,
    ZoomScale(f64),
    Undo,
    Redo,
    Erase,
    Explode,
    Join,
    Hatch,
    Constrain(ConstraintKind),
    /// Drives the selected arcs' radius; `None` locks the current radius.
    ConstrainRadius(Option<f64>),
    /// Drives the selected lines' length; `None` locks the current length.
    ConstrainDistance(Option<f64>),
    /// Drives the angle between two selected lines (degrees); `None` locks
    /// the current angle.
    ConstrainAngle(Option<f64>),
    /// Places n−1 points at equal arc-length divisions on each selected
    /// curve; `None` means the count was missing/invalid.
    Divide(Option<u32>),
    /// Places points every given arc-length interval on each selected
    /// curve; `None` means the interval was missing/invalid.
    Measure(Option<f64>),
    Unconstrain,
    /// Pins the selected geometry in place (a driving Fix constraint).
    Fix,
    LayerSet(String),
    LayerNew(String),
    SelectAll,
    Cancel,
    Unknown(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CoordInput {
    Absolute(f64, f64),
    Relative(f64, f64),
    PolarAbsolute { dist: f64, angle_deg: f64 },
    PolarRelative { dist: f64, angle_deg: f64 },
}

/// All typed numbers come through here: `nan` is a valid f64 literal and
/// `1e999` overflows to `inf`, and a single non-finite number reaching the
/// document poisons snapping and zoom-to-fit, so the command line refuses
/// them at the parse boundary.
fn parse_finite_f64(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

pub fn parse_coordinate(input: &str) -> Option<CoordInput> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    let (relative, body) = match s.strip_prefix('@') {
        Some(rest) => (true, rest.trim()),
        None => (false, s),
    };
    if let Some((d, a)) = body.split_once('<') {
        let dist = parse_finite_f64(d)?;
        let angle_deg = parse_finite_f64(a)?;
        return Some(if relative {
            CoordInput::PolarRelative { dist, angle_deg }
        } else {
            CoordInput::PolarAbsolute { dist, angle_deg }
        });
    }
    if let Some((x, y)) = body.split_once(',') {
        let xv = parse_finite_f64(x)?;
        let yv = parse_finite_f64(y)?;
        return Some(if relative {
            CoordInput::Relative(xv, yv)
        } else {
            CoordInput::Absolute(xv, yv)
        });
    }
    None
}

pub fn parse_command(input: &str) -> Command {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Command::Cancel;
    }

    let mut parts = trimmed.split_whitespace();
    let verb = parts.next().unwrap_or("").to_ascii_uppercase();
    let rest: Vec<&str> = parts.collect();

    match verb.as_str() {
        "LINE" | "L" => Command::Activate(Tool::Line { last: None }),
        "CIRCLE" | "C" => Command::Activate(Tool::Circle { center: None }),
        "ARC" | "A" => Command::Activate(Tool::Arc3 { pts: vec![] }),
        "ARCSCE" | "ASCE" => Command::Activate(Tool::ArcStartCenterEnd {
            start: None,
            center: None,
        }),
        "ARCCSE" | "ACSE" => Command::Activate(Tool::ArcCenterStartEnd {
            center: None,
            start: None,
        }),
        "CIRCLE2P" | "C2P" => Command::Activate(Tool::CircleTwoPoint { first: None }),
        "CIRCLE3P" | "C3P" => Command::Activate(Tool::CircleThreePoint { pts: vec![] }),
        "TTR" | "CIRCLETTR" => {
            let radius = rest
                .first()
                .and_then(|s| parse_finite_f64(s))
                .filter(|r| *r > 0.0)
                .unwrap_or(1.0);
            Command::Activate(Tool::CircleTtr {
                radius,
                first: None,
            })
        }
        "TTT" | "CIRCLETTT" => Command::Activate(Tool::CircleTtt { picks: vec![] }),
        "TANGENT" | "TAN" => Command::Activate(Tool::TangentLine { first: None }),
        "DIMENSION" | "DIM" | "DIMLINEAR" | "DIMALIGNED" | "DIMHORIZONTAL" | "DIMHOR"
        | "DIMVERTICAL" | "DIMVER" => Command::Activate(Tool::Dimension { p1: None, p2: None }),
        "DIMANGULAR" | "DIMANG" | "DIMANGLE" | "DIMANGLINES" | "DIMANG2" | "DIMANGL" => {
            Command::Activate(Tool::DimAngularLines {
                a: None,
                geom: None,
            })
        }
        "DIMRADIUS" | "DIMRAD" => Command::Activate(Tool::DimRadial {
            diameter: false,
            center: None,
            radius: 0.0,
        }),
        "DIMDIAMETER" | "DIMDIA" => Command::Activate(Tool::DimRadial {
            diameter: true,
            center: None,
            radius: 0.0,
        }),
        "ELLIPSE" | "EL" => Command::Activate(Tool::Ellipse {
            center: None,
            axis_end: None,
        }),
        "RECTANGLE" | "REC" | "RECTANG" => Command::Activate(Tool::Rectangle { first: None }),
        "MOVE" | "M" => Command::Activate(Tool::Move {
            base: None,
            ids: vec![],
        }),
        "COPY" | "CO" | "CP" => Command::Activate(Tool::Copy {
            base: None,
            ids: vec![],
        }),
        "POLYGON" | "POL" => {
            let sides = rest
                .first()
                .and_then(|s| s.parse::<usize>().ok())
                .filter(|n| *n >= 3);
            Command::Activate(Tool::Polygon {
                center: None,
                radius_point: None,
                sides,
            })
        }
        "SPLINE" | "SPL" => Command::Activate(Tool::Spline { pts: vec![] }),
        "POLYLINE" | "PLINE" | "PL" => Command::Activate(Tool::Polyline { pts: vec![] }),
        "SELECT" | "SE" => Command::Activate(Tool::Select),
        "TEXT" | "T" | "DT" | "DTEXT" | "MTEXT" | "MT" => Command::Activate(Tool::Text {
            anchor: None,
            height: 2.5,
        }),
        "ROTATE" | "RO" => Command::Activate(Tool::Rotate {
            base: None,
            ids: vec![],
        }),
        "SCALE" | "SC" => Command::Activate(Tool::Scale {
            base: None,
            reference: None,
            ids: vec![],
        }),
        "MIRROR" | "MI" => Command::Activate(Tool::Mirror {
            first: None,
            ids: vec![],
        }),
        "TRIM" | "TR" => Command::Activate(Tool::Trim),
        "EXTEND" | "EX" => Command::Activate(Tool::Extend),
        "OFFSET" | "O" => {
            let dist = rest
                .first()
                .and_then(|s| parse_finite_f64(s))
                .unwrap_or(1.0);
            Command::Activate(Tool::Offset { dist, source: None })
        }
        "FILLET" | "F" => {
            let radius = rest
                .first()
                .and_then(|s| parse_finite_f64(s))
                .unwrap_or(1.0);
            Command::Activate(Tool::Fillet {
                radius,
                first: None,
            })
        }
        "CHAMFER" | "CHA" => {
            let dist = rest
                .first()
                .and_then(|s| parse_finite_f64(s))
                .unwrap_or(1.0);
            Command::Activate(Tool::Chamfer { dist, first: None })
        }
        "BLEND" | "BL" => {
            // Optional args: continuity (g0..g3) and/or tension number, any order.
            let mut continuity = oxidraft_geometry::Continuity::G1;
            let mut tension = 1.0;
            for tok in rest {
                match tok.to_ascii_uppercase().as_str() {
                    "G0" => continuity = oxidraft_geometry::Continuity::G0,
                    "G1" => continuity = oxidraft_geometry::Continuity::G1,
                    "G2" => continuity = oxidraft_geometry::Continuity::G2,
                    "G3" => continuity = oxidraft_geometry::Continuity::G3,
                    other => {
                        if let Some(v) = parse_finite_f64(other) {
                            tension = v;
                        }
                    }
                }
            }
            Command::Activate(Tool::Blend {
                continuity,
                tension,
                first: None,
                second: None,
            })
        }
        "STRETCH" | "S" => Command::Activate(Tool::Stretch {
            c1: None,
            c2: None,
            base: None,
            ids: vec![],
        }),
        "HORIZONTAL" | "HOR" => Command::Constrain(ConstraintKind::Horizontal),
        "VERTICAL" | "VER" => Command::Constrain(ConstraintKind::Vertical),
        "PARALLEL" | "PAR" => Command::Constrain(ConstraintKind::Parallel),
        "PERPENDICULAR" | "PERP" => Command::Constrain(ConstraintKind::Perpendicular),
        "EQUALLENGTH" | "EQL" => Command::Constrain(ConstraintKind::EqualLength),
        "COINCIDENT" | "COI" => Command::Constrain(ConstraintKind::Coincident),
        // TAN is the tangent-line drawing tool, so the constraint follows
        // AutoCAD's GC* naming.
        "TANCON" | "GCTAN" | "GCTANGENT" => Command::Constrain(ConstraintKind::Tangent),
        // RAD would shadow nothing, but stay consistent with the *CON
        // family. A bare RADCON locks the current radius; DIACON takes the
        // value as a diameter.
        "RADCON" | "GCRAD" | "GCRADIUS" => {
            Command::ConstrainRadius(rest.first().and_then(|v| parse_finite_f64(v)))
        }
        "DIACON" | "GCDIA" | "GCDIAMETER" => Command::ConstrainRadius(
            rest.first()
                .and_then(|v| parse_finite_f64(v))
                .map(|d| d * 0.5),
        ),
        // A bare LENCON locks the current length; LENCON <value> drives it.
        "LENCON" | "GCLEN" | "GCLENGTH" => {
            Command::ConstrainDistance(rest.first().and_then(|v| parse_finite_f64(v)))
        }
        // A bare ANGCON locks the current angle between the two selected
        // lines; ANGCON <degrees> drives it.
        "ANGCON" | "GCANG" | "GCANGLE" => {
            Command::ConstrainAngle(rest.first().and_then(|v| parse_finite_f64(v)))
        }
        "DIVIDE" | "DIV" => Command::Divide(
            rest.first()
                .and_then(|v| v.trim().parse::<u32>().ok())
                .filter(|n| *n >= 2),
        ),
        "MEASURE" | "ME" => Command::Measure(
            rest.first()
                .and_then(|v| parse_finite_f64(v))
                .filter(|d| *d > 0.0),
        ),
        "UNCONSTRAIN" | "UNCON" => Command::Unconstrain,
        // Pin the selected geometry in place.
        "FIX" | "FIXCON" | "GCFIX" | "ANCHOR" => Command::Fix,
        // Smart dimension: pick geometry to add a driving length/radius/angle.
        "DIMCON" | "SMARTDIM" | "GCDIM" | "SD" => Command::Activate(Tool::DimConstraint {
            first: None,
            pending: None,
        }),
        "ERASE" | "E" | "DELETE" => Command::Erase,
        "DISJOINT" | "EXPLODE" | "X" => Command::Explode,
        "JOIN" | "J" => Command::Join,
        "HATCH" | "H" => Command::Hatch,
        "UNDO" | "U" => Command::Undo,
        "REDO" => Command::Redo,
        "ALL" => Command::SelectAll,
        "ZOOM" | "Z" => parse_zoom(&rest),
        "LAYER" | "LA" => parse_layer(&rest),
        _ => Command::Unknown(trimmed.to_string()),
    }
}

fn parse_zoom(rest: &[&str]) -> Command {
    match rest.first().map(|s| s.to_ascii_uppercase()) {
        Some(s) if s == "E" || s == "EXTENTS" => Command::ZoomExtents,
        Some(s) => match parse_finite_f64(&s) {
            Some(scale) if scale > 0.0 => Command::ZoomScale(scale),
            _ => Command::ZoomExtents,
        },
        None => Command::ZoomExtents,
    }
}

fn parse_layer(rest: &[&str]) -> Command {
    match (rest.first().map(|s| s.to_ascii_uppercase()), rest.get(1)) {
        (Some(s), Some(name)) if s == "S" || s == "SET" => Command::LayerSet((*name).to_string()),
        (Some(s), Some(name)) if s == "N" || s == "NEW" || s == "M" || s == "MAKE" => {
            Command::LayerNew((*name).to_string())
        }
        _ => Command::Unknown("LAYER".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_drawing_commands() {
        assert!(matches!(
            parse_command("LINE"),
            Command::Activate(Tool::Line { .. })
        ));
        assert!(matches!(
            parse_command("l"),
            Command::Activate(Tool::Line { .. })
        ));
        assert!(matches!(
            parse_command("CIRCLE"),
            Command::Activate(Tool::Circle { .. })
        ));
        assert!(matches!(
            parse_command("rec"),
            Command::Activate(Tool::Rectangle { .. })
        ));
        assert!(matches!(
            parse_command("M"),
            Command::Activate(Tool::Move { .. })
        ));
        assert!(matches!(
            parse_command("POLYGON"),
            Command::Activate(Tool::Polygon { sides: None, .. })
        ));
        assert!(matches!(
            parse_command("POL 6"),
            Command::Activate(Tool::Polygon { sides: Some(6), .. })
        ));
        assert!(matches!(
            parse_command("SPLINE"),
            Command::Activate(Tool::Spline { .. })
        ));
        assert!(matches!(
            parse_command("spl"),
            Command::Activate(Tool::Spline { .. })
        ));
        assert!(matches!(
            parse_command("POLYLINE"),
            Command::Activate(Tool::Polyline { .. })
        ));
        assert!(matches!(
            parse_command("pl"),
            Command::Activate(Tool::Polyline { .. })
        ));
    }

    #[test]
    fn parses_zoom() {
        assert!(matches!(parse_command("ZOOM E"), Command::ZoomExtents));
        assert!(matches!(
            parse_command("zoom extents"),
            Command::ZoomExtents
        ));
        assert!(matches!(parse_command("Z 2.5"), Command::ZoomScale(s) if (s - 2.5).abs() < 1e-9));
        assert!(matches!(parse_command("ZOOM"), Command::ZoomExtents));
    }

    #[test]
    fn parses_layer() {
        assert!(matches!(parse_command("LAYER SET walls"), Command::LayerSet(n) if n == "walls"));
        assert!(matches!(parse_command("la new hidden"), Command::LayerNew(n) if n == "hidden"));
    }

    #[test]
    fn parses_coordinates() {
        assert_eq!(
            parse_coordinate("10,20"),
            Some(CoordInput::Absolute(10.0, 20.0))
        );
        assert_eq!(
            parse_coordinate("  3.5 , -4 "),
            Some(CoordInput::Absolute(3.5, -4.0))
        );
        assert_eq!(
            parse_coordinate("@10,20"),
            Some(CoordInput::Relative(10.0, 20.0))
        );
        assert_eq!(
            parse_coordinate("@-2.5,0"),
            Some(CoordInput::Relative(-2.5, 0.0))
        );
        assert_eq!(
            parse_coordinate("5<90"),
            Some(CoordInput::PolarAbsolute {
                dist: 5.0,
                angle_deg: 90.0
            })
        );
        assert_eq!(
            parse_coordinate("@12<45"),
            Some(CoordInput::PolarRelative {
                dist: 12.0,
                angle_deg: 45.0
            })
        );
        assert_eq!(parse_coordinate("10"), None);
        assert_eq!(parse_coordinate("LINE"), None);
        assert_eq!(parse_coordinate(""), None);
        assert_eq!(parse_coordinate("@5"), None);
        assert_eq!(parse_coordinate("a,b"), None);
    }

    #[test]
    fn non_finite_typed_numbers_are_rejected() {
        // `nan`/`inf` are valid f64 literals and `1e999` overflows to inf;
        // none of them may reach the document as geometry.
        assert_eq!(parse_coordinate("inf,0"), None);
        assert_eq!(parse_coordinate("10,nan"), None);
        assert_eq!(parse_coordinate("@1e999<45"), None);
        assert!(matches!(
            parse_command("OFFSET inf"),
            Command::Activate(Tool::Offset { dist, .. }) if dist == 1.0
        ));
        assert!(matches!(
            parse_command("FILLET nan"),
            Command::Activate(Tool::Fillet { radius, .. }) if radius == 1.0
        ));
        assert!(matches!(
            parse_command("RADCON 1e999"),
            Command::ConstrainRadius(None)
        ));
        assert!(matches!(parse_command("Z 1e999"), Command::ZoomExtents));
    }

    #[test]
    fn parses_actions_and_unknown() {
        assert!(matches!(parse_command("UNDO"), Command::Undo));
        assert!(matches!(parse_command("u"), Command::Undo));
        assert!(matches!(parse_command("ERASE"), Command::Erase));
        assert!(matches!(parse_command("EXPLODE"), Command::Explode));
        assert!(matches!(parse_command("x"), Command::Explode));
        assert!(matches!(parse_command("JOIN"), Command::Join));
        assert!(matches!(parse_command("j"), Command::Join));
        assert!(matches!(parse_command("HATCH"), Command::Hatch));
        assert!(matches!(parse_command("h"), Command::Hatch));
        assert!(matches!(
            parse_command("PERP"),
            Command::Constrain(ConstraintKind::Perpendicular)
        ));
        assert!(matches!(
            parse_command("parallel"),
            Command::Constrain(ConstraintKind::Parallel)
        ));
        assert!(matches!(
            parse_command("HOR"),
            Command::Constrain(ConstraintKind::Horizontal)
        ));
        assert!(matches!(
            parse_command("eql"),
            Command::Constrain(ConstraintKind::EqualLength)
        ));
        assert!(matches!(
            parse_command("RADCON 2.5"),
            Command::ConstrainRadius(Some(v)) if v == 2.5
        ));
        assert!(matches!(
            parse_command("radcon"),
            Command::ConstrainRadius(None)
        ));
        assert!(matches!(
            parse_command("DIACON 5"),
            Command::ConstrainRadius(Some(v)) if v == 2.5
        ));
        assert!(matches!(
            parse_command("LENCON 3.5"),
            Command::ConstrainDistance(Some(v)) if v == 3.5
        ));
        assert!(matches!(
            parse_command("lencon"),
            Command::ConstrainDistance(None)
        ));
        assert!(matches!(
            parse_command("ANGCON 33.5"),
            Command::ConstrainAngle(Some(v)) if v == 33.5
        ));
        assert!(matches!(
            parse_command("angcon"),
            Command::ConstrainAngle(None)
        ));
        assert!(matches!(
            parse_command("DIVIDE 5"),
            Command::Divide(Some(5))
        ));
        assert!(matches!(parse_command("div 1"), Command::Divide(None)));
        assert!(matches!(parse_command("divide"), Command::Divide(None)));
        assert!(matches!(
            parse_command("MEASURE 2.5"),
            Command::Measure(Some(v)) if v == 2.5
        ));
        assert!(matches!(parse_command("me -3"), Command::Measure(None)));
        assert!(matches!(
            parse_command("measure nan"),
            Command::Measure(None)
        ));
        assert!(matches!(
            parse_command("tancon"),
            Command::Constrain(ConstraintKind::Tangent)
        ));
        assert!(
            matches!(
                parse_command("TAN"),
                Command::Activate(Tool::TangentLine { .. })
            ),
            "TAN stays the tangent-line drawing tool"
        );
        assert!(matches!(parse_command("UNCON"), Command::Unconstrain));
        assert!(matches!(parse_command("ALL"), Command::SelectAll));
        assert!(matches!(parse_command(""), Command::Cancel));
        assert!(matches!(parse_command("FLERP"), Command::Unknown(_)));
    }
}
