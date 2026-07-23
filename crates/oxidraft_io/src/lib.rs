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

/// Upper bound on control points accepted for a single imported curve. A
/// crafted file can declare a huge count on one line; a high-degree rational
/// Bézier is then subdivided whole (~O(n²)) and freezes rendering. No
/// legitimate drawing contains a curve remotely this dense, so counts past
/// this are rejected rather than built. Shared by the native and DXF readers.
pub(crate) const MAX_CURVE_CONTROL_POINTS: usize = 10_000;

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
