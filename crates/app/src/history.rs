//! Atomic Undo/Redo transaction stack combining DBML text and canvas positions.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// Maximum number of undo/redo snapshots retained per open document.
pub const MAX_HISTORY: usize = 100;

/// Atomic state of a document (DBML source text, card positions, and selection).
#[derive(Clone, Debug, PartialEq)]
pub struct DocSnapshot {
    /// Immutable DBML text buffer (`Arc<str>` allows $O(1)$ sharing across drag-only steps).
    pub text: Arc<str>,
    /// World coordinates of tables (`"<name>"`) and enums (`"enum:<name>"`).
    pub pos: HashMap<String, (f32, f32)>,
    /// Primary selected table.
    pub selected: Option<String>,
    /// Selected enum name.
    pub selected_enum: Option<String>,
    /// Additional selected tables (multi-selection).
    pub multi: Vec<String>,
}

impl DocSnapshot {
    /// Returns `true` when both the DBML text and all card positions are identical.
    #[must_use]
    pub fn same_state(&self, other: &Self) -> bool {
        (Arc::ptr_eq(&self.text, &other.text) || self.text == other.text) && self.pos == other.pos
    }
}

/// Bounded undo/redo stack for atomic document + layout transactions.
#[derive(Clone, Debug, Default)]
pub struct UndoHistory {
    past: VecDeque<DocSnapshot>,
    future: Vec<DocSnapshot>,
}

impl UndoHistory {
    /// Creates an empty [`UndoHistory`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            past: VecDeque::with_capacity(MAX_HISTORY),
            future: Vec::new(),
        }
    }

    /// Records the pre-operation `snapshot` as a reversible step and clears the redo stack.
    ///
    /// Consecutive snapshots with identical text and positions are deduplicated.
    pub fn push(&mut self, snapshot: DocSnapshot) {
        if self
            .past
            .back()
            .is_some_and(|last| last.same_state(&snapshot))
        {
            return;
        }
        if self.past.len() >= MAX_HISTORY {
            self.past.pop_front();
        }
        self.past.push_back(snapshot);
        self.future.clear();
    }

    /// Pops the most recent undo snapshot, saving `current` onto the redo stack.
    #[must_use]
    pub fn undo(&mut self, current: DocSnapshot) -> Option<DocSnapshot> {
        let prev = self.past.pop_back()?;
        if self.future.len() >= MAX_HISTORY {
            self.future.remove(0);
        }
        self.future.push(current);
        Some(prev)
    }

    /// Pops the most recent redo snapshot, saving `current` onto the undo stack.
    #[must_use]
    pub fn redo(&mut self, current: DocSnapshot) -> Option<DocSnapshot> {
        let next = self.future.pop()?;
        if self.past.len() >= MAX_HISTORY {
            self.past.pop_front();
        }
        self.past.push_back(current);
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(text: &str, x: f32, y: f32) -> DocSnapshot {
        let mut pos = HashMap::new();
        pos.insert("users".to_string(), (x, y));
        DocSnapshot {
            text: Arc::from(text),
            pos,
            selected: Some("users".to_string()),
            selected_enum: None,
            multi: Vec::new(),
        }
    }

    #[test]
    fn push_should_deduplicate_identical_text_and_positions() {
        let mut h = UndoHistory::new();
        h.push(snap("Table users { id int }", 10.0, 20.0));
        h.push(snap("Table users { id int }", 10.0, 20.0));
        let restored = h.undo(snap("Table users { id int }", 50.0, 60.0));
        assert!(restored.is_some());
        assert!(h.undo(snap("Table users { id int }", 10.0, 20.0)).is_none());
    }

    #[test]
    fn undo_and_redo_should_restore_composite_text_and_positions() {
        let mut h = UndoHistory::new();
        let s0 = snap("Table users { id int }", 0.0, 0.0);
        let s1 = snap(
            "Table users { id int }\nTable posts { id int }",
            240.0,
            120.0,
        );
        h.push(s0.clone());

        let undone = h.undo(s1.clone()).unwrap();
        assert_eq!(undone, s0);

        let redone = h.redo(undone).unwrap();
        assert_eq!(redone, s1);
    }

    #[test]
    fn push_after_undo_should_clear_redo_stack() {
        let mut h = UndoHistory::new();
        let s0 = snap("v0", 0.0, 0.0);
        let s1 = snap("v1", 10.0, 10.0);
        let s2 = snap("v2", 20.0, 20.0);
        h.push(s0.clone());
        let _ = h.undo(s1);
        h.push(s0);
        assert!(h.redo(s2).is_none());
    }

    #[test]
    fn push_should_evict_oldest_snapshot_when_exceeding_max_history() {
        let mut h = UndoHistory::new();
        for i in 0..=MAX_HISTORY {
            h.push(snap(&format!("v{i}"), i as f32, 0.0));
        }
        let mut count = 0;
        let mut cur = snap("current", 999.0, 0.0);
        while let Some(prev) = h.undo(cur.clone()) {
            cur = prev;
            count += 1;
        }
        assert_eq!(count, MAX_HISTORY);
        assert_eq!(cur.text.as_ref(), "v1");
    }
}
