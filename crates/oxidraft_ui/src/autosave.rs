//! Crash protection: while the document has unsaved changes, a recovery copy
//! is written every [`AUTOSAVE_INTERVAL`]. The file is deleted on every
//! successful manual save and when the user explicitly discards changes at
//! exit — so a recovery file found at startup means the last session ended
//! without either, i.e. a crash or kill, and the UI offers to restore it.

use crate::state::AppState;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(30);

/// Lives next to the executable like the crash log; temp dir as fallback.
pub fn recovery_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(std::env::temp_dir)
        .join("oxidraft_recovery.o2d")
}

/// Called once per frame; writes at most every [`AUTOSAVE_INTERVAL`], and
/// only while unsaved changes exist.
pub fn tick(app: &AppState, last: &mut Option<Instant>) {
    let due = last
        .map(|t| t.elapsed() >= AUTOSAVE_INTERVAL)
        .unwrap_or(true);
    if !due {
        return;
    }
    *last = Some(Instant::now());
    if !app.is_dirty() {
        return;
    }
    let mut doc = app.document.clone();
    doc.remove(app.origin_id);
    let _ = oxidraft_io::save_native(&doc, &recovery_path());
}

pub fn discard_recovery() {
    let _ = std::fs::remove_file(recovery_path());
}

/// A leftover recovery file with content means the previous session did not
/// end with a save or an explicit discard.
pub fn pending_recovery() -> Option<PathBuf> {
    let path = recovery_path();
    match std::fs::metadata(&path) {
        Ok(m) if m.len() > 0 => Some(path),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxidraft_document::EntityKind;
    use oxidraft_geometry::{Curve, LineSeg, Point2d};

    // One test only: the recovery file is a shared path, and parallel tests
    // in this process would race on it.
    #[test]
    fn autosave_and_restore_round_trip() {
        discard_recovery();
        let mut app = AppState::new(800.0, 600.0);
        app.history.snapshot(&app.document);
        app.document
            .add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(1.0, 2.0),
                Point2d::from_f64(7.0, 5.0),
            ))));
        assert!(app.is_dirty(), "an unsaved edit must read as dirty");

        // First tick has no timestamp yet, so it writes immediately.
        let mut last = None;
        tick(&app, &mut last);
        let path = pending_recovery().expect("dirty tick must leave a recovery file");

        let mut fresh = AppState::new(800.0, 600.0);
        assert!(fresh.restore_recovery(&path), "restore must load the file");
        assert!(
            fresh.is_dirty(),
            "restored work is unsaved until saved for real"
        );
        assert!(
            fresh.document.iter().any(|e| matches!(
                &e.kind,
                EntityKind::Curve(Curve::Line(l)) if (l.p1.x - 7.0).abs() < 1e-9
            )),
            "the drawn line survived the crash-recovery round trip"
        );
        assert!(
            fresh.current_file_path.is_none(),
            "recovered documents are untitled"
        );
        discard_recovery();
        assert!(pending_recovery().is_none(), "discard removes the file");
    }
}
