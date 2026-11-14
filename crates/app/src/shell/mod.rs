//! Janela principal. Um *projeto* é uma pasta com um ou mais arquivos `.dbml`
//! (ou um arquivo avulso); cada arquivo aberto vira uma aba com editor e canvas.
//! Layout: barra de título com abas · barra de atividades · painel lateral
//! (Arquivos / Esquema / Problemas) · editor · canvas · inspector · barra de status.
//! Todas as ações passam por `actions::catalog`.

mod panels;

use crate::actions::{self, *};
use crate::canvas::{Diagram, DiagramEvent, Tool};
use crate::glass::*;
use dbml_analysis::Severity;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
use gpui_kit::component::input::{Editor, EditorState, Input, InputEvent, InputState};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::menu::DropdownMenu;
use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Disableable as _, Icon, InteractiveElementExt as _, Sizable, WindowExt};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::{Path, PathBuf};

/// Um arquivo aberto (aba).
pub struct Doc {
    path: Option<PathBuf>,
    title: String,
    editor: Entity<EditorState>,
    diagram: Entity<Diagram>,
    saved_text: std::sync::Arc<str>,
    last_text: std::sync::Arc<str>,
    history: crate::history::UndoHistory,
    dirty: bool,
    diags: Vec<dbml_analysis::Diagnostic>,
}

impl Doc {
    fn counts(&self) -> (usize, usize) {
        let n = |s| self.diags.iter().filter(|d| d.severity == s).count();
        (n(Severity::Error), n(Severity::Warning))
    }
}

/// Campo em edição no inspector (um por vez, como no Figma).
#[derive(Clone, PartialEq, Eq, Debug)]
enum EditTarget {
    Table(String),
    ColName(String, String),
    ColType(String, String),
    ColDefault(String, String),
    ColCheck(String, String),
    IndexName(String, usize),
    Enum(String),
    EnumVal(String, String),
    NewEnumVal(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pane {
    Files,
    Schema,
    Problems,
}

const STANDARD_TYPES: &[(&str, &str)] = &[
    ("int", "4-byte integer"),
    ("bigint", "8-byte integer"),
    ("smallint", "2-byte integer"),
    ("serial", "auto-increment"),
    ("varchar(255)", "variable-length text"),
    ("text", "long text"),
    ("boolean", "boolean"),
    ("uuid", "unique identifier"),
    ("timestamp", "date and time"),
    ("timestamptz", "date/time with timezone"),
    ("date", "date"),
    ("time", "time"),
    ("decimal(10,2)", "exact decimal"),
    ("float", "floating point"),
    ("double", "double precision"),
    ("json", "JSON"),
    ("jsonb", "binary JSON"),
    ("bytea", "binary"),
];

fn extract_enums_from_dbml(text: &str) -> Vec<String> {
    let mut enums = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Enum ") {
            let name = rest
                .split(|c: char| c.is_whitespace() || c == '{')
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() && !enums.contains(&name.to_string()) {
                enums.push(name.to_string());
            }
        }
    }
    enums
}

pub struct Shell {
    /// Pasta do projeto; `None` = arquivos avulsos.
    project: Option<PathBuf>,
    /// `.dbml` encontrados na pasta do projeto.
    files: Vec<PathBuf>,
    docs: Vec<Doc>,
    active: usize,
    untitled: usize,
    pane: Pane,
    sidebar_open: bool,
    editor_open: bool,
    inspector_open: bool,
    mode: Mode,
    /// chrome translúcido (há blur no fundo da janela)
    translucent: bool,
    /// larguras dos painéis (salvas); `panels_gen` recria o grupo ao restaurar
    widths: (f32, f32, f32),
    panels_gen: usize,
    translucent_cards: bool,
    /// largura da coluna de tipos de dados no inspector
    col_type_w: f32,
    /// arraste horizontal para redimensionar a coluna de tipos (x inicial, `col_type_w` inicial)
    col_resize_start: Option<(Pixels, f32)>,
    editing: Option<(EditTarget, Entity<InputState>, Subscription)>,
    last_summary: Option<crate::canvas::Summary>,
    palette: Entity<CommandState>,
    focus: FocusHandle,
    subs: Vec<Subscription>,
    dragging_window: bool,
    ui_scale: f32,
    inspector_auto_hide: bool,
    restoring_session: bool,
    suppress_history: bool,
    import_dialect: sql_import::SqlDialect,
}

impl Focusable for Shell {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Aplica claro/escuro ao `gpui-component` e a identidade do app (fonte, acento, raios).
#[expect(
    dead_code,
    reason = "Non-scaled theme helper retained for API completeness"
)]
pub fn apply_theme(mode: Mode, cx: &mut App) {
    apply_theme_scaled(mode, 1.0, cx);
}

/// Aplica tema considerando a escala de UI configurada.
pub fn apply_theme_scaled(mode: Mode, scale: f32, cx: &mut App) {
    Theme::change(
        match mode {
            Mode::Dark => ThemeMode::Dark,
            Mode::Light => ThemeMode::Light,
        },
        None,
        cx,
    );
    let g = Glass::of_scaled(mode, false, scale);
    Theme::update(cx, |t| {
        t.font_family = FONT.into();
        t.font_size = px((13.5 * scale).round());
        t.radius = px((6. * scale).round());
        t.radius_lg = px((10. * scale).round());
        // popovers, menus e tooltips: camada elevada com sombra (como no Zed)
        t.shadow = true;
        // acento em todo lugar que o componente usa "primário" (o botão tem campos próprios)
        let (hover, active) = (g.accent.opacity(0.88), g.accent.opacity(0.76));
        t.colors.primary = g.accent;
        t.colors.primary_hover = hover;
        t.colors.primary_active = active;
        t.colors.primary_foreground = gpui_kit::white();
        t.colors.button_primary = g.accent;
        t.colors.button_primary_hover = hover;
        t.colors.button_primary_active = active;
        t.colors.button_primary_foreground = gpui_kit::white();
        t.colors.ring = g.accent;
        t.colors.popover = g.island.opacity(1.);
        t.colors.popover_foreground = g.text;
    });
}

/// Lista os `.dbml` de uma pasta (até 3 níveis, sem pastas ocultas), em ordem.
fn scan(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            let hidden = p
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'));
            if p.is_dir() && depth < 3 && !hidden {
                walk(&p, depth + 1, out);
            } else if p.extension().is_some_and(|e| e == "dbml") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, 0, &mut out);
    out
}

fn file_title(p: &Path) -> String {
    p.file_name()
        .map_or_else(|| "?".to_string(), |n| n.to_string_lossy().into_owned())
}

impl Shell {
    /// `start`: arquivo, pasta ou nada (abre o exemplo).
    pub fn new(
        start: Option<PathBuf>,
        mode: Mode,
        translucent: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            project: None,
            files: Vec::new(),
            docs: Vec::new(),
            active: 0,
            untitled: 0,
            pane: Pane::Schema,
            sidebar_open: std::env::var("DBM_SIDEBAR").is_ok()
                || crate::settings::load().sidebar_open,
            editor_open: std::env::var("DBM_EDITOR").is_ok(),
            inspector_open: std::env::var("DBM_INSPECTOR").is_ok()
                || crate::settings::load().inspector_open,
            mode,
            translucent,
            widths: {
                let p = crate::settings::load();
                (p.sidebar_w, p.editor_w, p.inspector_w)
            },
            // DBM_TRANSLUCENT: usado pelo harness de screenshot
            translucent_cards: std::env::var("DBM_TRANSLUCENT").is_ok()
                || crate::settings::load().translucent_cards,
            col_type_w: crate::settings::load().col_type_w,
            col_resize_start: None,
            panels_gen: 0,
            last_summary: None,
            editing: None,
            palette: cx.new(|cx| CommandState::new(window, cx)),
            focus: cx.focus_handle(),
            subs: Vec::new(),
            dragging_window: false,
            ui_scale: crate::settings::load().ui_scale,
            inspector_auto_hide: crate::settings::load().inspector_auto_hide,
            restoring_session: false,
            suppress_history: false,
            import_dialect: sql_import::SqlDialect::Postgres,
        };
        // DBM_PANE: painel inicial (usado pelo harness de screenshot)
        this.pane = match std::env::var("DBM_PANE").as_deref() {
            Ok("files") => Pane::Files,
            Ok("problems") => Pane::Problems,
            _ => Pane::Schema,
        };
        let preselect = std::env::var("DBM_SELECT").ok();
        match start {
            Some(p) if p.is_dir() => this.set_project(p, window, cx),
            Some(p) => this.open_path(p, window, cx),
            None => this.restore_session(window, cx),
        }
        // DBM_SELECT: tabela pré-selecionada (harness de screenshot)
        if let Some(t) = preselect {
            this.with_diagram(cx, |d| d.select(&t));
        }
        this
    }

    fn doc(&self) -> Option<&Doc> {
        self.docs.get(self.active)
    }

    fn snapshot_of(doc: &Doc, cx: &App) -> crate::history::DocSnapshot {
        let d = doc.diagram.read(cx);
        let (selected, selected_enum, multi) = d.selection_state();
        crate::history::DocSnapshot {
            text: doc.last_text.clone(),
            pos: d.positions().clone(),
            selected,
            selected_enum,
            multi,
        }
    }

    fn add_doc(
        &mut self,
        text: String,
        path: Option<PathBuf>,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let initial_arc: std::sync::Arc<str> = std::sync::Arc::from(text.as_str());
        let diagram = cx.new(|cx| Diagram::new(dbml_analysis::analyze(&text), cx));
        let editor = cx.new(|cx| {
            let mut st = EditorState::new(window, cx)
                .default_value(text)
                .line_number(true);
            crate::ide::install(&mut st, cx);
            st
        });
        let diags = editor.update(cx, crate::ide::refresh_diagnostics);
        // texto → Schema → canvas, a cada edição
        let sub = cx.subscribe_in(
            &editor,
            window,
            |this: &mut Shell, ed, ev: &InputEvent, _, cx| {
                if !matches!(ev, InputEvent::Change) {
                    return;
                }
                let val = ed.read(cx).value().to_string();
                let schema = dbml_analysis::analyze(&val);
                let diags = ed.update(cx, crate::ide::refresh_diagnostics);
                let suppress = this.suppress_history;
                if let Some(doc) = this.docs.iter_mut().find(|d| &d.editor == ed) {
                    if !suppress && val.as_str() != doc.last_text.as_ref() {
                        let d = doc.diagram.read(cx);
                        let (selected, selected_enum, multi) = d.selection_state();
                        doc.history.push(crate::history::DocSnapshot {
                            text: doc.last_text.clone(),
                            pos: d.positions().clone(),
                            selected,
                            selected_enum,
                            multi,
                        });
                    }
                    doc.last_text = std::sync::Arc::from(val.as_str());
                    doc.dirty = doc.last_text.as_ref() != doc.saved_text.as_ref();
                    doc.diags = diags;
                    if !suppress {
                        doc.diagram.update(cx, |d, cx| {
                            d.set_schema(schema);
                            cx.notify();
                        });
                    }
                }
                cx.notify();
            },
        );
        // repinta o shell só quando muda algo que ele mostra (seleção, zoom, dica...);
        // mover/arrastar no canvas redesenha apenas o canvas
        let obs = cx.observe(&diagram, |this: &mut Shell, d, cx| {
            let sum = d.read(cx).summary();
            if this.last_summary.as_ref() != Some(&sum) {
                let had_selection = this
                    .last_summary
                    .as_ref()
                    .is_some_and(|s| s.0.is_some() || !s.1.is_empty());
                let now_has_selection = sum.0.is_some() || !sum.1.is_empty();
                if now_has_selection && !had_selection {
                    this.inspector_open = true;
                    this.save_prefs();
                }
                this.last_summary = Some(sum);
                this.save_session(cx);
                cx.notify();
            }
        });
        let events = cx.subscribe_in(
            &diagram,
            window,
            |this: &mut Shell, diag_entity, ev: &DiagramEvent, window, cx| match ev {
                DiagramEvent::CreateRef { from, to } => {
                    let (f, t) = (from.clone(), to.clone());
                    this.edit_text(false, window, cx, |text| {
                        Some(dbml_edit::link_tables(text, &f.0, &t.0))
                    });
                }
                DiagramEvent::AddColumn(table) => this.add_column_to(table.clone(), window, cx),
                DiagramEvent::Rename(table) => {
                    this.inspector_open = true;
                    this.start_edit(EditTarget::Table(table.clone()), table.clone(), window, cx);
                }
                DiagramEvent::Select(table) => {
                    let _ = table;
                    this.inspector_open = true;
                    this.save_prefs();
                    cx.notify();
                }
                DiagramEvent::SelectEnum(name) => {
                    let _ = name;
                    this.inspector_open = true;
                    this.save_prefs();
                    cx.notify();
                }
                DiagramEvent::PositionsChanged(old_pos) => {
                    if let Some(doc) = this.docs.iter_mut().find(|d| &d.diagram == diag_entity) {
                        let (selected, selected_enum, multi) =
                            doc.diagram.read(cx).selection_state();
                        doc.history.push(crate::history::DocSnapshot {
                            text: doc.last_text.clone(),
                            pos: old_pos.clone(),
                            selected,
                            selected_enum,
                            multi,
                        });
                    }
                }
            },
        );
        self.subs.extend([sub, obs, events]);
        self.docs.push(Doc {
            path,
            title,
            editor,
            diagram,
            saved_text: initial_arc.clone(),
            last_text: initial_arc,
            history: crate::history::UndoHistory::new(),
            dirty: false,
            diags,
        });
        self.active = self.docs.len() - 1;
        cx.notify();
    }

    fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(i) = self
            .docs
            .iter()
            .position(|d| d.path.as_ref() == Some(&path))
        {
            self.active = i;
            self.save_session(cx);
            cx.notify();
            return;
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let title = file_title(&path);
                self.add_doc(text, Some(path), title, window, cx);
                self.save_session(cx);
            }
            Err(e) => window.push_notification(format!("Could not open file: {e}"), cx),
        }
    }

    fn set_project(&mut self, dir: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.files = scan(&dir);
        self.project = Some(dir);
        if std::env::var("DBM_PANE").is_err() {
            self.pane = Pane::Files;
        }
        if let Some(first) = self.files.first().cloned() {
            self.open_path(first, window, cx);
        }
        self.save_session(cx);
        cx.notify();
    }

    fn restore_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let session = crate::session::load();
        self.restoring_session = true;

        if let Some(dir) = session.project_dir.filter(|d| d.is_dir()) {
            self.files = scan(&dir);
            self.project = Some(dir);
        }

        for path in &session.open_tabs {
            if path.is_file() {
                self.open_path(path.clone(), window, cx);
                if let Some(vp) = session.viewports.get(path)
                    && let Some(doc) = self.docs.last()
                {
                    doc.diagram.update(cx, |d, _| {
                        d.set_viewport(vp.zoom, (vp.pan_x, vp.pan_y));
                    });
                }
            }
        }

        if self.docs.is_empty()
            && self.project.is_some()
            && let Some(first) = self.files.first().cloned()
        {
            self.open_path(first, window, cx);
        }

        if let Some(active_path) = session.active_file.as_ref()
            && let Some(idx) = self
                .docs
                .iter()
                .position(|d| d.path.as_ref() == Some(active_path))
        {
            self.active = idx;
        }

        if std::env::var("DBM_PANE").is_err()
            && let Some(pane) = session.pane.as_deref()
        {
            self.pane = match pane {
                "files" => Pane::Files,
                "problems" => Pane::Problems,
                _ => Pane::Schema,
            };
        }

        if std::env::var("DBM_EDITOR").is_err()
            && let Some(ed) = session.editor_open
        {
            self.editor_open = ed;
        }

        self.restoring_session = false;

        if self.docs.is_empty() && self.project.is_none() {
            let text = include_str!("../../assets/sample.dbml").to_string();
            self.add_doc(text, None, "example.dbml".into(), window, cx);
        }
    }

    fn save_session(&self, cx: &App) {
        if self.restoring_session {
            return;
        }
        let open_tabs: Vec<PathBuf> = self.docs.iter().filter_map(|d| d.path.clone()).collect();
        let active_file = self.doc().and_then(|d| d.path.clone());
        let pane = Some(
            match self.pane {
                Pane::Files => "files",
                Pane::Schema => "schema",
                Pane::Problems => "problems",
            }
            .to_string(),
        );
        let mut viewports = std::collections::BTreeMap::new();
        for doc in &self.docs {
            if let Some(path) = &doc.path {
                let (zoom, pan_x, pan_y) = doc.diagram.read(cx).viewport();
                viewports.insert(
                    path.clone(),
                    crate::session::ViewportState { zoom, pan_x, pan_y },
                );
            }
        }
        crate::session::save(&crate::session::SessionState {
            version: 1,
            project_dir: self.project.clone(),
            open_tabs,
            active_file,
            pane,
            editor_open: Some(self.editor_open),
            viewports,
        });
    }

    fn with_diagram(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Diagram)) {
        if let Some(doc) = self.docs.get(self.active) {
            doc.diagram.update(cx, |d, cx| {
                f(d);
                cx.notify();
            });
        }
    }

    // ---- ações: arquivos e abas ----

    fn new_file(&mut self, _: &NewFile, window: &mut Window, cx: &mut Context<Self>) {
        self.untitled += 1;
        let title = format!("untitled-{}.dbml", self.untitled);
        self.add_doc(String::new(), None, title, window, cx);
        self.editor_open = true;
        self.sidebar_open = false;
        self.save_prefs();
    }

    #[expect(
        clippy::unused_self,
        reason = "GPUI cx.listener requires &mut self on action handlers"
    )]
    fn open_file(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Open".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                for p in paths {
                    this.open_path(p, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    #[expect(
        clippy::unused_self,
        reason = "GPUI cx.listener requires &mut self on action handlers"
    )]
    fn open_folder(&mut self, _: &OpenFolder, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open Project".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(dir) = paths.into_iter().next() else {
                return;
            };
            this.update_in(cx, |this, window, cx| this.set_project(dir, window, cx))
                .ok();
        })
        .detach();
    }

    fn save_file(&mut self, _: &SaveFile, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc() else { return };
        if let Some(path) = doc.path.clone() {
            self.write(path, window, cx);
            return;
        }
        let dir = self
            .project
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let rx = cx.prompt_for_new_path(&dir, Some(&doc.title));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = rx.await else { return };
            this.update_in(cx, |this, window, cx| this.write(path, window, cx))
                .ok();
        })
        .detach();
    }

    fn write(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        let text = doc.editor.read(cx).value().to_string();
        match std::fs::write(&path, &text) {
            Ok(()) => {
                window.push_notification(format!("Saved to {}", path.display()), cx);
                doc.title = file_title(&path);
                doc.path = Some(path);
                let saved: std::sync::Arc<str> = std::sync::Arc::from(text.as_str());
                doc.saved_text = saved.clone();
                doc.last_text = saved;
                doc.dirty = false;
                if let Some(dir) = self.project.clone() {
                    self.files = scan(&dir);
                }
                self.save_session(cx);
            }
            Err(e) => window.push_notification(format!("Could not save file: {e}"), cx),
        }
        cx.notify();
    }

    fn force_close_tab_at(&mut self, i: usize, cx: &mut Context<Self>) {
        if i >= self.docs.len() {
            return;
        }
        self.docs.remove(i);
        if self.active >= self.docs.len() {
            self.active = self.docs.len().saturating_sub(1);
        } else if i < self.active {
            self.active -= 1;
        }
        self.save_session(cx);
        cx.notify();
    }

    fn save_and_close_tab_at(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.docs.get(i) else {
            return;
        };
        if let Some(path) = doc.path.clone() {
            let text = doc.editor.read(cx).value().to_string();
            match std::fs::write(&path, &text) {
                Ok(()) => {
                    window.push_notification(format!("Saved to {}", path.display()), cx);
                    self.force_close_tab_at(i, cx);
                }
                Err(e) => window.push_notification(format!("Could not save file: {e}"), cx),
            }
            return;
        }
        let dir = self
            .project
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let rx = cx.prompt_for_new_path(&dir, Some(&doc.title));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = rx.await else { return };
            this.update_in(cx, |this, window, cx| {
                let Some(doc) = this.docs.get(i) else { return };
                let text = doc.editor.read(cx).value().to_string();
                match std::fs::write(&path, &text) {
                    Ok(()) => {
                        window.push_notification(format!("Saved to {}", path.display()), cx);
                        this.force_close_tab_at(i, cx);
                    }
                    Err(e) => window.push_notification(format!("Could not save file: {e}"), cx),
                }
            })
            .ok();
        })
        .detach();
    }

    fn close_tab_at(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.docs.get(i) else {
            return;
        };
        if !doc.dirty {
            self.force_close_tab_at(i, cx);
            return;
        }
        let title = doc.title.clone();
        let shell = cx.entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let title = title.clone();
            let shell = shell.clone();
            dialog
                .w(px(440.))
                .title("Unsaved Changes")
                .content(move |content, _, _| {
                    let shell_save = shell.clone();
                    let shell_discard = shell.clone();
                    content.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(16.))
                            .child(div().text_size(TEXT_SM).child(format!(
                                "Do you want to save the changes made to \"{title}\"?"
                            )))
                            .child(
                                div()
                                    .flex()
                                    .justify_end()
                                    .gap(px(8.))
                                    .child(
                                        Button::new("dlg-cancel-close")
                                            .ghost()
                                            .small()
                                            .label("Cancel")
                                            .on_click(|_, window, cx| {
                                                window.close_dialog(cx);
                                            }),
                                    )
                                    .child(
                                        Button::new("dlg-discard-close")
                                            .ghost()
                                            .small()
                                            .label("Discard changes")
                                            .on_click(move |_, window, cx| {
                                                window.close_dialog(cx);
                                                shell_discard.update(cx, |s, cx| {
                                                    s.force_close_tab_at(i, cx);
                                                });
                                            }),
                                    )
                                    .child(
                                        Button::new("dlg-save-close")
                                            .primary()
                                            .small()
                                            .label("Save")
                                            .on_click(move |_, window, cx| {
                                                window.close_dialog(cx);
                                                shell_save.update(cx, |s, cx| {
                                                    s.save_and_close_tab_at(i, window, cx);
                                                });
                                            }),
                                    ),
                            ),
                    )
                })
        });
    }

    fn close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
        self.close_tab_at(self.active, window, cx);
    }

    fn next_tab(&mut self, _: &NextTab, _: &mut Window, cx: &mut Context<Self>) {
        if !self.docs.is_empty() {
            self.active = (self.active + 1) % self.docs.len();
            self.save_session(cx);
            cx.notify();
        }
    }

    fn prev_tab(&mut self, _: &PrevTab, _: &mut Window, cx: &mut Context<Self>) {
        if !self.docs.is_empty() {
            self.active = (self.active + self.docs.len() - 1) % self.docs.len();
            self.save_session(cx);
            cx.notify();
        }
    }

    fn toggle_left_pane(&mut self, target: Option<Pane>, cx: &mut Context<Self>) {
        match target {
            Some(pane) => {
                // clicar no painel já aberto recolhe a barra lateral; caso contrário abre exclusivamente
                if self.sidebar_open && !self.editor_open && self.pane == pane {
                    self.sidebar_open = false;
                } else {
                    self.pane = pane;
                    self.sidebar_open = true;
                    self.editor_open = false;
                }
            }
            None => {
                // alternar editor DBML de forma exclusiva com os outros painéis esquerdos
                if self.editor_open {
                    self.editor_open = false;
                } else {
                    self.editor_open = true;
                    self.sidebar_open = false;
                }
            }
        }
        self.save_prefs();
        self.save_session(cx);
        cx.notify();
    }

    fn show_pane(&mut self, pane: Pane, cx: &mut Context<Self>) {
        self.toggle_left_pane(Some(pane), cx);
    }

    fn show_files(&mut self, _: &ShowFiles, _: &mut Window, cx: &mut Context<Self>) {
        self.show_pane(Pane::Files, cx);
    }

    fn show_schema(&mut self, _: &ShowSchema, _: &mut Window, cx: &mut Context<Self>) {
        self.show_pane(Pane::Schema, cx);
    }

    fn show_problems(&mut self, _: &ShowProblems, _: &mut Window, cx: &mut Context<Self>) {
        self.show_pane(Pane::Problems, cx);
    }

    // ---- ações: editor ----

    fn format_document(&mut self, _: &FormatDocument, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc() else { return };
        let text = doc.editor.read(cx).value().to_string();
        let formatted = dbml_edit::format(&text);
        if formatted != text {
            // replace_all entra no histórico: Ctrl+Z desfaz a formatação
            doc.editor
                .update(cx, |e, cx| e.replace_all(formatted, window, cx));
        }
    }

    /// Abre o diálogo de salvar (com fallback no Linux) e grava `bytes`. `build` roda depois que o
    /// usuário escolhe o arquivo, para cancelar não custar nada. Sempre avisa sucesso ou falha.
    fn save_export(
        &mut self,
        suggested: &str,
        what: String,
        build: impl FnOnce() -> Result<Vec<u8>, String> + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dir = Self::resolve_export_dir(
            self.project.as_ref(),
            std::env::var_os("HOME").as_deref(),
            std::env::var_os("USERPROFILE").as_deref(),
        );
        let rx = cx.prompt_for_new_path(&dir, Some(suggested));
        let fallback_default = dir.join(suggested);
        cx.spawn_in(window, async move |_, cx| {
            let path = match rx.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) | Err(_) => return,
                Ok(Err(portal_err)) => {
                    if let Some(p) = Self::linux_save_dialog_fallback(&fallback_default) {
                        p
                    } else {
                        let _ = cx.update(|window, cx| {
                            window.push_notification(
                                format!("File dialog unavailable ({portal_err})"),
                                cx,
                            );
                        });
                        return;
                    }
                }
            };
            let res =
                build().and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()));
            cx.update(|window, cx| match res {
                Ok(()) => {
                    window.push_notification(format!("Exported {what} to {}", path.display()), cx);
                }
                Err(e) => window.push_notification(format!("Failed to export {what}: {e}"), cx),
            })
            .ok();
        })
        .detach();
    }

    fn export_stem(&self) -> String {
        self.doc()
            .and_then(|d| {
                std::path::Path::new(&d.title)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "schema".to_owned())
    }

    fn export_sql(
        &mut self,
        dialect: dbml_analysis::SqlDialect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.doc() else { return };
        let text = doc.editor.read(cx).value().to_string();
        let suggested = format!("{}.{}.sql", self.export_stem(), dialect.as_str());
        self.save_export(
            &suggested,
            format!("SQL ({})", dialect.as_str()),
            move || Ok(dbml_analysis::export_sql_ddl(&text, dialect).into_bytes()),
            window,
            cx,
        );
    }

    fn export_mermaid(&mut self, _: &ExportMermaid, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc() else { return };
        let schema = doc.diagram.read(cx).schema().clone();
        let suggested = format!("{}.mmd", self.export_stem());
        self.save_export(
            &suggested,
            "Mermaid".into(),
            move || Ok(diagram_export::to_mermaid(&schema).into_bytes()),
            window,
            cx,
        );
    }

    fn export_image(&mut self, kind: ImageKind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc() else { return };
        let (schema, pos) = {
            let d = doc.diagram.read(cx);
            (d.schema().clone(), d.positions().clone())
        };
        let theme = if self.mode == Mode::Dark {
            diagram_export::Theme::dark()
        } else {
            diagram_export::Theme::light()
        };
        let suggested = format!("{}.{}", self.export_stem(), kind.extension());
        self.save_export(
            &suggested,
            kind.label().into(),
            move || {
                let svg = diagram_export::to_svg(&schema, &pos, &theme);
                let fonts: Vec<&[u8]> = crate::FONTS.to_vec();
                match kind {
                    ImageKind::Svg => Ok(svg.into_bytes()),
                    ImageKind::Png => diagram_export::svg_to_png(&svg, &fonts, 2.0),
                    ImageKind::Jpeg => diagram_export::svg_to_jpeg(&svg, &fonts, 2.0, 92),
                }
                .map_err(|e| e.to_string())
            },
            window,
            cx,
        );
    }

    fn export_png(&mut self, _: &ExportPng, w: &mut Window, cx: &mut Context<Self>) {
        self.export_image(ImageKind::Png, w, cx);
    }

    fn export_jpeg(&mut self, _: &ExportJpeg, w: &mut Window, cx: &mut Context<Self>) {
        self.export_image(ImageKind::Jpeg, w, cx);
    }

    fn export_svg(&mut self, _: &ExportSvg, w: &mut Window, cx: &mut Context<Self>) {
        self.export_image(ImageKind::Svg, w, cx);
    }

    fn export_sql_postgres(
        &mut self,
        _: &ExportSqlPostgres,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_sql(dbml_analysis::SqlDialect::Postgres, w, cx);
    }

    fn export_sql_mysql(&mut self, _: &ExportSqlMysql, w: &mut Window, cx: &mut Context<Self>) {
        self.export_sql(dbml_analysis::SqlDialect::Mysql, w, cx);
    }

    fn export_sql_sqlite(&mut self, _: &ExportSqlSqlite, w: &mut Window, cx: &mut Context<Self>) {
        self.export_sql(dbml_analysis::SqlDialect::Sqlite, w, cx);
    }

    fn export_sql_sqlserver(
        &mut self,
        _: &ExportSqlSqlServer,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_sql(dbml_analysis::SqlDialect::SqlServer, w, cx);
    }

    fn export_sql_oracle(&mut self, _: &ExportSqlOracle, w: &mut Window, cx: &mut Context<Self>) {
        self.export_sql(dbml_analysis::SqlDialect::Oracle, w, cx);
    }

    fn resolve_export_dir(
        project: Option<&PathBuf>,
        home: Option<&std::ffi::OsStr>,
        userprofile: Option<&std::ffi::OsStr>,
    ) -> PathBuf {
        project
            .cloned()
            .or_else(|| home.or(userprofile).map(PathBuf::from))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
    }

    #[cfg(target_os = "linux")]
    fn linux_save_dialog_fallback(default_path: &std::path::Path) -> Option<PathBuf> {
        let default_str = default_path.to_string_lossy();
        if let Ok(out) = std::process::Command::new("zenity")
            .args([
                "--file-selection",
                "--save",
                "--confirm-overwrite",
                &format!("--filename={default_str}"),
            ])
            .output()
            && out.status.success()
        {
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !p.is_empty() {
                return Some(PathBuf::from(p));
            }
        }
        if let Ok(out) = std::process::Command::new("kdialog")
            .args(["--getsavefilename", &default_str, "*.sql|SQL files"])
            .output()
            && out.status.success()
        {
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !p.is_empty() {
                return Some(PathBuf::from(p));
            }
        }
        None
    }

    #[cfg(not(target_os = "linux"))]
    fn linux_save_dialog_fallback(_default_path: &std::path::Path) -> Option<PathBuf> {
        None
    }

    /// Aplica um preset de colunas à tabela selecionada (um passo de undo via `edit_text`).
    pub(crate) fn apply_column_preset(
        &mut self,
        preset: dbml_edit::ColumnPreset,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.doc() else { return };
        let Some(table) = doc.diagram.read(cx).selection_state().0 else {
            window.push_notification("Select a table to add a column preset", cx);
            return;
        };
        let text = doc.editor.read(cx).value().to_string();
        match dbml_edit::add_column_preset_text(&text, &table, preset) {
            None => window.push_notification(format!("Table '{table}' not found"), cx),
            Some(new) if new == text => {
                window.push_notification(format!("{}: columns already exist", preset.label()), cx);
            }
            Some(new) => self.edit_text(false, window, cx, move |_| Some(new)),
        }
    }

    fn add_preset_uuid_pk(&mut self, _: &AddPresetUuidPk, w: &mut Window, cx: &mut Context<Self>) {
        self.apply_column_preset(dbml_edit::ColumnPreset::UuidPk, w, cx);
    }

    fn add_preset_bigint_pk(
        &mut self,
        _: &AddPresetBigIntPk,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_column_preset(dbml_edit::ColumnPreset::BigIntPk, w, cx);
    }

    fn add_preset_timestamps(
        &mut self,
        _: &AddPresetTimestamps,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_column_preset(dbml_edit::ColumnPreset::Timestamps, w, cx);
    }

    fn add_preset_soft_delete(
        &mut self,
        _: &AddPresetSoftDelete,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_column_preset(dbml_edit::ColumnPreset::SoftDelete, w, cx);
    }

    /// "Connect to Database…": lê o catálogo de um banco (somente leitura) e gera o DBML.
    fn connect_database(
        &mut self,
        _: &ConnectDatabase,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::connect_db::{
            ConnectModel, Status, introspect_blocking, parse_schemas, test_blocking,
        };
        use db_introspect::{ConnectionParams, Dialect};

        let model = cx.new(|_| ConnectModel::new(Dialect::Postgres));
        let input = |window: &mut Window, cx: &mut Context<Self>, ph: &'static str| {
            cx.new(|cx| InputState::new(window, cx).placeholder(ph))
        };
        let host = input(window, cx, "localhost");
        let port = input(window, cx, "default port");
        let database = input(window, cx, "database name");
        let user = input(window, cx, "user");
        let password = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("password")
                .masked(true)
        });
        let path = input(window, cx, "/path/to/database.sqlite");
        let schemas = input(window, cx, "all schemas (or: public, sales)");
        let shell_entity = cx.entity().clone();
        let has_existing = self
            .doc()
            .is_some_and(|d| !d.editor.read(cx).value().trim().is_empty());
        let g = Glass::of_scaled(self.mode, self.translucent, self.ui_scale);

        window.open_dialog(cx, move |dialog, _, _| {
            let model = model.clone();
            let shell_entity = shell_entity.clone();
            let (host, port, database, user, password, path, schemas) = (
                host.clone(),
                port.clone(),
                database.clone(),
                user.clone(),
                password.clone(),
                path.clone(),
                schemas.clone(),
            );
            dialog
                .w(px(560.))
                .title("Connect to Database")
                .content(move |content, _, cx| {
                    let st = model.read(cx).clone();
                    let is_file = st.dialect == Dialect::Sqlite;
                    let connecting = st.status == Status::Connecting;

                    let engine_btn = |id: &'static str, label: &'static str, d: Dialect| {
                        let model = model.clone();
                        let b = Button::new(id).small().label(label);
                        let b = if st.dialect == d {
                            b.primary()
                        } else {
                            b.outline()
                        };
                        b.disabled(connecting).on_click(move |_, _, cx| {
                            model.update(cx, |m, cx| {
                                m.set_dialect(d);
                                cx.notify();
                            });
                        })
                    };
                    let row = |label: &'static str, field: Entity<InputState>| {
                        div()
                            .flex()
                            .items_center()
                            .gap(SP_SM)
                            .child(
                                div()
                                    .w(px(84.))
                                    .flex_none()
                                    .text_size(TEXT_XS)
                                    .text_color(g.text_muted)
                                    .child(label),
                            )
                            .child(div().flex_1().child(Input::new(&field).small()))
                    };

                    // Button factory: `test` only checks credentials and lists schemas.
                    let action_btn = |id: &'static str, test: bool| {
                        let (model, host, port, database, user, password, path, schemas) = (
                            model.clone(),
                            host.clone(),
                            port.clone(),
                            database.clone(),
                            user.clone(),
                            password.clone(),
                            path.clone(),
                            schemas.clone(),
                        );
                        let label = match (test, connecting) {
                            (true, _) => "Test connection",
                            (false, true) => "Connecting…",
                            (false, false) => "Connect",
                        };
                        let b = Button::new(id).small().label(label).disabled(connecting);
                        let b = if test { b.outline() } else { b.primary() };
                        b.on_click(move |_, window, cx| {
                            let m = model.read(cx).clone();
                            let mut params = ConnectionParams::new(m.dialect);
                            params.host = host.read(cx).value().to_string();
                            params.port = port.read(cx).value().trim().parse().ok();
                            params.database = database.read(cx).value().to_string();
                            params.user = user.read(cx).value().to_string();
                            params.password = password.read(cx).value().to_string();
                            params.path = path.read(cx).value().to_string();
                            params.trust_server_certificate = m.trust_cert;
                            let filter = parse_schemas(&schemas.read(cx).value());
                            model.update(cx, |m, cx| {
                                m.reset_result();
                                m.status = Status::Connecting;
                                cx.notify();
                            });
                            let model = model.clone();
                            window
                                .spawn(cx, async move |cx| {
                                    let exec = cx.background_executor().clone();
                                    if test {
                                        let result = exec
                                            .spawn(async move { test_blocking(&params, filter) })
                                            .await;
                                        cx.update(|_, cx| {
                                            model.update(cx, |m, cx| {
                                                m.finish_test(result);
                                                cx.notify();
                                            });
                                        })
                                        .ok();
                                    } else {
                                        let result = exec
                                            .spawn(
                                                async move { introspect_blocking(&params, filter) },
                                            )
                                            .await;
                                        cx.update(|_, cx| {
                                            model.update(cx, |m, cx| {
                                                m.finish(result);
                                                cx.notify();
                                            });
                                        })
                                        .ok();
                                    }
                                })
                                .detach();
                        })
                    };
                    let connect_btn = action_btn("db-connect", false);
                    let test_btn = action_btn("db-test", true);

                    let can_apply = st.can_apply();
                    let apply_btn =
                        |id: &'static str, label: String, replace: bool, primary: bool| {
                            let model = model.clone();
                            let shell_entity = shell_entity.clone();
                            let mut b = Button::new(id).small().label(label).disabled(!can_apply);
                            b = if primary { b.primary() } else { b.outline() };
                            b.on_click(move |_, window, cx| {
                                if replace && has_existing && !model.read(cx).confirm_replace {
                                    model.update(cx, |m, cx| {
                                        m.confirm_replace = true;
                                        cx.notify();
                                    });
                                    return;
                                }
                                let current = model.read(cx).clone();
                                shell_entity.update(cx, |s, cx| {
                                    s.edit_text(replace, window, cx, move |existing| {
                                        Some(current.build_merged_source(existing, replace))
                                    });
                                    if let Some(doc) = s.doc() {
                                        doc.diagram.update(cx, |d, cx| {
                                            d.fit();
                                            cx.notify();
                                        });
                                    }
                                });
                                window.close_dialog(cx);
                            })
                        };
                    let replace_label = if st.confirm_replace {
                        "Confirm Replace?".to_string()
                    } else {
                        "Replace".to_string()
                    };

                    let status_line = match &st.status {
                        Status::Idle => {
                            div().child("Read-only: only the catalog is read, never your data.")
                        }
                        Status::Connecting => div().child("Connecting and reading the schema…"),
                        Status::Failed(msg) => div()
                            .text_color(rgb(crate::glass::DIAG_ERROR))
                            .child(msg.clone()),
                        Status::Tested(msg) => div().child(msg.clone()),
                        Status::Ready => div().child(format!(
                            "Found {} table{}. Replace the document or insert them.",
                            st.tables,
                            if st.tables == 1 { "" } else { "s" }
                        )),
                    };

                    let mut form = div().flex().flex_col().gap(SP_SM);
                    if is_file {
                        let pick = {
                            let path = path.clone();
                            Button::new("db-pick-file")
                                .small()
                                .outline()
                                .label("Browse…")
                                .on_click(move |_, window, cx| {
                                    let rx = cx.prompt_for_paths(PathPromptOptions {
                                        files: true,
                                        directories: false,
                                        multiple: false,
                                        prompt: Some("Select SQLite database".into()),
                                    });
                                    let path = path.clone();
                                    window
                                        .spawn(cx, async move |cx| {
                                            let Ok(Ok(Some(paths))) = rx.await else {
                                                return;
                                            };
                                            let Some(p) = paths.into_iter().next() else {
                                                return;
                                            };
                                            cx.update(|window, cx| {
                                                path.update(cx, |e, cx| {
                                                    e.replace_all(
                                                        p.display().to_string(),
                                                        window,
                                                        cx,
                                                    );
                                                });
                                            })
                                            .ok();
                                        })
                                        .detach();
                                })
                        };
                        form = form.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(SP_SM)
                                .child(div().flex_1().child(row("File", path.clone())))
                                .child(pick),
                        );
                    } else {
                        form = form
                            .child(row("Host", host.clone()))
                            .child(row("Port", port.clone()))
                            .child(row("Database", database.clone()))
                            .child(row("User", user.clone()))
                            .child(row("Password", password.clone()))
                            .child(row("Schemas", schemas.clone()));
                        if st.dialect == Dialect::SqlServer {
                            let model = model.clone();
                            form = form.child(
                                Checkbox::new("db-trust")
                                    .label("Trust server certificate (self-signed, e.g. Docker)")
                                    .checked(st.trust_cert)
                                    .on_click(move |checked, _, cx| {
                                        let checked = *checked;
                                        model.update(cx, |m, cx| {
                                            m.trust_cert = checked;
                                            cx.notify();
                                        });
                                    }),
                            );
                        }
                    }

                    content.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(SP_MD)
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(engine_btn("db-pg", "PostgreSQL", Dialect::Postgres))
                                    .child(engine_btn("db-my", "MySQL", Dialect::Mysql))
                                    .child(engine_btn("db-ms", "SQL Server", Dialect::SqlServer))
                                    .child(engine_btn("db-sq", "SQLite", Dialect::Sqlite))
                                    .child(
                                        Button::new("db-or")
                                            .small()
                                            .outline()
                                            .label("Oracle (soon)")
                                            .disabled(true),
                                    ),
                            )
                            .child(form)
                            .child(
                                div()
                                    .id("db-status")
                                    .max_h(px(72.))
                                    .overflow_y_scroll()
                                    .text_size(TEXT_XS)
                                    .text_color(g.text_muted)
                                    .child(status_line),
                            )
                            .when(!st.warnings.is_empty(), |d| {
                                let mut list = div()
                                    .id("db-warn")
                                    .flex()
                                    .flex_col()
                                    .max_h(px(64.))
                                    .overflow_y_scroll()
                                    .text_size(TEXT_XS)
                                    .text_color(rgb(crate::glass::DIAG_WARNING));
                                for w in &st.warnings {
                                    list = list.child(w.clone());
                                }
                                d.child(list)
                            })
                            .child(
                                div()
                                    .flex()
                                    .justify_end()
                                    .gap(SP_SM)
                                    .child(
                                        Button::new("db-cancel")
                                            .small()
                                            .outline()
                                            .label("Cancel")
                                            .on_click(|_, window, cx| window.close_dialog(cx)),
                                    )
                                    .child(test_btn)
                                    .child(connect_btn)
                                    .child(apply_btn("db-replace", replace_label, true, false))
                                    .child(apply_btn(
                                        "db-insert",
                                        "Insert".to_string(),
                                        false,
                                        true,
                                    )),
                            ),
                    )
                })
        });
    }

    fn import_sql_ddl(&mut self, _: &ImportSqlDdl, window: &mut Window, cx: &mut Context<Self>) {
        let model = cx.new(|_| crate::import_sql::ImportSqlModel::new(self.import_dialect));
        let sql_editor = cx.new(|cx| EditorState::new(window, cx).line_number(true));
        let shell_entity = cx.entity().clone();
        let has_existing = self
            .doc()
            .is_some_and(|d| !d.editor.read(cx).value().trim().is_empty());

        let sub = cx.subscribe_in(&sql_editor, window, {
            let model = model.clone();
            move |_, ed, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    let sql = ed.read(cx).value().to_string();
                    model.update(cx, |m, cx| {
                        m.set_sql(sql);
                        cx.notify();
                    });
                }
            }
        });
        self.subs.push(sub);

        let g = Glass::of_scaled(self.mode, self.translucent, self.ui_scale);
        window.open_dialog(cx, move |dialog, _, _| {
            let model = model.clone();
            let sql_editor = sql_editor.clone();
            let shell_entity = shell_entity.clone();
            dialog
                .w(px(760.))
                .title("Import SQL DDL")
                .content(move |content, _, cx| {
                    let st = model.read(cx).clone();
                    let can_apply = st.can_apply();
                    let preview_text = if st.dbml.is_empty() {
                        "// DBML preview will appear here…".to_string()
                    } else {
                        st.dbml.clone()
                    };

                    let dialect_btn = |id: &'static str, label: &'static str, d: sql_import::SqlDialect| {
                        let active = st.dialect == d;
                        let model = model.clone();
                        let shell_entity = shell_entity.clone();
                        let b = Button::new(id).small().label(label);
                        let b = if active { b.primary() } else { b.outline() };
                        b.on_click(move |_, _, cx| {
                            shell_entity.update(cx, |s, _| s.import_dialect = d);
                            model.update(cx, |m, cx| {
                                m.set_dialect(d);
                                cx.notify();
                            });
                        })
                    };

                    let pick_btn = {
                        let sql_editor = sql_editor.clone();
                        let model = model.clone();
                        Button::new("imp-pick-file")
                            .small()
                            .outline()
                            .label("Choose .sql file…")
                            .on_click(move |_, window, cx| {
                                let rx = cx.prompt_for_paths(PathPromptOptions {
                                    files: true,
                                    directories: false,
                                    multiple: false,
                                    prompt: Some("Select SQL file".into()),
                                });
                                let sql_editor = sql_editor.clone();
                                let model = model.clone();
                                window.spawn(cx, async move |cx| {
                                    let Ok(Ok(Some(paths))) = rx.await else { return };
                                    let Some(path) = paths.into_iter().next() else { return };
                                    let Ok(bytes) = std::fs::read(&path) else { return };
                                    let content = String::from_utf8_lossy(&bytes).into_owned();
                                    cx.update(|window, cx| {
                                        if content.len() <= crate::import_sql::MAX_IMPORT_BYTES {
                                            sql_editor.update(cx, |e, cx| {
                                                e.replace_all(content.clone(), window, cx);
                                            });
                                        }
                                        model.update(cx, |m, cx| {
                                            m.set_sql(content);
                                            cx.notify();
                                        });
                                    })
                                    .ok();
                                })
                                .detach();
                            })
                    };

                    let apply_btn = |id: &'static str, label: String, replace: bool, primary: bool| {
                        let model = model.clone();
                        let shell_entity = shell_entity.clone();
                        let mut b = Button::new(id).small().label(label).disabled(!can_apply);
                        b = if primary { b.primary() } else { b.outline() };
                        b.on_click(move |_, window, cx| {
                            if replace && has_existing && !model.read(cx).confirm_replace {
                                model.update(cx, |m, cx| {
                                    m.confirm_replace = true;
                                    cx.notify();
                                });
                                return;
                            }
                            let current = model.read(cx).clone();
                            shell_entity.update(cx, |s, cx| {
                                // replace refaz o layout; append mantém os cartões existentes
                                // e o canvas coloca os novos em área livre
                                s.edit_text(replace, window, cx, move |existing| {
                                    Some(current.build_merged_source(existing, replace))
                                });
                                if let Some(doc) = s.doc() {
                                    doc.diagram.update(cx, |d, cx| {
                                        d.fit();
                                        cx.notify();
                                    });
                                }
                            });
                            window.close_dialog(cx);
                        })
                    };

                    let replace_label = if st.confirm_replace {
                        "Confirm Replace?".to_string()
                    } else {
                        "Replace".to_string()
                    };

                    content.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(SP_MD)
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .justify_between()
                                    .gap(SP_SM)
                                    .child(
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .items_center()
                                            .gap(px(6.))
                                            .child(dialect_btn("imp-pg", "PostgreSQL", sql_import::SqlDialect::Postgres))
                                            .child(dialect_btn("imp-my", "MySQL", sql_import::SqlDialect::Mysql))
                                            .child(dialect_btn("imp-sq", "SQLite", sql_import::SqlDialect::Sqlite))
                                            .child(dialect_btn("imp-ms", "SQL Server", sql_import::SqlDialect::SqlServer))
                                            .child(dialect_btn("imp-or", "Oracle", sql_import::SqlDialect::Oracle)),
                                    )
                                    .child(pick_btn),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(SP_SM)
                                    .h(px(280.))
                                    .child(
                                        div()
                                            .flex_1()
                                            .h_full()
                                            .rounded(px(6.))
                                            .border_1()
                                            .border_color(g.edge)
                                            .overflow_hidden()
                                            .child(Editor::new(&sql_editor)),
                                    )
                                    .child(
                                        div()
                                            .id("imp-preview-scroll")
                                            .flex_1()
                                            .h_full()
                                            .p(SP_SM)
                                            .rounded(px(6.))
                                            .border_1()
                                            .border_color(g.edge)
                                            .bg(g.sidebar)
                                            .overflow_y_scroll()
                                            .font_family("JetBrains Mono")
                                            .text_size(TEXT_XS)
                                            .text_color(if st.dbml.is_empty() { g.text_faint } else { g.text })
                                            .child(preview_text),
                                    ),
                            )
                            .when(st.too_large, |d| {
                                d.child(
                                    div()
                                        .text_size(TEXT_XS)
                                        .text_color(rgb(crate::glass::DIAG_WARNING))
                                        .child("Input exceeds 5 MB limit; please split the dump into smaller files."),
                                )
                            })
                            .when(!st.too_large && !st.warnings.is_empty(), |d| {
                                let mut list = div()
                                    .id("imp-warn-scroll")
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .max_h(px(72.))
                                    .overflow_y_scroll()
                                    .text_size(TEXT_XS)
                                    .text_color(rgb(crate::glass::DIAG_WARNING));
                                for w in &st.warnings {
                                    list = list.child(format!("{}:{}: {}", w.line, w.column, w.message));
                                }
                                d.child(list)
                            })
                            .child(
                                div()
                                    .flex()
                                    .justify_end()
                                    .gap(SP_SM)
                                    .child(
                                        Button::new("imp-cancel")
                                            .small()
                                            .outline()
                                            .label("Cancel")
                                            .on_click(|_, window, cx| window.close_dialog(cx)),
                                    )
                                    .child(apply_btn("imp-replace", replace_label, true, false))
                                    .child(apply_btn("imp-insert", "Insert".to_string(), false, true)),
                            ),
                    )
                })
        });
    }

    fn move_line(&mut self, up: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc() else { return };
        doc.editor.update(cx, |e, cx| {
            let text = e.value().to_string();
            let pos = e.cursor_position();
            let Some((range, new)) = dbml_edit::swap_lines(&text, pos.line as usize, up) else {
                return;
            };
            e.set_selected_range(range, cx);
            e.replace(new, window, cx);
            let line = if up { pos.line - 1 } else { pos.line + 1 };
            e.set_cursor_position(
                lsp_types::Position {
                    line,
                    character: pos.character,
                },
                window,
                cx,
            );
        });
    }

    fn move_line_up(&mut self, _: &MoveLineUp, window: &mut Window, cx: &mut Context<Self>) {
        self.move_line(true, window, cx);
    }

    fn move_line_down(&mut self, _: &MoveLineDown, window: &mut Window, cx: &mut Context<Self>) {
        self.move_line(false, window, cx);
    }

    /// Abre o editor e põe o cursor no início do diagnóstico.
    fn reveal(&mut self, offset: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.editor_open = true;
        self.sidebar_open = false;
        self.save_prefs();
        if let Some(doc) = self.doc() {
            doc.editor.update(cx, |e, cx| {
                let text = e.value().to_string();
                let before = &text[..offset.min(text.len())];
                let line = before.matches('\n').count() as u32;
                let character = before.rsplit('\n').next().unwrap_or("").chars().count() as u32;
                e.set_cursor_position(lsp_types::Position { line, character }, window, cx);
            });
        }
        cx.notify();
    }

    // ---- ações: janela ----

    fn quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        if !self.docs.iter().any(|d| d.dirty) {
            cx.quit();
            return;
        }
        let shell = cx.entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let shell = shell.clone();
            dialog
                .w(px(460.))
                .title("Unsaved Changes")
                .content(move |content, _, _| {
                    let shell_save = shell.clone();
                    content.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(16.))
                            .child(div().text_size(TEXT_SM).child(
                                "You have unsaved changes in open documents. Save before quitting?",
                            ))
                            .child(
                                div()
                                    .flex()
                                    .justify_end()
                                    .gap(px(8.))
                                    .child(
                                        Button::new("dlg-cancel-quit")
                                            .ghost()
                                            .small()
                                            .label("Cancel")
                                            .on_click(|_, window, cx| {
                                                window.close_dialog(cx);
                                            }),
                                    )
                                    .child(
                                        Button::new("dlg-discard-quit")
                                            .ghost()
                                            .small()
                                            .label("Discard changes")
                                            .on_click(|_, window, cx| {
                                                window.close_dialog(cx);
                                                cx.quit();
                                            }),
                                    )
                                    .child(
                                        Button::new("dlg-save-quit")
                                            .primary()
                                            .small()
                                            .label("Save All & Quit")
                                            .on_click(move |_, window, cx| {
                                                window.close_dialog(cx);
                                                let ok = shell_save.update(cx, |s, cx| {
                                                    for d in &mut s.docs {
                                                        if d.dirty
                                                            && let Some(p) = &d.path
                                                        {
                                                            let t = d
                                                                .editor
                                                                .read(cx)
                                                                .value()
                                                                .to_string();
                                                            if std::fs::write(p, &t).is_ok() {
                                                                let arc: std::sync::Arc<str> =
                                                                    std::sync::Arc::from(
                                                                        t.as_str(),
                                                                    );
                                                                d.saved_text = arc.clone();
                                                                d.last_text = arc;
                                                                d.dirty = false;
                                                            }
                                                        }
                                                    }
                                                    !s.docs.iter().any(|d| d.dirty)
                                                });
                                                if ok {
                                                    cx.quit();
                                                }
                                            }),
                                    ),
                            ),
                    )
                })
        });
    }

    fn open_palette(&mut self, _: &OpenPalette, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.palette.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            let state = state.clone();
            dialog
                .w(px(600.))
                .margin_top(px(80.))
                .content(move |content, _, _| {
                    let mut groups: Vec<(&str, Vec<CommandItem>)> = Vec::new();
                    for e in actions::catalog() {
                        if e.action.partial_eq(&OpenPalette) {
                            continue;
                        }
                        let item = CommandItem::new().label(e.label).action(e.action);
                        match groups.iter_mut().find(|(g, _)| *g == e.group) {
                            Some((_, v)) => v.push(item),
                            None => groups.push((e.group, vec![item])),
                        }
                    }
                    let mut cmd = Command::new(&state)
                        .placeholder("Search commands…")
                        .on_confirm(|_, window, cx| window.close_dialog(cx));
                    for (g, items) in groups {
                        cmd = cmd.group(CommandGroup::new().label(g).items(items));
                    }
                    content.child(cmd)
                })
        });
    }

    #[expect(
        clippy::unused_self,
        reason = "GPUI cx.listener requires &mut self on action handlers"
    )]
    fn show_shortcuts(&mut self, _: &ShowShortcuts, window: &mut Window, cx: &mut Context<Self>) {
        window.open_dialog(cx, |dialog, _, _| {
            dialog
                .w(px(520.))
                .title("Keyboard Shortcuts")
                .content(|content, window, _| {
                    let mut list = div().flex().flex_col().gap(px(6.));
                    for e in actions::catalog() {
                        list = list.child(
                            div()
                                .flex()
                                .justify_between()
                                .items_center()
                                .text_size(TEXT_SM)
                                .child(e.label)
                                .children(Kbd::binding_for_action(
                                    e.action.as_ref(),
                                    Some(e.context),
                                    window,
                                )),
                        );
                    }
                    content.child(list)
                })
        });
    }

    fn toggle_theme(&mut self, _: &ToggleTheme, _: &mut Window, cx: &mut Context<Self>) {
        self.mode = self.mode.toggled();
        self.apply_appearance(cx);
    }

    /// Aplica uma transformação no texto do arquivo ativo (com undo) e atualiza o canvas.
    fn edit_text(
        &mut self,
        relayout: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&str) -> Option<String>,
    ) {
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        let text = doc.editor.read(cx).value().to_string();
        let Some(new) = f(&text).filter(|n| *n != text) else {
            return;
        };
        let before = Self::snapshot_of(doc, cx);
        doc.history.push(before);
        let schema = dbml_analysis::analyze(&new);
        let new_arc: std::sync::Arc<str> = std::sync::Arc::from(new.as_str());
        doc.last_text = new_arc;
        doc.dirty = doc.last_text.as_ref() != doc.saved_text.as_ref();
        self.suppress_history = true;
        doc.editor
            .update(cx, |e, cx| e.replace_all(new, window, cx));
        self.suppress_history = false;
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.diagram.update(cx, |d, cx| {
                d.set_schema(schema);
                if relayout {
                    d.relayout();
                }
                cx.notify();
            });
        }
        cx.notify();
    }

    fn apply_snapshot(
        &mut self,
        snap: crate::history::DocSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        let cur_text = doc.editor.read(cx).value().to_string();
        let new_text = snap.text.to_string();
        let schema = dbml_analysis::analyze(&new_text);
        doc.last_text = snap.text;
        doc.dirty = doc.last_text.as_ref() != doc.saved_text.as_ref();
        let editor = doc.editor.clone();
        let diagram = doc.diagram.clone();
        if cur_text != new_text {
            self.suppress_history = true;
            editor.update(cx, |e, cx| e.replace_all(new_text, window, cx));
            self.suppress_history = false;
        }
        diagram.update(cx, |d, cx| {
            d.apply_snapshot(
                schema,
                snap.pos,
                snap.selected,
                snap.selected_enum,
                snap.multi,
            );
            cx.notify();
        });
        cx.notify();
    }

    fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        let cur = Self::snapshot_of(doc, cx);
        let Some(prev) = doc.history.undo(cur) else {
            return;
        };
        self.apply_snapshot(prev, window, cx);
    }

    fn redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        let cur = Self::snapshot_of(doc, cx);
        let Some(next) = doc.history.redo(cur) else {
            return;
        };
        self.apply_snapshot(next, window, cx);
    }

    fn start_edit(
        &mut self,
        target: EditTarget,
        initial: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = cx.new(|cx| InputState::new(window, cx).default_value(initial));
        let handle = state.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
        let sub = cx.subscribe_in(
            &state,
            window,
            |this: &mut Shell, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::Change => {
                    cx.notify();
                }
                InputEvent::PressEnter { .. } | InputEvent::Blur => {
                    this.commit_edit(window, cx);
                }
                InputEvent::Focus => {}
            },
        );
        self.editing = Some((target, state, sub));
        cx.notify();
    }

    /// Aplica um tipo selecionado via autocomplete imediatamente.
    fn apply_column_type(
        &mut self,
        table: &str,
        col: &str,
        ty: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editing = None;
        if !ty.is_empty() {
            self.edit_text(false, window, cx, |t| {
                dbml_edit::set_column_type(t, table, col, ty)
            });
        }
        cx.notify();
    }

    /// Aplica o campo em edição no DBML. Nomes inválidos são recusados com aviso.
    fn commit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((target, state, _)) = self.editing.take() else {
            return;
        };
        let value = state.read(cx).value().trim().to_string();
        let ident = !value.is_empty() && value.chars().all(|c| c.is_alphanumeric() || c == '_');
        match target {
            EditTarget::Table(old) if value != old => {
                if ident {
                    let new = value.clone();
                    self.edit_text(false, window, cx, |t| {
                        Some(dbml_edit::rename_table(t, &old, &new))
                    });
                    self.with_diagram(cx, |d| d.rename(&old, &value));
                } else {
                    window
                        .push_notification("Invalid table name: use letters, numbers, and _.", cx);
                }
            }
            EditTarget::ColName(table, old) if value != old => {
                if ident {
                    self.edit_text(false, window, cx, |t| {
                        dbml_edit::rename_column(t, &table, &old, &value)
                    });
                } else {
                    window
                        .push_notification("Invalid column name: use letters, numbers, and _.", cx);
                }
            }
            EditTarget::ColType(table, col) if !value.is_empty() => {
                self.edit_text(false, window, cx, |t| {
                    dbml_edit::set_column_type(t, &table, &col, &value)
                });
            }
            EditTarget::ColDefault(table, col) => {
                let opt = (!value.is_empty()).then_some(value.as_str());
                self.edit_text(false, window, cx, |t| {
                    dbml_edit::set_column_default(t, &table, &col, opt)
                });
            }
            EditTarget::ColCheck(table, col) => {
                let opt = (!value.is_empty()).then_some(value.as_str());
                self.edit_text(false, window, cx, |t| {
                    dbml_edit::set_column_check(t, &table, &col, opt)
                });
            }
            EditTarget::IndexName(table, idx) => {
                let existing = self.doc().and_then(|d| {
                    d.diagram
                        .read(cx)
                        .schema()
                        .table(&table)
                        .and_then(|tb| tb.indexes.get(idx).cloned())
                });
                if let Some(mut ix) = existing {
                    ix.name = (!value.is_empty()).then(|| value.clone());
                    self.edit_text(false, window, cx, |t| {
                        dbml_edit::update_index(t, &table, idx, &ix)
                    });
                }
            }
            EditTarget::Enum(old) if value != old => {
                if ident {
                    let new = value.clone();
                    self.edit_text(false, window, cx, |t| {
                        Some(dbml_edit::rename_enum(t, &old, &new))
                    });
                    self.with_diagram(cx, |d| d.rename_enum(&old, &value));
                } else {
                    window.push_notification("Invalid enum name: use letters, numbers, and _.", cx);
                }
            }
            EditTarget::EnumVal(en, old) if value != old => {
                if ident {
                    self.edit_text(false, window, cx, |t| {
                        dbml_edit::rename_enum_value(t, &en, &old, &value)
                    });
                } else {
                    window
                        .push_notification("Invalid enum value: use letters, numbers, and _.", cx);
                }
            }
            EditTarget::NewEnumVal(en) if !value.is_empty() => {
                if ident {
                    self.edit_text(false, window, cx, |t| {
                        dbml_edit::add_enum_value(t, &en, &value)
                    });
                } else {
                    window
                        .push_notification("Invalid enum value: use letters, numbers, and _.", cx);
                }
            }
            _ => {}
        }
        cx.notify();
    }

    fn add_column_to(&mut self, table: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc() else { return };
        let text = doc.editor.read(cx).value().to_string();
        let name = dbml_edit::unique_column_name(&text, &table, "column");
        self.edit_text(false, window, cx, |t| {
            dbml_edit::add_column_text(t, &table, &name, "int")
        });
        self.inspector_open = true;
        self.start_edit(EditTarget::ColName(table, name.clone()), name, window, cx);
    }

    fn new_table(&mut self, _: &NewTable, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc() else { return };
        let text = doc.editor.read(cx).value().to_string();
        let name = dbml_edit::unique_table_name(&text, "table");
        self.edit_text(false, window, cx, |t| Some(dbml_edit::add_table(t, &name)));
        self.with_diagram(cx, |d| d.place_new(&name));
        self.inspector_open = true;
        self.start_edit(EditTarget::Table(name.clone()), name, window, cx);
    }

    fn new_enum(&mut self, _: &NewEnum, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc() else { return };
        let text = doc.editor.read(cx).value().to_string();
        let name = dbml_edit::unique_enum_name(&text, "enum_type");
        self.edit_text(false, window, cx, |t| Some(dbml_edit::add_enum(t, &name)));
        self.with_diagram(cx, |d| d.place_new_enum(&name));
        self.inspector_open = true;
        self.start_edit(EditTarget::Enum(name.clone()), name, window, cx);
    }

    fn delete_selection(
        &mut self,
        _: &DeleteSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.doc() else { return };
        if let Some((from, to)) = doc.diagram.read(cx).selected_ref().cloned() {
            self.edit_text(false, window, cx, |t| {
                Some(dbml_edit::delete_ref(t, (&from.0, &from.1), (&to.0, &to.1)))
            });
            self.with_diagram(cx, super::canvas::Diagram::deselect);
            return;
        }
        if let Some(en) = doc.diagram.read(cx).selected_enum().map(|e| e.name.clone()) {
            self.edit_text(false, window, cx, |t| Some(dbml_edit::delete_enum(t, &en)));
            self.with_diagram(cx, super::canvas::Diagram::deselect);
            return;
        }
        let names = doc.diagram.read(cx).selection();
        if names.is_empty() {
            return;
        }
        self.edit_text(false, window, cx, |t| {
            Some(
                names
                    .iter()
                    .fold(t.to_string(), |acc, n| dbml_edit::delete_table(&acc, n)),
            )
        });
        self.with_diagram(cx, super::canvas::Diagram::deselect);
    }

    fn rename_selected(&mut self, _: &RenameSelected, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc() else { return };
        if let Some(t) = doc.diagram.read(cx).selected().map(|t| t.name.clone()) {
            self.inspector_open = true;
            self.start_edit(EditTarget::Table(t.clone()), t, window, cx);
        } else if let Some(en) = doc.diagram.read(cx).selected_enum().map(|e| e.name.clone()) {
            self.inspector_open = true;
            self.start_edit(EditTarget::Enum(en.clone()), en, window, cx);
        }
    }

    fn toggle_flag(
        &mut self,
        table: &str,
        col: &str,
        flag: &'static str,
        on: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if flag == "fk" && !on {
            // Desligar FK remove a relação (e a coluna FK vinculada)
            if let Some(doc) = self.doc() {
                let target_ref = doc
                    .diagram
                    .read(cx)
                    .schema()
                    .refs
                    .iter()
                    .find(|r| r.from.qualified_table() == table && r.from.column == col)
                    .map(|r| (r.to.qualified_table(), r.to.column.clone()));
                if let Some((to_t, to_c)) = target_ref {
                    self.edit_text(false, window, cx, |t| {
                        Some(dbml_edit::delete_ref(t, (table, col), (&to_t, &to_c)))
                    });
                    return;
                }
            }
        }
        self.edit_text(false, window, cx, |t| {
            dbml_edit::set_column_flag(t, table, col, flag, on)
        });
    }

    fn group_selection(&mut self, _: &GroupSelection, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc() else { return };
        let names = doc.diagram.read(cx).selection();
        if names.is_empty() {
            window.push_notification("Select tables (Shift+click) to group.", cx);
            return;
        }
        self.edit_text(true, window, cx, |t| {
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            Some(dbml_edit::group_tables(
                t,
                &refs,
                &dbml_edit::next_group_name(t),
            ))
        });
    }

    fn ungroup_selection(
        &mut self,
        _: &UngroupSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.doc() else { return };
        let names = doc.diagram.read(cx).selection();
        self.edit_text(true, window, cx, |t| {
            Some(
                names
                    .iter()
                    .fold(t.to_string(), |acc, n| dbml_edit::ungroup_table(&acc, n)),
            )
        });
    }

    fn set_color(
        &mut self,
        table: &str,
        color: Option<&'static str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_text(false, window, cx, |t| {
            dbml_edit::set_table_color(t, table, color)
        });
    }

    fn save_prefs(&self) {
        crate::settings::save(&crate::settings::Settings {
            mode: Some(self.mode),
            sidebar_w: self.widths.0,
            editor_w: self.widths.1,
            inspector_w: self.widths.2,
            col_type_w: self.col_type_w,
            translucent_cards: self.translucent_cards,
            ui_scale: self.ui_scale,
            inspector_auto_hide: self.inspector_auto_hide,
            sidebar_open: self.sidebar_open,
            inspector_open: self.inspector_open,
        });
    }

    fn apply_appearance(&mut self, cx: &mut Context<Self>) {
        apply_theme_scaled(self.mode, self.ui_scale, cx);
        self.save_prefs();
        self.save_session(cx);
        cx.notify();
    }

    fn set_ui_scale(&mut self, scale: f32, window: &mut Window, cx: &mut Context<Self>) {
        self.ui_scale = (scale * 100.0).round() / 100.0;
        self.apply_appearance(cx);
        window.push_notification(
            format!("UI Scale: {}%", (self.ui_scale * 100.0).round() as i32),
            cx,
        );
    }

    fn zoom_in_ui(&mut self, _: &ZoomInUi, window: &mut Window, cx: &mut Context<Self>) {
        let next = (self.ui_scale + 0.1).min(1.8);
        self.set_ui_scale(next, window, cx);
    }

    fn zoom_out_ui(&mut self, _: &ZoomOutUi, window: &mut Window, cx: &mut Context<Self>) {
        let next = (self.ui_scale - 0.1).max(0.75);
        self.set_ui_scale(next, window, cx);
    }

    fn reset_ui_zoom(&mut self, _: &ResetUiZoom, window: &mut Window, cx: &mut Context<Self>) {
        self.set_ui_scale(1.0, window, cx);
    }

    fn toggle_auto_collapse_inspector(
        &mut self,
        _: &ToggleAutoCollapseInspector,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.inspector_auto_hide = !self.inspector_auto_hide;
        self.save_prefs();
        let msg = if self.inspector_auto_hide {
            "Inspector: auto-collapse when nothing selected (enabled)"
        } else {
            "Inspector: always visible (disabled)"
        };
        window.push_notification(msg, cx);
        cx.notify();
    }

    fn reset_panels(&mut self, _: &ResetPanels, _: &mut Window, cx: &mut Context<Self>) {
        let d = crate::settings::Settings::default();
        self.widths = (d.sidebar_w, d.editor_w, d.inspector_w);
        self.col_type_w = d.col_type_w;
        self.panels_gen += 1;
        self.save_prefs();
        self.save_session(cx);
        cx.notify();
    }

    fn toggle_card_transparency(
        &mut self,
        _: &ToggleCardTransparency,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.translucent_cards = !self.translucent_cards;
        self.save_prefs();
        self.save_session(cx);
        cx.notify();
    }

    fn toggle_editor(&mut self, _: &ToggleEditor, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_left_pane(None, cx);
    }

    fn toggle_sidebar(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar_open && !self.editor_open {
            self.sidebar_open = false;
        } else {
            self.sidebar_open = true;
            self.editor_open = false;
        }
        self.save_prefs();
        self.save_session(cx);
        cx.notify();
    }

    fn toggle_inspector(&mut self, _: &ToggleInspector, _: &mut Window, cx: &mut Context<Self>) {
        self.inspector_open = !self.inspector_open;
        self.save_prefs();
        self.save_session(cx);
        cx.notify();
    }

    fn toggle_legend(&mut self, _: &ToggleLegend, _: &mut Window, cx: &mut Context<Self>) {
        self.with_diagram(cx, |d| d.legend_open = !d.legend_open);
    }

    fn fit(&mut self, _: &FitView, _: &mut Window, cx: &mut Context<Self>) {
        self.with_diagram(cx, super::canvas::Diagram::fit);
    }

    fn relayout(&mut self, _: &Relayout, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(doc) = self.docs.get_mut(self.active) {
            let before = Self::snapshot_of(doc, cx);
            doc.history.push(before);
        }
        self.with_diagram(cx, super::canvas::Diagram::relayout);
    }

    fn zoom_in(&mut self, _: &ZoomIn, _: &mut Window, cx: &mut Context<Self>) {
        self.with_diagram(cx, |d| d.zoom_by(1.25));
    }

    fn zoom_out(&mut self, _: &ZoomOut, _: &mut Window, cx: &mut Context<Self>) {
        self.with_diagram(cx, |d| d.zoom_by(1. / 1.25));
    }

    fn tool_select(&mut self, _: &ToolSelect, _: &mut Window, cx: &mut Context<Self>) {
        self.with_diagram(cx, |d| d.tool = Tool::Select);
    }

    fn tool_hand(&mut self, _: &ToolHand, _: &mut Window, cx: &mut Context<Self>) {
        self.with_diagram(cx, |d| d.tool = Tool::Hand);
    }

    fn deselect(&mut self, _: &Deselect, _: &mut Window, cx: &mut Context<Self>) {
        self.with_diagram(cx, super::canvas::Diagram::deselect);
    }
}

/// Ações disparadas de camadas fora do shell (paleta de comandos, menus, diálogos)
/// não passam pelos handlers do elemento; estes handlers globais as encaminham ao shell.
/// Formatos de exportação de imagem.
#[derive(Clone, Copy)]
enum ImageKind {
    Png,
    Jpeg,
    Svg,
}

impl ImageKind {
    const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Svg => "svg",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Svg => "SVG",
        }
    }
}

macro_rules! forward_actions {
    ($cx:ident, $win:ident, $shell:ident; $($a:ident => $m:ident),* $(,)?) => {
        $({
            let shell = $shell.clone();
            $cx.on_action(move |a: &$a, cx| {
                let shell = shell.clone();
                $win.update(cx, |_, window, cx| shell.update(cx, |s, cx| s.$m(a, window, cx))).ok();
            });
        })*
    };
}

pub fn register_global_actions(win: AnyWindowHandle, shell: &Entity<Shell>, cx: &mut App) {
    forward_actions!(cx, win, shell;
        NewFile => new_file,
        OpenFile => open_file,
        OpenFolder => open_folder,
        SaveFile => save_file,
        Undo => undo,
        Redo => redo,
        CloseTab => close_tab,
        NextTab => next_tab,
        PrevTab => prev_tab,
        ShowFiles => show_files,
        ShowSchema => show_schema,
        ShowProblems => show_problems,
        Quit => quit,
        FormatDocument => format_document,
        ExportSqlPostgres => export_sql_postgres,
        ExportSqlMysql => export_sql_mysql,
        ExportSqlSqlite => export_sql_sqlite,
        ExportSqlSqlServer => export_sql_sqlserver,
        ExportSqlOracle => export_sql_oracle,
        ExportMermaid => export_mermaid,
        ExportPng => export_png,
        ExportJpeg => export_jpeg,
        ExportSvg => export_svg,
        ImportSqlDdl => import_sql_ddl,
        ConnectDatabase => connect_database,
        AddPresetUuidPk => add_preset_uuid_pk,
        AddPresetBigIntPk => add_preset_bigint_pk,
        AddPresetTimestamps => add_preset_timestamps,
        AddPresetSoftDelete => add_preset_soft_delete,
        MoveLineUp => move_line_up,
        MoveLineDown => move_line_down,
        OpenPalette => open_palette,
        ShowShortcuts => show_shortcuts,
        ToggleTheme => toggle_theme,
        ToggleCardTransparency => toggle_card_transparency,
        ToggleEditor => toggle_editor,
        ToggleSidebar => toggle_sidebar,
        ToggleInspector => toggle_inspector,
        ToggleLegend => toggle_legend,
        FitView => fit,
        Relayout => relayout,
        ZoomIn => zoom_in,
        ZoomOut => zoom_out,
        ToolSelect => tool_select,
        ToolHand => tool_hand,
        Deselect => deselect,
        GroupSelection => group_selection,
        NewTable => new_table,
        NewEnum => new_enum,
        DeleteSelection => delete_selection,
        RenameSelected => rename_selected,
        ResetPanels => reset_panels,
        UngroupSelection => ungroup_selection,
        ZoomInUi => zoom_in_ui,
        ZoomOutUi => zoom_out_ui,
        ResetUiZoom => reset_ui_zoom,
        ToggleAutoCollapseInspector => toggle_auto_collapse_inspector,
    );
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let g = Glass::of_scaled(self.mode, self.translucent, self.ui_scale);
        // responsivo: painéis colapsam apenas em larguras extremamente reduzidas
        let width = f32::from(window.viewport_size().width);
        let has_selection = self.doc().is_some_and(|d| {
            let diag = d.diagram.read(cx);
            diag.selected().is_some() || !diag.selection().is_empty()
        });
        let show_sidebar = self.sidebar_open && !self.editor_open && width >= 500.;
        let show_editor = self.editor_open && width >= 500.;
        let show_inspector =
            self.inspector_open && (!self.inspector_auto_hide || has_selection) && width >= 600.;
        let titlebar = self.titlebar(&g, window.is_maximized(), width < 1000., cx);
        let rail = self.rail(&g);
        let right_rail = self.right_rail(&g, cx);
        // o canvas primeiro: ele aplica o ajuste à tela que a barra de status exibe
        let translucent_cards = self.translucent_cards;
        let canvas = self.doc().map(|d| d.diagram.clone()).inspect(|e| {
            e.update(cx, |d, cx| {
                if d.glass.map(|x| x.mode) != Some(g.mode)
                    || d.translucent_cards != translucent_cards
                {
                    d.glass = Some(g);
                    d.translucent_cards = translucent_cards;
                    cx.notify();
                }
            });
        });
        let sidebar = self.sidebar(&g, cx);
        let inspector = self.inspector(&g, window, cx);
        let editor = self.editor_panel(&g);
        let statusbar = self.statusbar(&g, width < 1000., cx);
        let work: AnyElement = match canvas {
            Some(canvas) => {
                let panels_id = SharedString::from(format!(
                    "main-panels-{}-{}-{}-{}",
                    self.panels_gen, show_sidebar, show_editor, show_inspector
                ));
                let mut resizable = h_resizable(panels_id);

                let has_sb = show_sidebar;
                let has_ed = show_editor;
                let has_insp = show_inspector;
                let weak = cx.entity().downgrade();
                resizable = resizable.on_resize(move |state, _, cx| {
                    let sizes = state.read(cx).sizes().clone();
                    weak.update(cx, |s, _| {
                        let mut ix = 0;
                        if has_sb && sizes.len() > ix {
                            let w = f32::from(sizes[ix]);
                            if w >= 120. {
                                s.widths.0 = w;
                            }
                            ix += 1;
                        }
                        if has_ed && sizes.len() > ix {
                            let w = f32::from(sizes[ix]);
                            if w >= 120. {
                                s.widths.1 = w;
                            }
                            ix += 1;
                        }
                        // ix é o canvas
                        ix += 1;
                        if has_insp && sizes.len() > ix {
                            let w = f32::from(sizes[ix]);
                            if w >= 120. {
                                s.widths.2 = w;
                            }
                        }
                        s.save_prefs();
                    })
                    .ok();
                });

                if show_sidebar {
                    resizable = resizable.child(
                        resizable_panel()
                            .size(px(self.widths.0))
                            .size_range(px(180.)..px(440.))
                            .child(sidebar),
                    );
                }

                if show_editor {
                    resizable = resizable.child(
                        resizable_panel()
                            .size(px(self.widths.1))
                            .size_range(px(280.)..px(900.))
                            .child(editor),
                    );
                }

                resizable = resizable.child(
                    resizable_panel()
                        .child(div().id("canvas").size_full().bg(g.surface).child(canvas)),
                );

                if show_inspector {
                    resizable = resizable.child(
                        resizable_panel()
                            .size(px(self.widths.2))
                            .size_range(px(240.)..px(800.))
                            .child(inspector),
                    );
                }

                resizable.into_any_element()
            }
            None => div()
                .size_full()
                .flex()
                .when(show_sidebar, |d| {
                    d.child(
                        div()
                            .w(px(self.widths.0))
                            .h_full()
                            .border_r_1()
                            .border_color(g.edge)
                            .child(sidebar),
                    )
                })
                .when(show_editor, |d| {
                    d.child(
                        div()
                            .w(px(self.widths.1))
                            .h_full()
                            .border_r_1()
                            .border_color(g.edge)
                            .child(editor),
                    )
                })
                .child(Self::empty_state(&g))
                .into_any_element(),
        };
        div()
            .id("shell")
            .key_context(actions::CTX)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::new_file))
            .on_action(cx.listener(Self::open_file))
            .on_action(cx.listener(Self::open_folder))
            .on_action(cx.listener(Self::save_file))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::close_tab))
            .on_action(cx.listener(Self::next_tab))
            .on_action(cx.listener(Self::prev_tab))
            .on_action(cx.listener(Self::show_files))
            .on_action(cx.listener(Self::show_schema))
            .on_action(cx.listener(Self::show_problems))
            .on_action(cx.listener(Self::quit))
            .on_action(cx.listener(Self::format_document))
            .on_action(cx.listener(Self::move_line_up))
            .on_action(cx.listener(Self::move_line_down))
            .on_action(cx.listener(Self::open_palette))
            .on_action(cx.listener(Self::import_sql_ddl))
            .on_action(cx.listener(Self::export_sql_postgres))
            .on_action(cx.listener(Self::export_sql_mysql))
            .on_action(cx.listener(Self::export_sql_sqlite))
            .on_action(cx.listener(Self::export_sql_sqlserver))
            .on_action(cx.listener(Self::export_sql_oracle))
            .on_action(cx.listener(Self::export_mermaid))
            .on_action(cx.listener(Self::export_png))
            .on_action(cx.listener(Self::export_jpeg))
            .on_action(cx.listener(Self::export_svg))
            .on_action(cx.listener(Self::connect_database))
            .on_action(cx.listener(Self::add_preset_uuid_pk))
            .on_action(cx.listener(Self::add_preset_bigint_pk))
            .on_action(cx.listener(Self::add_preset_timestamps))
            .on_action(cx.listener(Self::add_preset_soft_delete))
            .on_action(cx.listener(Self::show_shortcuts))
            .on_action(cx.listener(Self::toggle_theme))
            .on_action(cx.listener(Self::toggle_card_transparency))
            .on_action(cx.listener(Self::toggle_editor))
            .on_action(cx.listener(Self::toggle_sidebar))
            .on_action(cx.listener(Self::toggle_inspector))
            .on_action(cx.listener(Self::toggle_legend))
            .on_action(cx.listener(Self::fit))
            .on_action(cx.listener(Self::relayout))
            .on_action(cx.listener(Self::zoom_in))
            .on_action(cx.listener(Self::zoom_out))
            .on_action(cx.listener(Self::tool_select))
            .on_action(cx.listener(Self::tool_hand))
            .on_action(cx.listener(Self::deselect))
            .on_action(cx.listener(Self::new_table))
            .on_action(cx.listener(Self::new_enum))
            .on_action(cx.listener(Self::delete_selection))
            .on_action(cx.listener(Self::rename_selected))
            .on_action(cx.listener(Self::reset_panels))
            .on_action(cx.listener(Self::group_selection))
            .on_action(cx.listener(Self::ungroup_selection))
            .on_action(cx.listener(Self::zoom_in_ui))
            .on_action(cx.listener(Self::zoom_out_ui))
            .on_action(cx.listener(Self::reset_ui_zoom))
            .on_action(cx.listener(Self::toggle_auto_collapse_inspector))
            .size_full()
            .font_family(FONT)
            .text_color(g.text)
            .bg(g.chrome)
            .flex()
            .flex_col()
            .child(titlebar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(rail)
                    .child(div().flex_1().min_w_0().h_full().child(work))
                    .child(right_rail),
            )
            .child(statusbar)
    }
}

#[cfg(test)]
mod tests {
    use super::Shell;
    use std::ffi::OsStr;
    use std::path::PathBuf;

    #[test]
    fn resolve_export_dir_prefers_project_then_home_then_userprofile() {
        let proj = PathBuf::from("/tmp/project");
        assert_eq!(
            Shell::resolve_export_dir(
                Some(&proj),
                Some(OsStr::new("/home/u")),
                Some(OsStr::new("C:\\Users\\u")),
            ),
            proj
        );
        assert_eq!(
            Shell::resolve_export_dir(
                None,
                Some(OsStr::new("/home/u")),
                Some(OsStr::new("C:\\Users\\u")),
            ),
            PathBuf::from("/home/u")
        );
        assert_eq!(
            Shell::resolve_export_dir(None, None, Some(OsStr::new("C:\\Users\\u"))),
            PathBuf::from("C:\\Users\\u")
        );
    }

    #[test]
    fn all_catalog_sql_export_actions_are_bound_in_shell_render() {
        let src = include_str!("mod.rs");
        for handler in [
            "Self::export_sql_postgres",
            "Self::export_sql_mysql",
            "Self::export_sql_sqlite",
            "Self::export_sql_sqlserver",
            "Self::export_sql_oracle",
            "Self::export_mermaid",
            "Self::export_png",
            "Self::export_jpeg",
            "Self::export_svg",
            "Self::connect_database",
            "Self::add_preset_uuid_pk",
            "Self::add_preset_bigint_pk",
            "Self::add_preset_timestamps",
            "Self::add_preset_soft_delete",
        ] {
            assert!(
                src.contains(&format!(".on_action(cx.listener({handler}))")),
                "Missing Shell::render .on_action listener for {handler}"
            );
        }
        let count = |group: &str| {
            crate::actions::catalog()
                .into_iter()
                .filter(|e| e.group == group)
                .count()
        };
        assert_eq!(count(crate::actions::GROUP_EXPORT_CODE), 6); // 5 bancos + Mermaid
        assert_eq!(count(crate::actions::GROUP_EXPORT_IMAGE), 3); // PNG, JPEG, SVG
    }
}
