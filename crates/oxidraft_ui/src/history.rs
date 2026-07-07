use oxidraft_document::Document;
use std::collections::VecDeque;

pub struct History {
    past: VecDeque<Document>,
    future: Vec<Document>,
    limit: usize,
}

impl Default for History {
    fn default() -> Self {
        History {
            past: VecDeque::new(),
            future: Vec::new(),
            limit: 200,
        }
    }
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_limit(limit: usize) -> Self {
        History {
            past: VecDeque::new(),
            future: Vec::new(),
            limit,
        }
    }

    pub fn snapshot(&mut self, doc: &Document) {
        self.past.push_back(doc.clone());
        if self.past.len() > self.limit {
            self.past.pop_front();
        }
        self.future.clear();
    }
    pub fn undo(&mut self, doc: &Document) -> Option<Document> {
        let prev = self.past.pop_back()?;
        self.future.push(doc.clone());
        Some(prev)
    }
    pub fn redo(&mut self, doc: &Document) -> Option<Document> {
        let next = self.future.pop()?;
        self.past.push_back(doc.clone());
        Some(next)
    }
    pub fn discard_last(&mut self) {
        self.past.pop_back();
    }

    /// Pops the most recent snapshot back out for the caller to restore,
    /// without touching the redo stack — used to abort an in-progress
    /// interaction that may have edited several entities.
    pub fn rollback(&mut self) -> Option<Document> {
        self.past.pop_back()
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
    pub fn undo_depth(&self) -> usize {
        self.past.len()
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
}
