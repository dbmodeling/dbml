//! Atomic Undo/Redo transaction stack combining DBML text and canvas positions.
//!
//! Para não crescer sem limite ao digitar: edições de texto consecutivas dentro de
//! [`TEXT_COALESCE_WINDOW`] viram um único passo, o histórico tem orçamento em bytes
//! ([`MAX_HISTORY_BYTES`]) além do teto de entradas e o mapa de posições é compartilhado
//! (`Arc`) entre passos que só mudam o texto.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Maximum number of undo/redo snapshots retained per open document.
pub const MAX_HISTORY: usize = 100;

/// Orçamento aproximado de memória do histórico (passado + futuro) por documento.
pub const MAX_HISTORY_BYTES: usize = 16 * 1024 * 1024;

/// Intervalo máximo entre duas edições de texto para que formem um único passo de undo.
pub const TEXT_COALESCE_WINDOW: Duration = Duration::from_millis(1000);

/// Mapa de posições compartilhável entre snapshots.
pub type Positions = Arc<HashMap<String, (f32, f32)>>;

/// Atomic state of a document (DBML source text, card positions, and selection).
#[derive(Clone, Debug, PartialEq)]
pub struct DocSnapshot {
    /// Immutable DBML text buffer (`Arc<str>` allows $O(1)$ sharing across drag-only steps).
    pub text: Arc<str>,
    /// World coordinates of tables (`"<name>"`) and enums (`"enum:<name>"`); `Arc` permite
    /// compartilhar o mapa entre passos que só mudam o texto.
    pub pos: Positions,
    /// Estilo das relações (`edges` do sidecar).
    pub edges: diagram_export::EdgeStyles,
    /// Primary selected table.
    pub selected: Option<String>,
    /// Selected enum name.
    pub selected_enum: Option<String>,
    /// Additional selected tables (multi-selection).
    pub multi: Vec<String>,
}

impl DocSnapshot {
    /// Returns `true` when the DBML text, card positions and relation styles are identical.
    #[must_use]
    pub fn same_state(&self, other: &Self) -> bool {
        (Arc::ptr_eq(&self.text, &other.text) || self.text == other.text)
            && (Arc::ptr_eq(&self.pos, &other.pos) || self.pos == other.pos)
            && self.edges == other.edges
    }

    fn pos_bytes(&self) -> usize {
        self.pos
            .keys()
            .map(|k| k.len() + std::mem::size_of::<(String, (f32, f32))>())
            .sum()
    }
}

/// Bounded undo/redo stack for atomic document + layout transactions.
#[derive(Clone, Debug)]
pub struct UndoHistory {
    past: VecDeque<DocSnapshot>,
    future: Vec<DocSnapshot>,
    max_bytes: usize,
    /// Instante da última edição de texto do passo em andamento (`None` = nenhum passo aberto).
    last_text_edit: Option<Instant>,
}

impl Default for UndoHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl UndoHistory {
    /// Creates an empty [`UndoHistory`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_byte_budget(MAX_HISTORY_BYTES)
    }

    /// Cria um histórico com orçamento de memória customizado (em bytes).
    #[must_use]
    pub fn with_byte_budget(max_bytes: usize) -> Self {
        Self {
            past: VecDeque::with_capacity(MAX_HISTORY),
            future: Vec::new(),
            max_bytes,
            last_text_edit: None,
        }
    }

    /// Reaproveita o `Arc` de posições de um passo guardado quando o mapa é igual; senão
    /// aloca um novo. Evita clonar o mapa a cada passo só-texto.
    #[must_use]
    pub fn share_pos(&self, pos: &HashMap<String, (f32, f32)>) -> Positions {
        self.past
            .back()
            .into_iter()
            .chain(self.future.last())
            .find(|s| *s.pos == *pos)
            .map_or_else(|| Arc::new(pos.clone()), |s| s.pos.clone())
    }

    /// Records the pre-operation `snapshot` as a reversible step and clears the redo stack.
    ///
    /// Consecutive snapshots with identical text and positions are deduplicated. Encerra o
    /// passo de texto em andamento (a próxima digitação abre um passo novo).
    pub fn push(&mut self, snapshot: DocSnapshot) {
        self.last_text_edit = None;
        self.push_inner(snapshot);
    }

    /// Se `now` ainda está na janela do passo de texto aberto, estende-o (a edição é absorvida
    /// pelo snapshot já guardado) e devolve `true`. Não grava nada quando devolve `false`.
    pub fn extend_text_run(&mut self, now: Instant) -> bool {
        match self.last_text_edit {
            Some(prev) if now.saturating_duration_since(prev) <= TEXT_COALESCE_WINDOW => {
                self.last_text_edit = Some(now);
                true
            }
            _ => false,
        }
    }

    /// Como [`push`](Self::push), mas para uma edição de texto em `now`: edições consecutivas
    /// dentro de [`TEXT_COALESCE_WINDOW`] compartilham o snapshot do início da sequência.
    pub fn push_text_edit(&mut self, snapshot: DocSnapshot, now: Instant) {
        if self.extend_text_run(now) {
            return;
        }
        self.last_text_edit = self.push_inner(snapshot).then_some(now);
    }

    /// Devolve `true` se o snapshot foi gravado (não era duplicata).
    fn push_inner(&mut self, snapshot: DocSnapshot) -> bool {
        if self
            .past
            .back()
            .is_some_and(|last| last.same_state(&snapshot))
        {
            return false;
        }
        if self.past.len() >= MAX_HISTORY {
            self.past.pop_front();
        }
        self.past.push_back(snapshot);
        self.future.clear();
        self.enforce_budget();
        true
    }

    /// Bytes aproximados retidos (texto e posições compartilhados contam uma vez só).
    #[must_use]
    pub fn approx_bytes(&self) -> usize {
        let mut texts: HashSet<*const u8> = HashSet::new();
        let mut poses: HashSet<*const ()> = HashSet::new();
        let mut total = 0;
        for s in self.past.iter().chain(self.future.iter()) {
            if texts.insert(Arc::as_ptr(&s.text).cast::<u8>()) {
                total += s.text.len();
            }
            if poses.insert(Arc::as_ptr(&s.pos).cast::<()>()) {
                total += s.pos_bytes();
            }
        }
        total
    }

    /// Descarta passos mais antigos (passado, depois futuro) até caber no orçamento, sempre
    /// mantendo ao menos o passo mais recente.
    fn enforce_budget(&mut self) {
        while self.approx_bytes() > self.max_bytes {
            if self.past.len() > 1 {
                self.past.pop_front();
            } else if self.future.len() > 1 {
                self.future.remove(0);
            } else {
                break;
            }
        }
    }

    /// Pops the most recent undo snapshot, saving `current` onto the redo stack.
    #[must_use]
    pub fn undo(&mut self, current: DocSnapshot) -> Option<DocSnapshot> {
        self.last_text_edit = None;
        let prev = self.past.pop_back()?;
        if self.future.len() >= MAX_HISTORY {
            self.future.remove(0);
        }
        self.future.push(current);
        self.enforce_budget();
        Some(prev)
    }

    /// Pops the most recent redo snapshot, saving `current` onto the undo stack.
    #[must_use]
    pub fn redo(&mut self, current: DocSnapshot) -> Option<DocSnapshot> {
        self.last_text_edit = None;
        let next = self.future.pop()?;
        if self.past.len() >= MAX_HISTORY {
            self.past.pop_front();
        }
        self.past.push_back(current);
        self.enforce_budget();
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
            pos: Arc::new(pos),
            edges: diagram_export::EdgeStyles::new(),
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
    fn edge_style_change_is_its_own_undo_step() {
        let mut h = UndoHistory::new();
        let s0 = snap("v", 0.0, 0.0);
        let mut s1 = s0.clone();
        s1.edges.insert(
            "a.id->b.id".into(),
            diagram_export::EdgeStyle {
                straight: true,
                ..Default::default()
            },
        );
        h.push(s0.clone());
        h.push(s1.clone());
        assert_eq!(h.undo(s0.clone()), Some(s1));
        assert_eq!(h.undo(s0.clone()), Some(s0));
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

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    fn undo_count(h: &mut UndoHistory) -> usize {
        let mut count = 0;
        let mut cur = snap("current", 999.0, 0.0);
        while let Some(prev) = h.undo(cur.clone()) {
            cur = prev;
            count += 1;
        }
        count
    }

    #[test]
    fn text_edits_within_window_should_coalesce_into_one_step() {
        let t0 = Instant::now();
        let mut h = UndoHistory::new();
        h.push_text_edit(snap("a", 0.0, 0.0), at(t0, 0));
        h.push_text_edit(snap("ab", 0.0, 0.0), at(t0, 300));
        h.push_text_edit(snap("abc", 0.0, 0.0), at(t0, 900));
        let restored = h.undo(snap("abcd", 0.0, 0.0)).unwrap();
        assert_eq!(restored.text.as_ref(), "a");
        assert_eq!(undo_count(&mut h), 0);
    }

    #[test]
    fn text_edits_after_window_should_open_new_step() {
        let t0 = Instant::now();
        let mut h = UndoHistory::new();
        h.push_text_edit(snap("a", 0.0, 0.0), at(t0, 0));
        h.push_text_edit(snap("ab", 0.0, 0.0), at(t0, 500));
        h.push_text_edit(snap("abc", 0.0, 0.0), at(t0, 5000));
        assert_eq!(undo_count(&mut h), 2);
    }

    #[test]
    fn coalescing_window_should_slide_with_each_edit() {
        let t0 = Instant::now();
        let mut h = UndoHistory::new();
        for i in 0..10u64 {
            h.push_text_edit(snap(&format!("v{i}"), 0.0, 0.0), at(t0, i * 800));
        }
        assert_eq!(undo_count(&mut h), 1);
    }

    #[test]
    fn non_text_push_should_break_the_text_run() {
        let t0 = Instant::now();
        let mut h = UndoHistory::new();
        h.push_text_edit(snap("a", 0.0, 0.0), at(t0, 0));
        h.push(snap("a", 50.0, 50.0));
        h.push_text_edit(snap("ab", 50.0, 50.0), at(t0, 100));
        assert_eq!(undo_count(&mut h), 3);
    }

    #[test]
    fn undo_should_break_the_text_run() {
        let t0 = Instant::now();
        let mut h = UndoHistory::new();
        h.push_text_edit(snap("a", 0.0, 0.0), at(t0, 0));
        let _ = h.undo(snap("ab", 0.0, 0.0));
        h.push_text_edit(snap("x", 0.0, 0.0), at(t0, 100));
        assert!(h.redo(snap("y", 0.0, 0.0)).is_none());
        assert_eq!(undo_count(&mut h), 1);
    }

    #[test]
    fn deduplicated_text_edit_should_not_open_a_run() {
        let t0 = Instant::now();
        let mut h = UndoHistory::new();
        h.push(snap("a", 0.0, 0.0));
        h.push_text_edit(snap("a", 0.0, 0.0), at(t0, 0));
        h.push_text_edit(snap("ab", 0.0, 0.0), at(t0, 100));
        assert_eq!(undo_count(&mut h), 2);
    }

    #[test]
    fn extend_text_run_should_be_false_without_open_run() {
        let t0 = Instant::now();
        let mut h = UndoHistory::new();
        assert!(!h.extend_text_run(at(t0, 0)));
        h.push_text_edit(snap("a", 0.0, 0.0), at(t0, 0));
        assert!(h.extend_text_run(at(t0, 200)));
        assert!(!h.extend_text_run(at(t0, 5000)));
    }

    #[test]
    fn byte_budget_should_evict_oldest_snapshots() {
        let big = "x".repeat(1000);
        let mut h = UndoHistory::with_byte_budget(3500);
        for i in 0..10 {
            h.push(snap(&format!("{big}{i}"), i as f32, 0.0));
        }
        assert!(h.approx_bytes() <= 3500, "bytes = {}", h.approx_bytes());
        let n = undo_count(&mut h);
        assert!((1..10).contains(&n), "n = {n}");
    }

    #[test]
    fn byte_budget_should_keep_newest_even_if_oversized() {
        let mut h = UndoHistory::with_byte_budget(10);
        h.push(snap(&"y".repeat(500), 0.0, 0.0));
        assert_eq!(undo_count(&mut h), 1);
    }

    #[test]
    fn share_pos_should_reuse_arc_when_positions_equal() {
        let mut h = UndoHistory::new();
        let s0 = snap("a", 1.0, 2.0);
        h.push(s0.clone());
        let same = h.share_pos(&s0.pos);
        assert!(Arc::ptr_eq(&same, &s0.pos));
        let mut other = (*s0.pos).clone();
        other.insert("posts".into(), (3.0, 4.0));
        assert!(!Arc::ptr_eq(&h.share_pos(&other), &s0.pos));
    }

    #[test]
    fn text_only_steps_should_count_shared_positions_once() {
        let mut h = UndoHistory::new();
        let base = snap("a", 1.0, 2.0);
        h.push(base.clone());
        let single = h.approx_bytes();
        let mut next = base;
        next.text = Arc::from("b");
        h.push(next);
        assert_eq!(h.approx_bytes(), single + 1);
    }
}
