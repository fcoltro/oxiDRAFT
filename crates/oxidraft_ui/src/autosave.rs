//! Crash protection: while the document has unsaved changes, a recovery copy
//! is written every [`AUTOSAVE_INTERVAL`]. The file is deleted on every
//! successful manual save and when the user explicitly discards changes at
//! exit — so a recovery file found at startup means the last session ended
//! without either, i.e. a crash or kill, and the UI offers to restore it.
//!
//! Each instance writes its own `oxidraft_recovery_<pid>.o2d`, so concurrent
//! instances can't clobber each other's safety copy. Liveness is arbitrated
//! through a sidecar `.lock` file held under an exclusive OS lock for the
//! writer's lifetime: a recovery file whose sidecar can be locked (or is
//! missing) belongs to a dead session and is offered for restore; one whose
//! sidecar refuses the lock belongs to a still-running instance.

use crate::state::AppState;
use std::fs::TryLockError;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// Minimum time between autosave writes while the document has unsaved changes.
pub const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(30);

const RECOVERY_PREFIX: &str = "oxidraft_recovery";

/// Sidecar locks this instance holds; the open handles keep the OS locks
/// alive for the process lifetime, which is exactly the liveness signal.
static HELD_LOCKS: LazyLock<Mutex<Vec<(PathBuf, std::fs::File)>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// The recovery file and its sidecar are process-wide state; tests that
/// write or retire them (directly or through `save_file_to`) must hold this
/// so parallel test threads can't delete each other's file mid-assertion.
#[cfg(test)]
pub(crate) static RECOVERY_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Dirs that may hold recovery files, most preferred first: next to the
/// executable like the crash log, then the temp dir for installs where the
/// executable's directory isn't writable (Program Files, /usr/bin) — an
/// unwritable sole location would silently disable crash protection.
fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .into_iter()
        .collect();
    dirs.push(std::env::temp_dir());
    dirs.dedup();
    dirs
}

fn own_file_name() -> String {
    format!("{RECOVERY_PREFIX}_{}.o2d", std::process::id())
}

fn sidecar(recovery: &Path) -> PathBuf {
    recovery.with_extension("lock")
}

/// Creates and locks the sidecar for a recovery file we just wrote, once per
/// path. Failure is non-fatal: without the lock the file merely risks being
/// offered to a concurrently running instance, the pre-lock behavior.
fn mark_alive(recovery: &Path) {
    let lock_path = sidecar(recovery);
    let Ok(mut held) = HELD_LOCKS.lock() else {
        return;
    };
    if held.iter().any(|(p, _)| p == &lock_path) {
        return;
    }
    // The sidecar's contents never matter — only the lock held on it does.
    if let Ok(f) = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        && f.try_lock().is_ok()
    {
        held.push((lock_path, f));
    }
}

/// True when the session that wrote `recovery` still runs — its sidecar is
/// exclusively locked. A missing sidecar (legacy files, failed lock setup)
/// reads as dead, which at worst re-offers a live instance's copy.
fn session_is_live(recovery: &Path) -> bool {
    let Ok(f) = std::fs::File::open(sidecar(recovery)) else {
        return false;
    };
    match f.try_lock() {
        Ok(()) => false,
        Err(TryLockError::WouldBlock) => true,
        Err(TryLockError::Error(_)) => false,
    }
}

/// Called once per frame; writes at most every [`AUTOSAVE_INTERVAL`], and
/// only while unsaved changes exist. The timer stays disarmed while the
/// document is clean: the first frame with unsaved changes writes
/// immediately, so a quick sketch made right after startup (or right after
/// a save) is protected within one frame instead of riding a free-running
/// interval that could leave it unprotected for the full period.
pub fn tick(app: &AppState, last: &mut Option<Instant>) {
    if !app.is_dirty() {
        *last = None;
        return;
    }
    let due = last
        .map(|t| t.elapsed() >= AUTOSAVE_INTERVAL)
        .unwrap_or(true);
    if !due {
        return;
    }
    *last = Some(Instant::now());
    let mut doc = app.document.clone();
    doc.remove(app.origin_id);
    for dir in candidate_dirs() {
        let path = dir.join(own_file_name());
        if oxidraft_io::save_native(&doc, &path).is_ok() {
            mark_alive(&path);
            break;
        }
    }
}

/// Retires *this session's* recovery copy — called after a successful save
/// and when the user explicitly discards unsaved work at exit.
pub fn discard_recovery() {
    for dir in candidate_dirs() {
        let _ = std::fs::remove_file(dir.join(own_file_name()));
    }
}

/// Removes a specific recovery file (the one offered at startup) and its
/// sidecar, after the user restored or declined it.
pub fn remove_recovery_file(path: &Path) {
    let _ = std::fs::remove_file(sidecar(path));
    let _ = std::fs::remove_file(path);
}

/// Retires a recovery file that FAILED to restore. Renamed aside (`.bad`)
/// rather than deleted, so the bytes stay available for inspection — but
/// out of `pending_recovery`'s discovery set, so a corrupt file isn't
/// re-offered at every launch forever.
pub fn quarantine_recovery_file(path: &Path) {
    let _ = std::fs::remove_file(sidecar(path));
    let bad = path.with_extension("o2d.bad");
    if std::fs::rename(path, &bad).is_err() {
        let _ = std::fs::remove_file(path);
    }
}

/// A leftover recovery file with content, whose writer no longer runs, means
/// that session ended without a save or an explicit discard. Also sweeps
/// orphaned sidecars (clean exits can't delete their own held lock file).
pub fn pending_recovery() -> Option<PathBuf> {
    let mut found = None;
    for dir in candidate_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if name.starts_with(RECOVERY_PREFIX) && name.ends_with(".lock") {
                let data = path.with_extension("o2d");
                if !data.exists() && !session_is_live(&data) {
                    let _ = std::fs::remove_file(&path);
                }
                continue;
            }
            if !(name.starts_with(RECOVERY_PREFIX) && name.ends_with(".o2d")) {
                continue;
            }
            let has_content = std::fs::metadata(&path).is_ok_and(|m| m.len() > 0);
            if has_content && !session_is_live(&path) && found.is_none() {
                found = Some(path);
            }
        }
        if found.is_some() {
            break;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxidraft_document::EntityKind;
    use oxidraft_geometry::{Curve, LineSeg, Point2d};

    fn own_recovery_file() -> Option<PathBuf> {
        candidate_dirs()
            .into_iter()
            .map(|d| d.join(own_file_name()))
            .find(|p| std::fs::metadata(p).is_ok_and(|m| m.len() > 0))
    }

    // One test only: this session's recovery path and lock are process-wide
    // state, and parallel tests in this process would race on them.
    #[test]
    fn autosave_and_restore_round_trip() {
        let _guard = RECOVERY_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
        let path = own_recovery_file().expect("dirty tick must leave a recovery file");
        assert!(
            session_is_live(&path),
            "our own held sidecar lock must read as a live session"
        );
        assert_ne!(
            pending_recovery().as_deref(),
            Some(path.as_path()),
            "a live session's file must never be offered for restore"
        );

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
        assert!(own_recovery_file().is_none(), "discard removes the file");

        // A recovery file from a dead session (no one holds its sidecar) is
        // exactly what startup must detect and clean up.
        let dead = candidate_dirs()[0].join(format!("{RECOVERY_PREFIX}_0.o2d"));
        std::fs::write(&dead, "O2D 1\n").expect("plant a dead session's file");
        assert!(!session_is_live(&dead));
        assert!(
            pending_recovery().is_some(),
            "a dead session's file must be discoverable"
        );
        remove_recovery_file(&dead);
        assert!(!dead.exists(), "declining the offer removes the file");

        // While the document is clean the timer must stay disarmed, so the
        // very first edit after a long idle period is written immediately —
        // a free-running timer could leave a quick sketch unprotected for
        // the whole interval.
        let clean = AppState::new(800.0, 600.0);
        let mut armed = None;
        tick(&clean, &mut armed);
        assert!(armed.is_none(), "a clean document must not arm the timer");
        assert!(
            own_recovery_file().is_none(),
            "a clean document must not write a recovery file"
        );
        tick(&app, &mut armed);
        assert!(
            own_recovery_file().is_some(),
            "the first dirty tick after idling must write immediately"
        );
        discard_recovery();

        // A corrupt recovery file that failed to restore must be moved out
        // of the discovery set, or it is re-offered at every launch forever.
        let corrupt = candidate_dirs()[0].join(format!("{RECOVERY_PREFIX}_1.o2d"));
        std::fs::write(&corrupt, "not an o2d file at all").expect("plant corrupt file");
        let mut broken = AppState::new(800.0, 600.0);
        assert!(
            !broken.restore_recovery(&corrupt),
            "garbage must fail to restore"
        );
        quarantine_recovery_file(&corrupt);
        assert!(
            !corrupt.exists(),
            "the corrupt file must leave the discovery set"
        );
        assert_ne!(
            pending_recovery().as_deref(),
            Some(corrupt.as_path()),
            "a quarantined file must not be offered again"
        );
        let bad = corrupt.with_extension("o2d.bad");
        assert!(bad.exists(), "the bytes are kept aside for inspection");
        let _ = std::fs::remove_file(&bad);
    }
}
