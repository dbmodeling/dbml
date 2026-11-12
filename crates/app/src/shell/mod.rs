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
use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
use gpui_kit::component::input::{Editor, EditorState, Input, InputEvent, InputState};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::menu::DropdownMenu;
use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, InteractiveElementExt as _, Sizable, WindowExt};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::{Path, PathBuf};

/// Um arquivo aberto (aba).
pub struct Doc {
    path: Option<PathBuf>,
    title: String,
    editor: Entity<EditorState>,
    diagram: Entity<Diagram>,
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

    fn add_doc(
        &mut self,
        text: String,
        path: Option<PathBuf>,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
                let schema = dbml_analysis::analyze(&ed.read(cx).value());
                let diags = ed.update(cx, crate::ide::refresh_diagnostics);
                if let Some(doc) = this.docs.iter_mut().find(|d| &d.editor == ed) {
                    doc.dirty = true;
                    doc.diags = diags;
                    doc.diagram.update(cx, |d, cx| {
                        d.set_schema(schema);
                        cx.notify();
                    });
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
            |this: &mut Shell, _, ev: &DiagramEvent, window, cx| match ev {
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
            },
        );
        self.subs.extend([sub, obs, events]);
        self.docs.push(Doc {
            path,
            title,
            editor,
            diagram,
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
        match std::fs::write(&path, text) {
            Ok(()) => {
                window.push_notification(format!("Saved to {}", path.display()), cx);
                doc.title = file_title(&path);
                doc.path = Some(path);
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

    fn close_tab_at(&mut self, i: usize, cx: &mut Context<Self>) {
        if i >= self.docs.len() {
            return;
        }
        // shortcut: fecha sem perguntar mesmo com alterações; trocar por confirmação inline quando houver undo de arquivo
        self.docs.remove(i);
        if self.active >= self.docs.len() {
            self.active = self.docs.len().saturating_sub(1);
        } else if i < self.active {
            self.active -= 1;
        }
        self.save_session(cx);
        cx.notify();
    }

    fn close_tab(&mut self, _: &CloseTab, _: &mut Window, cx: &mut Context<Self>) {
        self.close_tab_at(self.active, cx);
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

    #[expect(
        clippy::unused_self,
        reason = "GPUI cx.listener requires &mut self on action handlers"
    )]
    fn quit(&mut self, _: &Quit, _: &mut Window, cx: &mut Context<Self>) {
        cx.quit();
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
        let Some(doc) = self.doc() else { return };
        let text = doc.editor.read(cx).value().to_string();
        let Some(new) = f(&text).filter(|n| *n != text) else {
            return;
        };
        let schema = dbml_analysis::analyze(&new);
        doc.editor
            .update(cx, |e, cx| e.replace_all(new, window, cx));
        doc.diagram.update(cx, |d, cx| {
            d.set_schema(schema);
            if relayout {
                d.relayout();
            }
            cx.notify();
        });
        cx.notify();
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
                    .find(|r| r.from.table == table && r.from.column == col)
                    .map(|r| (r.to.table.clone(), r.to.column.clone()));
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
        CloseTab => close_tab,
        NextTab => next_tab,
        PrevTab => prev_tab,
        ShowFiles => show_files,
        ShowSchema => show_schema,
        ShowProblems => show_problems,
        Quit => quit,
        FormatDocument => format_document,
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
