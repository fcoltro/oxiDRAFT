//! File format import/export: the native `.o2d` format ([`native`]), DXF
//! ([`dxf`]), SVG ([`svg`]), and PDF plotting ([`pdf`]). The internal `dim`
//! module holds the dimension-rendering geometry shared by the exporters.

pub(crate) mod dim;
pub mod dxf;
pub mod native;
pub mod pdf;
pub mod svg;

pub use dxf::{export_dxf, import_dxf};
pub use native::{
    from_string as from_o2d, load as load_native, save as save_native, to_string as to_o2d,
};
pub use pdf::{PAPER_PRESETS, PaperSize, PlotWindow, export_pdf, export_pdf_window};
pub use svg::{export_svg, import_svg};

use oxidraft_geometry::{Curve, CurveSegment, Point2d, tessellate_curve};

/// Upper bound on control points for a single imported *rational Bézier*.
///
/// A rational Bézier is evaluated as one piece, so `evaluate` is O(n²) in the
/// control-point count and allocates the homogeneous array every call.
/// Measured: n=1,000 costs 65 ms per tessellation and n=10,000 costs 8.4 s —
/// a 1.78 MB file of such curves took 167 s to open, and at 40 ms per
/// `evaluate` snapping and picking are unusable long before that. A real
/// drawing never exceeds degree ~10; this leaves three orders of headroom.
pub(crate) const MAX_RATIONAL_CONTROL_POINTS: usize = 256;

/// Upper bound on control points for a single imported *NURBS* curve.
///
/// Much higher than the rational cap because a NURBS decomposes into
/// fixed-degree cubic segments, so cost is linear in the control count rather
/// than quadratic — 10,000 control points measured 0.61 s versus 8.4 s for the
/// rational of the same size. Still bounded, since a crafted file can declare
/// any count it likes on one line.
pub(crate) const MAX_NURBS_CONTROL_POINTS: usize = 10_000;

// The rational cap must stay the tighter of the two — the whole point of
// separating them is that a rational's cost is quadratic where a NURBS's is
// linear. Checked at compile time so the relationship can't be inverted.
const _: () = assert!(MAX_RATIONAL_CONTROL_POINTS < MAX_NURBS_CONTROL_POINTS);

/// Writes through a temp file + fsync + rename so a crash or full disk
/// mid-write can never leave a truncated file where a good one used to be.
/// Every save of user work should go through this, whatever the format.
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut tmp_name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    // Write + fsync the temp file first. On any failure (a full disk mid-write
    // is the common one) remove the partial temp file, so a failed save can't
    // leave a stale `.tmp` littered beside the original — which stays untouched
    // until the atomic rename below succeeds.
    let write = || -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()
    };
    if let Err(e) = write() {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Tessellates `c` into a polyline fine enough for export, using a tolerance
/// scaled to the curve's own bounding box so tiny and huge curves both come
/// out smooth.
pub(crate) fn flatten_for_export(c: &Curve) -> Vec<Point2d> {
    let bb = c.bounding_box();
    let diag = ((bb.max.x - bb.min.x).powi(2) + (bb.max.y - bb.min.y).powi(2)).sqrt();
    let tol = (diag * 1e-3).max(1e-6);
    tessellate_curve(c, tol)
}
