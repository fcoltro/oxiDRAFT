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
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
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
