//! Undo/redo: keeps a capped stack of whole-document snapshots plus a
//! monotonic revision id per state, used to detect unsaved changes even
//! when the undo *depth* alone would alias two different states.

use oxidraft_document::Document;
use std::collections::VecDeque;

/// An undo/redo stack of document snapshots with a bounded size and a
/// monotonic revision counter for dirty-tracking.
pub struct History {
    past: VecDeque<(Document, u64)>,
    future: Vec<(Document, u64)>,
    limit: usize,
    /// Identity of the current document state. Dirty tracking compares this
    /// against the revision remembered at save time; comparing undo *depths*
    /// aliases — the cap pins the depth once it truncates old snapshots, and
    /// undo followed by new edits revisits old depths — so depth equality
    /// silently reported unsaved work as clean.
    current_rev: u64,
    next_rev: u64,
}

impl Default for History {
    fn default() -> Self {
        Self::with_limit(200)
    }
}

impl History {
    /// Creates an empty history with the default snapshot limit (200).
    pub fn new() -> Self {
        Self::default()
    }
    /// Creates an empty history that keeps at most `limit` undo snapshots.
    pub fn with_limit(limit: usize) -> Self {
        History {
            past: VecDeque::new(),
            future: Vec::new(),
            limit,
            current_rev: 0,
            next_rev: 1,
        }
    }

    /// Pushes `doc`'s current state onto the undo stack (evicting the oldest
    /// snapshot past the limit) and clears the redo stack, as any new edit
    /// invalidates it.
    pub fn snapshot(&mut self, doc: &Document) {
        self.past.push_back((doc.clone(), self.current_rev));
        if self.past.len() > self.limit {
            self.past.pop_front();
        }
        self.future.clear();
        self.current_rev = self.next_rev;
        self.next_rev += 1;
    }
    /// Pops the most recent undo snapshot, pushing `doc`'s current state onto
    /// the redo stack, or `None` if there's nothing to undo.
    pub fn undo(&mut self, doc: &Document) -> Option<Document> {
        let (prev, rev) = self.past.pop_back()?;
        self.future.push((doc.clone(), self.current_rev));
        self.current_rev = rev;
        Some(prev)
    }
    /// Pops the most recent redo snapshot, pushing `doc`'s current state back
    /// onto the undo stack, or `None` if there's nothing to redo.
    pub fn redo(&mut self, doc: &Document) -> Option<Document> {
        let (next, rev) = self.future.pop()?;
        self.past.push_back((doc.clone(), self.current_rev));
        self.current_rev = rev;
        Some(next)
    }

    /// Drops the snapshot taken for an operation that turned out to be a
    /// no-op; the document still equals that snapshot, so the current state
    /// *is* the popped revision.
    pub fn discard_last(&mut self) {
        if let Some((_, rev)) = self.past.pop_back() {
            self.current_rev = rev;
        }
    }

    /// Pops the most recent snapshot back out for the caller to restore,
    /// without touching the redo stack — used to abort an in-progress
    /// interaction that may have edited several entities.
    pub fn rollback(&mut self) -> Option<Document> {
        let (prev, rev) = self.past.pop_back()?;
        self.current_rev = rev;
        Some(prev)
    }

    /// Whether there's a snapshot to undo to.
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }
    /// Whether there's a snapshot to redo to.
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
    /// Number of snapshots currently on the undo stack.
    pub fn undo_depth(&self) -> usize {
        self.past.len()
    }
    /// Monotonic id of the state the document is in right now; equal ids
    /// mean the same state, regardless of the undo path taken to reach it.
    pub fn current_revision(&self) -> u64 {
        self.current_rev
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxidraft_document::EntityKind;
    use oxidraft_geometry::{Curve, LineSeg, Point2d};

    fn line(x: i64) -> EntityKind {
        EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            Point2d::from_i64(x, 0),
            Point2d::from_i64(x + 1, 1),
        )))
    }

    #[test]
    fn undo_redo_roundtrip() {
        let mut doc = Document::new();
        let mut hist = History::new();

        hist.snapshot(&doc);
        doc.add(line(0));
        assert_eq!(doc.len(), 1);

        hist.snapshot(&doc);
        doc.add(line(5));
        assert_eq!(doc.len(), 2);

        doc = hist.undo(&doc).unwrap();
        assert_eq!(doc.len(), 1);
        doc = hist.undo(&doc).unwrap();
        assert_eq!(doc.len(), 0);
        assert!(!hist.can_undo());

        doc = hist.redo(&doc).unwrap();
        assert_eq!(doc.len(), 1);
        doc = hist.redo(&doc).unwrap();
        assert_eq!(doc.len(), 2);
        assert!(!hist.can_redo());
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut doc = Document::new();
        let mut hist = History::new();
        hist.snapshot(&doc);
        doc.add(line(0));
        doc = hist.undo(&doc).unwrap();
        assert!(hist.can_redo());
        hist.snapshot(&doc);
        doc.add(line(9));
        assert!(!hist.can_redo(), "new edit must clear redo stack");
    }

    #[test]
    fn limit_bounds_growth() {
        let mut doc = Document::new();
        let mut hist = History::with_limit(3);
        for i in 0..10 {
            hist.snapshot(&doc);
            doc.add(line(i));
        }
        assert_eq!(hist.undo_depth(), 3);
    }

    #[test]
    fn revision_distinguishes_states_depth_would_alias() {
        let mut doc = Document::new();
        let mut hist = History::new();
        hist.snapshot(&doc);
        doc.add(line(0));
        let saved = hist.current_revision();
        assert_eq!(hist.undo_depth(), 1);

        // Undo, then take a different edit back to the same depth: the old
        // depth comparison read this as "clean" and lost the close prompt.
        doc = hist.undo(&doc).unwrap();
        hist.snapshot(&doc);
        doc.add(line(7));
        assert_eq!(hist.undo_depth(), 1, "same depth as at save time");
        assert_ne!(hist.current_revision(), saved, "but a different state");

        // Undo/redo returns to exactly the remembered revision.
        let here = hist.current_revision();
        doc = hist.undo(&doc).unwrap();
        assert_ne!(hist.current_revision(), here);
        let _ = hist.redo(&doc).unwrap();
        assert_eq!(hist.current_revision(), here);
    }

    #[test]
    fn cap_truncation_does_not_mask_new_edits() {
        let mut doc = Document::new();
        let mut hist = History::with_limit(3);
        for i in 0..3 {
            hist.snapshot(&doc);
            doc.add(line(i));
        }
        let saved = hist.current_revision();
        for i in 3..6 {
            hist.snapshot(&doc);
            doc.add(line(i));
        }
        assert_eq!(hist.undo_depth(), 3, "depth is pinned at the cap");
        assert_ne!(
            hist.current_revision(),
            saved,
            "revision keeps moving, so the document still reads dirty"
        );
    }

    #[test]
    fn discard_and_rollback_restore_the_snapshot_revision() {
        let mut doc = Document::new();
        let mut hist = History::new();
        let start = hist.current_revision();

        hist.snapshot(&doc); // op turns out to be a no-op
        hist.discard_last();
        assert_eq!(hist.current_revision(), start);

        hist.snapshot(&doc);
        doc.add(line(1)); // aborted interaction
        doc = hist.rollback().unwrap();
        assert_eq!(hist.current_revision(), start);
        assert_eq!(doc.len(), 0);
    }
}
