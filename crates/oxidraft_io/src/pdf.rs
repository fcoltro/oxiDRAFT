use oxidraft_document::Document;

/// A page size in millimetres, independent of the drawing's own [`Units`](oxidraft_document::Units).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaperSize {
    pub width_mm: f64,
    pub height_mm: f64,
}

impl PaperSize {
    pub const fn new(width_mm: f64, height_mm: f64) -> Self {
        Self {
            width_mm,
            height_mm,
        }
    }

    /// Swaps width/height so the long edge runs horizontally.
    pub fn landscape(self) -> Self {
        Self {
            width_mm: self.width_mm.max(self.height_mm),
            height_mm: self.width_mm.min(self.height_mm),
        }
    }

    /// Swaps width/height so the long edge runs vertically (the default for
    /// every entry in [`PAPER_PRESETS`]).
    pub fn portrait(self) -> Self {
        Self {
            width_mm: self.width_mm.min(self.height_mm),
            height_mm: self.width_mm.max(self.height_mm),
        }
    }
}

/// Common ISO/ANSI paper sizes, portrait orientation, in millimetres.
pub const PAPER_PRESETS: &[(&str, PaperSize)] = &[
    ("A4", PaperSize::new(210.0, 297.0)),
    ("A3", PaperSize::new(297.0, 420.0)),
    ("Letter", PaperSize::new(215.9, 279.4)),
    ("Legal", PaperSize::new(215.9, 355.6)),
    ("Tabloid", PaperSize::new(279.4, 431.8)),
];

/// Blank margin around the fitted drawing, in millimetres.
const MARGIN_MM: f64 = 10.0;

const PT_PER_MM: f64 = 72.0 / 25.4;

fn mm_to_pt(mm: f64) -> f64 {
    mm * PT_PER_MM
}

/// Renders `doc` onto a single page of `paper`, fit-to-page (uniform scale,
/// centered, a fixed margin), and returns the PDF file bytes.
///
/// Reuses [`crate::export_svg`] for the actual geometry-to-vector-path
/// translation (every curve kind, dimensions, colors, hatch fills) — this
/// function only wraps that output in an outer page-sized frame and hands it
/// to `svg2pdf`, so any future improvement to the SVG exporter applies here
/// too without changes.
pub fn export_pdf(doc: &Document, paper: PaperSize) -> Result<Vec<u8>, String> {
    let svg = paged_svg(doc, paper);

    let mut opts = svg2pdf::usvg::Options::default();
    opts.fontdb_mut().load_system_fonts();
    let tree = svg2pdf::usvg::Tree::from_str(&svg, &opts)
        .map_err(|e| format!("could not parse the generated page SVG: {e}"))?;

    svg2pdf::to_pdf(
        &tree,
        svg2pdf::ConversionOptions::default(),
        svg2pdf::PageOptions::default(),
    )
    .map_err(|e| format!("PDF conversion failed: {e}"))
}

/// Wraps the drawing's own SVG (sized to its content extents) inside an
/// outer page-sized SVG, nested so the inner `viewBox` + `preserveAspectRatio`
/// does the fit-to-page scaling/centering — no coordinate math needed here.
///
/// SVG user units are treated as PDF points 1:1 by `svg2pdf`'s default DPI
/// (72), so page dimensions are converted from millimetres to points and
/// written as bare numbers (no `mm`/`pt` unit suffix, which would otherwise
/// get reinterpreted through usvg's CSS-pixel unit conversion).
fn paged_svg(doc: &Document, paper: PaperSize) -> String {
    let inner = crate::svg::export_svg(doc);
    let (draw_w, draw_h) = parse_viewbox_size(&inner).unwrap_or((100.0, 100.0));
    let body_start = inner.find('>').map(|i| i + 1).unwrap_or(inner.len());
    let body_end = inner.rfind("</svg>").unwrap_or(inner.len());
    let body = &inner[body_start..body_end];

    let page_w = mm_to_pt(paper.width_mm.max(1.0));
    let page_h = mm_to_pt(paper.height_mm.max(1.0));
    let margin = mm_to_pt(MARGIN_MM).min(page_w * 0.4).min(page_h * 0.4);
    let content_w = (page_w - 2.0 * margin).max(1.0);
    let content_h = (page_h - 2.0 * margin).max(1.0);

    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{page_w:.4}\" height=\"{page_h:.4}\" \
         viewBox=\"0 0 {page_w:.4} {page_h:.4}\">\n\
         <rect x=\"0\" y=\"0\" width=\"{page_w:.4}\" height=\"{page_h:.4}\" fill=\"#ffffff\"/>\n\
         <svg x=\"{margin:.4}\" y=\"{margin:.4}\" width=\"{content_w:.4}\" height=\"{content_h:.4}\" \
         viewBox=\"0 0 {draw_w:.6} {draw_h:.6}\" preserveAspectRatio=\"xMidYMid meet\">\n\
         {body}\
         </svg>\n\
         </svg>\n"
    )
}

fn parse_viewbox_size(svg: &str) -> Option<(f64, f64)> {
    let key = "viewBox=\"";
    let start = svg.find(key)? + key.len();
    let rest = &svg[start..];
    let end = rest.find('"')?;
    let nums: Vec<f64> = rest[..end]
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();
    (nums.len() == 4).then(|| (nums[2], nums[3]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxidraft_document::EntityKind;
    use oxidraft_geometry::{Curve, LineSeg, Point2d};

    fn pt(x: f64, y: f64) -> Point2d {
        Point2d::from_f64(x, y)
    }

    #[test]
    fn empty_document_still_produces_a_valid_pdf() {
        let doc = Document::new();
        let bytes = export_pdf(&doc, PaperSize::new(210.0, 297.0)).expect("export should succeed");
        assert!(
            bytes.starts_with(b"%PDF-"),
            "should start with a PDF header"
        );
    }

    #[test]
    fn a_line_produces_a_valid_pdf() {
        let mut doc = Document::new();
        doc.add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(0.0, 0.0),
            pt(100.0, 50.0),
        ))));
        let bytes = export_pdf(&doc, PaperSize::new(210.0, 297.0)).expect("export should succeed");
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.len() > 200, "a real page should be more than a stub");
    }

    #[test]
    fn custom_paper_size_is_honored_in_the_wrapper_svg() {
        let doc = Document::new();
        let svg = paged_svg(&doc, PaperSize::new(100.0, 50.0));
        let (w, h) = parse_viewbox_size(&svg).unwrap();
        // The wrapper formats coordinates to 4 decimal places, so allow for
        // that string round-trip's rounding error rather than exact equality.
        assert!((w - mm_to_pt(100.0)).abs() < 1e-3);
        assert!((h - mm_to_pt(50.0)).abs() < 1e-3);
    }

    #[test]
    fn landscape_swaps_the_long_edge_to_width() {
        let a4 = PAPER_PRESETS[0].1; // 210 x 297, portrait
        let ls = a4.landscape();
        assert!((ls.width_mm - 297.0).abs() < 1e-9);
        assert!((ls.height_mm - 210.0).abs() < 1e-9);
    }
}
