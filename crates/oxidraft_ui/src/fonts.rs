//! System font discovery for text-entity styling, on-demand loading of
//! chosen fonts into egui, and outlining text into [`Curve`] geometry so
//! text entities can be drawn, exported, and boolean-ops'd like any other
//! curve.

use egui::{Context, FontFamily, FontId};
use oxidraft_geometry::{CubicBezier, Curve, LineSeg, Point2d, PolyCurve, Transform2d};
use std::collections::BTreeSet;
use std::sync::OnceLock;

fn db() -> &'static fontdb::Database {
    static DB: OnceLock<fontdb::Database> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        db
    })
}

struct FaceEntry {
    label: String,
    id: fontdb::ID,
    index: u32,
}

fn weight_name(w: u16) -> &'static str {
    match w {
        0..=149 => "Thin",
        150..=249 => "ExtraLight",
        250..=349 => "Light",
        350..=449 => "Regular",
        450..=549 => "Medium",
        550..=649 => "SemiBold",
        650..=749 => "Bold",
        750..=849 => "ExtraBold",
        _ => "Black",
    }
}

fn faces() -> &'static [FaceEntry] {
    static FACES: OnceLock<Vec<FaceEntry>> = OnceLock::new();
    FACES.get_or_init(|| {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for face in db().faces() {
            let fam = match face.families.first() {
                Some((n, _)) => n.clone(),
                None => continue,
            };
            let w = weight_name(face.weight.0);
            let italic = !matches!(face.style, fontdb::Style::Normal);
            let mut label =
                if w == "Regular" || fam.to_ascii_lowercase().contains(&w.to_ascii_lowercase()) {
                    fam.clone()
                } else {
                    format!("{fam} {w}")
                };
            if italic {
                label.push_str(" Italic");
            }
            if seen.insert(label.clone()) {
                out.push(FaceEntry {
                    label,
                    id: face.id,
                    index: face.index,
                });
            }
        }
        out.sort_by(|a, b| a.label.cmp(&b.label));
        out
    })
}

/// Display labels for every distinct system font face (family + weight +
/// italic), sorted alphabetically — populates the font-choice dropdown.
pub fn system_families() -> Vec<String> {
    faces().iter().map(|f| f.label.clone()).collect()
}

/// A sensible default font label: "Arial" if installed, otherwise the first
/// available face, or `None` if no fonts were found at all.
pub fn default_family_label() -> Option<String> {
    if faces().iter().any(|f| f.label == "Arial") {
        Some("Arial".to_string())
    } else {
        faces().first().map(|f| f.label.clone())
    }
}

/// Kicks off system font enumeration on a background thread so it's already
/// cached by the time the font dropdown is first opened. Safe to call
/// repeatedly — only the first call spawns the thread.
pub fn warm() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        std::thread::spawn(|| {
            let _ = faces();
        });
    });
}

fn family_bytes(label: &str) -> Option<(Vec<u8>, u32)> {
    let fe = faces().iter().find(|f| f.label == label)?;
    db().with_face_data(fe.id, |data, _idx| (data.to_vec(), fe.index))
}

fn requested_id() -> egui::Id {
    egui::Id::new("text_fonts_requested")
}

fn initialized_id() -> egui::Id {
    egui::Id::new("fonts_initialized")
}

const NOTO_SANS: &[u8] = include_bytes!("../assets/NotoSans-Regular.ttf");

/// Makes sure every font family in `needed` (plus the bundled default) is
/// loaded into egui's font atlas, re-registering fonts only when the
/// requested set actually changes.
pub fn ensure_fonts(ctx: &Context, needed: &BTreeSet<String>) {
    let want: Vec<String> = needed.iter().cloned().collect();
    let initialized = ctx.data(|d| d.get_temp::<bool>(initialized_id()).unwrap_or(false));
    let have = ctx
        .data(|d| d.get_temp::<Vec<String>>(requested_id()))
        .unwrap_or_default();
    if initialized && want == have {
        return;
    }
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Noto Sans".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(NOTO_SANS)),
    );
    if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
        prop.insert(0, "Noto Sans".to_owned());
    }
    for family in &want {
        if let Some((bytes, index)) = family_bytes(family) {
            let mut data = egui::FontData::from_owned(bytes);
            data.index = index;
            fonts
                .font_data
                .insert(family.clone(), std::sync::Arc::new(data));
            let mut chain = vec![family.clone()];
            if let Some(defaults) = fonts.families.get(&FontFamily::Proportional) {
                chain.extend(defaults.iter().cloned());
            }
            fonts
                .families
                .insert(FontFamily::Name(family.clone().into()), chain);
        }
    }
    ctx.set_fonts(fonts);
    ctx.data_mut(|d| {
        d.insert_temp(requested_id(), want);
        d.insert_temp(initialized_id(), true);
    });
}

/// The egui [`FontId`] for rendering text in `family` at `size`, falling
/// back to the default proportional font if `family` isn't loaded.
pub fn text_font_id(ctx: &Context, family: Option<&str>, size: f32) -> FontId {
    if let Some(fam) = family {
        let target = FontFamily::Name(fam.to_owned().into());
        if ctx.fonts(|f| f.families().contains(&target)) {
            return FontId::new(size, target);
        }
    }
    FontId::proportional(size)
}

struct ContourBuilder {
    scale: f64,
    ox: f64,
    contours: Vec<Vec<Curve>>,
    cur: Vec<Curve>,
    start: (f64, f64),
    pen: (f64, f64),
}

impl ContourBuilder {
    fn map(&self, x: f32, y: f32) -> Point2d {
        Point2d::from_f64(self.ox + x as f64 * self.scale, y as f64 * self.scale)
    }
}

impl ttf_parser::OutlineBuilder for ContourBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        if !self.cur.is_empty() {
            self.contours.push(std::mem::take(&mut self.cur));
        }
        self.pen = (self.ox + x as f64 * self.scale, y as f64 * self.scale);
        self.start = self.pen;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p1 = self.map(x, y);
        self.cur.push(Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(self.pen.0, self.pen.1),
            p1,
        )));
        self.pen = (p1.x, p1.y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let p0 = Point2d::from_f64(self.pen.0, self.pen.1);
        let c = self.map(x1, y1);
        let p1 = self.map(x, y);
        let c1 = Point2d::from_f64(
            p0.x + 2.0 / 3.0 * (c.x - p0.x),
            p0.y + 2.0 / 3.0 * (c.y - p0.y),
        );
        let c2 = Point2d::from_f64(
            p1.x + 2.0 / 3.0 * (c.x - p1.x),
            p1.y + 2.0 / 3.0 * (c.y - p1.y),
        );
        self.cur
            .push(Curve::Bezier(CubicBezier::new(p0, c1, c2, p1)));
        self.pen = (p1.x, p1.y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let p0 = Point2d::from_f64(self.pen.0, self.pen.1);
        let c1 = self.map(x1, y1);
        let c2 = self.map(x2, y2);
        let p1 = self.map(x, y);
        self.cur
            .push(Curve::Bezier(CubicBezier::new(p0, c1, c2, p1)));
        self.pen = (p1.x, p1.y);
    }

    fn close(&mut self) {
        if (self.pen.0 - self.start.0).hypot(self.pen.1 - self.start.1) > 1e-9 {
            self.cur.push(Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(self.pen.0, self.pen.1),
                Point2d::from_f64(self.start.0, self.start.1),
            )));
        }
        if !self.cur.is_empty() {
            self.contours.push(std::mem::take(&mut self.cur));
        }
        self.pen = self.start;
    }
}

/// Converts `content` into curve outlines (one closed contour per glyph
/// stroke) at `height` world units, positioned at `anchor` and rotated by
/// `rotation` radians. Returns an empty vec if `family` (or any system font)
/// can't be loaded or parsed.
pub fn outline_text(
    content: &str,
    family: Option<&str>,
    height: f64,
    anchor: Point2d,
    rotation: f64,
) -> Vec<Curve> {
    let fam = family.unwrap_or("Arial");
    let (bytes, index) = match family_bytes(fam)
        .or_else(|| system_families().first().and_then(|f| family_bytes(f)))
    {
        Some(v) => v,
        None => return Vec::new(),
    };
    let face = match ttf_parser::Face::parse(&bytes, index) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let upem = face.units_per_em() as f64;
    if upem <= 0.0 {
        return Vec::new();
    }
    let scale = height / upem;
    let mut all: Vec<Vec<Curve>> = Vec::new();
    let mut cursor = 0.0_f64;
    for ch in content.chars() {
        if let Some(gid) = face.glyph_index(ch) {
            let mut b = ContourBuilder {
                scale,
                ox: cursor,
                contours: Vec::new(),
                cur: Vec::new(),
                start: (0.0, 0.0),
                pen: (0.0, 0.0),
            };
            let _ = face.outline_glyph(gid, &mut b);
            if !b.cur.is_empty() {
                b.contours.push(std::mem::take(&mut b.cur));
            }
            all.extend(b.contours);
            let adv = face.glyph_hor_advance(gid).unwrap_or(0) as f64;
            cursor += adv * scale;
        } else if ch == ' ' {
            cursor += height * 0.3;
        }
    }
    if all.is_empty() {
        return Vec::new();
    }
    let xf = Transform2d::translation(anchor.x, anchor.y).compose(&Transform2d::rotation(rotation));
    all.into_iter()
        .map(|contour| {
            let segs: Vec<Curve> = contour.iter().map(|s| xf.apply_curve(s)).collect();
            Curve::Poly(Box::new(PolyCurve::new(segs)))
        })
        .collect()
}
