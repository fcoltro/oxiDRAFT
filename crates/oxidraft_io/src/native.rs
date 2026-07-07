use oxidraft_document::{
    Color, ConstraintKind, Document, Entity, EntityId, EntityKind, HatchPattern, Layer,
    LineTypeDef, LineTypeRef, LineWeight, SketchConstraint, Units,
};
use oxidraft_geometry::{
    CircularArc, CubicBezier, Curve, EllipticalArc, LineSeg, NurbsCurve, Point2d, PolyCurve,
    RationalBezier,
};
use std::fmt::Write as _;

const TAU: f64 = std::f64::consts::TAU;
pub const MAGIC: &str = "O2D";
/// The format's old name, from when the app was eiderFLAT — files written
/// before the rename to oxiDRAFT still start with this instead of [`MAGIC`].
/// Accepted on read so those files keep opening; never written.
const LEGACY_MAGIC: &str = "E2D";
pub const VERSION: u32 = 1;

pub fn to_string(doc: &Document) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "{MAGIC} {VERSION}");
    let _ = writeln!(s, "UNITS {}", units_name(doc.settings.units));
    let _ = writeln!(s, "GRID {}", doc.settings.grid_spacing);
    let _ = writeln!(s, "SNAP {}", doc.settings.snap_spacing);
    let ds = &doc.settings.dim_style;
    let _ = writeln!(
        s,
        "DIMSTYLE {} {} {} {}",
        ds.text_height,
        ds.arrow_size,
        ds.font.as_deref().map(esc).unwrap_or_else(|| "-".into()),
        ds.precision
    );

    for lt in &doc.line_types {
        let pat: Vec<String> = lt.pattern.iter().map(|p| p.to_string()).collect();
        let _ = writeln!(s, "LT {} {}", esc(&lt.name), pat.join(","));
    }
    for l in &doc.layers.layers {
        let _ = writeln!(
            s,
            "LAYER {} {},{},{} {} {} {} {}",
            esc(&l.name),
            l.color.0,
            l.color.1,
            l.color.2,
            l.on as u8,
            l.frozen as u8,
            l.locked as u8,
            esc(&linetype_name(&l.line_type))
        );
    }
    // Constraints reference entities by ordinal in the write order, because
    // entity ids are reassigned on load. Only entities that actually produce
    // a record get an ordinal — a skipped kind must not shift the ones after.
    let mut index = std::collections::HashMap::new();
    for e in doc.iter() {
        if write_entity(&mut s, e) {
            index.insert(e.id, index.len());
        }
    }
    for c in &doc.constraints {
        let Some(&ia) = index.get(&c.a) else { continue };
        match (c.b, c.pts) {
            (Some(b), Some((ea, eb))) => {
                let Some(&ib) = index.get(&b) else { continue };
                let _ = writeln!(s, "C {} {} {} {} {}", c.kind.code(), ia, ea, ib, eb);
            }
            (Some(b), None) => {
                let Some(&ib) = index.get(&b) else { continue };
                let _ = writeln!(s, "C {} {} {}", c.kind.code(), ia, ib);
            }
            (None, _) => {
                // Valued single kinds (Radius, Distance) append their value.
                match c.val {
                    Some(v) => {
                        let _ = writeln!(s, "C {} {} {}", c.kind.code(), ia, v);
                    }
                    None => {
                        let _ = writeln!(s, "C {} {}", c.kind.code(), ia);
                    }
                }
            }
        }
    }
    s
}

pub fn save(doc: &Document, path: &std::path::Path) -> std::io::Result<()> {
    crate::write_atomic(path, to_string(doc).as_bytes())
}

/// Serializes one entity record; returns false for kinds the format skips.
fn write_entity(s: &mut String, e: &Entity) -> bool {
    let layer = e.layer;
    let color = color_str(&e.color);
    let extra = format!(
        "{} {}",
        esc(&linetype_name(&e.line_type)),
        line_weight_str(&e.line_weight)
    );
    match &e.kind {
        EntityKind::Curve(Curve::Line(l)) => {
            let _ = writeln!(
                s,
                "E LINE {layer} {color} {} {} {extra}",
                pt(&l.p0),
                pt(&l.p1)
            );
        }
        EntityKind::Curve(Curve::Arc(a)) => {
            let _ = writeln!(
                s,
                "E ARC {layer} {color} {};{} {} {} {} {extra}",
                rat(a.center.x),
                rat(a.center.y),
                rat(a.radius),
                a.start_angle,
                a.end_angle
            );
        }
        EntityKind::Curve(Curve::Ellipse(el)) => {
            let _ = writeln!(
                s,
                "E ELLIPSE {layer} {color} {};{} {} {} {} {} {} {extra}",
                rat(el.center.x),
                rat(el.center.y),
                rat(el.semi_major),
                rat(el.semi_minor),
                el.rotation,
                el.start_angle,
                el.end_angle
            );
        }
        EntityKind::Curve(Curve::Bezier(b)) => {
            let _ = writeln!(
                s,
                "E BEZIER {layer} {color} {} {} {} {} {extra}",
                pt(&b.p0),
                pt(&b.p1),
                pt(&b.p2),
                pt(&b.p3)
            );
        }
        EntityKind::Curve(Curve::Rational(rb)) => {
            let _ = writeln!(
                s,
                "E RATIONAL {layer} {color} {} {extra}",
                control_fields(&rb.points, &rb.weights)
            );
        }
        EntityKind::Curve(Curve::Nurbs(nc)) => {
            let _ = writeln!(
                s,
                "E NURBS {layer} {color} {} {extra}",
                control_fields(&nc.control, &nc.weights)
            );
        }
        EntityKind::Curve(Curve::Poly(pc)) => {
            let _ = writeln!(s, "E POLY {layer} {color} {} {extra}", pc.segments.len());
            for seg in &pc.segments {
                write_segment(s, seg);
            }
        }
        EntityKind::Hatch {
            boundary,
            holes,
            fill,
            pattern,
        } => {
            let _ = writeln!(
                s,
                "E HATCH {layer} {color} {},{},{} {} {} {} {extra}",
                fill.0,
                fill.1,
                fill.2,
                boundary.len(),
                holes.len(),
                pattern_str(pattern)
            );
            for seg in boundary {
                write_segment(s, seg);
            }
            for hole in holes {
                let _ = writeln!(s, "HOLE {}", hole.len());
                for seg in hole {
                    write_segment(s, seg);
                }
            }
        }
        EntityKind::Point(p) => {
            let _ = writeln!(s, "E POINT {layer} {color} {} {extra}", pt(p));
        }
        EntityKind::Text {
            anchor,
            content,
            height,
            rotation,
            font,
        } => {
            let _ = writeln!(
                s,
                "E TEXT {layer} {color} {} {} {} {} {} {extra}",
                pt(anchor),
                height,
                rotation,
                esc(content),
                font.as_deref().map(esc).unwrap_or_else(|| "-".into())
            );
        }
        EntityKind::Dimension {
            p1,
            p2,
            line,
            height,
            override_text,
        } => {
            let _ = writeln!(
                s,
                "E DIM {layer} {color} {} {} {} {} {} {extra}",
                pt(p1),
                pt(p2),
                pt(line),
                height,
                dim_override(override_text)
            );
        }
        EntityKind::OrthoDim {
            p1,
            p2,
            line,
            vertical,
            height,
            override_text,
        } => {
            let _ = writeln!(
                s,
                "E DIMORTHO {layer} {color} {} {} {} {} {} {} {extra}",
                pt(p1),
                pt(p2),
                pt(line),
                *vertical as u8,
                height,
                dim_override(override_text)
            );
        }
        EntityKind::AngularDim {
            center,
            p1,
            p2,
            line,
            height,
            override_text,
        } => {
            let _ = writeln!(
                s,
                "E DIMANG {layer} {color} {} {} {} {} {} {} {extra}",
                pt(center),
                pt(p1),
                pt(p2),
                pt(line),
                height,
                dim_override(override_text)
            );
        }
        EntityKind::RadialDim {
            center,
            edge,
            diameter,
            height,
            override_text,
        } => {
            let _ = writeln!(
                s,
                "E DIMRAD {layer} {color} {} {} {} {} {} {extra}",
                pt(center),
                pt(edge),
                *diameter as u8,
                height,
                dim_override(override_text)
            );
        }
        _ => return false,
    }
    true
}

fn write_segment(s: &mut String, seg: &Curve) {
    match seg {
        Curve::Line(l) => {
            let _ = writeln!(s, "SEG LINE {} {}", pt(&l.p0), pt(&l.p1));
        }
        Curve::Arc(a) => {
            let _ = writeln!(
                s,
                "SEG ARC {};{} {} {} {}",
                rat(a.center.x),
                rat(a.center.y),
                rat(a.radius),
                a.start_angle,
                a.end_angle
            );
        }
        Curve::Bezier(b) => {
            let _ = writeln!(
                s,
                "SEG BEZIER {} {} {} {}",
                pt(&b.p0),
                pt(&b.p1),
                pt(&b.p2),
                pt(&b.p3)
            );
        }
        Curve::Rational(rb) => {
            let _ = writeln!(
                s,
                "SEG RATIONAL {}",
                control_fields(&rb.points, &rb.weights)
            );
        }
        Curve::Ellipse(e) => {
            let _ = writeln!(
                s,
                "SEG ELLIPSE {};{} {} {} {} {} {}",
                rat(e.center.x),
                rat(e.center.y),
                rat(e.semi_major),
                rat(e.semi_minor),
                e.rotation,
                e.start_angle,
                e.end_angle
            );
        }
        _ => s.push_str("SEG LINE 0;0 0;0\n"),
    }
}

fn control_fields(points: &[Point2d], weights: &[f64]) -> String {
    let mut out = points.len().to_string();
    for (p, w) in points.iter().zip(weights) {
        let _ = write!(out, " {} {}", pt(p), rat(*w));
    }
    out
}

pub fn from_string(text: &str) -> Result<Document, String> {
    let mut lines = text.lines().peekable();
    let header = lines.next().ok_or("empty file")?;
    let mut hp = header.split_whitespace();
    match hp.next() {
        Some(MAGIC) | Some(LEGACY_MAGIC) => {}
        _ => return Err("not an O2D file".into()),
    }
    let ver: u32 = hp
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or("missing version")?;
    if ver > VERSION {
        return Err(format!("unsupported version {}", ver));
    }

    let mut doc = Document::new();
    doc.layers.layers.clear();
    doc.line_types.clear();
    type PendingConstraint = (
        ConstraintKind,
        usize,
        Option<usize>,
        Option<(u8, u8)>,
        Option<f64>,
    );
    let mut pending_constraints: Vec<PendingConstraint> = Vec::new();
    // Constraints reference entities by ordinal in the file's write order.
    // Dropped records (corrupt values, unknown types) must still occupy their
    // slot here, or every later ordinal would shift onto the wrong entity.
    let mut entity_ids: Vec<Option<EntityId>> = Vec::new();

    while let Some(line) = lines.next() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut tok = line.split_whitespace();
        match tok.next() {
            Some("UNITS") => doc.settings.units = parse_units(tok.next().unwrap_or("")),
            Some("GRID") => doc.settings.grid_spacing = next_parse(&mut tok, 10.0),
            Some("SNAP") => doc.settings.snap_spacing = next_parse(&mut tok, 1.0),
            Some("DIMSTYLE") => {
                let ds = &mut doc.settings.dim_style;
                ds.text_height = next_parse(&mut tok, 2.5);
                ds.arrow_size = next_parse(&mut tok, 2.5);
                ds.font = parse_opt_text(&mut tok);
                ds.precision = next_parse(&mut tok, ds.precision);
            }
            Some("LT") => {
                if let Some(lt) = parse_lt(&mut tok) {
                    doc.line_types.push(lt);
                }
            }
            Some("LAYER") => {
                if let Some(l) = parse_layer(&mut tok) {
                    doc.layers.layers.push(l);
                }
            }
            Some("E") => {
                entity_ids.push(parse_entity(&mut tok, &mut lines, &mut doc));
            }
            Some("C") => {
                if let Some(kind) = tok.next().and_then(ConstraintKind::from_code)
                    && let Some(ia) = tok.next().and_then(|v| v.parse().ok())
                {
                    if kind == ConstraintKind::Coincident {
                        let ea: Option<u8> = tok.next().and_then(|v| v.parse().ok());
                        let ib: Option<usize> = tok.next().and_then(|v| v.parse().ok());
                        let eb: Option<u8> = tok.next().and_then(|v| v.parse().ok());
                        if let (Some(ea), Some(ib), Some(eb)) = (ea, ib, eb)
                            && ea <= 1
                            && eb <= 1
                        {
                            pending_constraints.push((kind, ia, Some(ib), Some((ea, eb)), None));
                        }
                    } else {
                        let ib = if kind.is_pair() {
                            tok.next().and_then(|v| v.parse().ok())
                        } else {
                            None
                        };
                        let val: Option<f64> = tok.next().and_then(|v| v.parse().ok());
                        // A valued kind without a sane value is corrupt.
                        let val_ok =
                            !kind.is_valued() || val.is_some_and(|v| v.is_finite() && v > 0.0);
                        if (!kind.is_pair() || ib.is_some()) && val_ok {
                            pending_constraints.push((kind, ia, ib, None, val));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Out-of-range references (truncated or hand-edited files) and references
    // to dropped records are discarded rather than mis-attached.
    for (kind, ia, ib, pts, val) in pending_constraints {
        let Some(&Some(a)) = entity_ids.get(ia) else {
            continue;
        };
        let b = match ib {
            Some(i) => match entity_ids.get(i) {
                Some(&Some(b)) => Some(b),
                _ => continue,
            },
            None => None,
        };
        doc.add_constraint(SketchConstraint {
            kind,
            a,
            b,
            pts,
            val,
        });
    }

    if doc.layers.layers.is_empty() {
        doc.layers.layers.push(Layer::new("0"));
    }
    Ok(doc)
}

pub fn load(path: &std::path::Path) -> std::io::Result<Document> {
    let text = std::fs::read_to_string(path)?;
    from_string(&text).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Parses one entity record; returns the id it was added under, or `None`
/// when the record is unknown or too corrupt to keep.
fn parse_entity<'a>(
    tok: &mut impl Iterator<Item = &'a str>,
    lines: &mut std::iter::Peekable<std::str::Lines>,
    doc: &mut Document,
) -> Option<EntityId> {
    let etype = tok.next()?;
    let layer: usize = next_parse(tok, 0);
    let color = parse_color(tok.next().unwrap_or("bylayer"));

    let kind = match etype {
        "LINE" => {
            let p0 = parse_pt(tok.next());
            let p1 = parse_pt(tok.next());
            Some(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
                p0, p1,
            ))))
        }
        "ARC" => {
            let c = parse_pt(tok.next());
            let r = parse_num(tok.next().unwrap_or("1"));
            let start = next_parse(tok, 0.0);
            let end = next_parse(tok, TAU);
            // A corrupt radius must drop the record, not panic the app.
            CircularArc::try_new(c, r, start, end)
                .ok()
                .map(|a| EntityKind::Curve(Curve::Arc(a)))
        }
        "ELLIPSE" => {
            let c = parse_pt(tok.next());
            let major = parse_num(tok.next().unwrap_or("1"));
            let minor = parse_num(tok.next().unwrap_or("1"));
            let rot = next_parse(tok, 0.0);
            let start = next_parse(tok, 0.0);
            let end = next_parse(tok, TAU);
            (major > 0.0 && minor > 0.0).then(|| {
                EntityKind::Curve(Curve::Ellipse(EllipticalArc::new(
                    c, major, minor, rot, start, end,
                )))
            })
        }
        "BEZIER" => {
            let p0 = parse_pt(tok.next());
            let p1 = parse_pt(tok.next());
            let p2 = parse_pt(tok.next());
            let p3 = parse_pt(tok.next());
            Some(EntityKind::Curve(Curve::Bezier(CubicBezier::new(
                p0, p1, p2, p3,
            ))))
        }
        "RATIONAL" => parse_control_data(tok)
            .map(|(p, w)| EntityKind::Curve(Curve::Rational(RationalBezier::new(p, w)))),
        "NURBS" => parse_control_data(tok)
            .map(|(c, w)| EntityKind::Curve(Curve::Nurbs(NurbsCurve::new(c, w)))),
        "POINT" => Some(EntityKind::Point(parse_pt(tok.next()))),
        "TEXT" => {
            let anchor = parse_pt(tok.next());
            let height = next_parse(tok, 1.0);
            let rotation = next_parse(tok, 0.0);
            let content = unesc(tok.next().unwrap_or(""));
            let font = parse_opt_text(tok);
            Some(EntityKind::Text {
                anchor,
                content,
                height,
                rotation,
                font,
            })
        }
        "DIM" => {
            let p1 = parse_pt(tok.next());
            let p2 = parse_pt(tok.next());
            let line = parse_pt(tok.next());
            let height = next_parse(tok, 2.5);
            let override_text = parse_opt_text(tok);
            Some(EntityKind::Dimension {
                p1,
                p2,
                line,
                height,
                override_text,
            })
        }
        "DIMORTHO" => {
            let p1 = parse_pt(tok.next());
            let p2 = parse_pt(tok.next());
            let line = parse_pt(tok.next());
            let vertical = next_flag(tok);
            let height = next_parse(tok, 2.5);
            let override_text = parse_opt_text(tok);
            Some(EntityKind::OrthoDim {
                p1,
                p2,
                line,
                vertical,
                height,
                override_text,
            })
        }
        "DIMANG" => {
            let center = parse_pt(tok.next());
            let p1 = parse_pt(tok.next());
            let p2 = parse_pt(tok.next());
            let line = parse_pt(tok.next());
            let height = next_parse(tok, 2.5);
            let override_text = parse_opt_text(tok);
            Some(EntityKind::AngularDim {
                center,
                p1,
                p2,
                line,
                height,
                override_text,
            })
        }
        "DIMRAD" => {
            let center = parse_pt(tok.next());
            let edge = parse_pt(tok.next());
            let diameter = next_flag(tok);
            let height = next_parse(tok, 2.5);
            let override_text = parse_opt_text(tok);
            Some(EntityKind::RadialDim {
                center,
                edge,
                diameter,
                height,
                override_text,
            })
        }
        "POLY" => {
            let count: usize = next_parse(tok, 0);
            let mut segs = Vec::new();
            for _ in 0..count {
                let Some(segline) = lines.next() else { break };
                if let Some(seg) = parse_segment(segline.trim()) {
                    segs.push(seg);
                }
            }
            Some(EntityKind::Curve(Curve::Poly(Box::new(PolyCurve::new(
                segs,
            )))))
        }
        "HATCH" => {
            let fill = parse_rgb_triple(tok.next().unwrap_or(""));
            let nb: usize = next_parse(tok, 0);
            let nh: usize = next_parse(tok, 0);
            let read_segs = |lines: &mut std::iter::Peekable<std::str::Lines>, n: usize| {
                let mut v = Vec::new();
                for _ in 0..n {
                    let Some(line) = lines.next() else { break };
                    if let Some(seg) = parse_segment(line.trim()) {
                        v.push(seg);
                    }
                }
                v
            };
            let pattern = parse_pattern(tok.next());
            let boundary = read_segs(lines, nb);
            let mut holes = Vec::new();
            for _ in 0..nh {
                // Stop at EOF: the declared count is untrusted, and a huge
                // value with no lines behind it must not spin the loop dry.
                let Some(line) = lines.next() else { break };
                let ns = line
                    .trim()
                    .strip_prefix("HOLE ")
                    .and_then(|c| c.trim().parse().ok())
                    .unwrap_or(0);
                holes.push(read_segs(lines, ns));
            }
            Some(EntityKind::Hatch {
                boundary,
                holes,
                fill,
                pattern,
            })
        }
        _ => None,
    };

    let line_type = parse_line_type_ref(tok.next().unwrap_or("ByLayer"));
    let line_weight = parse_line_weight(tok.next().unwrap_or("bylayer"));

    // A single NaN/inf coordinate poisons zoom-to-fit and the spatial index
    // for the whole session, so drop any entity carrying non-finite numbers —
    // the same salvage policy as out-of-range constraint references.
    let k = kind.filter(|k| k.is_finite())?;
    let id = doc.add_on_layer(k, layer.min(doc.layers.layers.len().saturating_sub(1)));
    if let Some(e) = doc.get_mut(id) {
        e.color = color;
        e.line_type = line_type;
        e.line_weight = line_weight;
    }
    Some(id)
}

fn parse_segment(line: &str) -> Option<Curve> {
    let mut tok = line.split_whitespace();
    if tok.next() != Some("SEG") {
        return None;
    }
    match tok.next()? {
        "LINE" => Some(Curve::Line(LineSeg::from_endpoints(
            parse_pt(tok.next()),
            parse_pt(tok.next()),
        ))),
        "ARC" => {
            let c = parse_pt(tok.next());
            let r = parse_num(tok.next().unwrap_or("1"));
            let start = next_parse(&mut tok, 0.0);
            let end = next_parse(&mut tok, TAU);
            CircularArc::try_new(c, r, start, end).ok().map(Curve::Arc)
        }
        "BEZIER" => Some(Curve::Bezier(CubicBezier::new(
            parse_pt(tok.next()),
            parse_pt(tok.next()),
            parse_pt(tok.next()),
            parse_pt(tok.next()),
        ))),
        "RATIONAL" => {
            parse_control_data(&mut tok).map(|(p, w)| Curve::Rational(RationalBezier::new(p, w)))
        }
        "ELLIPSE" => {
            let c = parse_pt(tok.next());
            let major = parse_num(tok.next().unwrap_or("1"));
            let minor = parse_num(tok.next().unwrap_or("1"));
            let rot = next_parse(&mut tok, 0.0);
            let start = next_parse(&mut tok, 0.0);
            let end = next_parse(&mut tok, TAU);
            (major > 0.0 && minor > 0.0).then(|| {
                Curve::Ellipse(EllipticalArc::new(c, major, minor, rot, start, end))
            })
        }
        _ => None,
    }
}

fn parse_control_data<'a, I: Iterator<Item = &'a str>>(
    tok: &mut I,
) -> Option<(Vec<Point2d>, Vec<f64>)> {
    let n: usize = tok.next().and_then(|v| v.parse().ok())?;
    let mut points = Vec::with_capacity(n.min(1024));
    let mut weights = Vec::with_capacity(n.min(1024));
    for _ in 0..n {
        let Some(p) = tok.next() else { break };
        points.push(parse_pt(Some(p)));
        weights.push(parse_num(tok.next().unwrap_or("1")));
    }
    (points.len() >= 2 && points.len() == weights.len() && weights.iter().all(|&w| w > 0.0))
        .then_some((points, weights))
}

fn rat(v: f64) -> String {
    format!("{}", v)
}
fn pt(p: &Point2d) -> String {
    format!("{};{}", rat(p.x), rat(p.y))
}

/// Pops the next token and parses it, falling back to `default` when absent or invalid.
fn next_parse<'a, T: std::str::FromStr>(tok: &mut impl Iterator<Item = &'a str>, default: T) -> T {
    tok.next().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Pops the next token and reads it as a `1`/`0` boolean flag (defaults to false).
fn next_flag<'a>(tok: &mut impl Iterator<Item = &'a str>) -> bool {
    tok.next() == Some("1")
}

fn parse_num(s: &str) -> f64 {
    if let Some((n, d)) = s.split_once('/') {
        let n: f64 = n.parse().unwrap_or(0.0);
        let d: f64 = d.parse().unwrap_or(1.0);
        if d != 0.0 { n / d } else { 0.0 }
    } else {
        s.parse().unwrap_or(0.0)
    }
}

fn parse_rgb_triple(s: &str) -> (u8, u8, u8) {
    let p: Vec<u8> = s.split(',').filter_map(|v| v.parse().ok()).collect();
    if p.len() == 3 {
        (p[0], p[1], p[2])
    } else {
        (128, 128, 128)
    }
}

fn parse_pt(s: Option<&str>) -> Point2d {
    let s = s.unwrap_or("0;0");
    let (x, y) = s.split_once(';').unwrap_or(("0", "0"));
    Point2d::new(parse_num(x), parse_num(y))
}

fn color_str(c: &Color) -> String {
    match c {
        Color::ByLayer => "bylayer".into(),
        Color::ByBlock => "byblock".into(),
        Color::Rgb(r, g, b) => format!("rgb:{}:{}:{}", r, g, b),
    }
}
fn parse_color(s: &str) -> Color {
    match s {
        "bylayer" => Color::ByLayer,
        "byblock" => Color::ByBlock,
        other => {
            if let Some(rest) = other.strip_prefix("rgb:") {
                let p: Vec<u8> = rest.split(':').filter_map(|v| v.parse().ok()).collect();
                if p.len() == 3 {
                    return Color::Rgb(p[0], p[1], p[2]);
                }
            }
            Color::ByLayer
        }
    }
}

fn parse_layer<'a>(tok: &mut impl Iterator<Item = &'a str>) -> Option<Layer> {
    let name = unesc(tok.next()?);
    let rgb: Vec<u8> = tok
        .next()?
        .split(',')
        .filter_map(|v| v.parse().ok())
        .collect();
    let on = tok.next()? == "1";
    let frozen = tok.next()? == "1";
    let locked = tok.next()? == "1";
    let lt = unesc(tok.next().unwrap_or("Continuous"));
    let mut l = Layer::new(name);
    if rgb.len() == 3 {
        l.color = (rgb[0], rgb[1], rgb[2]);
    }
    l.on = on;
    l.frozen = frozen;
    l.locked = locked;
    l.line_type = LineTypeRef::Named(lt);
    Some(l)
}

fn parse_lt<'a>(tok: &mut impl Iterator<Item = &'a str>) -> Option<LineTypeDef> {
    let name = unesc(tok.next()?);
    let pat: Vec<f64> = tok
        .next()
        .map(|s| s.split(',').filter_map(|v| v.parse().ok()).collect())
        .unwrap_or_default();
    Some(LineTypeDef {
        name,
        description: String::new(),
        pattern: pat,
    })
}

fn linetype_name(lt: &LineTypeRef) -> String {
    match lt {
        LineTypeRef::Named(n) => n.clone(),
        LineTypeRef::ByLayer => "ByLayer".into(),
        LineTypeRef::ByBlock => "ByBlock".into(),
    }
}

fn parse_line_type_ref(s: &str) -> LineTypeRef {
    match s {
        "ByLayer" => LineTypeRef::ByLayer,
        "ByBlock" => LineTypeRef::ByBlock,
        other => LineTypeRef::Named(unesc(other)),
    }
}

fn line_weight_str(w: &LineWeight) -> String {
    match w {
        LineWeight::ByLayer => "bylayer".into(),
        LineWeight::ByBlock => "byblock".into(),
        LineWeight::Hundredths(h) => format!("h:{h}"),
    }
}

fn parse_line_weight(s: &str) -> LineWeight {
    match s {
        "byblock" => LineWeight::ByBlock,
        other => other
            .strip_prefix("h:")
            .and_then(|v| v.parse().ok())
            .map(LineWeight::Hundredths)
            .unwrap_or(LineWeight::ByLayer),
    }
}

fn units_name(u: Units) -> &'static str {
    match u {
        Units::Unitless => "Unitless",
        Units::Millimeters => "Millimeters",
        Units::Centimeters => "Centimeters",
        Units::Meters => "Meters",
        Units::Kilometers => "Kilometers",
        Units::Inches => "Inches",
        Units::Feet => "Feet",
    }
}
fn parse_units(s: &str) -> Units {
    match s {
        "Centimeters" => Units::Centimeters,
        "Meters" => Units::Meters,
        "Kilometers" => Units::Kilometers,
        "Inches" => Units::Inches,
        "Feet" => Units::Feet,
        "Unitless" => Units::Unitless,
        _ => Units::Millimeters,
    }
}

fn pattern_str(p: &HatchPattern) -> String {
    match p {
        HatchPattern::Solid => "solid".into(),
        HatchPattern::Lines { angle_deg, spacing } => format!("lines:{angle_deg}:{spacing}"),
        HatchPattern::Cross { angle_deg, spacing } => format!("cross:{angle_deg}:{spacing}"),
        HatchPattern::Dots { spacing } => format!("dots:{spacing}"),
    }
}

fn parse_pattern(tok: Option<&str>) -> HatchPattern {
    let Some(s) = tok else {
        return HatchPattern::Solid;
    };
    let mut it = s.split(':');
    match it.next() {
        Some("lines") => HatchPattern::Lines {
            angle_deg: next_parse(&mut it, 45.0),
            spacing: next_parse(&mut it, 1.0),
        },
        Some("cross") => HatchPattern::Cross {
            angle_deg: next_parse(&mut it, 45.0),
            spacing: next_parse(&mut it, 1.0),
        },
        Some("dots") => HatchPattern::Dots {
            spacing: next_parse(&mut it, 1.0),
        },
        _ => HatchPattern::Solid,
    }
}

fn esc(s: &str) -> String {
    if s.is_empty() {
        return "_".into();
    }
    s.replace('\\', "\\\\").replace(' ', "\\s")
}

fn dim_override(o: &Option<String>) -> String {
    match o {
        None => "-".into(),
        Some(t) => esc(t),
    }
}

/// Reads an optional escaped string written as `-` when absent (dim overrides, fonts).
fn parse_opt_text<'a>(tok: &mut impl Iterator<Item = &'a str>) -> Option<String> {
    match tok.next() {
        None | Some("-") => None,
        Some(t) => Some(unesc(t)),
    }
}
fn unesc(s: &str) -> String {
    if s == "_" {
        return String::new();
    }
    s.replace("\\s", " ").replace("\\\\", "\\")
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxidraft_document::{EntityKind, HatchPattern};

    fn pt_i(x: i64, y: i64) -> Point2d {
        Point2d::from_i64(x, y)
    }

    #[test]
    fn roundtrip_f64_is_lossless() {
        let mut doc = Document::new();
        let third = 1.0 / 3.0;
        doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            Point2d::new(third, 0.0),
            Point2d::new(2.0, third),
        ))));

        let text = to_string(&doc);
        let doc2 = from_string(&text).unwrap();
        let es: Vec<_> = doc2.iter().collect();
        if let Some(Curve::Line(l)) = es[0].as_curve() {
            assert_eq!(l.p0.x, third);
            assert_eq!(l.p1.y, third);
        } else {
            panic!()
        }
    }

    #[test]
    fn roundtrip_constraints_remap_entity_ids() {
        let mut doc = Document::new();
        let a = doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt_i(0, 0),
            pt_i(4, 0),
        ))));
        let b = doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt_i(0, 1),
            pt_i(4, 2),
        ))));
        doc.add_constraint(SketchConstraint::single(ConstraintKind::Horizontal, a));
        doc.add_constraint(SketchConstraint::pair(ConstraintKind::Parallel, a, b));
        // Force reloaded ids to differ from the originals.
        let gap = doc.add(EntityKind::Point(pt_i(9, 9)));
        doc.remove(gap);

        let doc2 = from_string(&to_string(&doc)).unwrap();
        assert_eq!(doc2.constraints.len(), 2);
        let ids: Vec<_> = doc2.iter().map(|e| e.id).collect();
        assert_eq!(doc2.constraints[0].kind, ConstraintKind::Horizontal);
        assert_eq!(doc2.constraints[0].a, ids[0]);
        assert_eq!(doc2.constraints[0].b, None);
        assert_eq!(doc2.constraints[1].kind, ConstraintKind::Parallel);
        assert_eq!(doc2.constraints[1].a, ids[0]);
        assert_eq!(doc2.constraints[1].b, Some(ids[1]));
    }

    #[test]
    fn roundtrip_tangent_between_line_and_arc() {
        let mut doc = Document::new();
        let l = doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt_i(-4, 2),
            pt_i(4, 2),
        ))));
        let a = doc.add(EntityKind::Curve(Curve::Arc(
            oxidraft_geometry::CircularArc::new(pt_i(0, 0), 2.0, 0.0, std::f64::consts::TAU),
        )));
        doc.add_constraint(SketchConstraint::pair(ConstraintKind::Tangent, l, a));

        let doc2 = from_string(&to_string(&doc)).unwrap();
        assert_eq!(doc2.constraints.len(), 1);
        let c = doc2.constraints[0];
        assert_eq!(c.kind, ConstraintKind::Tangent);
        let ids: Vec<_> = doc2.iter().map(|e| e.id).collect();
        assert_eq!((c.a, c.b, c.pts), (ids[0], Some(ids[1]), None));
    }

    #[test]
    fn roundtrip_radius_keeps_the_driving_value() {
        let mut doc = Document::new();
        let a = doc.add(EntityKind::Curve(Curve::Arc(
            oxidraft_geometry::CircularArc::new(pt_i(0, 0), 2.5, 0.0, std::f64::consts::TAU),
        )));
        doc.add_constraint(SketchConstraint::radius(a, 2.5));

        let doc2 = from_string(&to_string(&doc)).unwrap();
        assert_eq!(doc2.constraints.len(), 1);
        let c = doc2.constraints[0];
        assert_eq!(c.kind, ConstraintKind::Radius);
        assert_eq!(c.val, Some(2.5));
        let ids: Vec<_> = doc2.iter().map(|e| e.id).collect();
        assert_eq!((c.a, c.b), (ids[0], None));
    }

    #[test]
    fn roundtrip_distance_keeps_the_driving_value() {
        let mut doc = Document::new();
        let a = doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt_i(0, 0),
            pt_i(4, 0),
        ))));
        doc.add_constraint(SketchConstraint::distance(a, 4.0));

        let doc2 = from_string(&to_string(&doc)).unwrap();
        assert_eq!(doc2.constraints.len(), 1);
        let c = doc2.constraints[0];
        assert_eq!(c.kind, ConstraintKind::Distance);
        assert_eq!(c.val, Some(4.0));
        let ids: Vec<_> = doc2.iter().map(|e| e.id).collect();
        assert_eq!((c.a, c.b), (ids[0], None));
    }

    #[test]
    fn radius_records_without_a_sane_value_are_dropped() {
        let mut doc = Document::new();
        doc.add(EntityKind::Curve(Curve::Arc(
            oxidraft_geometry::CircularArc::new(pt_i(0, 0), 2.0, 0.0, std::f64::consts::TAU),
        )));
        let base = to_string(&doc);
        for bad in ["C RAD 0", "C RAD 0 -1", "C RAD 0 nope", "C RAD 0 inf"] {
            let doc2 = from_string(&format!("{base}{bad}\n")).unwrap();
            assert_eq!(doc2.len(), 1, "the arc itself still loads");
            assert!(
                doc2.constraints.is_empty(),
                "corrupt radius record {bad:?} must be dropped"
            );
        }
    }

    #[test]
    fn roundtrip_coincident_keeps_endpoint_indices() {
        let mut doc = Document::new();
        let a = doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt_i(0, 0),
            pt_i(4, 0),
        ))));
        let b = doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt_i(4, 0),
            pt_i(4, 3),
        ))));
        doc.add_constraint(SketchConstraint::coincident(a, 1, b, 0));

        let doc2 = from_string(&to_string(&doc)).unwrap();
        assert_eq!(doc2.constraints.len(), 1);
        let c = doc2.constraints[0];
        assert_eq!(c.kind, ConstraintKind::Coincident);
        assert_eq!(c.pts, Some((1, 0)));
        let ids: Vec<_> = doc2.iter().map(|e| e.id).collect();
        assert_eq!(c.a, ids[0]);
        assert_eq!(c.b, Some(ids[1]));
    }

    #[test]
    fn constraints_with_bad_ordinals_are_dropped() {
        let text =
            "O2D 1\nE LINE 0 bylayer 0;0 4;0 ByLayer bylayer\nC H 0\nC PAR 0 7\nC EQL 1\nC WAT 0\n";
        let doc = from_string(text).unwrap();
        assert_eq!(doc.constraints.len(), 1, "only the valid H survives");
        assert_eq!(doc.constraints[0].kind, ConstraintKind::Horizontal);
    }

    #[test]
    fn roundtrip_layers_and_settings() {
        let mut doc = Document::new();
        doc.settings.units = Units::Inches;
        doc.layers.add(Layer::new("walls").with_color(255, 0, 0));
        let mut frozen = Layer::new("hidden");
        frozen.frozen = true;
        doc.layers.add(frozen);

        let doc2 = from_string(&to_string(&doc)).unwrap();
        assert_eq!(doc2.settings.units, Units::Inches);
        let w = doc2
            .layers
            .get(doc2.layers.index_of("walls").unwrap())
            .unwrap();
        assert_eq!(w.color, (255, 0, 0));
        assert!(
            doc2.layers
                .get(doc2.layers.index_of("hidden").unwrap())
                .unwrap()
                .frozen
        );
    }

    #[test]
    fn roundtrip_all_entity_types() {
        let mut doc = Document::new();
        doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt_i(0, 0),
            pt_i(5, 5),
        ))));
        doc.add(EntityKind::Curve(Curve::Arc(CircularArc::new(
            pt_i(3, 4),
            5.0,
            0.0,
            TAU,
        ))));
        doc.add(EntityKind::Curve(Curve::Bezier(CubicBezier::new(
            pt_i(0, 0),
            pt_i(1, 2),
            pt_i(3, 2),
            pt_i(4, 0),
        ))));
        doc.add(EntityKind::Point(pt_i(7, 8)));
        doc.add(EntityKind::Text {
            anchor: pt_i(1, 1),
            content: "hello world".into(),
            height: 2.5,
            rotation: 0.0,
            font: Some("Arial".into()),
        });

        let doc2 = from_string(&to_string(&doc)).unwrap();
        assert_eq!(doc2.len(), 5);
        let has_text = doc2.iter().any(
            |e| matches!(&e.kind, EntityKind::Text { content, .. } if content == "hello world"),
        );
        assert!(has_text);
    }

    #[test]
    fn roundtrip_entity_line_type_and_line_weight() {
        let mut doc = Document::new();
        let id = doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt_i(0, 0),
            pt_i(5, 5),
        ))));
        {
            let e = doc.get_mut(id).unwrap();
            e.line_type = LineTypeRef::Named("Dashed".into());
            e.line_weight = LineWeight::Hundredths(50);
        }
        let doc2 = from_string(&to_string(&doc)).unwrap();
        let e2 = doc2.get(id).unwrap();
        assert_eq!(e2.line_type, LineTypeRef::Named("Dashed".into()));
        assert_eq!(e2.line_weight, LineWeight::Hundredths(50));
    }

    #[test]
    fn old_format_line_without_trailing_fields_defaults_to_bylayer() {
        let dxf_free = "O2D 1\nLAYER 0 0,0,0 1 0 0 Continuous\nE LINE 0 bylayer 0;0 5;5\n";
        let doc = from_string(dxf_free).unwrap();
        let e = doc.iter().next().unwrap();
        assert_eq!(e.line_type, LineTypeRef::ByLayer);
        assert_eq!(e.line_weight, LineWeight::ByLayer);
    }

    #[test]
    fn roundtrip_dim_style() {
        let mut doc = Document::new();
        doc.settings.dim_style.text_height = 3.5;
        doc.settings.dim_style.arrow_size = 1.25;
        doc.settings.dim_style.font = Some("Arial".into());
        let doc2 = from_string(&to_string(&doc)).unwrap();
        assert!((doc2.settings.dim_style.text_height - 3.5).abs() < 1e-9);
        assert!((doc2.settings.dim_style.arrow_size - 1.25).abs() < 1e-9);
        assert_eq!(doc2.settings.dim_style.font.as_deref(), Some("Arial"));
    }

    #[test]
    fn roundtrip_dimension() {
        let mut doc = Document::new();
        doc.add(EntityKind::Dimension {
            p1: pt_i(0, 0),
            p2: pt_i(10, 0),
            line: pt_i(0, 3),
            height: 2.5,
            override_text: None,
        });
        let doc2 = from_string(&to_string(&doc)).unwrap();
        let e = doc2.iter().next().expect("one entity");
        match &e.kind {
            EntityKind::Dimension {
                p1,
                p2,
                line,
                height,
                ..
            } => {
                assert_eq!(*p1, pt_i(0, 0));
                assert_eq!(*p2, pt_i(10, 0));
                assert_eq!(*line, pt_i(0, 3));
                assert!((*height - 2.5).abs() < 1e-9);
            }
            other => panic!("expected a Dimension, got {other:?}"),
        }
    }

    #[test]
    fn roundtrip_angular_and_radial_dimensions() {
        let mut doc = Document::new();
        doc.add(EntityKind::AngularDim {
            center: pt_i(0, 0),
            p1: pt_i(10, 0),
            p2: pt_i(0, 10),
            line: pt_i(5, 5),
            height: 2.5,
            override_text: None,
        });
        doc.add(EntityKind::RadialDim {
            center: pt_i(3, 4),
            edge: pt_i(8, 4),
            diameter: true,
            height: 2.5,
            override_text: None,
        });
        let doc2 = from_string(&to_string(&doc)).unwrap();
        assert_eq!(doc2.len(), 2);
        let ang = doc2.iter().any(
            |e| matches!(&e.kind, EntityKind::AngularDim { center, .. } if *center == pt_i(0, 0)),
        );
        let rad = doc2.iter().any(
            |e| matches!(&e.kind, EntityKind::RadialDim { diameter, edge, .. } if *diameter && *edge == pt_i(8, 4)),
        );
        assert!(ang, "angular dimension survived the round-trip");
        assert!(rad, "radial (diameter) dimension survived the round-trip");
    }

    #[test]
    fn roundtrip_dimension_text_override() {
        let mut doc = Document::new();
        doc.add(EntityKind::Dimension {
            p1: pt_i(0, 0),
            p2: pt_i(10, 0),
            line: pt_i(0, 3),
            height: 2.5,
            override_text: Some("≈ 10 cm".into()),
        });
        let doc2 = from_string(&to_string(&doc)).unwrap();
        let ovr = doc2.iter().find_map(|e| match &e.kind {
            EntityKind::Dimension { override_text, .. } => override_text.clone(),
            _ => None,
        });
        assert_eq!(
            ovr.as_deref(),
            Some("≈ 10 cm"),
            "override text survives with spaces"
        );
    }

    #[test]
    fn roundtrip_dimstyle_precision() {
        let mut doc = Document::new();
        doc.settings.dim_style.precision = 4;
        let doc2 = from_string(&to_string(&doc)).unwrap();
        assert_eq!(doc2.settings.dim_style.precision, 4);
    }

    #[test]
    fn roundtrip_rational_is_lossless() {
        let mut doc = Document::new();
        let rb = RationalBezier::new(
            vec![pt_i(0, 0), pt_i(2, 4), pt_i(6, 4), pt_i(8, 0)],
            vec![1.0, 2.0, 0.5, 1.0],
        );
        doc.add(EntityKind::Curve(Curve::Rational(rb.clone())));

        let doc2 = from_string(&to_string(&doc)).unwrap();
        let e = doc2.iter().next().expect("one entity");
        if let EntityKind::Curve(Curve::Rational(r2)) = &e.kind {
            assert_eq!(r2.points, rb.points, "control points must survive exactly");
            assert_eq!(r2.weights, rb.weights, "weights must survive exactly");
        } else {
            panic!("expected a Rational curve after round-trip");
        }
    }

    #[test]
    fn roundtrip_polycurve_of_rational_segments() {
        let seg = || {
            RationalBezier::new(
                vec![pt_i(0, 0), pt_i(1, 2), pt_i(3, 2), pt_i(4, 0)],
                vec![1.0, 1.0, 1.0, 1.0],
            )
        };
        let mut doc = Document::new();
        doc.add(EntityKind::Curve(Curve::Poly(Box::new(PolyCurve::new(
            vec![Curve::Rational(seg()), Curve::Rational(seg())],
        )))));

        let doc2 = from_string(&to_string(&doc)).unwrap();
        let e = doc2.iter().next().expect("one entity");
        if let EntityKind::Curve(Curve::Poly(pc)) = &e.kind {
            assert_eq!(pc.segments.len(), 2);
            assert!(pc.segments.iter().all(|s| matches!(s, Curve::Rational(_))));
        } else {
            panic!("expected a PolyCurve of rational segments");
        }
    }

    #[test]
    fn roundtrip_nurbs_is_lossless() {
        let mut doc = Document::new();
        let nc = NurbsCurve::new(
            vec![pt_i(0, 0), pt_i(2, 5), pt_i(6, 5), pt_i(9, 0), pt_i(12, 4)],
            vec![1.0, 2.0, 0.5, 1.0, 3.0],
        );
        doc.add(EntityKind::Curve(Curve::Nurbs(nc.clone())));

        let doc2 = from_string(&to_string(&doc)).unwrap();
        let e = doc2.iter().next().expect("one entity");
        if let EntityKind::Curve(Curve::Nurbs(n2)) = &e.kind {
            assert_eq!(
                n2.control, nc.control,
                "control vertices must survive exactly"
            );
            assert_eq!(n2.weights, nc.weights, "weights must survive exactly");
        } else {
            panic!("expected a NURBS curve after round-trip");
        }
    }

    #[test]
    fn roundtrip_polycurve() {
        let mut doc = Document::new();
        let segs = vec![
            Curve::Line(LineSeg::from_endpoints(pt_i(0, 0), pt_i(4, 0))),
            Curve::Arc(CircularArc::new(
                pt_i(4, 2),
                2.0,
                -std::f64::consts::FRAC_PI_2,
                std::f64::consts::FRAC_PI_2,
            )),
        ];
        doc.add(EntityKind::Curve(Curve::Poly(Box::new(PolyCurve::new(
            segs,
        )))));
        let doc2 = from_string(&to_string(&doc)).unwrap();
        let es: Vec<_> = doc2.iter().collect();
        if let Some(Curve::Poly(pc)) = es[0].as_curve() {
            assert_eq!(pc.segments.len(), 2);
        } else {
            panic!()
        }
    }

    #[test]
    fn roundtrip_hatch() {
        let mut doc = Document::new();
        doc.add(EntityKind::Hatch {
            boundary: vec![
                Curve::Line(LineSeg::from_endpoints(pt_i(0, 0), pt_i(6, 0))),
                Curve::Line(LineSeg::from_endpoints(pt_i(6, 0), pt_i(6, 6))),
                Curve::Line(LineSeg::from_endpoints(pt_i(6, 6), pt_i(0, 6))),
                Curve::Line(LineSeg::from_endpoints(pt_i(0, 6), pt_i(0, 0))),
            ],
            holes: vec![vec![
                Curve::Line(LineSeg::from_endpoints(pt_i(2, 2), pt_i(4, 2))),
                Curve::Line(LineSeg::from_endpoints(pt_i(4, 2), pt_i(4, 4))),
                Curve::Line(LineSeg::from_endpoints(pt_i(4, 4), pt_i(2, 4))),
                Curve::Line(LineSeg::from_endpoints(pt_i(2, 4), pt_i(2, 2))),
            ]],
            fill: (10, 200, 150),
            pattern: HatchPattern::Lines {
                angle_deg: 30.0,
                spacing: 1.5,
            },
        });
        let doc2 = from_string(&to_string(&doc)).unwrap();
        let es: Vec<_> = doc2.iter().collect();
        match &es[0].kind {
            EntityKind::Hatch {
                boundary,
                holes,
                fill,
                pattern,
            } => {
                assert_eq!(boundary.len(), 4);
                assert_eq!(holes.len(), 1);
                assert_eq!(holes[0].len(), 4);
                assert_eq!(*fill, (10, 200, 150));
                assert_eq!(
                    *pattern,
                    HatchPattern::Lines {
                        angle_deg: 30.0,
                        spacing: 1.5
                    }
                );
            }
            other => panic!("expected Hatch, got {other:?}"),
        }
    }

    #[test]
    fn corrupt_counts_do_not_hang_or_oom() {
        let nurbs = format!(
            "{} {}\nE NURBS 0 bylayer 100000000 0;0 1 5;5 1\n",
            MAGIC, VERSION
        );
        let doc = from_string(&nurbs).expect("loads without hanging");
        assert_eq!(doc.len(), 1);

        let poly = format!(
            "{} {}\nE POLY 0 bylayer 100000000\nSEG LINE 0;0 4;0\n",
            MAGIC, VERSION
        );
        let doc = from_string(&poly).expect("loads without hanging");
        assert_eq!(doc.len(), 1);
    }

    #[test]
    fn rejects_bad_header() {
        assert!(from_string("NOPE 1\n").is_err());
        assert!(from_string("").is_err());
    }

    #[test]
    fn accepts_legacy_e2d_magic() {
        // Files saved before the rename to oxiDRAFT (extension .e2d, magic
        // "E2D") must keep opening under the new "O2D" magic/.o2d extension.
        let doc = from_string("E2D 1\nE LINE 0 bylayer 0;0 4;0 ByLayer bylayer\n")
            .expect("legacy E2D header should still load");
        assert_eq!(doc.len(), 1);
    }

    #[test]
    fn save_and_load_file_atomic() {
        let mut doc = Document::new();
        doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt_i(1, 2),
            pt_i(3, 4),
        ))));
        let dir = std::env::temp_dir();
        let path = dir.join("oxidraft_native_test.o2d");
        save(&doc, &path).unwrap();
        assert!(path.exists());
        let doc2 = load(&path).unwrap();
        assert_eq!(doc2.len(), 1);
        let _ = std::fs::remove_file(&path);
    }
}
