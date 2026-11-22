//! Janela principal. Um *projeto* é uma pasta com um ou mais arquivos `.dbml`
//! (ou um arquivo avulso); cada arquivo aberto vira uma aba com editor e canvas.
//! Layout: barra de título com abas · barra de atividades · painel lateral
//! (Arquivos / Esquema / Problemas) · editor · canvas · inspector · barra de status.
//! Todas as ações passam por `actions::catalog`.

pub(crate) mod guide_ui;
mod panels;

use crate::actions::{self, *};
use crate::canvas::{Diagram, DiagramEvent, Tool};
use crate::glass::*;
use crate::motion;
use dbml_analysis::Severity;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{
    Editor, EditorState, Escape, Input, InputEvent, InputState, RangeDecoration,
    RangeDecorationCollection, RangeDecorationStyle,
};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::popover::Popover;
use gpui_kit::component::resizable::{HandleEdge, resize_handle};
use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, InteractiveElementExt as _, Sizable, WindowExt};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::{Path, PathBuf};

/// Máximo de tabelas (e de colunas) listadas na paleta por busca.
const PALETTE_HITS: usize = 30;
const NEW_TABLE_NAME: &str = "new_table";

/// Um arquivo aberto (aba).
pub struct Doc {
    /// identifica a cópia de recuperação (`recovery`) desta aba
    id: u64,
    /// sublinhados de diagnóstico lidos pelo highlighter do editor
    marks: crate::ide::DiagMarks,
    /// geração do texto analisado; resultados de análise de gerações antigas são descartados
    parse_gen: u64,
    /// análise (analyze + lint) com debounce, em segundo plano
    parse_task: Option<Task<()>>,
    /// posições já gravadas (ou lidas) no sidecar de layout
    layout_saved: std::collections::HashMap<String, (f32, f32)>,
    /// gravação do sidecar com debounce
    layout_task: Option<Task<()>>,
    path: Option<PathBuf>,
    title: String,
    editor: Entity<EditorState>,
    diagram: Entity<Diagram>,
    saved_text: std::sync::Arc<str>,
    last_text: std::sync::Arc<str>,
    history: crate::history::UndoHistory,
    dirty: bool,
    autosave: Option<Task<()>>,
    /// conteúdo do disco já avisado como conflito (evita repetir o aviso a cada leitura)
    watch_warned: Option<std::sync::Arc<str>>,
    /// conteúdo do disco sobre o qual o primeiro Salvar avisou; repetir Salvar sobrescreve
    save_armed: Option<std::sync::Arc<str>>,
    diags: Vec<dbml_analysis::Diagnostic>,
    /// realce do bloco da tabela selecionada no editor
    table_hl: RangeDecorationCollection,
    /// (tabela, texto) para os quais `table_hl` foi calculado; evita reparsear a cada frame
    hl_key: (Option<String>, std::sync::Arc<str>),
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
    TableNote(String),
    ColName(String, String),
    ColType(String, String),
    ColDefault(String, String),
    ColCheck(String, String),
    ColNote(String, String),
    IndexName(String, usize),
    Enum(String),
    EnumVal(String, String),
    NewEnumVal(String),
}

/// Visão do espaço de trabalho: só canvas, canvas + editor, ou só editor.
#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Diagram,
    Split,
    Code,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pane {
    Files,
    Schema,
    Problems,
}

const STANDARD_TYPES: &[(&str, &str)] = &[
    ("int", "inteiro de 4 bytes"),
    ("bigint", "inteiro de 8 bytes"),
    ("smallint", "inteiro de 2 bytes"),
    ("serial", "autoincremento"),
    ("varchar(255)", "texto de tamanho variável"),
    ("text", "texto longo"),
    ("boolean", "booleano"),
    ("uuid", "identificador único"),
    ("timestamp", "data e hora"),
    ("timestamptz", "data/hora com fuso"),
    ("date", "data"),
    ("time", "hora"),
    ("decimal(10,2)", "decimal exato"),
    ("float", "ponto flutuante"),
    ("double", "precisão dupla"),
    ("json", "JSON"),
    ("jsonb", "JSON binário"),
    ("bytea", "binário"),
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
    /// com `editor_open`: editor ocupa tudo, sem canvas nem inspector
    code_only: bool,
    inspector_open: bool,
    mode: Mode,
    /// chrome translúcido (há blur no fundo da janela)
    translucent: bool,
    lang_pref: Option<crate::i18n::Lang>,
    /// larguras dos painéis (sidebar, editor, inspector; salvas em preferências)
    widths: (f32, f32, f32),
    /// largura da coluna de tipos de dados no inspector
    col_type_w: f32,
    editing: Option<(EditTarget, Entity<InputState>, Subscription)>,
    last_summary: Option<crate::canvas::Summary>,
    palette: Entity<CommandState>,
    quick_search: Entity<CommandState>,
    focus: FocusHandle,
    subs: Vec<Subscription>,
    dragging_window: bool,
    ui_scale: f32,
    inspector_auto_hide: bool,
    restoring_session: bool,
    suppress_history: bool,
    import_dialect: sql_import::SqlDialect,
    /// (caminho de origem, branch git) — recalculado só quando o arquivo ativo/pasta muda
    git_branch: (Option<PathBuf>, Option<String>),
    /// `.dbml` abertos/salvos recentemente, mais recente primeiro (tela inicial)
    recent: Vec<crate::session::RecentFile>,
    /// filtro do explorador (aberto = `Some`)
    filter: Option<(Entity<InputState>, Subscription)>,
    /// coluna expandida no acordeão do inspector: `(tabela, Option<coluna>)`
    open_col: Option<(String, Option<String>)>,
    /// seções/grupos recolhidos manualmente no painel Esquema
    collapsed: std::collections::HashSet<String>,
    last_canvas: Option<EntityId>,
    canvas_epoch: u64,
    popover_epoch: u64,
    /// tarefa que confere periodicamente se os arquivos abertos mudaram no disco
    disk_watcher: Option<Task<()>>,
    /// consulta a caixa de entrada de `handoff` (segunda instância, "Abrir com" do macOS)
    handoff_poll: Option<Task<()>>,
    /// gravação (com debounce) das cópias de recuperação das abas não salvas
    recovery_task: Option<Task<()>>,
    /// há cópias de recuperação no disco (ou pendentes de gravação)
    recovery_active: bool,
    /// leituras de arquivo/pasta em andamento em segundo plano (mostra "Abrindo…" na barra)
    loading: usize,
    /// `.dbml` de uma pasta fora do projeto (arquivo avulso), varridos em segundo plano
    loose_files: (PathBuf, Vec<PathBuf>),
    /// tabela a selecionar quando o primeiro documento abrir (harness de screenshot)
    preselect: Option<String>,
    /// tour guiado já concluído ou pulado pelo usuário
    onboarding_done: bool,
    /// última versão vista no modal de novidades
    last_seen_version: Option<String>,
    /// passo ativo do tour guiado (`Some(idx)` quando aberto)
    tour_step: Option<usize>,
    /// bounds reais medidos pelo layout do GPUI para cada alvo do tour
    tour_bounds: guide_ui::TourBoundsMap,
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
    apply_theme_scaled(mode, false, 1.0, cx);
}

/// Aplica tema considerando a escala de UI configurada.
pub fn apply_theme_scaled(mode: Mode, translucent: bool, scale: f32, cx: &mut App) {
    let scale = scale * crate::glass::UI_BASE;
    Theme::change(
        match mode {
            Mode::Dark => ThemeMode::Dark,
            Mode::Light => ThemeMode::Light,
        },
        None,
        cx,
    );
    let g = Glass::of_scaled(mode, false, scale / crate::glass::UI_BASE);
    Theme::update(cx, |t| {
        t.motion.duration_fast = motion::FAST;
        t.motion.duration_normal = motion::BASE;
        t.motion.duration_slow = motion::SLOW;
        t.motion.easing_enter = motion::easing();
        t.motion.easing_exit = motion::easing();
        t.motion.easing_move = motion::easing();
        // o Root do gpui-component pinta este fundo atrás de tudo: opaco, ele esconde a translucidez da janela
        if translucent {
            t.tokens.background = gpui_kit::transparent_black().into();
        }
        t.font_family = FONT.into();
        t.mono_font_family = crate::glass::FONT_MONO.into();
        t.font_size = px((13. * scale).round());
        t.radius = px((8. * scale).round());
        t.radius_lg = px((12. * scale).round());
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
        // `opacity` só multiplica o alfa (island é translúcido); menus precisam de fundo sólido
        t.colors.popover = g.modal.alpha(1.);
        t.colors.overlay = g.scrim;
        t.colors.popover_foreground = g.text;
        t.colors.border = g.edge;
        // linha selecionada/hover de listas e menus (paleta, menus suspensos)
        t.colors.accent = g.active;
        t.colors.accent_foreground = g.text;
    });
}

/// Moldura comum dos diálogos: fundo `modal` opaco, borda `edge_strong`, raio 14, título 13px
/// semibold. (A sombra é fixa no `gpui-component`.)
fn dialog_frame(d: Dialog, g: &Glass, width: f32, title: Option<&'static str>) -> Dialog {
    let d = d
        .entrance(gpui_kit::component::dialog::DialogEntrance::FadeSlide)
        .w(px(width))
        .bg(g.modal.alpha(1.))
        .border_color(g.edge_strong)
        .rounded(px(14.));
    match title {
        Some(t) => d.title(
            div()
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(t),
        ),
        None => d,
    }
}

/// Diálogo do v4: sem padding nem botão de fechar da moldura (o conteúdo traz o cabeçalho).
fn dialog_bare(d: Dialog, g: &Glass, width: f32) -> Dialog {
    dialog_frame(d, g, width, None)
        .close_button(false)
        .p(px(0.))
        .overflow_hidden()
}

/// Botão X do cabeçalho dos diálogos (Esc também fecha).
fn dialog_close(g: &Glass) -> Stateful<Div> {
    let (h, t) = (g.hover, g.text);
    div()
        .id("dialog-close")
        .role(Role::Button)
        .aria_label(crate::i18n::t("Fechar"))
        .flex_none()
        .p(px(6.))
        .rounded(px(6.))
        .cursor_pointer()
        .text_color(g.text_faint)
        .hover(move |s| s.bg(h).text_color(t))
        .on_click(|_, window, cx| window.close_dialog(cx))
        .child(Icon::new(IconName::X).size(px(15.)))
}

/// Cabeçalho dos diálogos: título 15px semibold, subtítulo 12.5px e X à direita.
fn dialog_header(g: &Glass, title: &'static str, subtitle: &'static str) -> Div {
    div()
        .flex()
        .items_start()
        .gap(px(12.))
        .pl(px(18.))
        .pr(px(12.))
        .pt(px(16.))
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(g.text_muted)
                        .child(subtitle),
                ),
        )
        .child(dialog_close(g))
}

/// Botão de rodapé dos diálogos: `primary` = acento, `outline` = contorno, senão texto.
fn dialog_btn(
    g: &Glass,
    id: &'static str,
    label: impl Into<SharedString>,
    icon: Option<IconName>,
    primary: bool,
    outline: bool,
    enabled: bool,
) -> Stateful<Div> {
    let (hover, text) = (g.hover, g.text);
    div()
        .id(id)
        .role(Role::Button)
        .h(px(32.))
        .px(px(14.))
        .rounded(px(8.))
        .flex()
        .items_center()
        .gap(px(7.))
        .text_size(TEXT_SM)
        .when(primary, |d| {
            d.bg(g.accent)
                .text_color(gpui_kit::white())
                .font_weight(FontWeight::MEDIUM)
        })
        .when(!primary, |d| d.text_color(g.text_muted))
        .when(outline, |d| d.border_1().border_color(g.edge_strong))
        .when(enabled, |d| {
            d.cursor_pointer()
                .when(!primary, |d| d.hover(move |s| s.bg(hover).text_color(text)))
        })
        .when(!enabled, |d| d.opacity(0.5))
        .children(icon.map(|i| Icon::new(i).size(px(13.))))
        .child(label.into())
}

/// Ícone da linha da paleta, por grupo do catálogo.
fn group_icon(group: &str) -> IconName {
    match group {
        "Arquivo" => IconName::File,
        "Editar" => IconName::Pencil,
        "Exibir" => IconName::PanelsTopLeft,
        "Canvas" => IconName::Frame,
        "Tabelas" => IconName::Table2,
        "Aparência" => IconName::Palette,
        "Editor" => IconName::Braces,
        actions::GROUP_EXPORT_CODE => IconName::Database,
        actions::GROUP_EXPORT_IMAGE => IconName::Image,
        _ => IconName::Command,
    }
}

/// Ícone específico de cada ação (para a paleta de comandos e os submenus do menu principal).
fn action_icon(action: &dyn Action, group: &str) -> IconName {
    if action.partial_eq(&NewFile) {
        IconName::FilePlus
    } else if action.partial_eq(&OpenFile) {
        IconName::FolderOpen
    } else if action.partial_eq(&OpenFolder) {
        IconName::FolderTree
    } else if action.partial_eq(&CloseTab) {
        IconName::X
    } else if action.partial_eq(&SaveFile) {
        IconName::Save
    } else if action.partial_eq(&ImportSqlDdl) {
        IconName::FileInput
    } else if action.partial_eq(&ConnectDatabase) {
        IconName::Database
    } else if action.partial_eq(&Quit) {
        IconName::LogOut
    } else if action.partial_eq(&Undo) {
        IconName::Undo2
    } else if action.partial_eq(&Redo) {
        IconName::Redo2
    } else if action.partial_eq(&NextTab) {
        IconName::ArrowRight
    } else if action.partial_eq(&PrevTab) {
        IconName::ArrowLeft
    } else if action.partial_eq(&ShowFiles) {
        IconName::Folder
    } else if action.partial_eq(&ShowSchema) {
        IconName::Table2
    } else if action.partial_eq(&ShowProblems) {
        IconName::TriangleAlert
    } else if action.partial_eq(&ToggleTheme) {
        IconName::SunMoon
    } else if action.partial_eq(&ThemeLight) {
        IconName::Sun
    } else if action.partial_eq(&ThemeDark) {
        IconName::Moon
    } else if action.partial_eq(&ToggleEditor) || action.partial_eq(&ShowCodeView) {
        IconName::Braces
    } else if action.partial_eq(&ShowDiagramView) {
        IconName::Workflow
    } else if action.partial_eq(&ShowSplitView) {
        IconName::Columns2
    } else if action.partial_eq(&FilterExplorer) {
        IconName::ListFilter
    } else if action.partial_eq(&ToggleSidebar) {
        IconName::PanelLeft
    } else if action.partial_eq(&ToggleInspector) || action.partial_eq(&ToggleAutoCollapseInspector)
    {
        IconName::PanelRight
    } else if action.partial_eq(&ToggleLegend) {
        IconName::Info
    } else if action.partial_eq(&ToolSelect) {
        IconName::MousePointer2
    } else if action.partial_eq(&ToolHand) {
        IconName::Hand
    } else if action.partial_eq(&FitView) {
        IconName::Scan
    } else if action.partial_eq(&Relayout) {
        IconName::LayoutGrid
    } else if action.partial_eq(&ZoomIn) {
        IconName::ZoomIn
    } else if action.partial_eq(&ZoomOut) {
        IconName::ZoomOut
    } else if action.partial_eq(&Deselect) {
        IconName::SquareDashed
    } else if action.partial_eq(&NewTable) {
        IconName::SquarePlus
    } else if action.partial_eq(&NewEnum) {
        IconName::List
    } else if action.partial_eq(&DeleteSelection) {
        IconName::Trash
    } else if action.partial_eq(&RenameSelected) || action.partial_eq(&EditTableNote) {
        IconName::Pencil
    } else if action.partial_eq(&DuplicateTable) {
        IconName::Copy
    } else if action.partial_eq(&RevealInDbml) {
        IconName::FileCode
    } else if action.partial_eq(&GroupSelection) || action.partial_eq(&UngroupSelection) {
        IconName::Group
    } else if action.partial_eq(&FormatDocument) {
        IconName::WandSparkles
    } else if action.partial_eq(&ExportMermaid) {
        IconName::GitFork
    } else if action.partial_eq(&ExportSvg) {
        IconName::PenTool
    } else if action.partial_eq(&AddPresetUuidPk) {
        IconName::FingerprintPattern
    } else if action.partial_eq(&AddPresetBigIntPk) {
        IconName::KeyRound
    } else if action.partial_eq(&AddPresetTimestamps) {
        IconName::Clock
    } else if action.partial_eq(&AddPresetSoftDelete) {
        IconName::Archive
    } else if action.partial_eq(&ShowShortcuts) {
        IconName::Keyboard
    } else if action.partial_eq(&ShowTour) {
        IconName::Info
    } else if action.partial_eq(&ShowWhatsNew) {
        IconName::WandSparkles
    } else {
        group_icon(group)
    }
}

/// Atalho à direita: mono 10.5px, `text_muted`, chip com fundo `input`, borda `edge_strong` e raio 5.
fn shortcut_chip(k: Kbd, g: &Glass) -> Kbd {
    k.font_family(FONT_MONO)
        .text_size(px(10.5))
        .text_color(g.text_muted)
        .bg(g.input)
        .border_1()
        .border_color(g.edge_strong)
        .rounded(px(5.))
}

/// Seletor segmentado do v4 (trilho `g.input`, ativo em acento) no lugar de uma fileira de botões.
/// `items`: (id, rótulo, ativo, habilitado); `pick` recebe o índice clicado.
fn segmented(
    g: &Glass,
    items: &[(&'static str, &'static str, bool, bool)],
    pick: impl Fn(usize, &mut App) + 'static,
) -> Div {
    let pick = std::rc::Rc::new(pick);
    let text = g.text;
    let mut row = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(2.))
        .p(px(3.))
        .rounded(px(9.))
        .bg(g.input);
    for (i, &(id, label, on, enabled)) in items.iter().enumerate() {
        let pick = pick.clone();
        row = row.child(
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(label)
                .h(px(26.))
                .px(px(10.))
                .rounded(px(6.))
                .flex()
                .items_center()
                .text_size(px(12.5))
                .text_color(if on {
                    gpui_kit::white()
                } else if enabled {
                    g.text_muted
                } else {
                    g.text_faint
                })
                .when(on, |d| d.bg(g.accent))
                .when(enabled && !on, |d| d.hover(move |s| s.text_color(text)))
                .when(enabled, |d| {
                    d.cursor_pointer().on_click(move |_, _, cx| pick(i, cx))
                })
                .child(label),
        );
    }
    row
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

/// Branch atual lendo `.git/HEAD` (sobe os diretórios; sem processo externo).
/// HEAD solto (detached) vira o hash abreviado.
fn git_branch(start: &Path) -> Option<String> {
    let git = start
        .ancestors()
        .map(|d| d.join(".git"))
        .find(|g| g.exists())?;
    // worktree/submódulo: `.git` é um arquivo "gitdir: <caminho>"
    let dir = if git.is_file() {
        let text = std::fs::read_to_string(&git).ok()?;
        git.parent()?.join(text.strip_prefix("gitdir:")?.trim())
    } else {
        git
    };
    let head = std::fs::read_to_string(dir.join("HEAD")).ok()?;
    let head = head.trim();
    Some(match head.strip_prefix("ref: refs/heads/") {
        Some(b) => b.to_string(),
        None => head.chars().take(7).collect(),
    })
}

/// Dica de recuperação para falha ao gravar o `.dbml`.
fn save_error_hint(e: &std::io::Error) -> &'static str {
    if file_in_use(e) {
        "O arquivo está em uso. Feche-o no outro aplicativo ou use Salvar como."
    } else if e.kind() == std::io::ErrorKind::StorageFull {
        "O disco está cheio. Libere espaço ou use Salvar como."
    } else if e.kind() == std::io::ErrorKind::PermissionDenied {
        "Sem permissão para gravar nesta pasta. Use Salvar como para escolher outra."
    } else {
        "Suas alterações continuam abertas. Tente salvar novamente ou use Salvar como."
    }
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
            sidebar_open: crate::harness::var("DBM_SIDEBAR").is_some()
                || crate::settings::load().sidebar_open,
            editor_open: crate::harness::var("DBM_EDITOR").is_some(),
            code_only: false,
            inspector_open: crate::harness::var("DBM_INSPECTOR").is_some()
                || crate::settings::load().inspector_open,
            mode,
            translucent,
            lang_pref: crate::settings::load().lang,
            widths: {
                let p = crate::settings::load();
                (p.sidebar_w, p.editor_w, p.inspector_w)
            },
            col_type_w: crate::settings::load().col_type_w,
            last_summary: None,
            editing: None,
            palette: cx.new(|cx| CommandState::new(window, cx)),
            quick_search: cx.new(|cx| CommandState::new(window, cx)),
            focus: cx.focus_handle(),
            subs: Vec::new(),
            dragging_window: false,
            ui_scale: crate::settings::load().ui_scale,
            inspector_auto_hide: crate::settings::load().inspector_auto_hide,
            restoring_session: false,
            suppress_history: false,
            import_dialect: sql_import::SqlDialect::Postgres,
            git_branch: (None, None),
            recent: crate::session::load()
                .recent
                .into_iter()
                .filter(|r| r.path.is_file())
                .collect(),
            filter: None,
            open_col: None,
            collapsed: std::collections::HashSet::new(),
            last_canvas: None,
            canvas_epoch: 0,
            popover_epoch: 0,
            disk_watcher: None,
            handoff_poll: None,
            recovery_task: None,
            recovery_active: false,
            loading: 0,
            loose_files: (PathBuf::new(), Vec::new()),
            preselect: crate::harness::var("DBM_SELECT"),
            onboarding_done: crate::settings::load().onboarding_done,
            last_seen_version: crate::settings::load().last_seen_version,
            tour_step: None,
            tour_bounds: guide_ui::TourBoundsMap::default(),
        };
        this.disk_watcher = Some(Self::spawn_disk_watcher(window, cx));
        this.handoff_poll = Some(Self::spawn_handoff_poll(window, cx));
        // DBM_PANE: painel inicial (usado pelo harness de screenshot)
        this.pane = match crate::harness::var("DBM_PANE").as_deref() {
            Some("files") => Pane::Files,
            Some("problems") => Pane::Problems,
            _ => Pane::Schema,
        };
        match start {
            Some(p) if p.is_dir() => this.set_project(p, window, cx),
            Some(p) => this.open_path(p, window, cx),
            None => this.restore_session(window, cx),
        }
        this.adopt_recovery(window, cx);
        let entity = cx.entity().downgrade();
        window.defer(cx, move |window, cx| {
            let _ = entity.update(cx, |this, cx| this.maybe_start_guide(window, cx));
        });
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
            pos: doc.history.share_pos(d.positions()),
            edges: d.edges().clone(),
            selected,
            selected_enum,
            multi,
        }
    }

    /// Grava `edges` no sidecar do arquivo (sem caminho = documento novo: nada a gravar).
    fn save_edges(doc: &Doc, cx: &App) {
        if let Some(path) = &doc.path
            && let Err(e) = crate::session::save_edges(path, doc.diagram.read(cx).edges())
        {
            eprintln!(
                "{}",
                crate::i18n::t("não foi possível gravar o layout de {}: {}")
                    .replacen("{}", &path.display().to_string(), 1)
                    .replacen("{}", &e.to_string(), 1)
            );
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
        let edges = path
            .as_deref()
            .map(crate::session::load_edges)
            .unwrap_or_default();
        let saved_positions = path
            .as_deref()
            .map(crate::session::load_positions)
            .unwrap_or_default();
        let tour_bounds = self.tour_bounds.clone();
        let diagram = cx.new(|cx| {
            let mut d = Diagram::new(dbml_analysis::analyze(&text), cx);
            d.set_edges(edges);
            d.seed_positions(saved_positions);
            d.tour_bounds = Some(tour_bounds);
            d
        });
        let layout_saved = diagram.read(cx).positions().clone();
        let mut marks = None;
        let editor = cx.new(|cx| {
            let mut st = EditorState::new(window, cx)
                .default_value(text)
                .line_number(true);
            marks = Some(crate::ide::install(&mut st, cx));
            st
        });
        let marks = marks.unwrap_or_default();
        let diags = editor.update(cx, crate::ide::refresh_diagnostics);
        *marks.borrow_mut() = crate::ide::diag_marks(&diags);
        let table_hl = editor.update(cx, |e, cx| {
            e.create_range_decorations_collection(Vec::new(), cx)
        });
        // texto → Schema → canvas, a cada edição
        let sub = cx.subscribe_in(
            &editor,
            window,
            |this: &mut Shell, ed, ev: &InputEvent, window, cx| {
                if !matches!(ev, InputEvent::Change) {
                    return;
                }
                let val = ed.read(cx).value().to_string();
                let suppress = this.suppress_history;
                if let Some(doc) = this.docs.iter_mut().find(|d| &d.editor == ed) {
                    let now = std::time::Instant::now();
                    if !suppress
                        && val.as_str() != doc.last_text.as_ref()
                        && !doc.history.extend_text_run(now)
                    {
                        // Passo novo: guarda o texto anterior e compartilha o mapa de
                        // posições com o passo vizinho (edições só-texto não o clonam).
                        let snap = Self::snapshot_of(doc, cx);
                        doc.history.push_text_edit(snap, now);
                    }
                    doc.last_text = std::sync::Arc::from(val.as_str());
                    doc.dirty = doc.last_text.as_ref() != doc.saved_text.as_ref();
                }
                // analyze + lint fora da thread da UI, 120 ms depois da última tecla
                this.schedule_analysis(ed, !suppress, cx);
                if !suppress && let Some(i) = this.doc_index(ed) {
                    this.schedule_autosave(i, window, cx);
                }
                this.publish_recovery(cx);
                cx.notify();
            },
        );
        // repinta o shell só quando muda algo que ele mostra (seleção, zoom, dica...);
        // mover/arrastar no canvas redesenha apenas o canvas
        let obs = cx.observe(&diagram, |this: &mut Shell, d, cx| {
            if let Some(i) = this.docs.iter().position(|doc| doc.diagram == d) {
                this.save_layout(i, cx);
            }
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
                            pos: doc.history.share_pos(old_pos),
                            edges: doc.diagram.read(cx).edges().clone(),
                            selected,
                            selected_enum,
                            multi,
                        });
                    }
                }
                DiagramEvent::EdgesChanged(old_edges) => {
                    if let Some(doc) = this.docs.iter_mut().find(|d| &d.diagram == diag_entity) {
                        let mut before = Self::snapshot_of(doc, cx);
                        before.edges.clone_from(old_edges);
                        doc.history.push(before);
                        Self::save_edges(doc, cx);
                    }
                }
            },
        );
        self.subs.extend([sub, obs, events]);
        self.docs.push(Doc {
            id: crate::recovery::next_id(),
            marks,
            parse_gen: 0,
            parse_task: None,
            layout_saved,
            layout_task: None,
            path,
            title,
            editor,
            diagram,
            saved_text: initial_arc.clone(),
            last_text: initial_arc,
            history: crate::history::UndoHistory::new(),
            dirty: false,
            autosave: None,
            watch_warned: None,
            save_armed: None,
            diags,
            table_hl,
            hl_key: (None, std::sync::Arc::from("")),
        });
        self.active = self.docs.len() - 1;
        // DBM_SELECT: tabela pré-selecionada (harness de screenshot)
        if let Some(t) = self.preselect.take() {
            self.with_diagram(cx, |d| d.select(&t));
        }
        cx.notify();
    }

    /// Abre `path` em uma aba. A leitura roda em segundo plano (pasta de rede lenta não congela
    /// a janela); `finish_open` conclui na thread da UI.
    /// Agenda `analyze` + `lint` do texto atual para 120 ms depois, em segundo plano. Cada edição
    /// invalida a anterior (geração), então digitar rápido analisa só o texto final. O resultado
    /// atualiza diagnósticos, sublinhados e, se `apply_schema`, o canvas.
    fn schedule_analysis(
        &mut self,
        editor: &Entity<EditorState>,
        apply_schema: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.doc_index(editor) else {
            return;
        };
        let doc = &mut self.docs[i];
        doc.parse_gen += 1;
        let generation = doc.parse_gen;
        let text = doc.last_text.clone();
        let bg = cx.background_executor().clone();
        let editor = editor.clone();
        doc.parse_task = Some(cx.spawn(async move |this, cx| {
            bg.timer(std::time::Duration::from_millis(120)).await;
            let (schema, diags) = bg
                .spawn(async move { (dbml_analysis::analyze(&text), dbml_analysis::lint(&text)) })
                .await;
            this.update(cx, |this, cx| {
                let Some(i) = this.doc_index(&editor) else {
                    return;
                };
                let doc = &mut this.docs[i];
                if doc.parse_gen != generation {
                    return;
                }
                *doc.marks.borrow_mut() = crate::ide::diag_marks(&diags);
                doc.diags = diags;
                if apply_schema {
                    doc.diagram.update(cx, |d, cx| {
                        d.set_schema(schema);
                        cx.notify();
                    });
                }
                editor.update(cx, |_, cx| cx.notify());
                cx.notify();
            })
            .ok();
        }));
    }

    /// Grava as posições dos cartões no sidecar (`<arquivo>.dbml.layout.json`) quando mudam
    /// (arrastar, desfazer/refazer, reorganizar, tabela nova), com debounce e em segundo plano.
    fn save_layout(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(doc) = self.docs.get_mut(i) else {
            return;
        };
        let Some(path) = doc.path.clone() else { return };
        let pos = doc.diagram.read(cx).positions().clone();
        if pos == doc.layout_saved {
            return;
        }
        doc.layout_saved.clone_from(&pos);
        let bg = cx.background_executor().clone();
        doc.layout_task = Some(cx.spawn(async move |_, _| {
            bg.timer(std::time::Duration::from_millis(400)).await;
            bg.spawn(async move {
                if let Err(e) = crate::session::save_positions(&path, &pos) {
                    eprintln!(
                        "{}",
                        crate::i18n::t("não foi possível gravar o layout de {}: {}")
                            .replacen("{}", &path.display().to_string(), 1)
                            .replacen("{}", &e.to_string(), 1)
                    );
                }
            })
            .await;
        }));
    }

    /// Publica as abas com edições não salvas (para o panic hook) e agenda a gravação das cópias
    /// de recuperação; salvar ou fechar a aba apaga a cópia na próxima gravação.
    fn publish_recovery(&mut self, cx: &mut Context<Self>) {
        let live: Vec<crate::recovery::Live> = self
            .docs
            .iter()
            .filter(|d| d.dirty)
            .map(|d| crate::recovery::Live {
                id: d.id,
                title: d.title.clone(),
                path: d.path.clone(),
                text: d.last_text.clone(),
            })
            .collect();
        crate::recovery::set_live(live.clone());
        if live.is_empty() && !self.recovery_active {
            return;
        }
        self.recovery_active = !live.is_empty();
        let Some(dir) = crate::recovery::dir() else {
            return;
        };
        let bg = cx.background_executor().clone();
        self.recovery_task = Some(cx.spawn(async move |_, _| {
            bg.timer(std::time::Duration::from_millis(1500)).await;
            bg.spawn(async move {
                if let Err(e) = crate::recovery::write_all(&dir, &live) {
                    eprintln!("recovery: {e}");
                }
            })
            .await;
        }));
    }

    /// Devolve como abas não salvas o que uma execução anterior deixou em `recovery/`. A aba
    /// herda o id da cópia, então ela continua sendo regravada/apagada normalmente. Abas de
    /// arquivo voltam sem caminho: gravar exige "Salvar como", nada sobrescreve o disco calado.
    fn adopt_recovery(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = crate::recovery::dir() else {
            return;
        };
        let entries = crate::recovery::read_all(&dir);
        if entries.is_empty() {
            return;
        }
        let count = entries.len();
        for entry in entries {
            let title = match &entry.path {
                Some(p) => crate::i18n::t("{} (recuperado)").replace("{}", &file_title(p)),
                None => crate::i18n::t("{} (recuperado)").replace("{}", &entry.title),
            };
            self.add_doc(entry.text, None, title, window, cx);
            if let Some(doc) = self.docs.last_mut() {
                doc.id = entry.id;
                doc.saved_text = std::sync::Arc::from("");
                doc.dirty = true;
            }
        }
        self.recovery_active = true;
        self.publish_recovery(cx);
        window.push_notification(
            crate::i18n::t("{} aba(s) com edições não salvas foram recuperadas de uma sessão que terminou de forma inesperada.")
                .replace("{}", &count.to_string()),
            cx,
        );
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
        let bg = cx.background_executor().clone();
        let read_path = path.clone();
        self.loading += 1;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = bg
                .spawn(async move { std::fs::read_to_string(&read_path) })
                .await;
            this.update_in(cx, |this, window, cx| {
                this.loading = this.loading.saturating_sub(1);
                this.finish_open(path, result, window, cx);
            })
            .ok();
        })
        .detach();
    }

    fn finish_open(
        &mut self,
        path: PathBuf,
        result: std::io::Result<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        match result {
            Ok(text) => {
                let title = file_title(&path);
                self.add_doc(text, Some(path.clone()), title, window, cx);
                if self.project.is_none()
                    && let Some(dir) = path.parent()
                {
                    Self::rescan(dir.to_path_buf(), window, cx);
                }
                self.touch_recent(path, cx);
                self.save_session(cx);
            }
            Err(e) => {
                eprintln!("Could not open {}: {e}", path.display());
                let hint = match e.kind() {
                    std::io::ErrorKind::NotFound => {
                        "O arquivo foi movido ou removido. Escolha o arquivo novamente."
                    }
                    std::io::ErrorKind::PermissionDenied => {
                        "Sem acesso ao arquivo. Verifique as permissões ou abra uma cópia."
                    }
                    std::io::ErrorKind::InvalidData => {
                        "O arquivo não está em UTF-8. Abra uma cópia salva em UTF-8."
                    }
                    _ => "Não foi possível abrir o arquivo. Selecione-o novamente.",
                };
                window.push_notification(crate::i18n::t(hint), cx);
                cx.notify();
            }
        }
    }

    /// Varre `dir` em segundo plano e guarda a lista (projeto ou pasta do arquivo avulso).
    fn rescan(dir: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let bg = cx.background_executor().clone();
        let scan_dir = dir.clone();
        cx.spawn_in(window, async move |this, cx| {
            let files = bg.spawn(async move { scan(&scan_dir) }).await;
            this.update_in(cx, |this, _, cx| {
                if this.project.as_ref() == Some(&dir) {
                    this.files = files;
                } else {
                    this.loose_files = (dir, files);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Registra o arquivo no histórico de recentes (não na restauração da sessão).
    fn touch_recent(&mut self, path: PathBuf, cx: &App) {
        if self.restoring_session {
            return;
        }
        let Some(doc) = self.docs.iter().find(|d| d.path.as_ref() == Some(&path)) else {
            return;
        };
        let tables = doc.diagram.read(cx).schema().tables.len();
        crate::session::push_recent(
            &mut self.recent,
            crate::session::RecentFile {
                path,
                tables,
                at: crate::session::now_secs(),
            },
        );
    }

    fn set_project(&mut self, dir: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.files.clear();
        self.project = Some(dir.clone());
        if crate::harness::var("DBM_PANE").is_none() {
            self.pane = Pane::Files;
        }
        self.save_session(cx);
        let bg = cx.background_executor().clone();
        self.loading += 1;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let scan_dir = dir.clone();
            let files = bg.spawn(async move { scan(&scan_dir) }).await;
            this.update_in(cx, |this, window, cx| {
                this.loading = this.loading.saturating_sub(1);
                if this.project.as_ref() != Some(&dir) {
                    return;
                }
                this.files = files;
                if let Some(first) = this.files.first().cloned() {
                    this.open_path(first, window, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Restaura a sessão anterior. Pasta, abas e arquivos são lidos em segundo plano; a janela
    /// abre na hora e `finish_restore` monta as abas quando a leitura termina.
    fn restore_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let session = crate::session::load();
        if crate::harness::var("DBM_PANE").is_none()
            && let Some(pane) = session.pane.as_deref()
        {
            self.pane = match pane {
                "files" => Pane::Files,
                "problems" => Pane::Problems,
                _ => Pane::Schema,
            };
        }
        if crate::harness::var("DBM_EDITOR").is_none()
            && let Some(ed) = session.editor_open
        {
            self.editor_open = ed;
            self.code_only = ed && session.code_view == Some(true);
        }
        let bg = cx.background_executor().clone();
        let (project, tabs) = (session.project_dir.clone(), session.open_tabs.clone());
        self.loading += 1;
        cx.spawn_in(window, async move |this, cx| {
            let loaded = bg
                .spawn(async move {
                    let project = project.filter(|d| d.is_dir());
                    let files = project.as_deref().map(scan).unwrap_or_default();
                    let mut docs = Vec::new();
                    for path in tabs {
                        if let Ok(text) = std::fs::read_to_string(&path) {
                            docs.push((path, text));
                        }
                    }
                    if docs.is_empty()
                        && let Some(first) = files.first()
                        && let Ok(text) = std::fs::read_to_string(first)
                    {
                        docs.push((first.clone(), text));
                    }
                    (project, files, docs)
                })
                .await;
            this.update_in(cx, |this, window, cx| {
                this.loading = this.loading.saturating_sub(1);
                this.finish_restore(&session, loaded, window, cx);
            })
            .ok();
        })
        .detach();
    }

    fn finish_restore(
        &mut self,
        session: &crate::session::SessionState,
        (project, files, docs): (Option<PathBuf>, Vec<PathBuf>, Vec<(PathBuf, String)>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.restoring_session = true;
        if project.is_some() && self.project.is_none() {
            self.files = files;
            self.project = project;
        }
        for (path, text) in docs {
            if self.docs.iter().any(|d| d.path.as_ref() == Some(&path)) {
                continue;
            }
            let title = file_title(&path);
            self.add_doc(text, Some(path.clone()), title, window, cx);
            if let Some(vp) = session.viewports.get(&path)
                && let Some(doc) = self.docs.last()
            {
                doc.diagram.update(cx, |d, _| {
                    d.set_viewport(vp.zoom, (vp.pan_x, vp.pan_y));
                });
            }
        }
        if let Some(active_path) = session.active_file.as_ref()
            && let Some(idx) = self
                .docs
                .iter()
                .position(|d| d.path.as_ref() == Some(active_path))
        {
            self.active = idx;
        }
        self.restoring_session = false;
        if self.docs.is_empty() && self.project.is_none() {
            let text = include_str!("../../assets/sample.dbml").to_string();
            self.add_doc(text, None, "example.dbml".into(), window, cx);
        }
        cx.notify();
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
            code_view: Some(self.code_only),
            recent: self.recent.clone(),
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
            prompt: Some(crate::i18n::t("Abrir").into()),
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
            prompt: Some(crate::i18n::t("Abrir projeto").into()),
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
            self.write(self.active, path, window, cx);
            return;
        }
        let dir = self
            .project
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let editor = doc.editor.clone();
        let rx = cx.prompt_for_new_path(&dir, Some(&doc.title));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = rx.await else { return };
            this.update_in(cx, |this, window, cx| {
                if let Some(i) = this.doc_index(&editor) {
                    this.write(i, path, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn doc_index(&self, editor: &Entity<EditorState>) -> Option<usize> {
        self.docs
            .iter()
            .position(|doc| doc.editor.entity_id() == editor.entity_id())
    }

    fn schedule_autosave(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let doc = &mut self.docs[i];
        doc.autosave = None;
        if !doc.dirty || doc.path.is_none() {
            return;
        }
        let editor = doc.editor.clone();
        let bg = cx.background_executor().clone();
        let timer = bg.timer(std::time::Duration::from_millis(600));
        doc.autosave = Some(cx.spawn_in(window, async move |this, cx| {
            timer.await;
            let Ok(Some((path, text, saved))) = this.update_in(cx, |this, _, cx| {
                let doc = &this.docs[this.doc_index(&editor)?];
                let path = doc.path.clone().filter(|_| doc.dirty)?;
                Some((
                    path,
                    doc.editor.read(cx).value().to_string(),
                    doc.saved_text.clone(),
                ))
            }) else {
                return;
            };
            // leitura de conferência e gravação fora da thread da UI
            let (write_path, write_text) = (path.clone(), text.clone());
            let result = bg
                .spawn(async move {
                    let on_disk = std::fs::read_to_string(&write_path).ok();
                    if on_disk.as_deref() != Some(saved.as_ref()) {
                        return Err(None);
                    }
                    crate::session::atomic_write_sync(&write_path, &write_text).map_err(Some)
                })
                .await;
            this.update_in(cx, |this, window, cx| {
                this.finish_autosave(&editor, &path, &text, result, window, cx);
            })
            .ok();
        }));
    }

    /// Conclui o autosave em segundo plano. `Err(None)` = o disco mudou (não sobrescreve).
    fn finish_autosave(
        &mut self,
        editor: &Entity<EditorState>,
        path: &Path,
        text: &str,
        result: Result<(), Option<std::io::Error>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.doc_index(editor) else {
            return;
        };
        let doc = &mut self.docs[i];
        if doc.path.as_deref() != Some(path) {
            return;
        }
        match result {
            Ok(()) => {
                doc.saved_text = std::sync::Arc::from(text);
                // se o usuário digitou durante a gravação, continua sujo (novo autosave já agendado)
                doc.dirty = doc.last_text.as_ref() != doc.saved_text.as_ref();
                doc.watch_warned = None;
                doc.save_armed = None;
            }
            Err(None) => {
                window.push_notification(
                    crate::i18n::t("O arquivo mudou no disco. Salve manualmente para confirmar."),
                    cx,
                );
            }
            Err(Some(e)) => {
                eprintln!("Could not save {}: {e}", path.display());
                window.push_notification(crate::i18n::t(save_error_hint(&e)), cx);
            }
        }
        self.publish_recovery(cx);
        cx.notify();
    }

    /// Abre na janela existente o que outra instância ou o sistema entregou (`handoff`).
    fn spawn_handoff_poll(window: &mut Window, cx: &mut Context<Self>) -> Task<()> {
        let bg = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            loop {
                bg.timer(std::time::Duration::from_millis(250)).await;
                let requests = crate::handoff::take();
                let notices = crate::session::take_notices();
                if requests.is_empty() && notices.is_empty() {
                    continue;
                }
                let alive = this.update_in(cx, |this, window, cx| {
                    for notice in notices {
                        window.push_notification(notice, cx);
                    }
                    if !requests.is_empty() {
                        window.activate_window();
                    }
                    for path in requests.into_iter().flat_map(|r| r.paths) {
                        if path.is_dir() {
                            this.set_project(path, window, cx);
                        } else {
                            this.open_path(path, window, cx);
                        }
                    }
                });
                if alive.is_err() {
                    return;
                }
            }
        })
    }

    /// Confere em segundo plano, a cada 1,5 s, se os arquivos abertos mudaram no disco
    /// (`git pull`, outro editor). A leitura roda fora da thread da UI.
    fn spawn_disk_watcher(window: &mut Window, cx: &mut Context<Self>) -> Task<()> {
        let bg = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            loop {
                bg.timer(std::time::Duration::from_millis(1500)).await;
                let Ok(paths) = this.update_in(cx, |this, _, _| {
                    this.docs
                        .iter()
                        .filter_map(|d| d.path.clone())
                        .collect::<Vec<_>>()
                }) else {
                    return;
                };
                if paths.is_empty() {
                    continue;
                }
                let reads = bg
                    .spawn(async move {
                        paths
                            .into_iter()
                            .map(|p| {
                                let text = crate::disk_sync::read_disk(&p);
                                (p, text)
                            })
                            .collect::<Vec<_>>()
                    })
                    .await;
                if this
                    .update_in(cx, |this, window, cx| {
                        this.apply_disk_reads(reads, window, cx);
                    })
                    .is_err()
                {
                    return;
                }
            }
        })
    }

    fn apply_disk_reads(
        &mut self,
        reads: Vec<(PathBuf, Option<String>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::disk_sync::{Watch, watch_decision};
        for (path, disk) in reads {
            let Some(i) = self
                .docs
                .iter()
                .position(|d| d.path.as_ref() == Some(&path))
            else {
                continue;
            };
            let doc = &mut self.docs[i];
            let decision = watch_decision(
                &doc.saved_text,
                &doc.last_text,
                disk.as_deref(),
                doc.watch_warned.as_deref(),
            );
            match decision {
                Watch::Idle => {}
                Watch::Reload(text) => self.reload_doc(i, &text, window, cx),
                Watch::Conflict(text) => {
                    doc.watch_warned = Some(std::sync::Arc::from(text.as_str()));
                    window.push_notification(
                        crate::i18n::t("{} mudou no disco e há edições não salvas aqui. Salvar sobrescreve a versão do disco.")
                            .replace("{}", &doc.title),
                        cx,
                    );
                }
            }
        }
    }

    /// Adota `text` (lido do disco) como conteúdo salvo e atual da aba `i`. A versão anterior
    /// do buffer entra no histórico, então Desfazer a recupera.
    fn reload_doc(&mut self, i: usize, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.docs.get_mut(i) else {
            return;
        };
        let new_arc: std::sync::Arc<str> = std::sync::Arc::from(text);
        let changed = doc.last_text != new_arc;
        if changed {
            let snap = Self::snapshot_of(doc, cx);
            doc.history.push(snap);
        }
        doc.saved_text = new_arc.clone();
        doc.last_text = new_arc;
        doc.dirty = false;
        doc.watch_warned = None;
        doc.save_armed = None;
        let editor = doc.editor.clone();
        let diagram = doc.diagram.clone();
        let edges = doc
            .path
            .as_deref()
            .map(crate::session::load_edges)
            .unwrap_or_default();
        let saved_positions = doc
            .path
            .as_deref()
            .map(crate::session::load_positions)
            .unwrap_or_default();
        let title = doc.title.clone();
        if editor.read(cx).value().as_ref() != text {
            self.suppress_history = true;
            editor.update(cx, |e, cx| e.replace_all(text.to_owned(), window, cx));
            self.suppress_history = false;
        }
        let schema = dbml_analysis::analyze(text);
        diagram.update(cx, |d, cx| {
            d.set_schema(schema);
            d.set_edges(edges);
            d.seed_positions(saved_positions);
            cx.notify();
        });
        if changed {
            window.push_notification(
                crate::i18n::t("{} foi recarregado do disco.").replace("{}", &title),
                cx,
            );
        }
        self.publish_recovery(cx);
        cx.notify();
    }

    /// Salvar explícito (Ctrl+S, fechar aba, sair): síncrono porque o chamador precisa do resultado.
    fn write(
        &mut self,
        i: usize,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(doc) = self.docs.get_mut(i) else {
            return false;
        };
        let text = doc.editor.read(cx).value().to_string();
        if doc.path.as_deref() == Some(path.as_path()) {
            let disk = crate::disk_sync::read_disk(&path);
            if !crate::disk_sync::save_allowed(
                &doc.saved_text,
                disk.as_deref(),
                doc.save_armed.as_deref(),
            ) {
                doc.save_armed = disk.map(|d| std::sync::Arc::from(d.as_str()));
                window.push_notification(
                    crate::i18n::t("O arquivo mudou no disco desde que foi aberto. Salve de novo para sobrescrever."),
                    cx,
                );
                return false;
            }
        }
        let ok = match crate::session::atomic_write_sync(&path, &text) {
            Ok(()) => {
                let saved: std::sync::Arc<str> = std::sync::Arc::from(text.as_str());
                doc.saved_text = saved.clone();
                doc.last_text = saved;
                doc.dirty = false;
                doc.watch_warned = None;
                doc.save_armed = None;
                self.publish_recovery(cx);
                let Some(doc) = self.docs.get_mut(i) else {
                    return true;
                };
                doc.autosave = None;
                window.push_notification(
                    crate::i18n::t("Salvo em {}").replace("{}", &path.display().to_string()),
                    cx,
                );
                doc.title = file_title(&path);
                doc.path = Some(path.clone());
                // arquivo novo/"salvar como": leva estilo das relações e posições para o sidecar
                Self::save_edges(doc, cx);
                doc.layout_saved.clear();
                self.save_layout(i, cx);
                if let Some(dir) = self.project.clone() {
                    Self::rescan(dir, window, cx);
                }
                self.touch_recent(path, cx);
                self.save_session(cx);
                true
            }
            Err(e) => {
                eprintln!("Could not save {}: {e}", path.display());
                window.push_notification(crate::i18n::t(save_error_hint(&e)), cx);
                false
            }
        };
        cx.notify();
        ok
    }

    fn force_close_tab_at(&mut self, i: usize, cx: &mut Context<Self>) {
        if i >= self.docs.len() {
            return;
        }
        self.docs.remove(i);
        self.publish_recovery(cx);
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
        let editor = doc.editor.clone();
        if let Some(path) = doc.path.clone() {
            if self.write(i, path, window, cx) {
                self.force_close_tab_at(i, cx);
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
                if let Some(i) = this.doc_index(&editor)
                    && this.write(i, path, window, cx)
                {
                    this.force_close_tab_at(i, cx);
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
        let editor = doc.editor.clone();
        let shell = cx.entity();
        let g = Glass::of_scaled(self.mode, self.translucent, self.ui_scale);
        window.open_dialog(cx, move |dialog, _, _| {
            let title = title.clone();
            let shell = shell.clone();
            let editor = editor.clone();
            dialog_frame(
                dialog,
                &g,
                440.,
                Some(crate::i18n::t("Alterações não salvas")),
            )
            .content(move |content, _, _| {
                let shell_save = shell.clone();
                let shell_discard = shell.clone();
                let editor_save = editor.clone();
                let editor_discard = editor.clone();
                content.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(16.))
                        .child(
                            div().text_size(TEXT_SM).child(
                                crate::i18n::t("Deseja salvar as alterações feitas em \"{}\"?")
                                    .replace("{}", &title),
                            ),
                        )
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(8.))
                                .child(
                                    Button::new("dlg-cancel-close")
                                        .ghost()
                                        .small()
                                        .label(crate::i18n::t("Cancelar"))
                                        .on_click(|_, window, cx| {
                                            window.close_dialog(cx);
                                        }),
                                )
                                .child(
                                    Button::new("dlg-discard-close")
                                        .ghost()
                                        .small()
                                        .label(crate::i18n::t("Descartar alterações"))
                                        .on_click(move |_, window, cx| {
                                            window.close_dialog(cx);
                                            shell_discard.update(cx, |s, cx| {
                                                if let Some(i) = s.doc_index(&editor_discard) {
                                                    s.force_close_tab_at(i, cx);
                                                }
                                            });
                                        }),
                                )
                                .child(
                                    Button::new("dlg-save-close")
                                        .primary()
                                        .small()
                                        .label(crate::i18n::t("Salvar"))
                                        .on_click(move |_, window, cx| {
                                            window.close_dialog(cx);
                                            shell_save.update(cx, |s, cx| {
                                                if let Some(i) = s.doc_index(&editor_save) {
                                                    s.save_and_close_tab_at(i, window, cx);
                                                }
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
        if let Some(pane) = target {
            // clicar no painel já aberto recolhe a barra lateral; caso contrário abre exclusivamente
            if self.sidebar_open && !self.editor_open && self.pane == pane {
                self.sidebar_open = false;
            } else {
                self.pane = pane;
                self.sidebar_open = true;
                self.editor_open = false;
                self.code_only = false;
            }
        } else {
            // alternar editor DBML de forma exclusiva com os outros painéis esquerdos;
            // de "Code" volta para "Split"
            if self.editor_open && !self.code_only {
                self.editor_open = false;
            } else {
                self.editor_open = true;
                self.sidebar_open = false;
            }
            self.code_only = false;
        }
        self.save_prefs();
        self.save_session(cx);
        cx.notify();
    }

    /// Realça no editor o bloco da tabela selecionada (fundo em `accent`; a biblioteca não
    /// desenha a barra lateral, só preenchimento). Só recalcula quando seleção ou texto mudam.
    fn sync_table_highlight(&mut self, cx: &mut Context<Self>) {
        let accent = Glass::of_scaled(self.mode, self.translucent, self.ui_scale).accent;
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        let selected = doc
            .diagram
            .read(cx)
            .selected()
            .map(schema::Table::qualified_name);
        if doc.hl_key.0 == selected && doc.hl_key.1 == doc.last_text {
            return;
        }
        let range = selected
            .as_deref()
            .and_then(|t| crate::ide::table_block(&doc.last_text, t));
        doc.hl_key = (selected, doc.last_text.clone());
        let fill = range.map(|r| {
            RangeDecoration::new(r)
                .with_style(RangeDecorationStyle::Fill)
                .with_color(accent.opacity(0.07))
        });
        doc.table_hl.set(fill.into_iter().collect(), cx);
    }

    fn view(&self) -> View {
        match (self.editor_open, self.code_only) {
            (false, _) => View::Diagram,
            (true, false) => View::Split,
            (true, true) => View::Code,
        }
    }

    fn set_view(&mut self, view: View, cx: &mut Context<Self>) {
        self.editor_open = view != View::Diagram;
        self.code_only = view == View::Code;
        if self.editor_open {
            self.sidebar_open = false;
        }
        self.save_prefs();
        self.save_session(cx);
        cx.notify();
    }

    fn show_diagram_view(&mut self, _: &ShowDiagramView, _: &mut Window, cx: &mut Context<Self>) {
        self.set_view(View::Diagram, cx);
    }

    fn show_split_view(&mut self, _: &ShowSplitView, _: &mut Window, cx: &mut Context<Self>) {
        self.set_view(View::Split, cx);
    }

    fn show_code_view(&mut self, _: &ShowCodeView, _: &mut Window, cx: &mut Context<Self>) {
        self.set_view(View::Code, cx);
    }

    /// Alterna o filtro do explorador de esquema.
    fn filter_explorer(&mut self, _: &FilterExplorer, window: &mut Window, cx: &mut Context<Self>) {
        let toggle = self.pane == Pane::Schema && self.sidebar_open && !self.editor_open;
        self.pane = Pane::Schema;
        self.sidebar_open = true;
        self.editor_open = false;
        self.code_only = false;
        if toggle {
            self.toggle_filter_explorer(window, cx);
        } else {
            self.open_filter(window, cx);
        }
        self.save_prefs();
        self.save_session(cx);
    }

    /// Alterna o campo de filtro no cabeçalho do painel Esquema.
    fn toggle_filter_explorer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.filter.is_some() {
            self.close_filter(window, cx);
        } else {
            self.open_filter(window, cx);
        }
    }

    fn open_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((input, _)) = &self.filter {
            let handle = input.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
            return;
        }
        let input = cx
            .new(|cx| InputState::new(window, cx).placeholder(crate::i18n::t("Filtrar tabelas…")));
        let handle = input.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
        let sub = cx.subscribe_in(
            &input,
            window,
            |_: &mut Shell, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    cx.notify();
                }
            },
        );
        self.filter = Some((input, sub));
        cx.notify();
    }

    /// Esc: limpa e fecha o filtro, devolvendo o foco ao shell.
    fn close_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.filter.take().is_some() {
            window.focus(&self.focus, cx);
            cx.notify();
        }
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
                        eprintln!("Could not open export file dialog: {portal_err}");
                        let _ = cx.update(|window, cx| {
                            window.push_notification(
                                crate::i18n::t("Não foi possível abrir a janela de salvamento. Tente exportar novamente."),
                                cx,
                            );
                        });
                        return;
                    }
                }
            };
            let res = build().and_then(|bytes| {
                crate::session::atomic_write_bytes(&path, &bytes).map_err(|error| {
                    eprintln!("Could not write export {}: {error}", path.display());
                    crate::i18n::t(export_write_hint(&error)).to_owned()
                })
            });
            cx.update(|window, cx| match res {
                Ok(()) => {
                    window
                        .push_notification(crate::i18n::t("Exportado {} para {}").replacen("{}", &what, 1).replacen("{}", &path.display().to_string(), 1), cx);
                }
                Err(e) => window.push_notification(crate::i18n::t("Falha ao exportar {}: {}").replacen("{}", &what, 1).replacen("{}", &e, 1), cx),
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
            move || {
                dbml_analysis::try_export_sql_ddl(&text, dialect)
                    .map(String::into_bytes)
                    .map_err(|err| {
                        use dbml_analysis::SqlExportError;
                        eprintln!("SQL generation failed: {err}");
                        match err {
                            SqlExportError::InvalidSource(message) => crate::i18n::t("Revise o DBML antes de exportar SQL: {}")
                                .replace("{}", &crate::i18n::diagnostic(&message)),
                            SqlExportError::SqliteSchema(schema) => crate::i18n::t("O SQLite não aceita o schema {} nesta exportação. Escolha outro dialeto.")
                                .replace("{}", &schema),
                            SqlExportError::UnknownDialect(_) => crate::i18n::t("Escolha outro dialeto para exportar SQL.").into(),
                        }
                    })
            },
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

    fn export_docs_markdown(
        &mut self,
        _: &ExportDocsMarkdown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.doc() else { return };
        let schema = doc.diagram.read(cx).schema().clone();
        let stem = self.export_stem();
        let suggested = format!("{stem}.md");
        self.save_export(
            &suggested,
            "Markdown Docs".into(),
            move || Ok(diagram_export::to_markdown(&schema, &stem).into_bytes()),
            window,
            cx,
        );
    }

    fn export_docs_html(
        &mut self,
        _: &ExportDocsHtml,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.doc() else { return };
        let (schema, pos, edges) = {
            let d = doc.diagram.read(cx);
            (d.schema().clone(), d.positions().clone(), d.edges().clone())
        };
        let theme = if self.mode == Mode::Dark {
            diagram_export::Theme::dark()
        } else {
            diagram_export::Theme::light()
        };
        let stem = self.export_stem();
        let suggested = format!("{stem}.html");
        self.save_export(
            &suggested,
            "HTML Docs".into(),
            move || {
                let svg = diagram_export::to_svg(&schema, &pos, &edges, &theme);
                Ok(diagram_export::to_html(&schema, &svg, &stem).into_bytes())
            },
            window,
            cx,
        );
    }

    fn export_image(&mut self, kind: ImageKind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc() else { return };
        let (schema, pos, edges, tables, enum_name, relation) = {
            let d = doc.diagram.read(cx);
            let (selected, enum_name, mut tables) = d.selection_state();
            tables.extend(selected);
            (
                d.schema().clone(),
                d.positions().clone(),
                d.edges().clone(),
                tables,
                enum_name,
                d.selected_ref().cloned(),
            )
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
                let selection = diagram_export::Selection {
                    tables: &tables,
                    enum_name: enum_name.as_deref(),
                    relation: relation.as_ref().map(|((from, from_col), (to, to_col))| {
                        (
                            (from.as_str(), from_col.as_str()),
                            (to.as_str(), to_col.as_str()),
                        )
                    }),
                };
                let svg = diagram_export::to_svg_with_selection(
                    &schema, &pos, &edges, &theme, &selection,
                );
                let fonts = &crate::FONTS;
                match kind {
                    ImageKind::Svg => Ok(embed_svg_fonts(svg).into_bytes()),
                    ImageKind::Png => diagram_export::svg_to_png(&svg, fonts, 2.0),
                    ImageKind::Jpeg => diagram_export::svg_to_jpeg(&svg, fonts, 2.0, 92),
                }
                .map_err(|error| {
                    eprintln!("Image generation failed: {error}");
                    crate::i18n::t("Tente exportar em SVG.").into()
                })
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
            window.push_notification(
                crate::i18n::t("Selecione uma tabela para adicionar a predefinição de coluna"),
                cx,
            );
            return;
        };
        let text = doc.editor.read(cx).value().to_string();
        match dbml_edit::add_column_preset_text(&text, &table, preset) {
            None => window.push_notification(
                crate::i18n::t("Tabela '{}' não encontrada").replace("{}", &table),
                cx,
            ),
            Some(new) if new == text => {
                window.push_notification(
                    crate::i18n::t("{}: as colunas já existem").replace("{}", preset.label()),
                    cx,
                );
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
        let port = input(window, cx, crate::i18n::t("porta padrão"));
        let database = input(window, cx, crate::i18n::t("nome do banco"));
        let user = input(window, cx, crate::i18n::t("usuário"));
        let password = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(crate::i18n::t("senha"))
                .masked(true)
        });
        let path = input(window, cx, "/path/to/database.sqlite");
        let schemas = input(
            window,
            cx,
            crate::i18n::t("todos os schemas (ou: public, sales)"),
        );
        let shell_entity = cx.entity();
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
            dialog_bare(dialog, &g, 560.).margin_top(px(76.)).content(
                move |content, _, cx| {
                    let st = model.read(cx).clone();
                    let is_file = st.dialect == Dialect::Sqlite;
                    let connecting = st.status == Status::Connecting;

                    let row = |label: &'static str, field: Entity<InputState>| {
                        div()
                            .flex()
                            .items_center()
                            .gap(SP_SM)
                            .gap(px(12.))
                            .child(
                                div()
                                    .w(px(96.))
                                    .flex_none()
                                    .text_size(px(12.5))
                                    .text_color(g.text_muted)
                                    .child(label),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .h(px(32.))
                                    .px(px(10.))
                                    .rounded(px(7.))
                                    .bg(g.input_solid)
                                    .border_1()
                                    .border_color(g.edge_strong)
                                    .flex()
                                    .items_center()
                                    .child(Input::new(&field).small().appearance(false)),
                            )
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
                            (true, _) => crate::i18n::t("Testar conexão"),
                            (false, true) => crate::i18n::t("Conectando…"),
                            (false, false) => crate::i18n::t("Conectar"),
                        };
                        let b = dialog_btn(
                            &g,
                            id,
                            label,
                            test.then_some(IconName::PlugZap),
                            !test,
                            test,
                            !connecting,
                        );
                        if connecting {
                            return b;
                        }
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
                            params.tls = m.tls;
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
                    let conflicts = shell_entity.read(cx).import_conflicts(&st.dbml, cx);
                    let apply_btn =
                        |id: &'static str, label: String, replace: bool, primary: bool| {
                            let model = model.clone();
                            let shell_entity = shell_entity.clone();
                            let enabled = can_apply && (replace || conflicts.is_empty());
                            let b = dialog_btn(&g, id, label, None, primary, !primary, enabled);
                            if !enabled {
                                return b;
                            }
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
                        crate::i18n::t("Confirmar substituição?").to_string()
                    } else {
                        crate::i18n::t("Substituir").to_string()
                    };

                    let status_line = match &st.status {
                        Status::Idle => {
                            div().child(crate::i18n::t("Somente leitura: apenas o catálogo é lido, nunca os seus dados."))
                        }
                        Status::Connecting => div().child(crate::i18n::t("Conectando e lendo o esquema…")),
                        Status::Failed(msg) => div()
                            .text_color(rgb(crate::glass::DIAG_ERROR))
                            .child(crate::i18n::t(connection_hint(msg, is_file))),
                        Status::Tested(msg) => div().child(msg.clone()),
                        Status::Ready => div().child(crate::i18n::t(if st.tables == 1 { "{} tabela encontrada. Substitua o documento ou insira-a." } else { "{} tabelas encontradas. Substitua o documento ou insira-as." }).replace("{}", &st.tables.to_string())),
                    };

                    let mut form = div().flex().flex_col().gap(px(8.)).px(px(18.)).py(px(10.));
                    if is_file {
                        let pick = {
                            let path = path.clone();
                            dialog_btn(&g, "db-pick-file", crate::i18n::t("Procurar…"), None, false, true, true)
                                .h(px(32.))
                                .text_color(g.text)
                                .on_click(move |_, window, cx| {
                                    let rx = cx.prompt_for_paths(PathPromptOptions {
                                        files: true,
                                        directories: false,
                                        multiple: false,
                                        prompt: Some(crate::i18n::t("Selecione o banco SQLite").into()),
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
                                .child(div().flex_1().child(row(crate::i18n::t("Arquivo"), path.clone())))
                                .child(pick),
                        );
                    } else {
                        form = form
                            .child(row("Host", host.clone()))
                            .child(row(crate::i18n::t("Porta"), port.clone()))
                            .child(row(crate::i18n::t("Banco"), database.clone()))
                            .child(row(crate::i18n::t("Usuário"), user.clone()))
                            .child(row(crate::i18n::t("Senha"), password.clone()))
                            .child(row("Schemas", schemas.clone()));
                        {
                            let model = model.clone();
                            let tls_modes = [
                                (crate::constants::ID_TLS_AUTO, "Auto", db_introspect::TlsMode::Auto),
                                (
                                    crate::constants::ID_TLS_VERIFY,
                                    crate::i18n::t("Verificar"),
                                    db_introspect::TlsMode::Verify,
                                ),
                                (
                                    crate::constants::ID_TLS_REQUIRE,
                                    crate::i18n::t("Exigir"),
                                    db_introspect::TlsMode::Require,
                                ),
                                (
                                    crate::constants::ID_TLS_DISABLE,
                                    crate::i18n::t("Desativar"),
                                    db_introspect::TlsMode::Disable,
                                ),
                            ];
                            let mut tls_row = div()
                                .flex()
                                .items_center()
                                .gap(px(crate::constants::TLS_BTN_GAP_PX));
                            for (id, label, mode) in tls_modes {
                                let model = model.clone();
                                let active = st.tls == mode;
                                tls_row = tls_row.child(
                                    dialog_btn(&g, id, label, None, active, !active, !connecting)
                                        .h(px(crate::constants::TLS_BTN_HEIGHT_PX))
                                        .on_click(move |_, _, cx| {
                                            model.update(cx, |m, cx| {
                                                m.tls = mode;
                                                cx.notify();
                                            });
                                        }),
                                );
                            }
                            form = form.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(crate::constants::TLS_ROW_GAP_PX))
                                    .child(
                                        div()
                                            .w(px(crate::constants::TLS_LABEL_WIDTH_PX))
                                            .flex_none()
                                            .text_size(px(crate::constants::TLS_LABEL_FONT_PX))
                                            .text_color(g.text_muted)
                                            .child(crate::constants::TLS_FIELD_LABEL),
                                    )
                                    .child(tls_row),
                            );
                        }
                        if st.dialect == Dialect::SqlServer {
                            let model = model.clone();
                            form = form.child(
                                Checkbox::new("db-trust")
                                    .label(crate::i18n::t("Confiar no certificado do servidor (autoassinado, ex.: Docker)"))
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

                    content
                        .child(dialog_header(
                            &g,
                            crate::i18n::t("Conectar ao banco"),
                            crate::i18n::t("Faça a engenharia reversa de um esquema existente para DBML."),
                        ))
                        .child(div().flex().px(px(18.)).pt(px(14.)).pb(px(6.)).child(segmented(
                                &g,
                                &[
                                    (
                                        "db-pg",
                                        "PostgreSQL",
                                        st.dialect == Dialect::Postgres,
                                        !connecting,
                                    ),
                                    ("db-my", "MySQL", st.dialect == Dialect::Mysql, !connecting),
                                    (
                                        "db-ms",
                                        "SQL Server",
                                        st.dialect == Dialect::SqlServer,
                                        !connecting,
                                    ),
                                    (
                                        "db-sq",
                                        "SQLite",
                                        st.dialect == Dialect::Sqlite,
                                        !connecting,
                                    ),
                                    ("db-or", crate::i18n::t("Oracle (em breve)"), false, false),
                                ],
                                {
                                    let model = model.clone();
                                    move |i, cx| {
                                        let d = [
                                            Dialect::Postgres,
                                            Dialect::Mysql,
                                            Dialect::SqlServer,
                                            Dialect::Sqlite,
                                        ][i];
                                        model.update(cx, |m, cx| {
                                            m.set_dialect(d);
                                            cx.notify();
                                        });
                                    }
                                },
                            )))
                        .child(form)
                        .child(
                            div()
                                .id("db-status")
                                .max_h(px(72.))
                                .overflow_y_scroll()
                                .mx(px(18.))
                                .mt(px(4.))
                                .px(px(12.))
                                .py(px(10.))
                                .rounded(px(8.))
                                .bg(g.input)
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .text_size(px(12.5))
                                .text_color(g.text_muted)
                                .child(
                                    Icon::new(IconName::ShieldCheck)
                                        .size(px(14.))
                                        .flex_none()
                                        .text_color(g.ok),
                                )
                                .child(status_line),
                        )
                        .when(!conflicts.is_empty(), |d| {
                            d.child(div().px(px(18.)).text_size(TEXT_XS)
                                .text_color(rgb(crate::glass::DIAG_WARNING))
                                .child(conflicts.clone()))
                        })
                        .when(!st.warnings.is_empty(), |d| {
                            let mut list = div()
                                .id("db-warn")
                                .flex()
                                .flex_col()
                                .max_h(px(64.))
                                .overflow_y_scroll()
                                .px(px(18.))
                                .pt(px(8.))
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
                                .items_center()
                                .gap(SP_SM)
                                .px(px(18.))
                                .py(px(16.))
                                .child(test_btn)
                                .child(div().flex_1())
                                .child(dialog_btn(&g, "db-cancel", crate::i18n::t("Cancelar"), None, false, false, true)
                                    .on_click(|_, window, cx| window.close_dialog(cx)))
                                .child(connect_btn)
                                .when(can_apply, |d| {
                                    d.child(apply_btn("db-replace", replace_label, true, false))
                                        .child(apply_btn("db-insert", crate::i18n::t("Inserir").to_string(), false, true))
                                }),
                        )
                },
            )
        });
    }

    fn import_conflicts(&self, incoming: &str, cx: &App) -> String {
        let Some(doc) = self.doc().filter(|_| !incoming.is_empty()) else {
            return String::new();
        };
        let conflicts = dbml_analysis::import_conflicts(&doc.editor.read(cx).value(), incoming);
        if conflicts.is_empty() {
            String::new()
        } else {
            crate::i18n::t("Já existem no modelo: {}.").replace("{}", &conflicts.join(", "))
        }
    }

    fn import_sql_ddl(&mut self, _: &ImportSqlDdl, window: &mut Window, cx: &mut Context<Self>) {
        use sql_import::SqlDialect;
        let model = cx.new(|_| crate::import_sql::ImportSqlModel::new(self.import_dialect));
        let sql_editor = cx.new(|cx| EditorState::new(window, cx).line_number(true));
        let shell_entity = cx.entity();
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
            dialog_bare(dialog, &g, 820.)
                .margin_top(px(76.))
                .content(move |content, _, cx| {
                    let st = model.read(cx).clone();
                    let can_apply = st.can_apply();
                    let conflicts = shell_entity.read(cx).import_conflicts(&st.dbml, cx);
                    let (n_tables, n_refs) = st.counts();
                    let preview_text = if st.dbml.is_empty() {
                        crate::i18n::t("// A prévia em DBML aparece aqui…").to_string()
                    } else {
                        st.dbml.clone()
                    };

                    let pick_btn = {
                        let sql_editor = sql_editor.clone();
                        let model = model.clone();
                        dialog_btn(&g, "imp-pick-file", crate::i18n::t("Escolher arquivo .sql…"), Some(IconName::FileUp), false, true, true)
                            .h(px(30.))
                            .px(px(12.))
                            .text_size(px(12.5))
                            .text_color(g.text)
                            .on_click(move |_, window, cx| {
                                let rx = cx.prompt_for_paths(PathPromptOptions {
                                    files: true,
                                    directories: false,
                                    multiple: false,
                                    prompt: Some(crate::i18n::t("Selecione o arquivo SQL").into()),
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
                        let enabled = can_apply && (replace || conflicts.is_empty());
                        let b = dialog_btn(&g, id, label, None, primary, !primary, enabled);
                        if !enabled {
                            return b;
                        }
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
                        crate::i18n::t("Confirmar substituição?").to_string()
                    } else {
                        crate::i18n::t("Substituir").to_string()
                    };

                    // painel de 300px: cabeçalho de 30px (ícone, título, meta) + corpo
                    let pane = |icon: IconName, title: &'static str, meta: &'static str, body: AnyElement| {
                        div()
                            .flex_1()
                            .min_w_0()
                            .h(px(300.))
                            .flex()
                            .flex_col()
                            .rounded(px(10.))
                            .border_1()
                            .border_color(g.edge_strong)
                            .bg(g.surface)
                            .overflow_hidden()
                            .child(
                                div()
                                    .h(px(30.))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap(px(7.))
                                    .px(px(10.))
                                    .border_b_1()
                                    .border_color(g.edge_soft)
                                    .text_size(px(11.5))
                                    .text_color(g.text_faint)
                                    .child(Icon::new(icon).size(px(12.)))
                                    .child(title)
                                    .child(div().flex_1())
                                    .child(meta),
                            )
                            .child(div().flex_1().min_h_0().child(body))
                    };

                    content
                        .child(dialog_header(
                            &g,
                            crate::i18n::t("Importar SQL DDL"),
                            crate::i18n::t("Cole instruções CREATE TABLE. A prévia em DBML atualiza enquanto você digita."),
                        ))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .px(px(18.))
                                .py(px(14.))
                                .child(segmented(
                                    &g,
                                    &[
                                        ("imp-pg", "PostgreSQL", st.dialect == SqlDialect::Postgres, true),
                                        ("imp-my", "MySQL", st.dialect == SqlDialect::Mysql, true),
                                        ("imp-sq", "SQLite", st.dialect == SqlDialect::Sqlite, true),
                                        ("imp-ms", "SQL Server", st.dialect == SqlDialect::SqlServer, true),
                                        ("imp-or", "Oracle", st.dialect == SqlDialect::Oracle, true),
                                    ],
                                    {
                                        let (model, shell_entity) = (model.clone(), shell_entity.clone());
                                        move |i, cx| {
                                            let d = [
                                                SqlDialect::Postgres,
                                                SqlDialect::Mysql,
                                                SqlDialect::Sqlite,
                                                SqlDialect::SqlServer,
                                                SqlDialect::Oracle,
                                            ][i];
                                            shell_entity.update(cx, |s, _| s.import_dialect = d);
                                            model.update(cx, |m, cx| {
                                                m.set_dialect(d);
                                                cx.notify();
                                            });
                                        }
                                    },
                                ))
                                .child(div().flex_1())
                                .child(pick_btn),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(px(10.))
                                .px(px(18.))
                                .child(pane(
                                    IconName::FileCode,
                                    "SQL",
                                    "",
                                    Editor::new(&sql_editor).h_full().into_any_element(),
                                ))
                                .child(pane(
                                    IconName::Eye,
                                    crate::i18n::t("Prévia DBML"),
                                    crate::i18n::t("somente leitura"),
                                    div()
                                        .id("imp-preview-scroll")
                                        .size_full()
                                        .p(SP_SM)
                                        .overflow_y_scroll()
                                        .font_family(crate::glass::FONT_MONO)
                                        .text_size(TEXT_XS)
                                        .text_color(if st.dbml.is_empty() { g.text_faint } else { g.text })
                                        .child(preview_text)
                                        .into_any_element(),
                                )),
                        )
                        .when(st.too_large, |d| {
                            d.child(
                                div()
                                    .px(px(18.))
                                    .pt(px(10.))
                                    .text_size(TEXT_XS)
                                    .text_color(rgb(crate::glass::DIAG_WARNING))
                                    .child(crate::i18n::t("A entrada excede o limite de 5 MB; divida o dump em arquivos menores.")),
                            )
                        })
                        .when(!conflicts.is_empty(), |d| {
                            d.child(div().px(px(18.)).text_size(TEXT_XS)
                                .text_color(rgb(crate::glass::DIAG_WARNING))
                                .child(conflicts.clone()))
                        })
                        .when(!st.too_large && !st.warnings.is_empty(), |d| {
                            let mut list = div()
                                .id("imp-warn-scroll")
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .max_h(px(72.))
                                .overflow_y_scroll()
                                .px(px(18.))
                                .pt(px(10.))
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
                                .items_center()
                                .gap(SP_SM)
                                .px(px(18.))
                                .pt(px(14.))
                                .pb(px(16.))
                                .child(if can_apply {
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.))
                                        .text_size(px(12.5))
                                        .text_color(g.text_muted)
                                        .child(Icon::new(IconName::CircleCheck).size(px(13.)).text_color(g.ok))
                                        .child(crate::i18n::t(match (n_tables == 1, n_refs == 1) {
                                            (true, true) => "{} tabela · {} relação detectada",
                                            (true, false) => "{} tabela · {} relações detectadas",
                                            (false, true) => "{} tabelas · {} relação detectada",
                                            (false, false) => "{} tabelas · {} relações detectadas",
                                        }).replacen("{}", &n_tables.to_string(), 1).replacen("{}", &n_refs.to_string(), 1))
                                } else {
                                    div()
                                        .text_size(px(12.5))
                                        .text_color(g.text_faint)
                                        .child(crate::i18n::t("Cole um SQL para ver a prévia"))
                                })
                                .child(div().flex_1())
                                .child(dialog_btn(&g, "imp-cancel", crate::i18n::t("Cancelar"), None, false, false, true)
                                    .on_click(|_, window, cx| window.close_dialog(cx)))
                                .child(apply_btn("imp-replace", replace_label, true, false))
                                .child(apply_btn("imp-insert", crate::i18n::t("Inserir").to_string(), false, true)),
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
        let g = Glass::of_scaled(self.mode, self.translucent, self.ui_scale);
        window.open_dialog(cx, move |dialog, _, _| {
            let shell = shell.clone();
            dialog_frame(dialog, &g, 460., Some(crate::i18n::t("Alterações não salvas"))).content(move |content, _, _| {
                let shell_save = shell.clone();
                content.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(16.))
                        .child(div().text_size(TEXT_SM).child(
                            crate::i18n::t("Você tem alterações não salvas em documentos abertos. Salvar antes de sair?"),
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
                                        .label(crate::i18n::t("Cancelar"))
                                        .on_click(|_, window, cx| {
                                            window.close_dialog(cx);
                                        }),
                                )
                                .child(
                                    Button::new("dlg-discard-quit")
                                        .ghost()
                                        .small()
                                        .label(crate::i18n::t("Descartar alterações"))
                                        .on_click(|_, window, cx| {
                                            window.close_dialog(cx);
                                            cx.quit();
                                        }),
                                )
                                .child(
                                    Button::new("dlg-save-quit")
                                        .primary()
                                        .small()
                                        .label(crate::i18n::t("Salvar tudo e sair"))
                                        .on_click(move |_, window, cx| {
                                            window.close_dialog(cx);
                                            shell_save.update(cx, |s, cx| s.save_next_and_quit(window, cx));
                                        }),
                                ),
                        ),
                )
            })
        });
    }

    fn save_next_and_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.docs.iter().position(|doc| doc.dirty) else {
            cx.quit();
            return;
        };
        let doc = &self.docs[i];
        if let Some(path) = doc.path.clone() {
            if self.write(i, path, window, cx) {
                self.save_next_and_quit(window, cx);
            }
            return;
        }
        let editor = doc.editor.clone();
        let dir = self
            .project
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let rx = cx.prompt_for_new_path(&dir, Some(&doc.title));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = rx.await else { return };
            this.update_in(cx, |this, window, cx| {
                if let Some(i) = this.doc_index(&editor)
                    && this.write(i, path, window, cx)
                {
                    this.save_next_and_quit(window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn open_palette(&mut self, _: &OpenPalette, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.palette.clone();
        let shell = cx.entity();
        // tabelas e colunas do documento ativo; só entram na lista quando há busca digitada
        let tables: Vec<(String, Vec<String>)> = self.doc().map_or_else(Vec::new, |d| {
            d.diagram
                .read(cx)
                .schema()
                .tables
                .iter()
                .map(|t| {
                    let cols = t.columns.iter().map(|c| c.name.clone()).collect();
                    (t.qualified_name(), cols)
                })
                .collect()
        });
        let g = Glass::of_scaled(self.mode, self.translucent, self.ui_scale);
        window.open_dialog(cx, move |dialog, _, _| {
            let (shell, tables) = (shell.clone(), tables.clone());
            let state = state.clone();
            // sem título/botão de fechar e sem padding: a própria paleta ocupa a moldura (Esc fecha)
            dialog_bare(dialog, &g, 600.)
                .margin_top(px(64.))
                .content(move |content, _, cx| {
                    let mut groups: Vec<(&str, Vec<CommandItem>)> = Vec::new();
                    // ação de cada item, por seção: despachada no `on_confirm`, depois de fechar a
                    // paleta (o `.action` do item a dispararia antes e o `close_dialog` fecharia o
                    // diálogo recém-aberto, p.ex. Atalhos, Importar SQL e Conectar ao banco)
                    let mut acts: Vec<Vec<Box<dyn Action>>> = Vec::new();
                    for e in actions::catalog() {
                        if e.action.partial_eq(&OpenPalette) {
                            continue;
                        }
                        // linha própria: ícone da ação, nome e atalho como chip mono à direita
                        let (label, ctx, icon) =
                            (e.label, e.context, action_icon(e.action.as_ref(), e.group));
                        let act = e.action.boxed_clone();
                        let item = CommandItem::new().label(label).child(move |window, _| {
                            div()
                                .flex_1()
                                .h(px(18.))
                                .flex()
                                .items_center()
                                .justify_between()
                                .text_size(TEXT_SM)
                                .line_height(px(18.))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(10.))
                                        .child(
                                            Icon::new(icon).size(px(14.)).text_color(g.text_faint),
                                        )
                                        .child(label),
                                )
                                .children(
                                    Kbd::binding_for_action(act.as_ref(), Some(ctx), window)
                                        .map(|k| shortcut_chip(k, &g)),
                                )
                        });
                        if let Some(i) = groups.iter().position(|(g, _)| *g == e.group) {
                            groups[i].1.push(item);
                            acts[i].push(e.action);
                        } else {
                            groups.push((e.group, vec![item]));
                            acts.push(vec![e.action]);
                        }
                    }
                    // alvo (tabela) de cada item das seções de tabelas/colunas, por seção
                    let mut targets: Vec<Vec<String>> = vec![Vec::new(); groups.len()];
                    let query = state.read(cx).query(cx).trim().to_lowercase();
                    if !query.is_empty() {
                        let hit = |text: &str| text.to_lowercase().contains(&query);
                        let found_tables: Vec<&str> = tables
                            .iter()
                            .map(|(t, _)| t.as_str())
                            .filter(|t| hit(t))
                            .take(PALETTE_HITS)
                            .collect();
                        let found_cols: Vec<(&str, String)> = tables
                            .iter()
                            .flat_map(|(t, cols)| {
                                cols.iter().map(move |c| (t.as_str(), format!("{t}.{c}")))
                            })
                            .filter(|(_, label)| hit(label))
                            .take(PALETTE_HITS)
                            .collect();
                        let item = |icon: IconName, label: String| {
                            let shown = label.clone();
                            CommandItem::new().label(label).child(move |_, _| {
                                div()
                                    .flex_1()
                                    .h(px(18.))
                                    .flex()
                                    .items_center()
                                    .gap(SP_SM)
                                    .child(Icon::new(icon).size(px(14.)).text_color(g.text_faint))
                                    .child(
                                        div()
                                            .font_family(FONT_MONO)
                                            .text_size(px(12.5))
                                            .child(shown.clone()),
                                    )
                            })
                        };
                        if !found_tables.is_empty() {
                            targets.push(found_tables.iter().map(|t| (*t).to_string()).collect());
                            let items = found_tables
                                .iter()
                                .map(|t| item(IconName::Table2, (*t).to_string()));
                            groups.push(("Tabelas", items.collect()));
                        }
                        if !found_cols.is_empty() {
                            targets
                                .push(found_cols.iter().map(|(t, _)| (*t).to_string()).collect());
                            let items = found_cols
                                .iter()
                                .map(|(_, l)| item(IconName::Columns3, l.clone()));
                            groups.push(("Colunas", items.collect()));
                        }
                    }
                    let shell = shell.clone();
                    let mut cmd = Command::new(&state)
                        .placeholder(crate::i18n::t("Buscar tabelas, colunas, comandos…"))
                        .bordered(false)
                        .max_h(px(420.))
                        .footer(move |state, _, _| {
                            let n = state.matched_count();
                            div()
                                .h(px(34.))
                                .flex()
                                .items_center()
                                .gap(px(16.))
                                .px(px(14.))
                                .border_t_1()
                                .border_color(g.edge_soft)
                                .text_size(px(11.5))
                                .text_color(g.text_faint)
                                .child(crate::i18n::t("↑↓ navegar"))
                                .child(crate::i18n::t("↵ executar"))
                                .child(crate::i18n::t("Esc fechar"))
                                .child(div().flex_1())
                                .child(crate::i18n::t("{} comandos").replace("{}", &n.to_string()))
                        })
                        // raio do item = 11 - 4 (padding da lista) = 7
                        .rounded(px(11.))
                        .on_confirm(move |ix, window, cx| {
                            window.close_dialog(cx);
                            if let Some(a) = acts.get(ix.section).and_then(|v| v.get(ix.row)) {
                                window.dispatch_action(a.boxed_clone(), cx);
                            }
                            if let Some(table) = targets.get(ix.section).and_then(|v| v.get(ix.row))
                            {
                                let table = table.clone();
                                shell.update(cx, |this, cx| this.go_to_table(&table, window, cx));
                            }
                        });
                    for (g, items) in groups {
                        cmd = cmd.group(CommandGroup::new().label(crate::i18n::t(g)).items(items));
                    }
                    content.child(cmd)
                })
        });
        // o foco vai para o campo de busca (digitar filtra, setas navegam)
        let state = self.palette.clone();
        window.defer(cx, move |window, cx| {
            state.update(cx, |s, cx| s.focus(window, cx));
        });
    }

    /// Seleciona a tabela no canvas e, com o editor aberto, rola o editor até ela.
    fn go_to_table(&mut self, table: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.with_diagram(cx, |d| d.focus_table(table));
        if !self.editor_open {
            return;
        }
        let Some(doc) = self.doc() else { return };
        let text = doc.editor.read(cx).value().to_string();
        if let Some(block) = crate::ide::table_block(&text, table) {
            self.reveal(block.start, window, cx);
        }
    }

    fn toggle_focus_mode(&mut self, _: &ToggleFocusMode, _: &mut Window, cx: &mut Context<Self>) {
        self.with_diagram(cx, Diagram::toggle_focus_mode);
        cx.notify();
    }

    fn cycle_focus_depth(&mut self, _: &CycleFocusDepth, _: &mut Window, cx: &mut Context<Self>) {
        self.with_diagram(cx, Diagram::cycle_focus_depth);
        cx.notify();
    }

    /// `true` com o modo Comparar ativo na aba atual (canvas somente leitura).
    fn diff_active(&self, cx: &App) -> bool {
        self.doc().is_some_and(|d| d.diagram.read(cx).diff_active())
    }

    /// Entra/sai do modo Comparar (#18): base = arquivo no `HEAD` do git; head = buffer atual.
    fn toggle_diff_mode(&mut self, _: &ToggleDiffMode, _: &mut Window, cx: &mut Context<Self>) {
        if self.diff_active(cx) {
            self.with_diagram(cx, Diagram::exit_diff);
            cx.notify();
            return;
        }
        let Some(doc) = self.doc() else { return };
        let base = crate::git_base::head_source(doc.path.as_deref())
            .map(|src| dbml_analysis::analyze(&src))
            .map_err(crate::git_base::HeadError::message);
        self.with_diagram(cx, |d| d.enter_diff(base));
        cx.notify();
    }

    fn next_diff_change(&mut self, _: &NextDiffChange, _: &mut Window, cx: &mut Context<Self>) {
        self.with_diagram(cx, |d| d.step_diff(true));
    }

    fn prev_diff_change(&mut self, _: &PrevDiffChange, _: &mut Window, cx: &mut Context<Self>) {
        self.with_diagram(cx, |d| d.step_diff(false));
    }

    fn open_quick_search(&mut self, _: &QuickSearch, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc() else { return };
        let schema = doc.diagram.read(cx).schema().clone();
        let state = self.quick_search.clone();
        let shell = cx.entity();
        let g = Glass::of_scaled(self.mode, self.translucent, self.ui_scale);
        window.open_dialog(cx, move |dialog, _, _| {
            let (shell, schema) = (shell.clone(), schema.clone());
            let state = state.clone();
            dialog_bare(dialog, &g, 560.)
                .margin_top(px(64.))
                .content(move |content, _, cx| {
                    let query = state.read(cx).query(cx);
                    let hits = dbml_analysis::search_schema(&schema, &query);
                    let mut items = Vec::with_capacity(hits.len().min(60));
                    let mut targets: Vec<dbml_analysis::SearchKind> = Vec::new();
                    for hit in hits.into_iter().take(60) {
                        let (badge_txt, icon) = match &hit.kind {
                            dbml_analysis::SearchKind::Table { .. } => ("TABLE", IconName::Table2),
                            dbml_analysis::SearchKind::Column { .. } => ("COL", IconName::Columns3),
                            dbml_analysis::SearchKind::Enum { .. } => ("ENUM", IconName::List),
                        };
                        let label = hit.label.clone();
                        let detail = hit.detail.clone();
                        targets.push(hit.kind);
                        items.push(CommandItem::new().label(format!("{label} {detail}")).child(
                            move |_, _| {
                                div()
                                    .flex_1()
                                    .h(px(20.))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(SP_SM)
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(SP_SM)
                                            .min_w_0()
                                            .child(
                                                Icon::new(icon)
                                                    .size(px(14.))
                                                    .text_color(g.text_faint),
                                            )
                                            .child(
                                                div()
                                                    .font_family(FONT_MONO)
                                                    .text_size(px(12.5))
                                                    .truncate()
                                                    .child(label.clone()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.))
                                            .flex_none()
                                            .child(
                                                div()
                                                    .font_family(FONT_MONO)
                                                    .text_size(px(11.))
                                                    .text_color(g.text_faint)
                                                    .child(detail.clone()),
                                            )
                                            .child(
                                                div()
                                                    .px(px(5.))
                                                    .py(px(1.))
                                                    .rounded(px(4.))
                                                    .bg(g.selected)
                                                    .font_family(FONT_MONO)
                                                    .text_size(px(9.5))
                                                    .text_color(g.text_muted)
                                                    .child(badge_txt),
                                            ),
                                    )
                            },
                        ));
                    }
                    let shell = shell.clone();
                    let count = targets.len();
                    let cmd = Command::new(&state)
                        .placeholder(crate::i18n::t("Buscar tabelas, colunas, tipos, enums…"))
                        .bordered(false)
                        .max_h(px(400.))
                        .group(
                            CommandGroup::new()
                                .label(crate::i18n::t("Resultados"))
                                .items(items),
                        )
                        .footer(move |_, _, _| {
                            div()
                                .h(px(34.))
                                .flex()
                                .items_center()
                                .gap(px(16.))
                                .px(px(14.))
                                .border_t_1()
                                .border_color(g.edge_soft)
                                .text_size(px(11.5))
                                .text_color(g.text_faint)
                                .child(crate::i18n::t("↑↓ navegar"))
                                .child(crate::i18n::t("↵ ir para"))
                                .child(crate::i18n::t("Esc fechar"))
                                .child(div().flex_1())
                                .child(format!("{count}"))
                        })
                        .rounded(px(11.))
                        .on_confirm(move |ix, window, cx| {
                            window.close_dialog(cx);
                            if let Some(kind) = targets.get(ix.row).cloned() {
                                shell.update(cx, |this, cx| match kind {
                                    dbml_analysis::SearchKind::Table { table, .. } => {
                                        this.go_to_table(&table, window, cx);
                                    }
                                    dbml_analysis::SearchKind::Column { table, column, .. } => {
                                        this.with_diagram(cx, |d| {
                                            d.focus_column(&table, &column);
                                        });
                                        this.open_col = Some((table.clone(), Some(column)));
                                        if this.editor_open
                                            && let Some(doc) = this.doc()
                                        {
                                            let text = doc.editor.read(cx).value().to_string();
                                            if let Some(block) =
                                                crate::ide::table_block(&text, &table)
                                            {
                                                this.reveal(block.start, window, cx);
                                            }
                                        }
                                        cx.notify();
                                    }
                                    dbml_analysis::SearchKind::Enum { name, .. } => {
                                        this.with_diagram(cx, |d| d.focus_enum(&name));
                                        cx.notify();
                                    }
                                });
                            }
                        });
                    content.child(cmd)
                })
        });
        let state = self.quick_search.clone();
        window.defer(cx, move |window, cx| {
            state.update(cx, |s, cx| s.focus(window, cx));
        });
    }

    fn show_shortcuts(&mut self, _: &ShowShortcuts, window: &mut Window, cx: &mut Context<Self>) {
        let g = Glass::of_scaled(self.mode, self.translucent, self.ui_scale);
        let filter =
            cx.new(|cx| InputState::new(window, cx).placeholder(crate::i18n::t("Filtrar atalhos")));
        window.open_dialog(cx, move |dialog, _, _| {
            let filter = filter.clone();
            dialog_bare(dialog, &g, 800.).content(move |content, window, cx| {
                let query = filter.read(cx).value().trim().to_lowercase();
                // grupos na ordem do catálogo; só ações com atalho, filtradas pela busca
                let mut groups: Vec<(&str, Vec<(&str, Kbd)>)> = Vec::new();
                for e in actions::catalog() {
                    if e.binding.is_none() || !e.label.to_lowercase().contains(&query) {
                        continue;
                    }
                    let Some(k) =
                        Kbd::binding_for_action(e.action.as_ref(), Some(e.context), window)
                    else {
                        continue;
                    };
                    match groups.iter_mut().find(|(n, _)| *n == e.group) {
                        Some((_, v)) => v.push((e.label, k)),
                        None => groups.push((e.group, vec![(e.label, k)])),
                    }
                }
                // duas colunas: cada grupo vai para a coluna com menos linhas
                let col = || div().flex_1().flex().flex_col();
                let (mut left, mut right, mut n_left, mut n_right) = (col(), col(), 0, 0);
                for (name, items) in groups {
                    let n = items.len() + 2;
                    let mut d = div().flex().flex_col().child(
                        div()
                            .pt(px(14.))
                            .pb(px(6.))
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(g.text_faint)
                            .child(crate::i18n::t(name).to_uppercase()),
                    );
                    for (label, k) in items {
                        d = d.child(
                            div()
                                .h(px(28.))
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .border_b_1()
                                .border_color(g.edge_soft)
                                .text_size(TEXT_SM)
                                .child(div().flex_1().child(label))
                                .child(shortcut_chip(k, &g)),
                        );
                    }
                    if n_left <= n_right {
                        (left, n_left) = (left.child(d), n_left + n);
                    } else {
                        (right, n_right) = (right.child(d), n_right + n);
                    }
                }
                content
                    .child(
                        div()
                            .h(px(52.))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .pl(px(18.))
                            .pr(px(12.))
                            .border_b_1()
                            .border_color(g.edge_soft)
                            .child(
                                Icon::new(IconName::Keyboard)
                                    .size(px(16.))
                                    .text_color(g.accent),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(crate::i18n::t("Atalhos de teclado")),
                            )
                            .child(
                                div()
                                    .w(px(200.))
                                    .h(px(28.))
                                    .px(px(8.))
                                    .rounded(px(7.))
                                    .bg(g.input)
                                    .border_1()
                                    .border_color(g.edge_soft)
                                    .flex()
                                    .items_center()
                                    .child(
                                        Input::new(&filter).small().appearance(false).prefix(
                                            Icon::new(IconName::Search)
                                                .size(px(12.))
                                                .text_color(g.text_faint),
                                        ),
                                    ),
                            )
                            .child(dialog_close(&g)),
                    )
                    .child(
                        div()
                            .id("shortcuts-list")
                            .max_h(px(560.))
                            .overflow_y_scroll()
                            .flex()
                            .gap(px(28.))
                            .px(px(18.))
                            .pb(px(18.))
                            .child(left)
                            .child(right),
                    )
            })
        });
    }

    fn set_theme(
        &mut self,
        mode: Mode,
        translucent: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mode = mode;
        self.translucent = translucent;
        window.set_background_appearance(crate::platform::window_background(translucent));
        self.apply_appearance(cx);
    }

    fn toggle_theme(&mut self, _: &ToggleTheme, window: &mut Window, cx: &mut Context<Self>) {
        self.set_theme(self.mode.toggled(), self.translucent, window, cx);
    }

    fn theme_light(&mut self, _: &ThemeLight, window: &mut Window, cx: &mut Context<Self>) {
        self.set_theme(Mode::Light, self.translucent, window, cx);
    }

    fn theme_dark(&mut self, _: &ThemeDark, window: &mut Window, cx: &mut Context<Self>) {
        self.set_theme(Mode::Dark, self.translucent, window, cx);
    }

    fn lang_portuguese(&mut self, _: &LangPortuguese, _: &mut Window, cx: &mut Context<Self>) {
        self.lang_pref = Some(crate::i18n::Lang::PtBr);
        crate::i18n::set(crate::i18n::Lang::PtBr);
        for doc in &self.docs {
            doc.diagram.update(cx, |_, cx| cx.notify());
        }
        self.apply_appearance(cx);
    }

    fn lang_english(&mut self, _: &LangEnglish, _: &mut Window, cx: &mut Context<Self>) {
        self.lang_pref = Some(crate::i18n::Lang::En);
        crate::i18n::set(crate::i18n::Lang::En);
        for doc in &self.docs {
            doc.diagram.update(cx, |_, cx| cx.notify());
        }
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
        // modo Comparar é somente leitura: nenhuma ação da UI edita o DBML
        if self.diff_active(cx) {
            return;
        }
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
        self.schedule_autosave(self.active, window, cx);
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
        let edges_changed = *diagram.read(cx).edges() != snap.edges;
        diagram.update(cx, |d, cx| {
            d.apply_snapshot(
                schema,
                std::sync::Arc::unwrap_or_clone(snap.pos),
                snap.selected,
                snap.selected_enum,
                snap.multi,
            );
            d.set_edges(snap.edges);
            cx.notify();
        });
        if edges_changed && let Some(doc) = self.docs.get(self.active) {
            Self::save_edges(doc, cx);
        }
        self.schedule_autosave(self.active, window, cx);
        self.publish_recovery(cx);
        cx.notify();
    }

    fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        if self.diff_active(cx) {
            return;
        }
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
        if self.diff_active(cx) {
            return;
        }
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
                    window.push_notification(
                        crate::i18n::t("Nome de tabela inválido: use letras, números e _."),
                        cx,
                    );
                }
            }
            EditTarget::TableNote(table) => {
                let opt = (!value.is_empty()).then_some(value.as_str());
                self.edit_text(false, window, cx, |t| {
                    patched(t, dbml_edit::set_table_note(t, &table, opt))
                });
            }
            EditTarget::ColName(table, old) if value != old => {
                if ident {
                    self.edit_text(false, window, cx, |t| {
                        dbml_edit::rename_column(t, &table, &old, &value)
                    });
                } else {
                    window.push_notification(
                        crate::i18n::t("Nome de coluna inválido: use letras, números e _."),
                        cx,
                    );
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
            EditTarget::ColNote(table, col) => {
                let opt = (!value.is_empty()).then_some(value.as_str());
                self.edit_text(false, window, cx, |t| {
                    dbml_edit::set_column_note(t, &table, &col, opt)
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
                    window.push_notification(
                        crate::i18n::t("Nome de enum inválido: use letras, números e _."),
                        cx,
                    );
                }
            }
            EditTarget::EnumVal(en, old) if value != old => {
                if ident {
                    self.edit_text(false, window, cx, |t| {
                        dbml_edit::rename_enum_value(t, &en, &old, &value)
                    });
                } else {
                    window.push_notification(
                        crate::i18n::t("Valor de enum inválido: use letras, números e _."),
                        cx,
                    );
                }
            }
            EditTarget::NewEnumVal(en) if !value.is_empty() => {
                if ident {
                    self.edit_text(false, window, cx, |t| {
                        dbml_edit::add_enum_value(t, &en, &value)
                    });
                } else {
                    window.push_notification(
                        crate::i18n::t("Valor de enum inválido: use letras, números e _."),
                        cx,
                    );
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
        self.open_col = Some((table.clone(), Some(name.clone())));
        self.start_edit(EditTarget::ColName(table, name.clone()), name, window, cx);
    }

    fn new_table(&mut self, _: &NewTable, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc() else { return };
        let text = doc.editor.read(cx).value().to_string();
        let name = dbml_edit::unique_table_name(&text, NEW_TABLE_NAME);
        self.edit_text(false, window, cx, |t| Some(dbml_edit::add_table(t, &name)));
        self.with_diagram(cx, |d| d.place_new(&name));
        self.inspector_open = true;
        self.open_col = Some((name.clone(), None));
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
            window.push_notification(
                crate::i18n::t("Selecione tabelas (Shift+clique) para agrupar."),
                cx,
            );
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

    /// Move a coluna uma posição (`delta` -1/+1) dentro da tabela, com undo.
    fn move_column(
        &mut self,
        table: &str,
        col: &str,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_text(false, window, cx, |t| {
            patched(t, dbml_edit::move_column(t, table, col, delta))
        });
    }

    fn duplicate_table(&mut self, _: &DuplicateTable, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc() else { return };
        let Some(src) = doc
            .diagram
            .read(cx)
            .selected()
            .map(schema::Table::qualified_name)
        else {
            return;
        };
        let mut copy = None;
        self.edit_text(false, window, cx, |t| {
            let (patch, name) = dbml_edit::duplicate_table(t, &src).ok()?;
            copy = Some(name);
            Some(patch.apply(t))
        });
        let Some(copy) = copy else { return };
        // a cópia nasce deslocada da original; apply_snapshot posiciona e seleciona
        self.with_diagram(cx, |d| {
            let mut pos = d.positions().clone();
            if let Some(&(x, y)) = pos.get(&src) {
                pos.insert(copy.clone(), (x + 32., y + 32.));
            }
            let schema = d.schema().clone();
            d.apply_snapshot(schema, pos, Some(copy), None, Vec::new());
        });
    }

    fn reveal_in_dbml(&mut self, _: &RevealInDbml, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc() else { return };
        let Some(name) = doc
            .diagram
            .read(cx)
            .selected()
            .map(schema::Table::qualified_name)
        else {
            return;
        };
        let text = doc.editor.read(cx).value().to_string();
        let Some(range) = dbml_edit::table_range(&text, &name) else {
            return;
        };
        if !self.editor_open {
            self.code_only = false; // abre em Split, não em Code
        }
        self.reveal(range.start, window, cx);
    }

    fn edit_table_note(&mut self, _: &EditTableNote, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc() else { return };
        let Some((qname, note)) = doc
            .diagram
            .read(cx)
            .selected()
            .map(|t| (t.qualified_name(), t.note.clone().unwrap_or_default()))
        else {
            return;
        };
        self.inspector_open = true;
        self.start_edit(EditTarget::TableNote(qname), note, window, cx);
    }

    fn assign_group(
        &mut self,
        table: &str,
        group: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_text(true, window, cx, |t| {
            Some(dbml_edit::group_tables(t, &[table], group))
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
            lang: self.lang_pref,
            translucent: Some(self.translucent),
            sidebar_w: self.widths.0,
            editor_w: self.widths.1,
            inspector_w: self.widths.2,
            col_type_w: self.col_type_w,
            ui_scale: self.ui_scale,
            inspector_auto_hide: self.inspector_auto_hide,
            sidebar_open: self.sidebar_open,
            inspector_open: self.inspector_open,
            onboarding_done: self.onboarding_done,
            last_seen_version: self.last_seen_version.clone(),
        });
    }

    fn apply_appearance(&mut self, cx: &mut Context<Self>) {
        apply_theme_scaled(self.mode, self.translucent, self.ui_scale, cx);
        self.save_prefs();
        self.save_session(cx);
        cx.notify();
    }

    fn set_ui_scale(&mut self, scale: f32, window: &mut Window, cx: &mut Context<Self>) {
        self.ui_scale = (scale * 100.0).round() / 100.0;
        self.apply_appearance(cx);
        window.push_notification(
            crate::i18n::t("Escala da interface: {}%")
                .replace("{}", &((self.ui_scale * 100.0).round() as i32).to_string()),
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
            crate::i18n::t("Inspetor: recolher automaticamente sem seleção (ativado)")
        } else {
            crate::i18n::t("Inspetor: sempre visível (desativado)")
        };
        window.push_notification(msg, cx);
        cx.notify();
    }

    fn reset_panels(&mut self, _: &ResetPanels, _: &mut Window, cx: &mut Context<Self>) {
        let d = crate::settings::Settings::default();
        self.widths = (d.sidebar_w, d.editor_w, d.inspector_w);
        self.col_type_w = d.col_type_w;
        self.save_prefs();
        self.save_session(cx);
        cx.notify();
    }

    fn toggle_editor(&mut self, _: &ToggleEditor, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_left_pane(None, cx);
    }

    fn toggle_sidebar(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar_open || self.editor_open {
            self.sidebar_open = false;
            self.editor_open = false;
            self.code_only = false;
        } else {
            self.sidebar_open = true;
            self.editor_open = false;
            self.code_only = false;
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

    fn toggle_minimap(&mut self, _: &ToggleMinimap, _: &mut Window, cx: &mut Context<Self>) {
        self.with_diagram(cx, |d| d.minimap_open = !d.minimap_open);
    }

    fn fit(&mut self, _: &FitView, _: &mut Window, cx: &mut Context<Self>) {
        self.with_diagram(cx, super::canvas::Diagram::fit);
    }

    fn relayout(&mut self, _: &Relayout, _: &mut Window, cx: &mut Context<Self>) {
        if self.diff_active(cx) {
            return;
        }
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

    fn reset_zoom(&mut self, _: &ResetZoom, _: &mut Window, cx: &mut Context<Self>) {
        self.with_diagram(cx, Diagram::reset_zoom);
    }

    fn tool_select(&mut self, _: &ToolSelect, _: &mut Window, cx: &mut Context<Self>) {
        self.with_diagram(cx, |d| d.tool = Tool::Select);
    }

    fn tool_hand(&mut self, _: &ToolHand, _: &mut Window, cx: &mut Context<Self>) {
        self.with_diagram(cx, |d| d.tool = Tool::Hand);
    }

    fn deselect(&mut self, _: &Deselect, _: &mut Window, cx: &mut Context<Self>) {
        // Esc no modo Comparar sai do modo (restaura vista e seleção)
        if self.diff_active(cx) {
            self.with_diagram(cx, Diagram::exit_diff);
            cx.notify();
            return;
        }
        self.with_diagram(cx, super::canvas::Diagram::deselect);
    }

    fn toggle_edge_straight(
        &mut self,
        _: &ToggleEdgeStraight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(doc) = self.doc() {
            doc.diagram.update(cx, |d, cx| d.edit_edge(false, cx));
        }
    }

    fn reset_edge_curve(&mut self, _: &ResetEdgeCurve, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(doc) = self.doc() {
            doc.diagram.update(cx, |d, cx| d.edit_edge(true, cx));
        }
    }
}

fn connection_hint(message: &str, sqlite: bool) -> &'static str {
    let empty = "Conectado, mas nenhuma tabela foi encontrada. Verifique o banco e o schema.";
    if message == crate::i18n::t(empty) {
        return empty;
    }
    let message = message.to_ascii_lowercase();
    if [
        "authenticat",
        "login failed",
        "access denied for user",
        "password",
        "28p01",
        "18456",
    ]
    .iter()
    .any(|reason| message.contains(reason))
    {
        "O banco recusou a autenticação. Confira o usuário e a senha."
    } else if [
        "timeout",
        "timed out",
        "time out",
        "connection refused",
        "could not resolve",
    ]
    .iter()
    .any(|reason| message.contains(reason))
    {
        "O banco não respondeu. Confira o host, a porta e se o serviço está rodando."
    } else if message.contains("could not read the catalog") {
        "Não foi possível ler o esquema. Verifique as permissões de leitura do catálogo."
    } else if sqlite {
        "Não foi possível abrir o banco SQLite. Confira o caminho e as permissões do arquivo."
    } else {
        "Não foi possível conectar. Confira os dados da conexão e tente novamente."
    }
}

fn file_in_use(error: &std::io::Error) -> bool {
    cfg!(windows) && matches!(error.raw_os_error(), Some(32 | 1224))
}

fn export_write_hint(error: &std::io::Error) -> &'static str {
    if file_in_use(error) {
        return "O arquivo está em uso. Feche-o no outro aplicativo ou exporte com outro nome.";
    }
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => {
            "Sem permissão para gravar neste local. Escolha outra pasta."
        }
        std::io::ErrorKind::StorageFull => {
            "O disco está cheio. Libere espaço ou escolha outra pasta."
        }
        _ => "Não foi possível gravar o arquivo. Tente outro nome ou outra pasta.",
    }
}

fn embed_svg_fonts(mut svg: String) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use std::fmt::Write as _;

    let mut style = String::from("<style>");
    for ((family, weight), font) in [
        ("Geist", 400),
        ("Geist", 500),
        ("Geist", 600),
        ("Geist Mono", 400),
        ("Geist Mono", 500),
    ]
    .into_iter()
    .zip(crate::FONTS)
    {
        let data = STANDARD.encode(font);
        let _ = write!(
            style,
            "@font-face{{font-family:'{family}';font-weight:{weight};src:url(data:font/ttf;base64,{data}) format('truetype')}}"
        );
    }
    style.push_str("</style>");
    if let Some((root, _)) = svg.split_once('>') {
        svg.insert_str(root.len() + 1, &style);
    }
    svg
}

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

/// Texto resultante de um patch do `dbml-edit`; erro vira `None` (no-op = texto igual).
fn patched(text: &str, patch: Result<dbml_edit::Patch, dbml_edit::EditError>) -> Option<String> {
    patch.ok().map(|p| p.apply(text))
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
        ExportDocsHtml => export_docs_html,
        ExportDocsMarkdown => export_docs_markdown,
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
        QuickSearch => open_quick_search,
        ToggleFocusMode => toggle_focus_mode,
        CycleFocusDepth => cycle_focus_depth,
        ToggleDiffMode => toggle_diff_mode,
        NextDiffChange => next_diff_change,
        PrevDiffChange => prev_diff_change,
        ShowShortcuts => show_shortcuts,
        ShowTour => show_tour,
        ShowWhatsNew => show_whats_new,
        ToggleTheme => toggle_theme,
        ThemeLight => theme_light,
        ThemeDark => theme_dark,
        LangPortuguese => lang_portuguese,
        LangEnglish => lang_english,
        ToggleEditor => toggle_editor,
        ShowDiagramView => show_diagram_view,
        ShowSplitView => show_split_view,
        ShowCodeView => show_code_view,
        FilterExplorer => filter_explorer,
        ToggleSidebar => toggle_sidebar,
        ToggleInspector => toggle_inspector,
        ToggleLegend => toggle_legend,
        ToggleMinimap => toggle_minimap,
        FitView => fit,
        Relayout => relayout,
        ZoomIn => zoom_in,
        ZoomOut => zoom_out,
        ResetZoom => reset_zoom,
        ToolSelect => tool_select,
        ToolHand => tool_hand,
        Deselect => deselect,
        ToggleEdgeStraight => toggle_edge_straight,
        ResetEdgeCurve => reset_edge_curve,
        GroupSelection => group_selection,
        NewTable => new_table,
        NewEnum => new_enum,
        DuplicateTable => duplicate_table,
        RevealInDbml => reveal_in_dbml,
        EditTableNote => edit_table_note,
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

#[derive(Clone, Copy)]
enum ResizeEdge {
    Sidebar,
    Editor,
    Inspector,
}

#[derive(Clone)]
struct ResizeDrag;
impl Render for ResizeDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut g = Glass::of_scaled(self.mode, self.translucent, self.ui_scale);
        g.editor_open = self.editor_open;
        // responsivo: painéis colapsam apenas em larguras extremamente reduzidas
        let width = f32::from(window.viewport_size().width);
        let has_selection = self.doc().is_some_and(|d| {
            let diag = d.diagram.read(cx);
            diag.selected().is_some()
                || !diag.selection().is_empty()
                || diag.selected_enum().is_some()
                || diag.selected_ref().is_some()
        });
        let show_sidebar = self.sidebar_open && !self.editor_open && width >= 500.;
        let show_editor = self.editor_open && width >= 500.;
        let show_inspector = self.inspector_open
            && (!self.inspector_auto_hide || has_selection)
            && !self.docs.is_empty()
            && width >= 600.;
        // "Code": só o editor; canvas e inspector nem são montados
        let code_view = self.view() == View::Code && show_editor;
        let show_inspector = show_inspector && !code_view;
        let panel_duration = if cx.has_active_drag() {
            std::time::Duration::ZERO
        } else {
            motion::SLOW
        };
        let sidebar_progress = motion::value(
            "sidebar-motion",
            f32::from(show_sidebar),
            panel_duration,
            window,
            cx,
        );
        let editor_progress = motion::value(
            "editor-motion",
            f32::from(show_editor),
            panel_duration,
            window,
            cx,
        );
        let inspector_progress = motion::value(
            "inspector-motion",
            f32::from(show_inspector),
            panel_duration,
            window,
            cx,
        );
        let code_progress = motion::value(
            "code-view-motion",
            f32::from(code_view),
            panel_duration,
            window,
            cx,
        );
        let editor_width =
            self.widths.1 + ((width - f32::from(RAIL_W)).max(0.) - self.widths.1) * code_progress;
        self.sync_table_highlight(cx);
        let titlebar = self.titlebar(
            &g,
            window.is_maximized(),
            width < 1000.,
            (show_inspector, has_selection),
            cx,
        );
        let rail = self.rail(&g, cx);
        // o canvas primeiro: ele aplica o ajuste à tela que a barra de status exibe
        let canvas = self.doc().map(|d| d.diagram.clone()).inspect(|e| {
            e.update(cx, |d, cx| {
                if d.glass.map(|x| (x.mode, x.translucent, x.editor_open))
                    != Some((g.mode, g.translucent, g.editor_open))
                {
                    d.glass = Some(g);
                    cx.notify();
                }
            });
        });
        let canvas_id = canvas.as_ref().map(Entity::entity_id);
        if self.last_canvas != canvas_id {
            self.last_canvas = canvas_id;
            self.canvas_epoch = self.canvas_epoch.wrapping_add(1);
        }
        let sidebar = self.sidebar(&g, window, cx);
        let inspector = self.inspector(&g, window, cx);
        let editor = self.editor_panel(&g, cx);
        let origin = self
            .doc()
            .and_then(|d| d.path.clone())
            .or_else(|| self.project.clone());
        if self.git_branch.0 != origin {
            let branch = origin.as_deref().and_then(git_branch);
            self.git_branch = (origin, branch);
        }
        let statusbar = self.statusbar(&g, width < 1000., cx);
        let handle = |id: &'static str, edge: HandleEdge, target: ResizeEdge| {
            resize_handle(id, Axis::Horizontal)
                .inside(edge)
                .on_drag(target, |_, _, _, cx| {
                    cx.stop_propagation();
                    cx.new(|_| ResizeDrag)
                })
        };
        let work = div()
            .id("work")
            .size_full()
            .flex()
            .on_drag_move(
                cx.listener(move |this, e: &DragMoveEvent<ResizeEdge>, _, cx| {
                    let x = f32::from(e.event.position.x);
                    let left = f32::from(e.bounds.left());
                    let right = f32::from(e.bounds.right());
                    match *e.drag(cx) {
                        ResizeEdge::Sidebar => {
                            let w = (x - left).clamp(180., 440.);
                            if (this.widths.0 - w).abs() >= 0.5 {
                                this.widths.0 = w;
                                this.save_prefs();
                                cx.notify();
                            }
                        }
                        ResizeEdge::Editor => {
                            let start = left + if show_sidebar { this.widths.0 } else { 0. };
                            let w = (x - start).clamp(280., 900.);
                            if (this.widths.1 - w).abs() >= 0.5 {
                                this.widths.1 = w;
                                this.save_prefs();
                                cx.notify();
                            }
                        }
                        ResizeEdge::Inspector => {
                            let w = (right - x).clamp(240., 800.);
                            if (this.widths.2 - w).abs() >= 0.5 {
                                this.widths.2 = w;
                                this.save_prefs();
                                cx.notify();
                            }
                        }
                    }
                }),
            )
            .when(sidebar_progress > 0., |d| {
                d.child(
                    div()
                        .relative()
                        .flex_none()
                        .w(px(self.widths.0 * sidebar_progress))
                        .h_full()
                        .overflow_hidden()
                        .opacity(sidebar_progress)
                        .child(div().w(px(self.widths.0)).h_full().child(sidebar))
                        .child(guide_ui::track_tour_anchor(
                            crate::guide::TourTarget::Sidebar,
                            self.tour_bounds.clone(),
                        ))
                        .child(handle(
                            "resize-sidebar",
                            HandleEdge::Trailing,
                            ResizeEdge::Sidebar,
                        )),
                )
            })
            .when(editor_progress > 0., |d| {
                d.child(
                    div()
                        .relative()
                        .flex_none()
                        .w(px(editor_width * editor_progress))
                        .h_full()
                        .overflow_hidden()
                        .opacity(editor_progress)
                        .child(div().w(px(editor_width)).h_full().child(editor))
                        .when(!code_view, |d| {
                            d.child(handle(
                                "resize-editor",
                                HandleEdge::Trailing,
                                ResizeEdge::Editor,
                            ))
                        }),
                )
            })
            .when(code_progress < 1. || !code_view, |d| {
                d.child(match canvas {
                    Some(canvas) => div()
                        .id("canvas")
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .bg(g.surface)
                        .child(canvas)
                        .with_animation(
                            ("canvas-file", self.canvas_epoch),
                            motion::animation(motion::BASE),
                            Styled::opacity,
                        )
                        .into_any_element(),
                    None => div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .child(self.empty_state(&g, cx))
                        .into_any_element(),
                })
            })
            .when(inspector_progress > 0., |d| {
                d.child(
                    div()
                        .relative()
                        .flex_none()
                        .w(px(self.widths.2 * inspector_progress))
                        .h_full()
                        .overflow_hidden()
                        .opacity(inspector_progress)
                        .child(div().w(px(self.widths.2)).h_full().child(inspector))
                        .child(guide_ui::track_tour_anchor(
                            crate::guide::TourTarget::Inspector,
                            self.tour_bounds.clone(),
                        ))
                        .child(handle(
                            "resize-inspector",
                            HandleEdge::Leading,
                            ResizeEdge::Inspector,
                        )),
                )
            })
            .into_any_element();
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
            .on_action(cx.listener(Self::open_quick_search))
            .on_action(cx.listener(Self::toggle_diff_mode))
            .on_action(cx.listener(Self::next_diff_change))
            .on_action(cx.listener(Self::prev_diff_change))
            .on_action(cx.listener(Self::toggle_focus_mode))
            .on_action(cx.listener(Self::cycle_focus_depth))
            .on_action(cx.listener(Self::import_sql_ddl))
            .on_action(cx.listener(Self::export_sql_postgres))
            .on_action(cx.listener(Self::export_sql_mysql))
            .on_action(cx.listener(Self::export_sql_sqlite))
            .on_action(cx.listener(Self::export_sql_sqlserver))
            .on_action(cx.listener(Self::export_sql_oracle))
            .on_action(cx.listener(Self::export_mermaid))
            .on_action(cx.listener(Self::export_docs_html))
            .on_action(cx.listener(Self::export_docs_markdown))
            .on_action(cx.listener(Self::export_png))
            .on_action(cx.listener(Self::export_jpeg))
            .on_action(cx.listener(Self::export_svg))
            .on_action(cx.listener(Self::connect_database))
            .on_action(cx.listener(Self::add_preset_uuid_pk))
            .on_action(cx.listener(Self::add_preset_bigint_pk))
            .on_action(cx.listener(Self::add_preset_timestamps))
            .on_action(cx.listener(Self::add_preset_soft_delete))
            .on_action(cx.listener(Self::show_shortcuts))
            .on_action(cx.listener(Self::show_tour))
            .on_action(cx.listener(Self::show_whats_new))
            .on_action(cx.listener(Self::toggle_theme))
            .on_action(cx.listener(Self::theme_light))
            .on_action(cx.listener(Self::theme_dark))
            .on_action(cx.listener(Self::lang_portuguese))
            .on_action(cx.listener(Self::lang_english))
            .on_action(cx.listener(Self::toggle_editor))
            .on_action(cx.listener(Self::show_diagram_view))
            .on_action(cx.listener(Self::show_split_view))
            .on_action(cx.listener(Self::show_code_view))
            .on_action(cx.listener(Self::filter_explorer))
            .on_action(cx.listener(Self::toggle_sidebar))
            .on_action(cx.listener(Self::toggle_inspector))
            .on_action(cx.listener(Self::toggle_legend))
            .on_action(cx.listener(Self::toggle_minimap))
            .on_action(cx.listener(Self::fit))
            .on_action(cx.listener(Self::relayout))
            .on_action(cx.listener(Self::zoom_in))
            .on_action(cx.listener(Self::zoom_out))
            .on_action(cx.listener(Self::reset_zoom))
            .on_action(cx.listener(Self::tool_select))
            .on_action(cx.listener(Self::tool_hand))
            .on_action(cx.listener(Self::deselect))
            .on_action(cx.listener(Self::toggle_edge_straight))
            .on_action(cx.listener(Self::reset_edge_curve))
            .on_action(cx.listener(Self::new_table))
            .on_action(cx.listener(Self::new_enum))
            .on_action(cx.listener(Self::duplicate_table))
            .on_action(cx.listener(Self::reveal_in_dbml))
            .on_action(cx.listener(Self::edit_table_note))
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
                    .child(div().flex_1().min_w_0().h_full().child(work)),
            )
            .child(statusbar)
            .children(self.render_tour_overlay(&g, window, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::{Shell, git_branch, patched};

    #[test]
    fn export_write_errors_provide_a_recovery_action() {
        use std::io::{Error, ErrorKind};

        assert!(
            super::export_write_hint(&Error::from(ErrorKind::PermissionDenied))
                .contains("Escolha outra pasta")
        );
        #[cfg(windows)]
        assert!(
            super::export_write_hint(&Error::from_raw_os_error(1224))
                .contains("exporte com outro nome")
        );
    }

    #[test]
    fn svg_embeds_geist_faces_and_keeps_text_editable() {
        let svg = super::embed_svg_fonts("<svg><text>orders</text></svg>".into());
        assert!(svg.starts_with("<svg><style>"));
        assert!(svg.contains("font-family:'Geist Mono';font-weight:500"));
        assert_eq!(
            svg.matches("data:font/ttf;base64,").count(),
            crate::FONTS.len()
        );
        assert!(svg.ends_with("</style><text>orders</text></svg>"));
    }

    use std::ffi::OsStr;
    use std::path::PathBuf;

    #[test]
    fn new_table_name_is_valid_and_unique() {
        let name = dbml_edit::unique_table_name("", super::NEW_TABLE_NAME);
        let text = dbml_edit::add_table("", &name);
        assert_eq!(name, "new_table");
        assert!(dbml_analysis::analyze(&text).table(&name).is_some());
        assert_ne!(
            dbml_edit::unique_table_name(&text, super::NEW_TABLE_NAME),
            name
        );
    }

    #[test]
    fn git_branch_reads_head_walking_up() {
        let root = std::env::temp_dir().join(format!("dbm-git-{}", std::process::id()));
        let sub = root.join("a/b");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join(".git/HEAD"), "ref: refs/heads/feat/x\n").unwrap();
        assert_eq!(git_branch(&sub.join("x.dbml")).as_deref(), Some("feat/x"));
        std::fs::write(root.join(".git/HEAD"), "0123456789abcdef\n").unwrap();
        assert_eq!(git_branch(&sub).as_deref(), Some("0123456"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn patched_applies_note_and_move() {
        let t = "Table a {\n  x int\n  y int\n}\n";
        let n = patched(t, dbml_edit::set_table_note(t, "a", Some("hi"))).unwrap();
        assert!(n.contains("Note: 'hi'"));
        let m = patched(t, dbml_edit::move_column(t, "a", "y", -1)).unwrap();
        assert!(m.find("y int").unwrap() < m.find("x int").unwrap());
        // limites e tabela inexistente não alteram o texto
        assert_eq!(
            patched(t, dbml_edit::move_column(t, "a", "x", -1)).unwrap(),
            t
        );
        assert!(patched(t, dbml_edit::move_column(t, "zz", "x", 1)).is_none());
    }

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
            "Self::export_docs_html",
            "Self::export_docs_markdown",
            "Self::open_quick_search",
            "Self::toggle_focus_mode",
            "Self::cycle_focus_depth",
            "Self::toggle_diff_mode",
            "Self::next_diff_change",
            "Self::prev_diff_change",
            "Self::toggle_minimap",
            "Self::export_png",
            "Self::export_jpeg",
            "Self::export_svg",
            "Self::connect_database",
            "Self::add_preset_uuid_pk",
            "Self::add_preset_bigint_pk",
            "Self::add_preset_timestamps",
            "Self::add_preset_soft_delete",
            "Self::show_code_view",
            "Self::filter_explorer",
            "Self::duplicate_table",
            "Self::reveal_in_dbml",
            "Self::theme_light",
            "Self::theme_dark",
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
        assert_eq!(count(crate::actions::GROUP_EXPORT_CODE), 8); // 5 bancos + Mermaid + HTML/MD docs
        assert_eq!(count(crate::actions::GROUP_EXPORT_IMAGE), 3); // PNG, JPEG, SVG
    }
}
