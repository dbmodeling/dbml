//! Canvas do diagrama: entidade com pan/zoom/arrastar, tabelas em vidro e relações ortogonais.
//! Coordenadas de mundo → tela: `tela = mundo * zoom + pan` (relativo à origem do canvas).

use crate::actions::{self, *};
use crate::constants::{
    CARD_ROWS_TOP_PAD, DIFF_DASH_GAP_PX, DIFF_DASH_PX, DIFF_EDGE_W_PX, DIFF_FOCUS_MIN_ZOOM,
    DIFF_GHOST_GAP, DIFF_GHOST_OPACITY, DIFF_GHOST_PER_ROW, DIFF_HALO_ALPHA, DIFF_HALO_SPREAD_PX,
    DIFF_ITEM_H_PX, DIFF_MARK_W_PX, DIFF_MINIMAP_GHOST_ALPHA, DIFF_PANEL_MARGIN_PX,
    DIFF_PANEL_MAX_H_PX, DIFF_PANEL_PAD_PX, DIFF_PANEL_W_PX, DIFF_ROW_TINT, DIFF_UNCHANGED_OPACITY,
    ID_DIFF_EXIT, ID_DIFF_GHOST_PREFIX, ID_DIFF_ITEM_PREFIX, ID_DIFF_LIST, ID_DIFF_PANEL,
    ID_TOOL_DIFF, TOOL_ISLAND_OFFSET_Y_PX,
};
use crate::diff_view::{DiffGroup, DiffStatus, DiffTarget, DiffView};
use crate::git_base::BaseState;
use crate::glass::{
    FONT_MONO, Glass, Mode, TABLE_COLORS, TEXT_SM, TEXT_XS, card_shadow, elevation, hex,
};
use crate::motion::{self, Tween};
use diagram_export::{EdgeStyle, EdgeStyles};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use schema::Schema;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use layout::{CARD_W, GROUP_PAD, HEADER_H, ROW_H, card_h};
const MIN_ZOOM: f32 = 0.2;
const MAX_ZOOM: f32 = 2.5;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Hand,
}

/// O que o shell exibe do canvas: seleção, multisseleção, zoom %, dica, contagens.
pub type Summary = (Option<String>, Vec<String>, i32, String, usize, usize);

/// Pedidos do canvas que viram edição do DBML (o shell aplica no texto, com undo).
pub enum DiagramEvent {
    CreateRef {
        from: (String, String),
        to: (String, String),
    },
    AddColumn(String),
    Rename(String),
    Select(String),
    SelectEnum(String),
    PositionsChanged(HashMap<String, (f32, f32)>),
    /// estilo de relação mudou; carrega o estado anterior (undo) — o shell grava o sidecar
    EdgesChanged(EdgeStyles),
}

impl EventEmitter<DiagramEvent> for Diagram {}

enum Drag {
    /// ligando coluna → coluna (cria relação ao soltar sobre outra coluna)
    Link {
        from: (String, String),
        now: (f32, f32),
    },
    /// move as tabelas selecionadas; `last` em coordenadas de mundo
    Cards {
        last: (f32, f32),
    },
    /// retângulo de seleção (mundo)
    Marquee {
        start: (f32, f32),
        now: (f32, f32),
    },
    /// arrasta todas as tabelas do grupo; `last` em coordenadas de mundo
    Group {
        name: String,
        last: (f32, f32),
    },
    /// arrasta um cartão de Enum; `last` em coordenadas de mundo
    Enum {
        name: String,
        last: (f32, f32),
    },
    Pan {
        last: Point<Pixels>,
    },
    /// arrasta a alça da relação selecionada; `start` = estilos antes do arrasto (um passo de undo)
    Edge {
        last: (f32, f32),
        start: EdgeStyles,
    },
    /// arrasta o retângulo da câmera dentro do minimapa (`grab` = deslocamento em px no minimapa)
    Minimap {
        grab: (f32, f32),
    },
}

/// Alvo do último clique direito (escolhe os itens do menu de contexto).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CtxOn {
    Background,
    Table,
    Edge,
}

pub type RefPair = ((String, String), (String, String));
/// Cúbica `[p0, c1, c2, p3]` em coordenadas de mundo.
type Curve = [layout::Pos; 4];

/// Geometria de uma relação: (tabela origem, tabela destino, chave do sidecar, curva já com o
/// estilo, curva automática, trechos desenhados).
type RefGeom = (String, String, String, Curve, Curve, Vec<Curve>);

/// Rotas das relações já calculadas. Vale enquanto `fingerprint` (posições, tamanhos dos cartões,
/// relações e estilos) não muda; pan e zoom não a invalidam (as rotas são em coordenadas de mundo).
#[derive(Default)]
struct RouteCache {
    fingerprint: Option<u64>,
    geoms: HashMap<usize, Option<Rc<RefGeom>>>,
}

struct LayoutMotion {
    from: HashMap<String, (f32, f32)>,
    to: HashMap<String, (f32, f32)>,
    progress: Tween<1>,
}

/// Estado do canvas salvo ao entrar no modo Comparar; restaurado na saída.
struct DiffSaved {
    pan: (f32, f32),
    zoom: f32,
    selected: Option<String>,
    selected_enum: Option<String>,
    multi: Vec<String>,
    selected_ref: Option<RefPair>,
    focus_mode: bool,
    highlighted_column: Option<(String, String)>,
}

/// Modo Comparar (#18): canvas somente leitura com o diff contra o `HEAD` do git.
struct DiffSession {
    /// Esquema da base: carregando (git em segundo plano), lido, ou a chave pt-BR do erro.
    base: BaseState<Schema>,
    view: Rc<DiffView>,
    ghost_tables: HashMap<String, (f32, f32)>,
    ghost_enums: HashMap<String, (f32, f32)>,
    /// Item do resumo em foco (navegação por teclado).
    cursor: Option<usize>,
    saved: DiffSaved,
}

impl DiffSession {
    /// Retângulos de mundo dos cartões fantasma (tabelas e enums removidos desde o `HEAD`).
    fn ghost_rects(&self) -> Vec<layout::Rect> {
        let tables = self.view.removed_tables.iter().filter_map(|t| {
            let &(x, y) = self.ghost_tables.get(&t.qualified_name())?;
            Some(layout::Rect {
                x,
                y,
                w: CARD_W,
                h: card_h(t.columns.len()),
            })
        });
        let enums = self.view.removed_enums.iter().filter_map(|e| {
            let &(x, y) = self.ghost_enums.get(&e.name)?;
            Some(layout::Rect {
                x,
                y,
                w: ENUM_W,
                h: enum_h(e),
            })
        });
        tables.chain(enums).collect()
    }
}

/// Cor do status no modo Comparar: verde = adicionado, âmbar = modificado, vermelho = removido.
fn diff_color(g: &Glass, status: DiffStatus) -> Hsla {
    match status {
        DiffStatus::Added => g.ok,
        DiffStatus::Modified => g.warning,
        DiffStatus::Removed => g.danger,
    }
}

/// Rótulo (pt-BR, traduzido na exibição) de um grupo do resumo.
const fn diff_group_label(group: DiffGroup) -> &'static str {
    match group {
        DiffGroup::Tables => "Tabelas",
        DiffGroup::Columns => "Colunas",
        DiffGroup::Indexes => "Índices",
        DiffGroup::Enums => "Enums",
        DiffGroup::Refs => "Relações",
    }
}

pub struct Diagram {
    schema: Schema,
    /// Posição de mundo por nome de tabela (sobrevive a reparse).
    pos: HashMap<String, (f32, f32)>,
    pan: (f32, f32),
    zoom: f32,
    viewport_motion: Option<Tween<3>>,
    layout_motion: Option<LayoutMotion>,
    appearing: HashMap<String, Tween<1>>,
    origin: Point<Pixels>,
    size: Size<Pixels>,
    drag: Option<Drag>,
    drag_start_pos: Option<HashMap<String, (f32, f32)>>,
    /// Último cartão tocado; desenhado por cima dos demais.
    top: Option<String>,
    selected: Option<String>,
    selected_enum: Option<String>,
    /// relação selecionada no canvas `((from_table, from_col), (to_table, to_col))`
    selected_ref: Option<RefPair>,
    /// seleção extra (Shift+clique); a seleção total é `selected` + `multi`
    multi: Vec<String>,
    pub tool: Tool,
    /// barra de espaço pressionada: arrastar move a vista (temporariamente)
    space: bool,
    pub legend_open: bool,
    pub minimap_open: bool,
    minimap_origin: std::cell::Cell<Option<Point<Pixels>>>,
    minimap_xform: std::cell::Cell<Option<crate::canvas_geom::MinimapTransform>>,
    focus: FocusHandle,
    /// tema atual (o shell atualiza ao trocar); o canvas é uma view própria
    pub glass: Option<Glass>,
    needs_fit: bool,
    ctx_on: CtxOn,
    context_menu: Option<(Entity<PopupMenu>, Point<Pixels>)>,
    context_subscription: Option<Subscription>,
    hovered_tool: Option<&'static str>,
    pressed_tool: Option<&'static str>,
    /// estilo por relação (`edges` do sidecar), por `diagram_export::edge_key`
    edges: EdgeStyles,
    /// incrementa a cada troca de `schema`; entra na impressão digital do cache de rotas
    schema_rev: u64,
    route_cache: RefCell<RouteCache>,
    /// Modo Foco (#12): destaca a tabela selecionada e sua vizinhança FK; esmaece o restante.
    focus_mode: bool,
    /// Profundidade do Modo Foco (1-hop ou 2-hop).
    focus_depth: usize,
    /// Coluna destacada via Busca Rápida (`(qualified_table, column)`).
    highlighted_column: Option<(String, String)>,
    /// Mapa compartilhado de bounds reais dos controles para o Spotlight Tour.
    pub tour_bounds: Option<crate::shell::guide_ui::TourBoundsMap>,
    /// Modo Comparar ativo (somente leitura).
    diff: Option<DiffSession>,
    /// Ticket do último pedido de base do modo Comparar (descarta resultados obsoletos).
    diff_ticket: u64,
}

impl Diagram {
    pub fn new(schema: Schema, cx: &mut Context<Self>) -> Self {
        let mut d = Self {
            schema: Schema::default(),
            pos: HashMap::new(),
            pan: (0., 0.),
            zoom: 1.,
            viewport_motion: None,
            layout_motion: None,
            appearing: HashMap::new(),
            origin: Point::default(),
            size: Size::default(),
            drag: None,
            drag_start_pos: None,
            top: None,
            selected: None,
            selected_enum: None,
            selected_ref: None,
            multi: Vec::new(),
            tool: Tool::Select,
            space: false,
            legend_open: false,
            minimap_open: true,
            minimap_origin: std::cell::Cell::new(None),
            minimap_xform: std::cell::Cell::new(None),
            focus: cx.focus_handle(),
            glass: None,
            needs_fit: true,
            ctx_on: CtxOn::Background,
            context_menu: None,
            context_subscription: None,
            hovered_tool: None,
            pressed_tool: None,
            edges: EdgeStyles::new(),
            schema_rev: 0,
            route_cache: RefCell::default(),
            focus_mode: false,
            focus_depth: 1,
            highlighted_column: None,
            tour_bounds: None,
            diff: None,
            diff_ticket: 0,
        };
        d.set_schema(schema);
        d
    }

    /// Troca o esquema mantendo a posição dos cartões que já existiam. Os novos partem do slot
    /// do auto-layout e são afastados (`layout::place_without_overlap`) se caírem em cima de
    /// um cartão que o usuário já posicionou.
    pub fn set_schema(&mut self, schema: Schema) {
        if let Some(layout) = self.layout_motion.take() {
            self.pos = layout.to;
        }
        let auto = auto_layout(&schema);
        let old = std::mem::take(&mut self.pos);
        let cards = schema
            .tables
            .iter()
            .map(|t| (t.qualified_name(), CARD_W, card_h(t.columns.len())))
            .chain(
                schema
                    .enums
                    .iter()
                    .map(|e| (enum_key(e), ENUM_W, enum_h(e))),
            );
        let (mut fixed, mut incoming, mut incoming_keys) = (Vec::new(), Vec::new(), Vec::new());
        for (key, w, h) in cards {
            if let Some(&(x, y)) = old.get(&key) {
                fixed.push(layout::Rect { x, y, w, h });
                self.pos.insert(key, (x, y));
            } else {
                let (x, y) = auto.get(&key).copied().unwrap_or((0., 0.));
                incoming.push(layout::Rect { x, y, w, h });
                incoming_keys.push(key);
            }
        }
        let placed = layout::place_without_overlap(&fixed, &incoming, PLACE_GAP);
        for (key, p) in incoming_keys.into_iter().zip(placed) {
            if !self.needs_fit && !motion::disabled() {
                self.appearing
                    .insert(key.clone(), Tween::new([0.], [1.], motion::BASE));
            }
            self.pos.insert(key, (p.x, p.y));
        }
        self.appearing.retain(|key, _| self.pos.contains_key(key));
        self.schema = schema;
        self.schema_rev += 1;
        self.refresh_diff();
    }

    /// `true` com o modo Comparar ativo (canvas somente leitura).
    pub fn diff_active(&self) -> bool {
        self.diff.is_some()
    }

    /// Entra no modo Comparar com a base ainda carregando; devolve o ticket que
    /// [`Self::finish_diff`] exige. Guarda vista e seleção para restaurar na saída; não altera
    /// texto, posições nem histórico.
    pub fn begin_diff(&mut self) -> u64 {
        self.diff_ticket += 1;
        self.enter_diff(BaseState::Loading(self.diff_ticket));
        self.diff_ticket
    }

    /// `true` se o modo Comparar ainda espera a base do pedido `ticket`.
    pub fn diff_waiting_for(&self, ticket: u64) -> bool {
        self.diff
            .as_ref()
            .is_some_and(|s| matches!(s.base, BaseState::Loading(t) if t == ticket))
    }

    /// Aplica a base lida em segundo plano. Descarta (`false`) se o modo foi desligado ou
    /// religado (outro ticket) desde o pedido.
    pub fn finish_diff(&mut self, ticket: u64, base: Result<Schema, &'static str>) -> bool {
        let Some(session) = &mut self.diff else {
            return false;
        };
        if !session.base.resolve(ticket, base) {
            return false;
        }
        self.refresh_diff();
        true
    }

    fn enter_diff(&mut self, base: BaseState<Schema>) {
        let saved = self.diff.take().map_or_else(
            || DiffSaved {
                pan: self.pan,
                zoom: self.zoom,
                selected: self.selected.take(),
                selected_enum: self.selected_enum.take(),
                multi: std::mem::take(&mut self.multi),
                selected_ref: self.selected_ref.take(),
                focus_mode: std::mem::replace(&mut self.focus_mode, false),
                highlighted_column: self.highlighted_column.take(),
            },
            |old| old.saved,
        );
        self.drag = None;
        self.context_menu = None;
        self.diff = Some(DiffSession {
            base,
            view: Rc::default(),
            ghost_tables: HashMap::new(),
            ghost_enums: HashMap::new(),
            cursor: None,
            saved,
        });
        self.refresh_diff();
    }

    /// Sai do modo Comparar restaurando vista e seleção de antes.
    pub fn exit_diff(&mut self) {
        let Some(session) = self.diff.take() else {
            return;
        };
        let s = session.saved;
        let keep = |name: &Option<String>, ok: bool| name.clone().filter(|_| ok);
        self.selected = keep(
            &s.selected,
            s.selected
                .as_deref()
                .is_some_and(|n| self.schema.table(n).is_some()),
        );
        self.selected_enum = keep(
            &s.selected_enum,
            s.selected_enum
                .as_deref()
                .is_some_and(|n| self.schema.enums.iter().any(|e| e.name == n)),
        );
        self.multi = s
            .multi
            .into_iter()
            .filter(|n| self.schema.table(n).is_some())
            .collect();
        self.selected_ref = s.selected_ref;
        self.focus_mode = s.focus_mode;
        self.highlighted_column = s.highlighted_column;
        self.animate_viewport([s.pan.0, s.pan.1, s.zoom]);
    }

    /// Recalcula o diff e os cartões fantasma (o head é o esquema atual do canvas).
    fn refresh_diff(&mut self) {
        let Some(session) = &self.diff else { return };
        let Some(base) = session.base.ready() else {
            return;
        };
        let view = crate::diff_view::build_diff_view(&schema::diff_schemas(base, &self.schema));
        let pos = self.positions();
        let mut occupied: Vec<layout::Rect> = Vec::new();
        for t in &self.schema.tables {
            if let Some(&(x, y)) = pos.get(&t.qualified_name()) {
                let h = card_h(crate::diff_view::diff_row_count(t, &view));
                occupied.push(layout::Rect { x, y, w: CARD_W, h });
            }
        }
        for en in &self.schema.enums {
            if let Some(&(x, y)) = pos.get(&enum_key(en)) {
                occupied.push(layout::Rect {
                    x,
                    y,
                    w: ENUM_W,
                    h: enum_h(en),
                });
            }
        }
        let slots: Vec<(f32, f32)> = view
            .removed_tables
            .iter()
            .map(|t| (CARD_W, card_h(t.columns.len())))
            .chain(view.removed_enums.iter().map(|e| (ENUM_W, enum_h(e))))
            .collect();
        let row_max_w = DIFF_GHOST_PER_ROW * (CARD_W + DIFF_GHOST_GAP);
        let mut placed =
            crate::diff_view::place_ghosts(&occupied, &slots, DIFF_GHOST_GAP, row_max_w)
                .into_iter();
        let ghost_tables = view
            .removed_tables
            .iter()
            .zip(placed.by_ref())
            .map(|(t, p)| (t.qualified_name(), p))
            .collect();
        let ghost_enums = view
            .removed_enums
            .iter()
            .zip(placed)
            .map(|(e, p)| (e.name.clone(), p))
            .collect();
        if let Some(session) = &mut self.diff {
            session.cursor = session.cursor.filter(|&i| i < view.items.len());
            session.view = Rc::new(view);
            session.ghost_tables = ghost_tables;
            session.ghost_enums = ghost_enums;
        }
    }

    /// Retângulo de mundo de um alvo do resumo (tabela/enum do head ou fantasma, relação).
    fn diff_target_rect(&self, target: &DiffTarget) -> Option<(f32, f32, f32, f32)> {
        let session = self.diff.as_ref()?;
        let table_rect = |name: &str| -> Option<(f32, f32, f32, f32)> {
            if let Some(t) = self.schema.table(name) {
                let (x, y) = *self.positions().get(&t.qualified_name())?;
                let rows = crate::diff_view::diff_row_count(t, &session.view);
                return Some((x, y, CARD_W, card_h(rows)));
            }
            let t = session
                .view
                .removed_tables
                .iter()
                .find(|t| t.qualified_name() == name)?;
            let (x, y) = *session.ghost_tables.get(name)?;
            Some((x, y, CARD_W, card_h(t.columns.len())))
        };
        match target {
            DiffTarget::Table { name, .. } => table_rect(name),
            DiffTarget::Enum { name, removed } => {
                if *removed {
                    let en = session
                        .view
                        .removed_enums
                        .iter()
                        .find(|e| &e.name == name)?;
                    let (x, y) = *session.ghost_enums.get(name)?;
                    Some((x, y, ENUM_W, enum_h(en)))
                } else {
                    let en = self.schema.enums.iter().find(|e| &e.name == name)?;
                    let (x, y) = *self.positions().get(&enum_key(en))?;
                    Some((x, y, ENUM_W, enum_h(en)))
                }
            }
            DiffTarget::Ref { key, .. } => {
                let r = &session.view.refs.get(key)?.r;
                let a = table_rect(&r.from.qualified_table())?;
                let b = table_rect(&r.to.qualified_table())?;
                let (x0, y0) = (a.0.min(b.0), a.1.min(b.1));
                let (x1, y1) = ((a.0 + a.2).max(b.0 + b.2), (a.1 + a.3).max(b.1 + b.3));
                Some((x0, y0, x1 - x0, y1 - y0))
            }
        }
    }

    /// Centraliza a vista no item `ix` do resumo (clique ou teclado); não mexe na seleção.
    pub fn focus_diff_item(&mut self, ix: usize) {
        let Some(target) = self
            .diff
            .as_ref()
            .and_then(|s| s.view.items.get(ix))
            .map(|i| i.target.clone())
        else {
            return;
        };
        if let Some(session) = &mut self.diff {
            session.cursor = Some(ix);
        }
        let Some((x, y, w, h)) = self.diff_target_rect(&target) else {
            return;
        };
        let (vw, vh) = (f32::from(self.size.width), f32::from(self.size.height));
        let z = if self.zoom < DIFF_FOCUS_MIN_ZOOM {
            1.0
        } else {
            self.zoom
        };
        let (cx_, cy_) = (x + w / 2., y + h / 2.);
        self.animate_viewport([vw / 2. - cx_ * z, vh / 2. - cy_ * z, z]);
    }

    /// Vai ao próximo (`forward`) ou anterior item do resumo, em ciclo.
    pub fn step_diff(&mut self, forward: bool) {
        let Some(session) = &self.diff else { return };
        let n = session.view.items.len();
        if n == 0 {
            return;
        }
        let next = match (session.cursor, forward) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
        };
        self.focus_diff_item(next);
    }

    /// Aplica posições salvas (sidecar de layout): cartões presentes mantêm o lugar salvo, os que
    /// não estão no sidecar partem do auto-layout sem cair em cima dos demais; chaves de
    /// tabelas que não existem mais são descartadas.
    pub fn seed_positions(&mut self, saved: HashMap<String, (f32, f32)>) {
        if saved.is_empty() {
            return;
        }
        self.layout_motion = None;
        self.pos = saved;
        let schema = std::mem::take(&mut self.schema);
        self.set_schema(schema);
    }

    pub fn relayout(&mut self) {
        self.advance_motion(Instant::now(), motion::disabled());
        let from = self.pos.clone();
        self.layout_motion = None;
        self.pos = auto_layout(&self.schema);
        self.fit();
        if !motion::disabled() && from != self.pos {
            self.layout_motion = Some(LayoutMotion {
                from: from.clone(),
                to: self.pos.clone(),
                progress: Tween::new([0.], [1.], motion::SLOW),
            });
            self.pos = from;
        }
    }

    pub fn selected(&self) -> Option<&schema::Table> {
        self.schema.table(self.selected.as_deref()?)
    }

    /// Relações em que a tabela participa (como origem ou destino).
    pub fn refs_of(&self, table: &str) -> Vec<&schema::Ref> {
        self.schema
            .refs
            .iter()
            .filter(|r| r.from.qualified_table() == table || r.to.qualified_table() == table)
            .collect()
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn zoom(&self) -> f32 {
        self.viewport().0
    }

    pub fn viewport(&self) -> (f32, f32, f32) {
        let [x, y, z] = self
            .viewport_motion
            .as_ref()
            .map_or([self.pan.0, self.pan.1, self.zoom], |m| m.to);
        (z, x, y)
    }

    pub fn positions(&self) -> &HashMap<String, (f32, f32)> {
        self.layout_motion.as_ref().map_or(&self.pos, |m| &m.to)
    }

    pub fn selection_state(&self) -> (Option<String>, Option<String>, Vec<String>) {
        (
            self.selected.clone(),
            self.selected_enum.clone(),
            self.multi.clone(),
        )
    }

    pub fn apply_snapshot(
        &mut self,
        schema: Schema,
        pos: HashMap<String, (f32, f32)>,
        selected: Option<String>,
        selected_enum: Option<String>,
        multi: Vec<String>,
    ) {
        self.viewport_motion = None;
        self.layout_motion = None;
        if !self.needs_fit && !motion::disabled() {
            for key in pos.keys().filter(|key| !self.pos.contains_key(*key)) {
                self.appearing
                    .insert(key.clone(), Tween::new([0.], [1.], motion::BASE));
            }
        }
        self.appearing.retain(|key, _| pos.contains_key(key));
        self.schema = schema;
        self.schema_rev += 1;
        self.pos = pos;
        self.selected = selected;
        self.selected_enum = selected_enum;
        self.selected_ref = None;
        self.multi = multi;
        self.refresh_diff();
    }

    pub fn set_viewport(&mut self, zoom: f32, pan: (f32, f32)) {
        if zoom.is_finite() && zoom > 0.0 && pan.0.is_finite() && pan.1.is_finite() {
            self.viewport_motion = None;
            self.zoom = zoom;
            self.pan = pan;
            self.needs_fit = false;
        }
    }

    fn resize_viewport(&mut self, origin: Point<Pixels>, size: Size<Pixels>) {
        if !self.needs_fit
            && (self.selected.is_some() || self.selected_enum.is_some())
            && f32::from(self.size.width) > 0.
            && f32::from(self.size.height) > 0.
            && f32::from(size.width) > 0.
            && f32::from(size.height) > 0.
        {
            let mut dx = f32::from(size.width - self.size.width) / 2.;
            let mut dy = f32::from(size.height - self.size.height) / 2.;
            if let Some(name) = &self.selected
                && let (Some(table), Some(&(x, y))) =
                    (self.schema.table(name), self.positions().get(name))
            {
                let center_x = (x + CARD_W / 2.) * self.zoom + self.pan.0 + dx;
                let center_y = (y + card_h(table.columns.len()) / 2.) * self.zoom + self.pan.1 + dy;
                let (w, h) = (f32::from(size.width), f32::from(size.height));
                if center_x < 0. || center_x > w {
                    dx += w / 2. - center_x;
                }
                if center_y < 0. || center_y > h {
                    dy += h / 2. - center_y;
                }
            }
            self.pan.0 += dx;
            self.pan.1 += dy;
            if let Some(tween) = &mut self.viewport_motion {
                tween.translate(dx, dy);
            }
        }
        self.origin = origin;
        self.size = size;
    }

    /// Seleciona a tabela e centraliza a vista nela em um zoom legível.
    pub fn focus_table(&mut self, name: &str) {
        let Some(t) = self.schema.table(name) else {
            return;
        };
        let qname = t.qualified_name();
        let (x, y) = self
            .positions()
            .get(&qname)
            .copied()
            .unwrap_or(crate::constants::DEFAULT_CARD_POS);
        let (w, h) = (f32::from(self.size.width), f32::from(self.size.height));
        let target_zoom = if self.zoom < 0.85 { 1.0 } else { self.zoom };
        let (cx_, cy_) = (x + CARD_W / 2., y + card_h(t.columns.len()) / 2.);
        self.animate_viewport([
            w / 2. - cx_ * target_zoom,
            h / 2. - cy_ * target_zoom,
            target_zoom,
        ]);
        self.selected = Some(qname.clone());
        self.multi.clear();
        self.selected_enum = None;
        self.selected_ref = None;
        self.highlighted_column = None;
        self.top = Some(qname);
    }

    /// Seleciona a tabela, destaca a coluna alvo e centraliza a vista nela.
    pub fn focus_column(&mut self, table: &str, column: &str) {
        self.focus_table(table);
        if let Some(t) = self.schema.table(table) {
            self.highlighted_column = Some((t.qualified_name(), column.to_string()));
        }
    }

    /// Seleciona o enum e centraliza a vista nele em um zoom legível.
    pub fn focus_enum(&mut self, name: &str) {
        let Some(en) = self
            .schema
            .enums
            .iter()
            .find(|e| e.name == name || e.qualified_name() == name)
        else {
            return;
        };
        let key = enum_key(en);
        let en_name = en.name.clone();
        let (x, y) = self.positions().get(&key).copied().unwrap_or((0., 0.));
        let (w, h) = (f32::from(self.size.width), f32::from(self.size.height));
        let target_zoom = if self.zoom < 0.85 { 1.0 } else { self.zoom };
        let (cx_, cy_) = (x + ENUM_W / 2., y + enum_h(en) / 2.);
        self.animate_viewport([
            w / 2. - cx_ * target_zoom,
            h / 2. - cy_ * target_zoom,
            target_zoom,
        ]);
        self.selected = None;
        self.multi.clear();
        self.selected_ref = None;
        self.highlighted_column = None;
        self.selected_enum = Some(en_name);
    }

    pub fn focus_mode(&self) -> bool {
        self.focus_mode && !self.selection().is_empty()
    }

    pub fn toggle_focus_mode(&mut self) {
        if self.focus_mode {
            self.focus_mode = false;
        } else if !self.selection().is_empty() {
            self.focus_mode = true;
        }
    }

    pub fn cycle_focus_depth(&mut self) {
        self.focus_depth = if self.focus_depth == 1 { 2 } else { 1 };
        if !self.selection().is_empty() {
            self.focus_mode = true;
        }
    }

    /// O minimapa tem o que mostrar: tabelas do esquema ou fantasmas do modo Comparar.
    fn minimap_has_cards(&self) -> bool {
        !self.schema.tables.is_empty()
            || self
                .diff
                .as_ref()
                .is_some_and(|s| !s.ghost_tables.is_empty() || !s.ghost_enums.is_empty())
    }

    fn minimap_transform(
        &self,
    ) -> Option<(crate::canvas_geom::MinimapTransform, (f32, f32, f32, f32))> {
        if !self.minimap_has_cards() {
            return None;
        }
        let (vw, vh) = (
            f32::from(self.size.width).max(1.0),
            f32::from(self.size.height).max(1.0),
        );
        let vp_x = -self.pan.0 / self.zoom;
        let vp_y = -self.pan.1 / self.zoom;
        let vp_w = vw / self.zoom;
        let vp_h = vh / self.zoom;
        let viewport = layout::Rect {
            x: vp_x,
            y: vp_y,
            w: vp_w,
            h: vp_h,
        };
        let (min_x, min_y, max_x, max_y) = crate::canvas_geom::world_bounds(
            std::iter::once(viewport).chain(self.content_rects(&self.pos)),
        )?;
        let margin = 40.0;
        let xf = crate::canvas_geom::MinimapTransform::compute(
            min_x - margin,
            min_y - margin,
            max_x + margin,
            max_y + margin,
            crate::canvas_geom::MINIMAP_W,
            crate::canvas_geom::MINIMAP_H,
            crate::canvas_geom::MINIMAP_PAD,
        );
        Some((xf, (vp_x, vp_y, vp_w, vp_h)))
    }

    fn pan_from_minimap(&mut self, screen_pos: Point<Pixels>, grab: (f32, f32)) {
        let (Some(origin), Some(xf)) = (self.minimap_origin.get(), self.minimap_xform.get()) else {
            return;
        };
        let mx = f32::from(screen_pos.x - origin.x) - grab.0;
        let my = f32::from(screen_pos.y - origin.y) - grab.1;
        let (vw, vh) = (f32::from(self.size.width), f32::from(self.size.height));
        self.pan = xf.pan_for_map_point(mx, my, self.zoom, vw, vh);
    }

    /// Frase curta para a barra de status: o que o usuário pode fazer agora.
    pub fn hint(&self) -> String {
        if self.diff.is_some() {
            return crate::i18n::t(
                "Comparar com HEAD (somente leitura) · clique em um item do resumo · N / Shift+N navega · Esc sai",
            )
            .into();
        }
        match (&self.drag, self.tool) {
            (Some(Drag::Cards { .. }), _) => match self.multi.len() {
                0 => crate::i18n::t("Movendo {}").replacen("{}", &self.selected.clone().unwrap_or_default(), 1),
                n => crate::i18n::t("Movendo {} tabelas").replace("{}", &(n + 1).to_string()),
            },
            (Some(Drag::Link { from, .. }), _) => {
                crate::i18n::t("Relação de {}.{}: solte na coluna de destino").replacen("{}", &from.0, 1).replacen("{}", &from.1, 1)
            }
            (Some(Drag::Marquee { .. }), _) => crate::i18n::t("Selecionando: solte para selecionar as tabelas tocadas").into(),
            (Some(Drag::Pan { .. }), _) => crate::i18n::t("Movendo a vista").into(),
            (Some(Drag::Group { name, .. }), _) => crate::i18n::t("Movendo grupo {}").replace("{}", name),
            (Some(Drag::Enum { name, .. }), _) => crate::i18n::t("Movendo enum {}").replace("{}", name),
            (Some(Drag::Edge { .. }), _) => crate::i18n::t("Curvando a relação: solte para gravar").into(),
            (Some(Drag::Minimap { .. }), _) => crate::i18n::t("Navegando pelo minimapa").into(),
            (None, Tool::Hand) => crate::i18n::t("Mão: arraste para mover a vista · V volta a Selecionar").into(),
            (None, Tool::Select) if !self.multi.is_empty() => {
                crate::i18n::t("{} tabelas selecionadas · Ctrl+G agrupa").replace("{}", &(self.multi.len() + 1).to_string())
            }
            (None, Tool::Select) if self.selected_ref.is_some() => {
                crate::i18n::t("Relação selecionada · arraste a alça ou use as setas para curvar · Delete remove a linha e a coluna FK · Esc limpa a seleção")
                    .into()
            }
            (None, Tool::Select) if self.selected.is_some() => {
                crate::i18n::t("Arraste para mover a tabela · Esc limpa a seleção").into()
            }
            (None, Tool::Select) => {
                crate::i18n::t("Clique em uma tabela para ver detalhes · arraste o fundo para mover a vista · role para dar zoom").into()
            }
        }
    }

    /// Todas as tabelas selecionadas (principal primeiro).
    pub fn selection(&self) -> Vec<String> {
        self.selected.iter().chain(&self.multi).cloned().collect()
    }

    fn is_selected(&self, name: &str) -> bool {
        self.selected.as_deref() == Some(name) || self.multi.iter().any(|m| m == name)
    }

    pub fn select(&mut self, name: &str) {
        if self.schema.table(name).is_some() {
            self.selected = Some(name.to_string());
            self.selected_enum = None;
            self.selected_ref = None;
            self.multi.clear();
        }
    }

    pub fn selected_enum(&self) -> Option<&schema::EnumDef> {
        let name = self.selected_enum.as_deref()?;
        self.schema.enums.iter().find(|e| e.name == name)
    }

    pub fn select_enum(&mut self, name: &str) {
        if self.schema.enums.iter().any(|e| e.name == name) {
            self.selected_enum = Some(name.to_string());
            self.selected = None;
            self.selected_ref = None;
            self.multi.clear();
        }
    }

    pub fn selected_ref(&self) -> Option<&RefPair> {
        self.selected_ref.as_ref()
    }

    pub fn deselect(&mut self) {
        self.multi.clear();
        self.selected = None;
        self.selected_enum = None;
        self.selected_ref = None;
        self.focus_mode = false;
        self.highlighted_column = None;
    }

    pub fn zoom_by(&mut self, factor: f32) {
        let (zoom, pan_x, pan_y) = self.viewport();
        let c = (
            f32::from(self.size.width) / 2.,
            f32::from(self.size.height) / 2.,
        );
        let z = (zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let k = z / zoom;
        self.animate_viewport([c.0 - (c.0 - pan_x) * k, c.1 - (c.1 - pan_y) * k, z]);
    }

    pub fn reset_zoom(&mut self) {
        self.zoom_by(1. / self.viewport().0);
    }

    pub fn fit(&mut self) {
        self.advance_motion(Instant::now(), motion::disabled());
        let from = [self.pan.0, self.pan.1, self.zoom];
        self.do_fit();
        let to = [self.pan.0, self.pan.1, self.zoom];
        self.pan = (from[0], from[1]);
        self.zoom = from[2];
        self.viewport_motion = None;
        self.animate_viewport(to);
    }

    /// Retângulos de mundo de todos os cartões desenhados: tabelas e enums do esquema (em `pos`)
    /// e, no modo Comparar, os fantasmas (removidos desde o `HEAD`).
    fn content_rects(&self, pos: &HashMap<String, (f32, f32)>) -> Vec<layout::Rect> {
        let rows = |t: &schema::Table| {
            self.diff.as_ref().map_or(t.columns.len(), |s| {
                crate::diff_view::diff_row_count(t, &s.view)
            })
        };
        let tables = self.schema.tables.iter().filter_map(|t| {
            let &(x, y) = pos.get(&t.qualified_name())?;
            Some(layout::Rect {
                x,
                y,
                w: CARD_W,
                h: card_h(rows(t)),
            })
        });
        let enums = self.schema.enums.iter().filter_map(|en| {
            let &(x, y) = pos.get(&enum_key(en))?;
            Some(layout::Rect {
                x,
                y,
                w: ENUM_W,
                h: enum_h(en),
            })
        });
        let ghosts = self
            .diff
            .as_ref()
            .map(DiffSession::ghost_rects)
            .unwrap_or_default();
        tables.chain(enums).chain(ghosts).collect()
    }

    fn do_fit(&mut self) {
        let (w, h) = (f32::from(self.size.width), f32::from(self.size.height));
        if w < 1. || h < 1. {
            return;
        }
        let Some((x0, y0, x1, y1)) =
            crate::canvas_geom::world_bounds(self.content_rects(self.positions()))
        else {
            return;
        };
        let pad = 48.;
        // a ilha de ferramentas fica à esquerda (12 + 40 × escala base 1,3) e o grupo desenha 24 além
        let pad_x = 12. + 40. * crate::glass::UI_BASE + 24. + 12.;
        let z = ((w - 2. * pad_x) / (x1 - x0))
            .min((h - 2. * pad) / (y1 - y0))
            .clamp(MIN_ZOOM, 1.0);
        self.zoom = z;
        // centraliza; se não couber, alinha ao início para mostrar as primeiras tabelas inteiras
        let fit_x = (w - (x1 - x0) * z) / 2.;
        let fit_y = (h - (y1 - y0) * z) / 2.;
        self.pan = (fit_x.max(pad_x) - x0 * z, fit_y.max(pad) - y0 * z);
        self.needs_fit = false;
    }

    fn to_world(&self, p: Point<Pixels>) -> (f32, f32) {
        let (lx, ly) = self.to_local(p);
        ((lx - self.pan.0) / self.zoom, (ly - self.pan.1) / self.zoom)
    }

    /// Fim de arrasto; o retângulo de seleção seleciona as tabelas que ele toca.
    /// Coluna sob o ponto (mundo), para soltar uma relação.
    fn column_at(&self, p: (f32, f32)) -> Option<(String, String)> {
        self.schema.tables.iter().find_map(|t| {
            let qname = t.qualified_name();
            let &(x, y) = self.pos.get(&qname)?;
            let inside =
                p.0 >= x && p.0 <= x + CARD_W && p.1 >= y && p.1 <= y + card_h(t.columns.len());
            let row = ((p.1 - y - HEADER_H - 4.) / ROW_H).floor();
            (inside && row >= 0.)
                .then(|| t.columns.get(row as usize))
                .flatten()
                .map(|c| (qname, c.name.clone()))
        })
    }

    /// Tabela sob o ponto (mundo), para soltar uma relação em qualquer parte do cartão.
    fn table_at(&self, p: (f32, f32)) -> Option<(String, String)> {
        self.schema.tables.iter().find_map(|t| {
            let qname = t.qualified_name();
            let &(x, y) = self.pos.get(&qname)?;
            let inside =
                p.0 >= x && p.0 <= x + CARD_W && p.1 >= y && p.1 <= y + card_h(t.columns.len());
            if !inside {
                return None;
            }
            let pk_col = t
                .columns
                .iter()
                .find(|c| c.pk)
                .or_else(|| t.columns.first())
                .map_or_else(|| "id".to_string(), |c| c.name.clone());
            Some((qname, pk_col))
        })
    }

    /// Geometria de uma relação: (tabela origem, tabela destino, chave do sidecar, curva já com o
    /// estilo, curva automática, trechos desenhados), em mundo. Sem estilo, os trechos desviam dos
    /// outros cartões. `None` se faltar tabela ou coluna.
    fn compute_ref_geom(&self, r: &schema::Ref) -> Option<RefGeom> {
        let ft = self.schema.table(&r.from.qualified_table())?;
        let tt = self.schema.table(&r.to.qualified_table())?;
        let fc = ft.columns.iter().position(|c| c.name == r.from.column)?;
        let tc = tt.columns.iter().position(|c| c.name == r.to.column)?;
        let (source_key, target_key) = (ft.qualified_name(), tt.qualified_name());
        let (ax, ay) = *self.pos.get(&source_key)?;
        let (bx, by) = *self.pos.get(&target_key)?;
        let ay = ay + HEADER_H + 4. + ROW_H * (fc as f32 + 0.5);
        let by = by + HEADER_H + 4. + ROW_H * (tc as f32 + 0.5);
        let auto = diagram_export::relation_curve(ax, ay, bx, by);
        let key = diagram_export::edge_key(&source_key, &r.from, &target_key, &r.to);
        let style = self.edges.get(&key);
        let curve = diagram_export::styled_curve(auto, style);
        let segs = if style.is_some() {
            vec![curve]
        } else {
            let obstacles: Vec<layout::Rect> = self
                .schema
                .tables
                .iter()
                .filter_map(|t| {
                    let q = t.qualified_name();
                    if q == source_key || q == target_key {
                        return None;
                    }
                    let (x, y) = *self.pos.get(&q)?;
                    Some(layout::Rect {
                        x,
                        y,
                        w: CARD_W,
                        h: card_h(t.columns.len()),
                    })
                })
                .collect();
            diagram_export::relation_segments(ax, ay, bx, by, &obstacles)
        };
        Some((source_key, target_key, key, curve, auto, segs))
    }

    /// Impressão digital do que as rotas dependem: relações (`schema_rev`), posição e altura de
    /// cada cartão, e estilos de relação. Custa O(tabelas + estilos), sem rotear nada.
    fn route_fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.schema_rev.hash(&mut h);
        for t in &self.schema.tables {
            t.columns.len().hash(&mut h);
            if let Some(&(x, y)) = self.pos.get(&t.qualified_name()) {
                x.to_bits().hash(&mut h);
                y.to_bits().hash(&mut h);
            }
        }
        // soma de hashes por entrada: independe da ordem de iteração do mapa
        let styles = self.edges.iter().fold(0_u64, |acc, (key, st)| {
            let mut e = std::collections::hash_map::DefaultHasher::new();
            key.hash(&mut e);
            (st.straight, st.dx.to_bits(), st.dy.to_bits()).hash(&mut e);
            acc.wrapping_add(e.finish())
        });
        styles.hash(&mut h);
        h.finish()
    }

    /// Descarta as rotas em cache se algo de que dependem mudou. Chamar uma vez por quadro
    /// (e por evento de mouse) antes de ler `ref_geom`.
    fn validate_routes(&self) {
        let fp = self.route_fingerprint();
        let mut cache = self.route_cache.borrow_mut();
        if cache.fingerprint != Some(fp) {
            cache.fingerprint = Some(fp);
            cache.geoms.clear();
        }
    }

    /// Geometria da relação `idx` (índice em `schema.refs`), do cache de rotas. `None` se faltar
    /// tabela ou coluna.
    fn ref_geom(&self, idx: usize) -> Option<Rc<RefGeom>> {
        if let Some(hit) = self.route_cache.borrow().geoms.get(&idx) {
            return hit.clone();
        }
        let geom = self
            .schema
            .refs
            .get(idx)
            .and_then(|r| self.compute_ref_geom(r))
            .map(Rc::new);
        self.route_cache
            .borrow_mut()
            .geoms
            .insert(idx, geom.clone());
        geom
    }

    fn is_selected_ref(&self, source_key: &str, r: &schema::Ref, target_key: &str) -> bool {
        self.selected_ref.as_ref().is_some_and(|(f, t)| {
            f.0 == source_key && f.1 == r.from.column && t.0 == target_key && t.1 == r.to.column
        })
    }

    /// Relação (`Ref`) sob o ponto (mundo), amostrando a Bézier (~6 px na tela).
    fn ref_at(&self, p: (f32, f32)) -> Option<RefPair> {
        self.validate_routes();
        self.schema.refs.iter().enumerate().find_map(|(i, r)| {
            let geom = self.ref_geom(i)?;
            let (source_key, target_key, _, _, _, segs) = &*geom;
            segs.iter()
                .any(|c| crate::canvas_geom::point_hits_curve(p, c, 6. / self.zoom))
                .then(|| {
                    (
                        (source_key.clone(), r.from.column.clone()),
                        (target_key.clone(), r.to.column.clone()),
                    )
                })
        })
    }

    /// Chave, curva com estilo e curva automática da relação selecionada.
    fn selected_edge(&self) -> Option<(String, Curve, Curve, layout::Pos)> {
        self.validate_routes();
        self.schema.refs.iter().enumerate().find_map(|(i, r)| {
            let geom = self.ref_geom(i)?;
            let (s, t, key, curve, auto, segs) = &*geom;
            // alça no meio do trecho desenhado (a rota que desvia pode não ser a curva)
            let mid = diagram_export::curve_mid(&segs[segs.len() / 2]);
            self.is_selected_ref(s, r, t)
                .then(|| (key.clone(), *curve, *auto, mid))
        })
    }

    /// O ponto (mundo) cai na alça da relação selecionada (círculo de ~8 px no ponto médio).
    fn on_edge_handle(&self, p: (f32, f32)) -> bool {
        self.selected_edge()
            .is_some_and(|(_, _, _, m)| (p.0 - m.x).hypot(p.1 - m.y) <= 8. / self.zoom)
    }

    pub fn edges(&self) -> &EdgeStyles {
        &self.edges
    }

    pub fn set_edges(&mut self, edges: EdgeStyles) {
        self.edges = edges;
    }

    /// Move a alça da relação selecionada por (mx, my) em mundo: dx/dy andam /0.75 para a alça
    /// acompanhar o ponteiro. Relação reta vira curva a partir da posição reta atual.
    fn move_edge(&mut self, mx: f32, my: f32) {
        let Some((key, curve, auto, _)) = self.selected_edge() else {
            return;
        };
        let old = self.edges.get(&key).copied().unwrap_or_default();
        let (ox, oy) = if old.straight {
            let (s, a) = (
                diagram_export::curve_mid(&curve),
                diagram_export::curve_mid(&auto),
            );
            ((s.x - a.x) / 0.75, (s.y - a.y) / 0.75)
        } else {
            (old.dx, old.dy)
        };
        let style = EdgeStyle {
            straight: false,
            dx: ox + mx / 0.75,
            dy: oy + my / 0.75,
        };
        if style == EdgeStyle::default() {
            self.edges.remove(&key);
        } else {
            self.edges.insert(key, style);
        }
    }

    /// Relação reta/curva (`reset = false`) ou restauração da curva automática.
    pub fn edit_edge(&mut self, reset: bool, cx: &mut Context<Self>) {
        let Some((key, _, _, _)) = self.selected_edge() else {
            return;
        };
        let before = self.edges.clone();
        let cur = self.edges.get(&key).copied();
        let next = match cur {
            _ if reset => None,
            Some(s) if s.straight => Some(EdgeStyle {
                straight: false,
                ..s
            }),
            s => Some(EdgeStyle {
                straight: true,
                ..s.unwrap_or_default()
            }),
        }
        .filter(|s| *s != EdgeStyle::default());
        match next {
            Some(s) => self.edges.insert(key, s),
            None => self.edges.remove(&key),
        };
        if self.edges != before {
            cx.emit(DiagramEvent::EdgesChanged(before));
        }
        cx.notify();
    }

    /// Posiciona uma tabela recém-criada no centro da vista e a seleciona.
    pub fn place_new(&mut self, name: &str) {
        let (w, h) = (f32::from(self.size.width), f32::from(self.size.height));
        let at = (
            (w / 2. - self.pan.0) / self.zoom - CARD_W / 2.,
            (h / 2. - self.pan.1) / self.zoom - 60.,
        );
        self.place_new_at(name, at);
    }

    /// Posiciona uma tabela recém-criada em uma coordenada explícita de mundo e a seleciona.
    pub fn place_new_at(&mut self, name: &str, at: (f32, f32)) {
        self.pos.insert(name.to_string(), at);
        self.selected = Some(name.to_string());
        self.selected_enum = None;
        self.selected_ref = None;
        self.multi.clear();
        self.top = Some(name.to_string());
    }

    /// Calcula o ponto médio entre duas tabelas no canvas (ou `None` se alguma não tiver posição).
    #[must_use]
    pub fn midpoint_between(&self, left: &str, right: &str) -> Option<(f32, f32)> {
        let lp = self.pos.get(left)?;
        let rp = self.pos.get(right)?;
        Some((
            f32::midpoint(lp.0, rp.0).round(),
            f32::midpoint(lp.1, rp.1).round(),
        ))
    }

    /// Posiciona um enum recém-criado no centro da vista e o seleciona.
    pub fn place_new_enum(&mut self, name: &str) {
        let (w, h) = (f32::from(self.size.width), f32::from(self.size.height));
        let at = (
            (w / 2. - self.pan.0) / self.zoom - ENUM_W / 2.,
            (h / 2. - self.pan.1) / self.zoom - 50.,
        );
        self.pos.insert(format!("enum:{name}"), at);
        self.selected = None;
        self.multi.clear();
        self.selected_ref = None;
        self.selected_enum = Some(name.to_string());
    }

    /// Mantém posição e seleção quando um enum é renomeado.
    pub fn rename_enum(&mut self, old: &str, new: &str) {
        let old_key = format!("enum:{old}");
        if let Some(p) = self.pos.remove(&old_key) {
            self.pos.insert(format!("enum:{new}"), p);
        }
        if self.selected_enum.as_deref() == Some(old) {
            self.selected_enum = Some(new.to_string());
        }
    }

    /// Mantém posição e seleção quando a tabela é renomeada.
    pub fn rename(&mut self, old: &str, new: &str) {
        if let Some(p) = self.pos.remove(old) {
            self.pos.insert(new.to_string(), p);
        }
        for s in self.selected.iter_mut().chain(self.multi.iter_mut()) {
            if s == old {
                *s = new.to_string();
            }
        }
    }

    fn end_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(old_pos) = self.drag_start_pos.take()
            && old_pos != self.pos
        {
            cx.emit(DiagramEvent::PositionsChanged(old_pos));
        }
        if let Some(Drag::Link { from, now }) = &self.drag
            && let Some(to) = self.link_target(from, *now)
        {
            cx.emit(DiagramEvent::CreateRef {
                from: from.clone(),
                to,
            });
        }
        if let Some(Drag::Edge { start, .. }) = &self.drag
            && *start != self.edges
        {
            cx.emit(DiagramEvent::EdgesChanged(start.clone()));
        }
        if let Some(Drag::Marquee { start, now }) = self.drag.take() {
            let (x0, x1) = (start.0.min(now.0), start.0.max(now.0));
            let (y0, y1) = (start.1.min(now.1), start.1.max(now.1));
            if x1 - x0 > 2. || y1 - y0 > 2. {
                for t in &self.schema.tables {
                    let qname = t.qualified_name();
                    let Some(&(x, y)) = self.pos.get(&qname) else {
                        continue;
                    };
                    let hit =
                        x < x1 && x + CARD_W > x0 && y < y1 && y + card_h(t.columns.len()) > y0;
                    if hit && !self.is_selected(&qname) {
                        if self.selected.is_none() {
                            self.selected = Some(qname);
                        } else {
                            self.multi.push(qname);
                        }
                    }
                }
            }
        }
        self.drag = None;
        cx.notify();
    }

    /// Alvo válido de uma relação em arraste: coluna (ou cartão) de outra tabela.
    fn link_target(&self, from: &(String, String), at: (f32, f32)) -> Option<(String, String)> {
        self.column_at(at)
            .or_else(|| self.table_at(at))
            .filter(|to| to.0 != from.0)
    }

    fn to_local(&self, p: Point<Pixels>) -> (f32, f32) {
        (
            f32::from(p.x - self.origin.x),
            f32::from(p.y - self.origin.y),
        )
    }

    fn zoom_at(&mut self, cursor: (f32, f32), factor: f32) {
        self.viewport_motion = None;
        let z = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let k = z / self.zoom;
        self.pan = (
            cursor.0 - (cursor.0 - self.pan.0) * k,
            cursor.1 - (cursor.1 - self.pan.1) * k,
        );
        self.zoom = z;
    }

    fn animate_viewport(&mut self, to: [f32; 3]) {
        self.advance_motion(Instant::now(), motion::disabled());
        let from = [self.pan.0, self.pan.1, self.zoom];
        if motion::disabled()
            || from
                .iter()
                .zip(to)
                .all(|(a, b)| (*a - b).abs() < f32::EPSILON)
        {
            self.pan = (to[0], to[1]);
            self.zoom = to[2];
            self.viewport_motion = None;
        } else {
            self.viewport_motion = Some(Tween::new(from, to, motion::SLOW));
        }
    }

    fn advance_motion(&mut self, now: Instant, reduced: bool) -> bool {
        if let Some(tween) = &self.viewport_motion {
            let [x, y, z] = if reduced { tween.to } else { tween.sample(now) };
            self.pan = (x, y);
            self.zoom = z;
            if reduced || tween.finished(now) {
                self.viewport_motion = None;
            }
        }
        if let Some(layout) = &self.layout_motion {
            let t = if reduced {
                1.
            } else {
                layout.progress.sample(now)[0]
            };
            for (name, &(x, y)) in &layout.to {
                let &(sx, sy) = layout.from.get(name).unwrap_or(&(x, y));
                self.pos
                    .insert(name.clone(), (sx + (x - sx) * t, sy + (y - sy) * t));
            }
            if (reduced || layout.progress.finished(now))
                && let Some(finished) = self.layout_motion.take()
            {
                self.pos = finished.to;
            }
        }
        self.appearing.retain(|_, t| !reduced && !t.finished(now));
        self.viewport_motion.is_some() || self.layout_motion.is_some() || !self.appearing.is_empty()
    }

    /// Manipulação direta cancela o percurso e continua da posição visível.
    fn interrupt_motion(&mut self) {
        self.viewport_motion = None;
        self.layout_motion = None;
    }
}

fn auto_layout(schema: &Schema) -> HashMap<String, (f32, f32)> {
    let col_counts: Vec<usize> = schema.tables.iter().map(|t| t.columns.len()).collect();
    let idx = |n: &str| {
        schema
            .tables
            .iter()
            .position(|t| t.qualified_name() == n)
            .or_else(|| {
                (!n.contains('.'))
                    .then(|| schema.tables.iter().position(|t| t.name == n))
                    .flatten()
            })
    };
    let edges: Vec<_> = schema
        .refs
        .iter()
        .filter_map(|r| {
            let from = r.from.qualified_table();
            let to = r.to.qualified_table();
            Some((idx(&from)?, idx(&to)?))
        })
        .collect();
    let clusters = (!schema.groups.is_empty()).then(|| {
        let mut c: Vec<Vec<usize>> = schema
            .groups
            .iter()
            .map(|g| g.tables.iter().filter_map(|t| idx(t)).collect())
            .collect();
        c.push(
            (0..schema.tables.len())
                .filter(|&i| {
                    schema
                        .group_of(&schema.tables[i].qualified_name())
                        .is_none()
                })
                .collect(),
        );
        c
    });
    let pos = layout::compute_diagram_positions(&col_counts, &edges, clusters.as_deref());
    let mut map: HashMap<String, (f32, f32)> = schema
        .tables
        .iter()
        .zip(pos)
        .map(|(t, p)| (t.qualified_name(), (p.x, p.y)))
        .collect();
    // enums: uma fileira abaixo da tabela mais baixa (altura pareada pela chave, não pela
    // ordem de iteração do HashMap), sem encostar em nenhum cartão
    let table_rects: Vec<layout::Rect> = schema
        .tables
        .iter()
        .filter_map(|t| {
            let &(x, y) = map.get(&t.qualified_name())?;
            Some(layout::Rect {
                x,
                y,
                w: CARD_W,
                h: card_h(t.columns.len()),
            })
        })
        .collect();
    let max_y = table_rects
        .iter()
        .map(|r| r.y + r.h)
        .fold(0.0_f32, f32::max);
    let enum_y = if schema.tables.is_empty() {
        0.0
    } else {
        max_y + 64.0
    };
    let enum_rects: Vec<layout::Rect> = schema
        .enums
        .iter()
        .enumerate()
        .map(|(i, en)| layout::Rect {
            x: i as f32 * (ENUM_W + 32.0),
            y: enum_y,
            w: ENUM_W,
            h: enum_h(en),
        })
        .collect();
    let placed = layout::place_without_overlap(&table_rects, &enum_rects, PLACE_GAP);
    for (en, p) in schema.enums.iter().zip(placed) {
        map.insert(enum_key(en), (p.x, p.y));
    }
    map
}

/// Largura do cartão de enum em pixels de mundo.
const ENUM_W: f32 = 220.0;
/// Folga mínima entre cartões reposicionados para evitar sobreposição.
const PLACE_GAP: f32 = 40.0;

/// Chave de posição de um enum (`enum:<schema.nome>`), igual à do sidecar da Web.
fn enum_key(en: &schema::EnumDef) -> String {
    format!("enum:{}", en.qualified_name())
}

/// Linha de valor do cartão de enum (v4).
const ENUM_ROW_H: f32 = 26.0;

/// Altura do cartão de enum: cabeçalho + até 6 valores (+ linha "+N a mais") + 6px em cima/embaixo.
fn enum_h(en: &schema::EnumDef) -> f32 {
    // enum sem valores mostra a linha crate::i18n::t("(sem valores)")
    let rows = en.values.len().clamp(1, 6) + usize::from(en.values.len() > 6);
    HEADER_H + rows as f32 * ENUM_ROW_H + 12.
}

/// O canvas renderiza sozinho: arrastar/zoom não redesenha a janela inteira.
impl Render for Diagram {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.glass {
            Some(g) => self.view(&g, window, cx),
            None => div().into_any_element(),
        }
    }
}

impl Diagram {
    /// O que o shell mostra do canvas (barra de status, inspector, lista): só quando
    /// isto muda o shell precisa redesenhar.
    pub fn summary(&self) -> Summary {
        (
            self.selected.clone(),
            self.multi.clone(),
            // O shell persiste a sessão ao mudar o resumo; não grave um arquivo por frame.
            (self.zoom() * 100.).round() as i32,
            self.hint(),
            self.schema.tables.len(),
            self.schema.refs.len() + self.schema.groups.len(),
        )
    }

    pub fn view(&mut self, g: &Glass, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        struct Edge {
            a: Point<Pixels>,
            b: Point<Pixels>,
            sa: f32,
            sb: f32,
            pts: Vec<[Point<Pixels>; 4]>,
            lit: f32,
            dim: f32,
            ends: (&'static str, &'static str),
            /// cor do status no modo Comparar (adicionada/modificada)
            tint: Option<Hsla>,
        }
        const FOCUS_DIM: f32 = 0.18;
        let now = Instant::now();
        if self.advance_motion(now, motion::disabled() || cx.reduce_motion()) {
            window.request_animation_frame();
        }
        if self.needs_fit {
            self.do_fit();
        }
        let z = self.zoom;
        let (pan_x, pan_y) = self.pan;
        // cursor da ação em curso: vale na janela toda (o cartão sob o ponteiro não o troca)
        let drag_cursor = match &self.drag {
            Some(
                Drag::Pan { .. }
                | Drag::Cards { .. }
                | Drag::Group { .. }
                | Drag::Enum { .. }
                | Drag::Edge { .. }
                | Drag::Minimap { .. },
            ) => Some(CursorStyle::ClosedHand),
            Some(Drag::Marquee { .. } | Drag::Link { .. }) => Some(CursorStyle::Crosshair),
            None if self.space => Some(CursorStyle::OpenHand),
            None => None,
        };
        let s = move |v: f32| px(v * z);
        let schema = &self.schema;
        let in_diff = self.diff.is_some();
        let dv: Option<Rc<DiffView>> = self.diff.as_ref().map(|d| d.view.clone());
        let diff_dim = |status: Option<DiffStatus>| {
            if in_diff && status.is_none() {
                DIFF_UNCHANGED_OPACITY
            } else {
                1.0
            }
        };
        let focus_sub: Option<schema::FocusSubgraph> = if self.focus_mode {
            let seeds: std::collections::HashSet<String> = self.selection().into_iter().collect();
            (!seeds.is_empty()).then(|| schema.neighborhood(&seeds, self.focus_depth))
        } else {
            None
        };

        // arestas coluna→coluna: âncoras locais ao canvas + cardinalidade + destaque
        self.validate_routes();
        let edges: Vec<Edge> = schema
            .refs
            .iter()
            .enumerate()
            .filter_map(|(i, r)| {
                let geom = self.ref_geom(i)?;
                let (source_key, target_key, key, _, auto, segs) = &*geom;
                // lado dos rótulos pela curva automática: o estilo não move rótulos/portas
                let [a, c1, c2, b] = *auto;
                let sa = (c1.x - a.x).signum();
                let sb = (c2.x - b.x).signum();
                let pts: Vec<_> = segs
                    .iter()
                    .map(|c| c.map(|p| point(px(p.x * z + pan_x), px(p.y * z + pan_y))))
                    .collect();
                let ends = match r.kind {
                    schema::RefKind::ManyToOne => ("N", "1"),
                    schema::RefKind::OneToMany => ("1", "N"),
                    schema::RefKind::OneToOne => ("1", "1"),
                    schema::RefKind::ManyToMany => ("N", "N"),
                };
                let is_sel_ref = self.is_selected_ref(source_key, r, target_key);
                let lit =
                    is_sel_ref || self.is_selected(source_key) || self.is_selected(target_key);
                let lit = motion::value(
                    SharedString::from(format!("edge-light:{key}")),
                    f32::from(lit),
                    motion::BASE,
                    window,
                    cx,
                );
                let status = dv.as_ref().and_then(|v| v.ref_status(r));
                let dim = match &focus_sub {
                    Some(sub) if !sub.ref_indices.contains(&i) => FOCUS_DIM,
                    _ => 1.0,
                } * diff_dim(status);
                Some(Edge {
                    a: pts.first()?[0],
                    b: pts.last()?[3],
                    sa,
                    sb,
                    pts,
                    lit,
                    dim,
                    ends,
                    tint: status.map(|st| diff_color(g, st)),
                })
            })
            .collect();
        // alça da relação selecionada: ponto da curva em t=0.5
        let handle = self
            .selected_edge()
            .map(|(_, _, _, m)| point(px(m.x * z + pan_x), px(m.y * z + pan_y)));
        // rótulos N/1: 9px para dentro da aresta, acima da linha (centrados numa caixa de 12px)
        let labels: Vec<(Point<Pixels>, &'static str, f32, f32)> = edges
            .iter()
            .flat_map(|e| {
                [(e.a, e.sa, e.ends.0), (e.b, e.sb, e.ends.1)].map(|(p, side, text)| {
                    (
                        point(p.x + px((side * 11. - 6.) * z), p.y - px(17. * z)),
                        text,
                        e.lit,
                        e.dim,
                    )
                })
            })
            .collect();
        // relação sendo arrastada: linha provisória da coluna de origem até o cursor;
        // tracejada em aviso enquanto o alvo é inválido, acento contínuo sobre um alvo válido
        let pending = if let Some(Drag::Link { from, now }) = &self.drag
            && let (Some(t), Some(&(x, y))) = (schema.table(&from.0), self.pos.get(&from.0))
            && let Some(row) = t.columns.iter().position(|c| c.name == from.1)
        {
            let a = point(
                px((x + CARD_W) * z + pan_x),
                px((y + HEADER_H + 4. + ROW_H * (row as f32 + 0.5)) * z + pan_y),
            );
            let b = point(px(now.0 * z + pan_x), px(now.1 * z + pan_y));
            Some((a, b, self.link_target(from, *now).is_some()))
        } else {
            None
        };
        // modo Comparar: relações removidas, retas tracejadas entre as linhas das colunas
        let removed_edges: Vec<(Point<Pixels>, Point<Pixels>)> = self
            .diff
            .as_ref()
            .map(|d| {
                let pos = self.positions();
                let at = |q: &str| pos.get(q).or_else(|| d.ghost_tables.get(q)).copied();
                let row_y = |(_, y): (f32, f32), row: usize| {
                    y + HEADER_H + CARD_ROWS_TOP_PAD + ROW_H * (row as f32 + 0.5)
                };
                d.view
                    .refs
                    .values()
                    .filter(|rd| rd.status == DiffStatus::Removed)
                    .filter_map(|rd| {
                        let (fq, tq) = (rd.r.from.qualified_table(), rd.r.to.qualified_table());
                        let (a, b) = (at(&fq)?, at(&tq)?);
                        let fr = crate::diff_view::column_row(
                            &fq,
                            rd.r.from.primary_column(),
                            &schema.tables,
                            &d.view,
                        )?;
                        let tr = crate::diff_view::column_row(
                            &tq,
                            rd.r.to.primary_column(),
                            &schema.tables,
                            &d.view,
                        )?;
                        let rightward = b.0 >= a.0;
                        let ax = if rightward { a.0 + CARD_W } else { a.0 };
                        let bx = if rightward { b.0 } else { b.0 + CARD_W };
                        let to_screen =
                            |x: f32, y: f32| point(px(x * z + pan_x), px(y * z + pan_y));
                        Some((to_screen(ax, row_y(a, fr)), to_screen(bx, row_y(b, tr))))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let removed_color = g.danger.opacity(DIFF_GHOST_OPACITY);
        let warning = g.warning;
        let stroke = g.accent;
        // Traço das relações inativas.
        let line = g.rel_stroke;
        let surface = g.card;
        let dot = g.grid;
        let entity = cx.entity();
        let mut root = div()
            .id("diagram")
            .role(Role::Group)
            .aria_label(crate::i18n::t("Diagrama"))
            .key_context(actions::CANVAS)
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .overflow_hidden()
            .cursor(match (&self.drag, self.tool) {
                (Some(Drag::Pan { .. }), _) => CursorStyle::ClosedHand,
                (_, Tool::Hand) => CursorStyle::OpenHand,
                _ if self.space => CursorStyle::OpenHand,
                _ => CursorStyle::Arrow,
            })
            // roda: move a vista (Shift = horizontal); Ctrl+roda: zoom no cursor (como no Figma)
            .on_scroll_wheel(cx.listener(|this, e: &ScrollWheelEvent, _, cx| {
                this.viewport_motion = None;
                let d = e.delta.pixel_delta(px(24.));
                let (dx, dy) = (f32::from(d.x), f32::from(d.y));
                if e.modifiers.control || e.modifiers.platform {
                    let cur = this.to_local(e.position);
                    this.zoom_at(cur, (1. + dy * 0.0025).clamp(0.5, 2.));
                } else if e.modifiers.shift && dx == 0. {
                    this.pan.0 += dy;
                } else {
                    this.pan.0 += dx;
                    this.pan.1 += dy;
                }
                cx.notify();
            }))
            // pinça do touchpad: Windows (DirectManipulation), macOS e Wayland mandam PinchEvent, não Ctrl+roda
            .on_pinch(cx.listener(|this, e: &PinchEvent, _, cx| {
                let cur = this.to_local(e.position);
                this.zoom_at(cur, (1. + e.delta).clamp(0.5, 2.));
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "space" && !this.space {
                    this.space = true;
                    cx.notify();
                }
                // setas com relação selecionada: movem a alça 8 (Shift = 1) unidades de mundo
                let step = if e.keystroke.modifiers.shift { 1. } else { 8. };
                let d = match e.keystroke.key.as_str() {
                    "left" => (-step, 0.),
                    "right" => (step, 0.),
                    "up" => (0., -step),
                    "down" => (0., step),
                    _ => return,
                };
                if this.drag.is_none() && this.selected_edge().is_some() {
                    let before = this.edges.clone();
                    this.move_edge(d.0, d.1);
                    cx.emit(DiagramEvent::EdgesChanged(before));
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_key_up(cx.listener(|this, e: &KeyUpEvent, _, cx| {
                if e.keystroke.key == "space" {
                    this.space = false;
                    cx.notify();
                }
            }))
            // botão do meio: mover a vista em qualquer ferramenta
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    this.interrupt_motion();
                    window.focus(&this.focus, cx);
                    this.drag = Some(Drag::Pan { last: e.position });
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    this.interrupt_motion();
                    window.focus(&this.focus, cx);
                    // modo Comparar é somente leitura: arrastar em qualquer lugar move a vista
                    if this.tool == Tool::Hand || this.space || this.diff.is_some() {
                        this.drag = Some(Drag::Pan { last: e.position });
                    } else {
                        let at = this.to_world(e.position);
                        if this.on_edge_handle(at) {
                            this.drag = Some(Drag::Edge {
                                last: at,
                                start: this.edges.clone(),
                            });
                            cx.notify();
                            return;
                        }
                        if let Some(hit_ref) = this.ref_at(at) {
                            this.selected = None;
                            this.multi.clear();
                            this.selected_ref = Some(hit_ref);
                            cx.notify();
                            return;
                        }
                        // arrastar no vazio = seleção por retângulo (Shift soma à seleção)
                        if !e.modifiers.shift {
                            this.selected = None;
                            this.selected_ref = None;
                            this.multi.clear();
                            this.focus_mode = false;
                            this.highlighted_column = None;
                        }
                        this.drag = Some(Drag::Marquee { start: at, now: at });
                    }
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                let now = this.to_world(e.position);
                match &mut this.drag {
                    Some(Drag::Group { name, last }) => {
                        let (dx, dy) = (now.0 - last.0, now.1 - last.1);
                        *last = now;
                        let members = this
                            .schema
                            .groups
                            .iter()
                            .find(|g| &g.name == name)
                            .map(|g| g.tables.clone())
                            .unwrap_or_default();
                        for t in members {
                            if let Some(p) = this.pos.get_mut(&t) {
                                p.0 += dx;
                                p.1 += dy;
                            }
                        }
                    }
                    Some(Drag::Cards { last }) => {
                        // move todas as tabelas selecionadas juntas
                        let (dx, dy) = (now.0 - last.0, now.1 - last.1);
                        *last = now;
                        for t in this.selected.iter().chain(&this.multi) {
                            if let Some(p) = this.pos.get_mut(t) {
                                p.0 += dx;
                                p.1 += dy;
                            }
                        }
                    }
                    Some(Drag::Enum { name, last }) => {
                        let (dx, dy) = (now.0 - last.0, now.1 - last.1);
                        *last = now;
                        let key = this
                            .schema
                            .enums
                            .iter()
                            .find(|e| e.name == *name)
                            .map_or_else(|| format!("enum:{name}"), enum_key);
                        if let Some(p) = this.pos.get_mut(&key) {
                            p.0 += dx;
                            p.1 += dy;
                        }
                    }
                    Some(Drag::Edge { last, .. }) => {
                        let (dx, dy) = (now.0 - last.0, now.1 - last.1);
                        *last = now;
                        this.move_edge(dx, dy);
                    }
                    Some(Drag::Marquee { now: n, .. } | Drag::Link { now: n, .. }) => *n = now,
                    Some(Drag::Pan { last }) => {
                        this.pan.0 += f32::from(e.position.x - last.x);
                        this.pan.1 += f32::from(e.position.y - last.y);
                        *last = e.position;
                    }
                    Some(Drag::Minimap { grab }) => {
                        let grab = *grab;
                        this.pan_from_minimap(e.position, grab);
                    }
                    None => return,
                }
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| this.end_drag(cx)),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(|this, _: &MouseUpEvent, _, cx| this.end_drag(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| this.end_drag(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Middle,
                cx.listener(|this, _: &MouseUpEvent, _, cx| this.end_drag(cx)),
            )
            // botão direito: seleciona a tabela sob o cursor (o menu de contexto lê o alvo)
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    if this.diff.is_some() {
                        return;
                    }
                    this.interrupt_motion();
                    window.focus(&this.focus, cx);
                    let at = this.to_world(e.position);
                    let hit = this.table_at(at).map(|(t, _)| t);
                    if let Some(t) = &hit
                        && !this.is_selected(t)
                    {
                        this.selected = Some(t.clone());
                        this.multi.clear();
                        this.selected_enum = None;
                        this.selected_ref = None;
                        cx.emit(DiagramEvent::Select(t.clone()));
                    }
                    this.ctx_on = if hit.is_some() {
                        CtxOn::Table
                    } else if let Some(r) = this.ref_at(at) {
                        this.selected = None;
                        this.multi.clear();
                        this.selected_enum = None;
                        this.selected_ref = Some(r);
                        CtxOn::Edge
                    } else {
                        CtxOn::Background
                    };
                    this.open_context_menu(e.position, window, cx);
                    cx.notify();
                }),
            );

        root = root.child(
            canvas(
                |_, _, _| (),
                move |bounds, (), window, cx| {
                    // informa origem/tamanho reais ao estado (usados por zoom e fit)
                    entity.update(cx, |d, cx| {
                        if d.origin != bounds.origin || d.size != bounds.size {
                            d.resize_viewport(bounds.origin, bounds.size);
                            cx.notify();
                        }
                        // ajuste à tela assim que o tamanho real é conhecido (avisa o shell)
                        if d.needs_fit {
                            d.do_fit();
                            cx.notify();
                        }
                    });
                    if let Some(c) = drag_cursor {
                        window.set_window_cursor_style(c);
                    }
                    // grade de pontos que acompanha pan/zoom (some quando fica densa demais)
                    // passo mínimo de 20 px: grade densa custava ~10 mil quads por quadro
                    let mut step = 24. * z;
                    while step < 20. {
                        step *= 2.;
                    }
                    let (ox, oy) = (pan_x.rem_euclid(step), pan_y.rem_euclid(step));
                    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                    let mut y = oy;
                    while y < h {
                        let mut x = ox;
                        while x < w {
                            let p = bounds.origin + point(px(x - 0.75), px(y - 0.75));
                            window.paint_quad(fill(Bounds::new(p, size(px(1.5), px(1.5))), dot));
                            x += step;
                        }
                        y += step;
                    }
                    for e in edges {
                        let (a, b) = (bounds.origin + e.a, bounds.origin + e.b);
                        let pts: Vec<_> =
                            e.pts.iter().map(|c| c.map(|p| bounds.origin + p)).collect();
                        let color = e
                            .tint
                            .unwrap_or_else(|| motion::color(line, stroke, e.lit))
                            .opacity(e.dim);
                        // Halo sob o traço da relação ativa.
                        if e.lit > 0.
                            && let Some(path) = bezier(&pts, 8. * z)
                        {
                            window.paint_path(path, stroke.opacity(0.16 * e.lit * e.dim));
                        }
                        let w = (1.4 + 0.4 * e.lit) * z.max(0.5);
                        if let Some(path) = bezier(&pts, w) {
                            window.paint_path(path, color);
                        }
                        // ponto cheio na origem, anel (fundo do canvas) no destino
                        let r = (3. * z).max(2.);
                        window.paint_quad(
                            fill(
                                Bounds::new(a - point(px(r), px(r)), size(px(2. * r), px(2. * r))),
                                color,
                            )
                            .corner_radii(px(r)),
                        );
                        let r = (3.5 * z).max(2.5);
                        window.paint_quad(
                            fill(
                                Bounds::new(b - point(px(r), px(r)), size(px(2. * r), px(2. * r))),
                                surface,
                            )
                            .corner_radii(px(r))
                            .border_widths(px((1.5 * z).max(1.)))
                            .border_color(color),
                        );
                    }
                    for (a, b) in removed_edges {
                        let mut pb = PathBuilder::stroke(px(DIFF_EDGE_W_PX));
                        pb = pb.dash_array(&[px(DIFF_DASH_PX), px(DIFF_DASH_GAP_PX)]);
                        pb.move_to(bounds.origin + a);
                        pb.line_to(bounds.origin + b);
                        if let Ok(path) = pb.build() {
                            window.paint_path(path, removed_color);
                        }
                    }
                    if let Some(h) = handle {
                        // círculo de 10 px (tamanho fixo na tela): acento com aro da cor do cartão
                        let r = 5.;
                        window.paint_quad(
                            fill(
                                Bounds::new(
                                    bounds.origin + h - point(px(r), px(r)),
                                    size(px(2. * r), px(2. * r)),
                                ),
                                stroke,
                            )
                            .corner_radii(px(r))
                            .border_widths(px(2.))
                            .border_color(surface),
                        );
                    }
                    if let Some((a, b, valid)) = pending {
                        let (a, b) = (bounds.origin + a, bounds.origin + b);
                        let mut pb = PathBuilder::stroke(px(1.6));
                        if !valid {
                            pb = pb.dash_array(&[px(5.), px(4.)]);
                        }
                        pb.move_to(a);
                        pb.line_to(b);
                        let color = if valid { stroke } else { warning };
                        if let Ok(path) = pb.build() {
                            window.paint_path(path, color);
                        }
                        let r = (3. * z).max(2.);
                        let at = a - point(px(r), px(r));
                        window.paint_quad(
                            fill(Bounds::new(at, size(px(2. * r), px(2. * r))), color)
                                .corner_radii(px(r)),
                        );
                    }
                },
            )
            .absolute()
            .size_full(),
        );

        for (at, text, lit, dim) in labels {
            root = root.child(
                div()
                    .absolute()
                    .left(at.x)
                    .top(at.y)
                    .w(s(12.))
                    .text_center()
                    .text_size(s(10.))
                    .font_family(FONT_MONO)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(motion::color(g.text_faint, g.accent, lit))
                    .opacity(dim)
                    .child(text),
            );
        }

        // molduras de grupo (abaixo dos cartões): caixa que envolve as tabelas do grupo
        let dark = g.mode == Mode::Dark;
        for gr in &schema.groups {
            let members: Vec<_> = gr
                .tables
                .iter()
                .filter_map(|t| Some((self.pos.get(t)?, schema.table(t)?)))
                .collect();
            if members.is_empty() {
                continue;
            }
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for ((x, y), t) in &members {
                x0 = x0.min(*x);
                y0 = y0.min(*y);
                x1 = x1.max(x + CARD_W);
                y1 = y1.max(y + card_h(t.columns.len()));
            }
            let pad = GROUP_PAD - 8.;
            let c = gr
                .color
                .as_deref()
                .and_then(hex)
                .unwrap_or_else(|| group_color(schema, &gr.name));
            let group_dim = match &focus_sub {
                Some(sub)
                    if !members
                        .iter()
                        .any(|(_, t)| sub.tables.contains(&t.qualified_name())) =>
                {
                    FOCUS_DIM
                }
                _ => 1.0,
            };
            let name = gr.name.clone();
            root = root.child(
                div()
                    .id(SharedString::from(format!("group-{}", gr.name)))
                    .role(Role::Group)
                    .aria_label(crate::i18n::t("Grupo {}").replace("{}", &gr.name))
                    .absolute()
                    .left(s(x0 - pad) + px(pan_x))
                    .top(s(y0 - pad - 8.) + px(pan_y))
                    .w(s(x1 - x0 + 2. * pad))
                    .h(s(y1 - y0 + 2. * pad + 8.))
                    .rounded(s(20.))
                    .border_1()
                    .border_color(c.opacity(if dark { 0.18 } else { 0.22 }))
                    .bg(c.opacity(if dark { 0.06 } else { 0.07 }))
                    .opacity(group_dim)
                    // etiqueta em pílula: ponto, nome, contagem
                    .child(
                        div()
                            .id(SharedString::from(format!("group-label-{}", gr.name)))
                            .absolute()
                            .top(s(12.))
                            .left(s(16.))
                            .h(s(20.))
                            .flex()
                            .items_center()
                            .gap(s(6.))
                            .text_size(s(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(c)
                            .when(self.tool == Tool::Select, gpui_kit::Styled::cursor_move)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                                    if this.tool == Tool::Hand || this.diff.is_some() {
                                        return;
                                    }
                                    this.interrupt_motion();
                                    window.focus(&this.focus, cx);
                                    let (lx, ly) = this.to_local(e.position);
                                    let at = (
                                        (lx - this.pan.0) / this.zoom,
                                        (ly - this.pan.1) / this.zoom,
                                    );
                                    this.drag_start_pos = Some(this.pos.clone());
                                    this.drag = Some(Drag::Group {
                                        name: name.clone(),
                                        last: at,
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            )
                            .child(Icon::new(IconName::Layers).size(s(13.)))
                            .child(gr.name.clone())
                            .child(
                                div()
                                    .font_family(FONT_MONO)
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_size(s(10.5))
                                    .px(s(6.))
                                    .rounded(s(9.))
                                    .bg(c.opacity(0.14))
                                    .child(members.len().to_string()),
                            ),
                    ),
            );
        }

        // colunas que são origem de alguma relação (FK)
        let fks: std::collections::HashSet<(String, &str)> = schema
            .refs
            .iter()
            .map(|r| (r.from.qualified_table(), r.from.column.as_str()))
            .collect();
        // selo IDX: ciano da paleta (hue 200 do protótipo)
        let idx_color = hex(TABLE_COLORS[1].1).unwrap_or(g.link);
        let mut order: Vec<_> = schema.tables.iter().collect();
        order.sort_by_key(|t| self.top.as_deref() == Some(t.qualified_name().as_str()));
        let (vw, vh) = (f32::from(self.size.width), f32::from(self.size.height));
        for t in order {
            let qname = t.qualified_name();
            let (x, y) = self
                .pos
                .get(&qname)
                .copied()
                .unwrap_or(crate::constants::DEFAULT_CARD_POS);
            // fora da vista (com margem): não monta o cartão
            let (sx, sy) = (x * z + pan_x, y * z + pan_y);
            if vw > 0.
                && (sx > vw + 80.
                    || sy > vh + 80.
                    || sx + CARD_W * z < -80.
                    || sy + card_h(t.columns.len()) * z < -80.)
            {
                continue;
            }
            let name = qname.clone();
            let on = motion::value(
                SharedString::from(format!("table-selection:{qname}")),
                f32::from(self.is_selected(&qname)),
                motion::BASE,
                window,
                cx,
            );
            let dragging = match &self.drag {
                Some(Drag::Cards { .. }) => self.is_selected(&qname),
                Some(Drag::Group { name, .. }) => schema
                    .groups
                    .iter()
                    .any(|g| &g.name == name && g.tables.contains(&qname)),
                _ => false,
            };
            let lift = motion::value(
                SharedString::from(format!("table-lift:{qname}")),
                f32::from(dragging),
                motion::FAST,
                window,
                cx,
            );
            let appear = self.appearing.get(&qname).map_or(1., |t| t.sample(now)[0]);
            let scale = 0.96 + 0.04 * appear;
            let s = |v: f32| px(v * z * scale);
            // cor: a da tabela (headercolor) ou a do grupo
            let grp = schema.group_of(&qname);
            let tint = t
                .color
                .as_deref()
                .or_else(|| grp.and_then(|gr| gr.color.as_deref()))
                .and_then(hex)
                .or_else(|| grp.map(|gr| group_color(schema, &gr.name)));
            let table_status = dv.as_ref().and_then(|v| v.table_status(&qname));
            let mark = |status: DiffStatus, k: f32| {
                div()
                    .flex_none()
                    .w(px(DIFF_MARK_W_PX * k))
                    .text_center()
                    .font_family(FONT_MONO)
                    .font_weight(FontWeight::BOLD)
                    .text_color(diff_color(g, status))
                    .child(status.mark())
            };
            let mut rows: Vec<AnyElement> = Vec::new();
            for (i, c) in t.columns.iter().enumerate() {
                let col_status = dv.as_ref().and_then(|v| v.column_status(&qname, &c.name));
                let fk = c.fk || fks.contains(&(qname.clone(), c.name.as_str()));
                let has_idx = t
                    .indexes
                    .iter()
                    .any(|ix| ix.references_column(c.name.as_str()));
                let has_def = c.default_value.is_some();
                let has_chk = c.check.is_some();
                let col_note = c
                    .note
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned);
                let is_hi_col = self
                    .highlighted_column
                    .as_ref()
                    .is_some_and(|(tb, cl)| tb == &qname && cl == &c.name);
                let icon = if c.pk {
                    Some(Icon::new(IconName::KeyRound).text_color(g.accent))
                } else if fk {
                    Some(Icon::new(IconName::Link).text_color(g.link))
                } else {
                    Some(Icon::new(IconName::Dot).text_color(g.text_faint))
                };
                let k = z;
                let neutral = |label| badge(label, gpui_kit::transparent_black(), g.text_faint, k);
                let from = (qname.clone(), c.name.clone());
                let note_col_id = SharedString::from(format!("col-note-{qname}-{}", c.name));
                rows.push(
                    div()
                        .id(("col", i))
                        .group("col")
                        .relative()
                        .h(s(ROW_H))
                        .px(s(12.))
                        .flex()
                        .items_center()
                        .gap(s(8.))
                        .text_size(s(13.))
                        .when(is_hi_col, |d| d.bg(g.accent.opacity(0.14)))
                        .when_some(col_status, |d, st| {
                            d.bg(diff_color(g, st).opacity(DIFF_ROW_TINT))
                        })
                        .when(!in_diff, |d| {
                            d.hover({
                                let h = g.hover;
                                move |st| st.bg(h)
                            })
                        })
                        // trilho fixo do ícone: nomes alinhados com ou sem ícone
                        .child(
                            div()
                                .w(s(14.))
                                .flex()
                                .justify_center()
                                .children(icon.map(|i| i.size(s(12.)))),
                        )
                        .child(
                            div()
                                .flex_1()
                                .truncate()
                                .text_size(s(12.5))
                                .text_color(g.text)
                                .when(c.pk, |d| d.font_weight(FontWeight::MEDIUM))
                                .child(c.name.clone()),
                        )
                        .when_some(col_note, |d, note_text| {
                            d.child(
                                div()
                                    .id(note_col_id)
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .tooltip(move |window, cx| {
                                        Tooltip::new(note_text.clone()).build(window, cx)
                                    })
                                    .child(
                                        Icon::new(IconName::Info)
                                            .size(s(11.))
                                            .text_color(g.text_muted),
                                    ),
                            )
                        })
                        .when(has_idx, |d| {
                            d.child(badge("IDX", gpui_kit::transparent_black(), idx_color, k))
                        })
                        .when(c.not_null, |d| d.child(neutral("NN")))
                        .when(c.unique, |d| d.child(neutral("UQ")))
                        .when(has_def, |d| d.child(neutral("DEF")))
                        .when(has_chk, |d| d.child(neutral("CHK")))
                        .child(
                            div()
                                .text_size(s(11.))
                                .font_family(FONT_MONO)
                                .text_color(g.text_faint)
                                .child(c.ty.clone()),
                        )
                        .when_some(col_status, |d, st| d.child(mark(st, k)))
                        // porta: arraste até outra coluna para criar a relação
                        .child(
                            div()
                                .id(("port", i))
                                .absolute()
                                .right(s(-6.))
                                .top(s(ROW_H / 2. - 6.))
                                .size(s(12.))
                                .rounded_full()
                                .border_2()
                                .border_color(g.accent)
                                .bg(g.card)
                                .invisible()
                                .when(!in_diff, |d| {
                                    d.group_hover("col", gpui_kit::Styled::visible)
                                        .cursor_crosshair()
                                })
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                                        if this.diff.is_some() {
                                            return;
                                        }
                                        this.interrupt_motion();
                                        window.focus(&this.focus, cx);
                                        let now = this.to_world(e.position);
                                        this.drag = Some(Drag::Link {
                                            from: from.clone(),
                                            now,
                                        });
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                ),
                        )
                        .into_any_element(),
                );
            }
            // modo Comparar: colunas removidas no fim do cartão (não deslocam as arestas)
            let ghost_cols = dv
                .as_ref()
                .and_then(|v| v.tables.get(&qname))
                .map(|d| d.ghost_columns.as_slice())
                .unwrap_or_default();
            for c in ghost_cols {
                rows.push(
                    ghost_row(g, &c.name, &c.ty, z * scale)
                        .child(mark(DiffStatus::Removed, z))
                        .into_any_element(),
                );
            }
            let n_rows = t.columns.len() + ghost_cols.len();
            let table_dim = match &focus_sub {
                Some(sub) if !sub.tables.contains(&qname) => FOCUS_DIM,
                _ => 1.0,
            } * diff_dim(table_status);
            let table_note = t
                .note
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
            let table_note_id = SharedString::from(format!("tbl-note-{qname}"));
            root = root.child(
                div()
                    .id(SharedString::from(format!("table-{qname}")))
                    .role(Role::Group)
                    .aria_label(crate::i18n::t("Tabela {}").replace("{}", &qname))
                    .absolute()
                    .left(px(sx + CARD_W * z * (1. - scale) / 2.))
                    .top(px(sy + card_h(n_rows) * z * (1. - scale) / 2. - 3. * lift))
                    .w(s(CARD_W))
                    .h(s(card_h(n_rows)))
                    .rounded(s(CARD_R))
                    .group("card")
                    .border_1()
                    .border_color(table_status.map_or_else(
                        || motion::color(g.edge_soft, g.accent, on),
                        |st| diff_color(g, st),
                    ))
                    .opacity(appear * table_dim)
                    // sombra suave sempre; selecionado ganha o anel do destaque
                    .shadow({
                        let mut shadow = card_shadow(g.mode);
                        shadow.offset.y += px(4. * lift);
                        shadow.blur_radius += px(8. * lift);
                        shadow.color = shadow.color.opacity(appear.powi(3));
                        let mut sh = vec![shadow];
                        if on > 0. {
                            let mut halo = sel_ring(g);
                            halo.color = halo.color.opacity(on * appear.powi(3));
                            sh.push(halo);
                        }
                        if let Some(st) = table_status {
                            sh.push(diff_halo(diff_color(g, st)));
                        }
                        sh
                    })
                    .bg(g.card)
                    .when(
                        self.tool == Tool::Select && !in_diff,
                        gpui_kit::Styled::cursor_move,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                            // modo Comparar: sem seleção/arrasto; o fundo move a vista
                            if this.tool == Tool::Hand || this.space || this.diff.is_some() {
                                return;
                            }
                            this.interrupt_motion();
                            window.focus(&this.focus, cx);
                            if e.click_count >= 2 {
                                // duplo clique: renomear (o inspector abre o campo de nome)
                                this.selected = Some(name.clone());
                                this.multi.clear();
                                cx.emit(DiagramEvent::Rename(name.clone()));
                                cx.stop_propagation();
                                cx.notify();
                                return;
                            }
                            let (lx, ly) = this.to_local(e.position);
                            let (px_, py_) = this
                                .pos
                                .get(&name)
                                .copied()
                                .unwrap_or(crate::constants::DEFAULT_CARD_POS);
                            let grab = (
                                (lx - this.pan.0) / this.zoom - px_,
                                (ly - this.pan.1) / this.zoom - py_,
                            );
                            this.top = Some(name.clone());
                            if e.modifiers.shift {
                                // Shift+clique: adiciona/remove da seleção
                                if this.selected.is_none() {
                                    this.selected = Some(name.clone());
                                } else if this.selected.as_deref() == Some(name.as_str()) {
                                    this.selected = this.multi.pop();
                                } else if let Some(i) = this.multi.iter().position(|m| *m == name) {
                                    this.multi.remove(i);
                                } else {
                                    this.multi.push(name.clone());
                                }
                            } else if !this.is_selected(&name) {
                                this.selected = Some(name.clone());
                                this.multi.clear();
                            }
                            this.selected_enum = None;
                            this.selected_ref = None;
                            cx.emit(DiagramEvent::Select(name.clone()));
                            let _ = grab;
                            this.drag_start_pos = Some(this.pos.clone());
                            this.drag = Some(Drag::Cards {
                                last: this.to_world(e.position),
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    // Cabeçalho na geometria compartilhada do layout.
                    .child(
                        div()
                            .relative()
                            .h(s(HEADER_H))
                            .px(s(12.))
                            .flex()
                            .items_center()
                            .gap(s(9.))
                            .border_b_1()
                            .border_color(g.edge_soft)
                            .child(
                                Icon::new(IconName::Table2)
                                    .size(s(14.))
                                    .text_color(tint.unwrap_or(g.accent)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .truncate()
                                    .text_size(s(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(g.text)
                                    .child(t.name.clone()),
                            )
                            .when_some(t.schema.as_ref(), |d, sch| {
                                d.child(
                                    div()
                                        .px(s(5.))
                                        .rounded(s(4.))
                                        .bg(g.selected)
                                        .text_size(s(10.))
                                        .text_color(g.text_muted)
                                        .child(sch.clone()),
                                )
                            })
                            .when_some(table_note, |d, note_text| {
                                d.child(
                                    div()
                                        .id(table_note_id)
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .tooltip(move |window, cx| {
                                            Tooltip::new(note_text.clone()).build(window, cx)
                                        })
                                        .child(
                                            Icon::new(IconName::Info)
                                                .size(s(12.))
                                                .text_color(g.text_muted),
                                        ),
                                )
                            })
                            .child(
                                div()
                                    .min_w(s(18.))
                                    .h(s(18.))
                                    .px(s(4.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(s(10.5))
                                    .font_family(FONT_MONO)
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(g.text_faint)
                                    .child(t.columns.len().to_string()),
                            )
                            .when_some(table_status, |d, st| d.child(mark(st, z))),
                    )
                    .child(div().pt(s(CARD_ROWS_TOP_PAD)).children(rows))
                    .when(!in_diff, |d| {
                        d.child({
                            let table = qname.clone();
                            div()
                                .id(SharedString::from(format!("addcol-{qname}")))
                                .role(Role::Button)
                                .aria_label(crate::i18n::t("Adicionar coluna"))
                                .absolute()
                                .bottom(s(-11.))
                                .left(s(CARD_W / 2. - 11.))
                                .size(s(22.))
                                .rounded_full()
                                .border_1()
                                .border_color(g.edge)
                                .bg(g.card)
                                .flex()
                                .items_center()
                                .justify_center()
                                .invisible()
                                .group_hover("card", gpui_kit::Styled::visible)
                                .hover({
                                    let a = g.accent;
                                    move |st| st.border_color(a)
                                })
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.emit(DiagramEvent::AddColumn(table.clone()));
                                }))
                                .child(
                                    Icon::new(IconName::Plus)
                                        .size(s(12.))
                                        .text_color(g.text_muted),
                                )
                        })
                    }),
            );
        }
        let selected_table = self.selected().cloned();
        for en in &self.schema.enums {
            let key = enum_key(en);
            let Some(&(x, y)) = self.pos.get(&key) else {
                continue;
            };
            let is_sel = self.selected_enum.as_deref() == Some(en.name.as_str());
            let is_ref = selected_table.as_ref().is_some_and(|t| {
                t.columns
                    .iter()
                    .any(|c| c.ty.rsplit('.').next().is_some_and(|ty| ty == en.name))
            });
            let on = motion::value(
                SharedString::from(format!("enum-selection:{key}")),
                f32::from(is_sel || is_ref),
                motion::BASE,
                window,
                cx,
            );
            let selected = motion::value(
                SharedString::from(format!("enum-halo:{key}")),
                f32::from(is_sel),
                motion::BASE,
                window,
                cx,
            );
            let dragging = matches!(&self.drag, Some(Drag::Enum { name, .. }) if *name == en.name);
            let lift = motion::value(
                SharedString::from(format!("enum-lift:{key}")),
                f32::from(dragging),
                motion::FAST,
                window,
                cx,
            );
            let appear = self.appearing.get(&key).map_or(1., |t| t.sample(now)[0]);
            let scale = 0.96 + 0.04 * appear;
            let s = |v: f32| px(v * z * scale);
            let en_name = en.name.clone();
            let shown = en.values.iter().take(6);
            let overflow = en.values.len().saturating_sub(6);
            let enum_dim = match &focus_sub {
                Some(sub) => {
                    let used_in_sub = schema.tables.iter().any(|t| {
                        sub.tables.contains(&t.qualified_name())
                            && t.columns
                                .iter()
                                .any(|c| c.ty.rsplit('.').next().is_some_and(|ty| ty == en.name))
                    });
                    if used_in_sub { 1.0 } else { FOCUS_DIM }
                }
                None => 1.0,
            };
            let enum_diff = dv.as_ref().and_then(|v| v.enums.get(&en.name));
            let enum_status = enum_diff.map(|d| d.status);
            let enum_dim = enum_dim * diff_dim(enum_status);
            let value_status = |name: &str| enum_diff.and_then(|d| d.values.get(name).copied());
            let ghost_values = enum_diff
                .map(|d| d.ghost_values.as_slice())
                .unwrap_or_default();
            let card = div()
                .id(SharedString::from(format!("enum-card-{}", en.name)))
                .absolute()
                .left(px(x * z + pan_x + ENUM_W * z * (1. - scale) / 2.))
                .top(px(
                    y * z + pan_y + enum_h(en) * z * (1. - scale) / 2. - 3. * lift
                ))
                .w(s(ENUM_W))
                .rounded(s(CARD_R))
                .border_1()
                .border_color(enum_status.map_or_else(
                    || motion::color(g.edge, g.accent, on),
                    |st| diff_color(g, st),
                ))
                .opacity(appear * enum_dim)
                .shadow({
                    let mut shadow = card_shadow(g.mode);
                    shadow.offset.y += px(4. * lift);
                    shadow.blur_radius += px(8. * lift);
                    shadow.color = shadow.color.opacity(appear.powi(3));
                    let mut sh = vec![shadow];
                    if selected > 0. {
                        let mut halo = sel_ring(g);
                        halo.color = halo.color.opacity(selected * appear.powi(3));
                        sh.push(halo);
                    }
                    if let Some(st) = enum_status {
                        sh.push(diff_halo(diff_color(g, st)));
                    }
                    sh
                })
                .bg(g.card)
                .when(
                    self.tool == Tool::Select && !in_diff,
                    gpui_kit::Styled::cursor_move,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                        if this.tool == Tool::Hand || this.space || this.diff.is_some() {
                            return;
                        }
                        this.interrupt_motion();
                        window.focus(&this.focus, cx);
                        this.select_enum(&en_name);
                        cx.emit(DiagramEvent::SelectEnum(en_name.clone()));
                        this.drag_start_pos = Some(this.pos.clone());
                        this.drag = Some(Drag::Enum {
                            name: en_name.clone(),
                            last: this.to_world(e.position),
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .h(s(HEADER_H))
                        .px(s(12.))
                        .flex()
                        .items_center()
                        .gap(s(9.))
                        .rounded_t(s(CARD_R - 1.))
                        .border_b_1()
                        .border_color(g.edge_soft)
                        .child(Icon::new(IconName::List).size(s(14.)).text_color(g.accent))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(s(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(g.text)
                                .child(en.name.clone()),
                        )
                        .child(badge(
                            "ENUM",
                            gpui_kit::transparent_black(),
                            g.text_faint,
                            z,
                        )),
                )
                .child(
                    div()
                        .py(s(6.))
                        .px(s(14.))
                        .flex()
                        .flex_col()
                        .font_family(FONT_MONO)
                        .text_size(s(12.5))
                        .text_color(g.text_muted)
                        .children(shown.map(|v| {
                            let st = value_status(&v.name);
                            div()
                                .h(s(ENUM_ROW_H))
                                .flex()
                                .items_center()
                                .gap(s(9.))
                                .when_some(st, |d, st| {
                                    d.bg(diff_color(g, st).opacity(DIFF_ROW_TINT))
                                })
                                .child(Icon::new(IconName::Dot).size(s(14.)).text_color(g.accent))
                                .child(div().flex_1().truncate().child(v.name.clone()))
                                .when_some(st, |d, st| {
                                    d.child(
                                        div()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(diff_color(g, st))
                                            .child(st.mark()),
                                    )
                                })
                        }))
                        .children(ghost_values.iter().map(|name| {
                            div()
                                .h(s(ENUM_ROW_H))
                                .flex()
                                .items_center()
                                .gap(s(9.))
                                .opacity(DIFF_GHOST_OPACITY)
                                .bg(g.danger.opacity(DIFF_ROW_TINT))
                                .child(Icon::new(IconName::Dot).size(s(14.)).text_color(g.danger))
                                .child(div().flex_1().truncate().line_through().child(name.clone()))
                                .child(
                                    div()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(g.danger)
                                        .child(DiffStatus::Removed.mark()),
                                )
                        }))
                        .when(en.values.is_empty(), |d| {
                            d.child(
                                div()
                                    .h(s(ENUM_ROW_H))
                                    .flex()
                                    .items_center()
                                    .text_color(g.text_faint)
                                    .child(crate::i18n::t("(sem valores)")),
                            )
                        })
                        .when(overflow > 0, |d| {
                            d.child(
                                div()
                                    .h(s(ENUM_ROW_H))
                                    .flex()
                                    .items_center()
                                    .text_color(g.text_faint)
                                    .child(
                                        crate::i18n::t("+{} a mais")
                                            .replace("{}", &overflow.to_string()),
                                    ),
                            )
                        }),
                );
            root = root.child(card);
        }
        // modo Comparar: cartões fantasma (translúcidos) do que só existe no HEAD
        if let Some(d) = &self.diff {
            let ghost_card = |id: String, (x, y): (f32, f32), icon: IconName, name: &str| {
                div()
                    .id(SharedString::from(id))
                    .role(Role::Group)
                    .aria_label(crate::i18n::t("Removido desde o HEAD: {}").replace("{}", name))
                    .absolute()
                    .left(px(x * z + pan_x))
                    .top(px(y * z + pan_y))
                    .rounded(s(CARD_R))
                    .border_1()
                    .border_color(g.danger)
                    .bg(g.card)
                    .opacity(DIFF_GHOST_OPACITY)
                    .child(
                        div()
                            .h(s(HEADER_H))
                            .px(s(12.))
                            .flex()
                            .items_center()
                            .gap(s(9.))
                            .border_b_1()
                            .border_color(g.edge_soft)
                            .child(Icon::new(icon).size(s(14.)).text_color(g.danger))
                            .child(
                                div()
                                    .flex_1()
                                    .truncate()
                                    .text_size(s(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(g.text)
                                    .line_through()
                                    .child(name.to_owned()),
                            )
                            .child(
                                div()
                                    .font_family(FONT_MONO)
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(g.danger)
                                    .child(DiffStatus::Removed.mark()),
                            ),
                    )
            };
            for t in &d.view.removed_tables {
                let qname = t.qualified_name();
                let Some(&p) = d.ghost_tables.get(&qname) else {
                    continue;
                };
                root = root.child(
                    ghost_card(
                        format!("{ID_DIFF_GHOST_PREFIX}-table-{qname}"),
                        p,
                        IconName::Table2,
                        &qname,
                    )
                    .w(s(CARD_W))
                    .h(s(card_h(t.columns.len())))
                    .child(
                        div()
                            .pt(s(CARD_ROWS_TOP_PAD))
                            .children(t.columns.iter().map(|c| ghost_row(g, &c.name, &c.ty, z))),
                    ),
                );
            }
            for en in &d.view.removed_enums {
                let Some(&p) = d.ghost_enums.get(&en.name) else {
                    continue;
                };
                root = root.child(
                    ghost_card(
                        format!("{ID_DIFF_GHOST_PREFIX}-enum-{}", en.name),
                        p,
                        IconName::List,
                        &en.name,
                    )
                    .w(s(ENUM_W))
                    .child(
                        div()
                            .pt(s(CARD_ROWS_TOP_PAD))
                            .children(en.values.iter().map(|v| ghost_row(g, &v.name, "", z))),
                    ),
                );
            }
        }
        if let Some(Drag::Marquee { start, now }) = &self.drag {
            let (x0, y0) = (start.0.min(now.0), start.1.min(now.1));
            let (w, h) = ((start.0 - now.0).abs(), (start.1 - now.1).abs());
            root = root.child(
                div()
                    .absolute()
                    .left(px(x0 * z + pan_x))
                    .top(px(y0 * z + pan_y))
                    .w(px(w * z))
                    .h(px(h * z))
                    .border_1()
                    .border_color(g.accent)
                    .bg(g.accent.opacity(0.08)),
            );
        }
        root = root.children(self.overlays(g, window, cx));
        if let Some(d) = &self.diff {
            root = root.child(Self::diff_panel(d, g, cx));
        }
        if let Some((menu, position)) = &self.context_menu {
            root = root.child(
                deferred(
                    anchored().child(
                        div()
                            .w(window.bounds().size.width)
                            .h(window.bounds().size.height)
                            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                            .child(
                                anchored()
                                    .position(*position)
                                    .snap_to_window_with_margin(px(8.))
                                    .child(motion::enter(
                                        ("context-menu-enter", menu.entity_id()),
                                        div().relative().child(menu.clone()),
                                    )),
                            ),
                    ),
                )
                .with_priority(gpui_kit::base::POPUP_PRIORITY),
            );
        }
        root.into_any_element()
    }
}

impl Diagram {
    /// Painel de resumo do modo Comparar: contagens, itens agrupados (clique centraliza a vista),
    /// estado vazio ("sem diferenças") e de erro (sem base no git).
    fn diff_panel(d: &DiffSession, g: &Glass, cx: &mut Context<Self>) -> Stateful<Div> {
        let t = crate::i18n::t;
        let summary = d.view.summary;
        let counts = [
            (DiffStatus::Added, summary.added),
            (DiffStatus::Modified, summary.modified),
            (DiffStatus::Removed, summary.removed),
        ];
        let header = div()
            .flex()
            .items_center()
            .gap(px(DIFF_PANEL_PAD_PX))
            .child(
                div()
                    .flex_1()
                    .text_size(TEXT_SM)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(g.text)
                    .child(t("Comparar com HEAD")),
            )
            .when(d.base.ready().is_some(), |h| {
                h.children(counts.map(|(st, n)| {
                    div()
                        .font_family(FONT_MONO)
                        .text_size(TEXT_XS)
                        .text_color(diff_color(g, st))
                        .child(format!("{}{n}", st.mark()))
                }))
            })
            .child(
                div()
                    .id(ID_DIFF_EXIT)
                    .role(Role::Button)
                    .aria_label(t("Sair do modo Comparar"))
                    .px(px(DIFF_PANEL_PAD_PX / 2.))
                    .rounded(px(DIFF_PANEL_PAD_PX / 2.))
                    .text_size(TEXT_XS)
                    .text_color(g.text_muted)
                    .cursor_pointer()
                    .hover(|s| s.bg(g.hover))
                    .tooltip(|window, cx| {
                        Tooltip::new(crate::i18n::t("Sair do modo Comparar"))
                            .action(&ToggleDiffMode, Some(actions::CTX))
                            .build(window, cx)
                    })
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(ToggleDiffMode), cx);
                    })
                    .child("Esc"),
            );
        let message = |text: &'static str, color: Hsla| {
            div()
                .py(px(DIFF_PANEL_PAD_PX))
                .text_size(TEXT_SM)
                .text_color(color)
                .child(t(text))
        };
        let body = match &d.base {
            BaseState::Loading(_) => {
                message("Lendo o arquivo no HEAD do git…", g.text_muted).into_any_element()
            }
            BaseState::Failed(msg) => message(msg, g.danger).into_any_element(),
            BaseState::Ready(_) if d.view.is_empty() => {
                message("Sem diferenças em relação ao HEAD", g.text_muted).into_any_element()
            }
            BaseState::Ready(_) => {
                let mut list = div()
                    .id(ID_DIFF_LIST)
                    .flex()
                    .flex_col()
                    .max_h(px(DIFF_PANEL_MAX_H_PX))
                    .overflow_y_scroll();
                for group in DiffGroup::ALL {
                    let n = d.view.group_count(group);
                    if n == 0 {
                        continue;
                    }
                    list = list.child(
                        div()
                            .pt(px(DIFF_PANEL_PAD_PX))
                            .text_size(TEXT_XS)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(g.text_faint)
                            .child(format!("{} · {n}", t(diff_group_label(group)))),
                    );
                    for (ix, item) in d
                        .view
                        .items
                        .iter()
                        .enumerate()
                        .filter(|(_, i)| i.group == group)
                    {
                        let color = diff_color(g, item.status);
                        let current = d.cursor == Some(ix);
                        list = list.child(
                            div()
                                .id((ID_DIFF_ITEM_PREFIX, ix))
                                .role(Role::Button)
                                .aria_label(format!("{} {}", item.status.mark(), item.label))
                                .h(px(DIFF_ITEM_H_PX))
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap(px(DIFF_PANEL_PAD_PX / 2.))
                                .px(px(DIFF_PANEL_PAD_PX / 2.))
                                .rounded(px(DIFF_PANEL_PAD_PX / 2.))
                                .cursor_pointer()
                                .when(current, |r| r.bg(g.selected))
                                .hover(|s| s.bg(g.hover))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.focus_diff_item(ix);
                                    cx.notify();
                                }))
                                .child(
                                    div()
                                        .flex_none()
                                        .w(px(DIFF_MARK_W_PX))
                                        .text_center()
                                        .font_family(FONT_MONO)
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(color)
                                        .child(item.status.mark()),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .font_family(FONT_MONO)
                                        .text_size(TEXT_XS)
                                        .text_color(g.text)
                                        .child(item.label.clone()),
                                )
                                .when(!item.detail.is_empty(), |r| {
                                    r.child(
                                        div()
                                            .flex_none()
                                            .max_w(px(DIFF_PANEL_W_PX / 2.))
                                            .truncate()
                                            .font_family(FONT_MONO)
                                            .text_size(TEXT_XS)
                                            .text_color(g.text_faint)
                                            .child(item.detail.clone()),
                                    )
                                }),
                        );
                    }
                }
                list.into_any_element()
            }
        };
        div()
            .id(ID_DIFF_PANEL)
            .role(Role::Region)
            .aria_label(t("Comparar com HEAD"))
            .absolute()
            .top(px(DIFF_PANEL_MARGIN_PX))
            .right(px(DIFF_PANEL_MARGIN_PX))
            .w(px(DIFF_PANEL_W_PX))
            .p(px(DIFF_PANEL_PAD_PX))
            .flex()
            .flex_col()
            .rounded(px(DIFF_PANEL_MARGIN_PX))
            .border_1()
            .border_color(g.edge_soft)
            .bg(g.island)
            .shadow(elevation(g.mode))
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(header)
            .child(body)
    }

    fn open_context_menu(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(g) = self.glass else { return };
        let (on, focus) = (self.ctx_on, self.focus.clone());
        let menu = PopupMenu::build(window, cx, move |menu, _, _| {
            let mut menu = menu.action_context(focus.clone()).min_w(px(220.));
            for item in context_menu_items(on) {
                menu = match item {
                    None => menu.separator(),
                    Some((label, keys, action, danger)) => {
                        menu.menu_element(action, move |_, _| {
                            div()
                                .h(px(30.))
                                .w_full()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(16.))
                                .text_color(if danger { g.danger } else { g.text })
                                .child(label)
                                .child(
                                    div()
                                        .font_family(FONT_MONO)
                                        .text_size(px(11.))
                                        .text_color(g.text_faint)
                                        .child(keys),
                                )
                        })
                    }
                };
            }
            menu
        });
        self.context_subscription =
            Some(
                cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, _, cx| {
                    this.context_menu = None;
                    cx.notify();
                }),
            );
        menu.focus_handle(cx).focus(window, cx);
        self.context_menu = Some((menu, position));
    }

    /// Ferramentas, zoom, legenda e estado de arquivo vazio.
    fn overlays(&self, g: &Glass, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let legend_progress = motion::value(
            "legend-motion",
            f32::from(self.legend_open),
            motion::BASE,
            window,
            cx,
        );
        // vidro flutuante: fundo `island`, contorno `edge_strong`, elevação
        let float = |el: Stateful<Div>| {
            el.rounded(px(12.))
                .border_1()
                .border_color(g.edge_soft)
                .bg(g.island)
                .shadow(elevation(g.mode))
                .occlude()
                // clique nas ilhas não inicia pan/seleção no fundo nem abre o menu de contexto
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        };
        let island = |id: &'static str, label: &'static str| {
            float(div().id(id))
                .role(Role::Toolbar)
                .aria_label(label)
                .flex()
                .items_center()
                .gap(px(2.))
                .p(px(4.))
        };
        // botão de ferramenta: a ativa ganha o fundo `sel`; desabilitado = esmaecido, sem clique
        let mut tool_btn = |id: &'static str,
                            icon: IconName,
                            tip: &'static str,
                            action: Box<dyn Action>,
                            on: bool,
                            enabled: bool,
                            side: f32| {
            let tip_action = action.boxed_clone();
            let active = motion::value(
                (ElementId::from(id), "active"),
                f32::from(on),
                motion::BASE,
                window,
                cx,
            );
            let hover = motion::value(
                (ElementId::from(id), "hover"),
                f32::from(enabled && self.hovered_tool == Some(id)),
                motion::FAST,
                window,
                cx,
            );
            let press = motion::value(
                (ElementId::from(id), "press"),
                f32::from(enabled && self.pressed_tool == Some(id)),
                motion::FAST,
                window,
                cx,
            );
            let active_background = if id == "tool-select" {
                g.accent
            } else {
                g.accent.opacity(0.16)
            };
            let active_color = if id == "tool-select" {
                gpui_kit::white()
            } else {
                g.accent
            };
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(tip)
                .size(px(side))
                .rounded(px(8.))
                .flex()
                .items_center()
                .justify_center()
                .text_color(motion::color(g.text_muted, active_color, active))
                .bg(motion::color(
                    g.hover.opacity(hover),
                    active_background,
                    active,
                ))
                .opacity(if enabled { 1. - 0.18 * press } else { 0.4 })
                .on_hover(cx.listener(move |this, over, _, cx| {
                    this.hovered_tool = if *over { Some(id) } else { None };
                    cx.notify();
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.pressed_tool = Some(id);
                        cx.notify();
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.pressed_tool = None;
                        cx.notify();
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.pressed_tool = None;
                        cx.notify();
                    }),
                )
                .tooltip(move |window, cx| {
                    Tooltip::new(tip)
                        .action(tip_action.as_ref(), Some(actions::CANVAS))
                        .build(window, cx)
                })
                .when(enabled, |d| {
                    d.on_click(move |_, window, cx| {
                        window.dispatch_action(action.boxed_clone(), cx);
                    })
                })
                .child(Icon::new(icon).size(px(16.)))
        };
        let hsep = || div().h(px(1.)).mx(px(4.)).my(px(3.)).bg(g.edge_soft);
        let vsep = || div().w(px(1.)).h(px(14.)).mx(px(2.)).bg(g.edge_soft);
        let mut out = Vec::new();

        // Ferramentas à esquerda.
        let n_sel = usize::from(self.selected.is_some()) + self.multi.len();
        let mut tools = island("canvas-tools", crate::i18n::t("Ferramentas")).flex_col();
        for (id, icon, tip, action, sep) in tool_items() {
            let on = match id {
                "tool-select" => self.tool == Tool::Select,
                "tool-hand" => self.tool == Tool::Hand,
                "tool-focus" => self.focus_mode(),
                ID_TOOL_DIFF => self.diff_active(),
                "tool-editor" => g.editor_open,
                _ => false,
            };
            let enabled = match id {
                "tool-group" => n_sel >= 2,
                "tool-focus" => n_sel >= 1 || self.focus_mode,
                _ => true,
            };
            tools = tools
                .when(sep, |d| d.child(hsep()))
                .child(tool_btn(id, icon, tip, action, on, enabled, 32.));
        }
        out.push(
            div()
                .absolute()
                .left(px(12.))
                .top(relative(0.5))
                .mt(px(TOOL_ISLAND_OFFSET_Y_PX))
                .child(tools)
                .when_some(self.tour_bounds.clone(), |d, map| {
                    d.child(crate::shell::guide_ui::track_tour_anchor(
                        crate::guide::TourTarget::TopbarAdd,
                        map,
                    ))
                })
                .into_any_element(),
        );
        if self.focus_mode() {
            let depth = self.focus_depth;
            let focus_bar = island("canvas-focus-bar", crate::i18n::t("Modo Foco"))
                .px(px(10.))
                .gap(px(8.))
                .child(
                    div()
                        .text_size(TEXT_XS)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(g.accent)
                        .child(crate::i18n::t("Modo Foco")),
                )
                .child(
                    div()
                        .id("focus-depth-pill")
                        .role(Role::Button)
                        .px(px(7.))
                        .py(px(2.))
                        .rounded(px(6.))
                        .bg(g.accent.opacity(0.16))
                        .text_size(TEXT_XS)
                        .font_family(FONT_MONO)
                        .text_color(g.accent)
                        .cursor_pointer()
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(CycleFocusDepth), cx);
                        })
                        .child(if depth == 1 { "1-hop" } else { "2-hop" }),
                )
                .child(
                    div()
                        .id("focus-exit-btn")
                        .role(Role::Button)
                        .px(px(6.))
                        .py(px(2.))
                        .rounded(px(6.))
                        .text_size(TEXT_XS)
                        .text_color(g.text_muted)
                        .cursor_pointer()
                        .hover(|s| s.bg(g.hover))
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(ToggleFocusMode), cx);
                        })
                        .child("Esc"),
                );
            out.push(
                div()
                    .absolute()
                    .top(px(12.))
                    .left(relative(0.5))
                    .ml(px(-95.))
                    .child(focus_bar)
                    .into_any_element(),
            );
        }

        // Zoom, ajuste e legenda no canto inferior direito.
        let pct = format!("{}%", (self.zoom * 100.).round() as i32);
        out.push(
            div()
                .absolute()
                .bottom(px(12.))
                .right(px(12.))
                .child(
                    island("canvas-zoom", "Zoom")
                        .font_family(FONT_MONO)
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(TEXT_XS)
                        .child(tool_btn(
                            "zoom-out",
                            IconName::Minus,
                            crate::i18n::t("Diminuir zoom"),
                            Box::new(ZoomOut),
                            false,
                            true,
                            30.,
                        ))
                        .child(
                            div()
                                .id("zoom-pct")
                                .role(Role::Button)
                                .aria_label(crate::i18n::t("Restaurar zoom (100%)"))
                                .w(px(38.))
                                .flex()
                                .justify_center()
                                .text_color(g.text)
                                .tooltip(|window, cx| {
                                    Tooltip::new(crate::i18n::t("Restaurar zoom (100%)"))
                                        .action(&ResetZoom, Some(actions::CANVAS))
                                        .build(window, cx)
                                })
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(ResetZoom), cx);
                                })
                                .child(pct),
                        )
                        .child(tool_btn(
                            "zoom-in",
                            IconName::Plus,
                            crate::i18n::t("Aumentar zoom"),
                            Box::new(ZoomIn),
                            false,
                            true,
                            30.,
                        ))
                        .child(vsep())
                        .child(tool_btn(
                            "zoom-fit",
                            IconName::Scan,
                            crate::i18n::t("Ajustar à tela"),
                            Box::new(FitView),
                            false,
                            true,
                            30.,
                        ))
                        .child(vsep())
                        .child(tool_btn(
                            "toggle-minimap",
                            IconName::Frame,
                            crate::i18n::t("Minimapa"),
                            Box::new(ToggleMinimap),
                            self.minimap_open && self.minimap_has_cards(),
                            self.minimap_has_cards(),
                            30.,
                        ))
                        .child(tool_btn(
                            "help-legend",
                            IconName::Info,
                            crate::i18n::t("Legenda"),
                            Box::new(ToggleLegend),
                            self.legend_open,
                            true,
                            30.,
                        )),
                )
                .into_any_element(),
        );

        let minimap_visible = self.minimap_open && self.minimap_has_cards();
        if minimap_visible && let Some((xf, vp_world)) = self.minimap_transform() {
            self.minimap_xform.set(Some(xf));
            let map_w = crate::canvas_geom::MINIMAP_W;
            let map_h = crate::canvas_geom::MINIMAP_H;
            let schema = &self.schema;
            let dark = g.mode == Mode::Dark;
            let mut mm_groups: Vec<(f32, f32, f32, f32, Hsla)> = Vec::new();
            for gr in &schema.groups {
                let members: Vec<_> = gr
                    .tables
                    .iter()
                    .filter_map(|t| Some((self.pos.get(t)?, schema.table(t)?)))
                    .collect();
                if members.is_empty() {
                    continue;
                }
                let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                for ((x, y), t) in &members {
                    x0 = x0.min(*x);
                    y0 = y0.min(*y);
                    x1 = x1.max(x + CARD_W);
                    y1 = y1.max(y + card_h(t.columns.len()));
                }
                let pad = GROUP_PAD - 8.;
                let c = gr
                    .color
                    .as_deref()
                    .and_then(hex)
                    .unwrap_or_else(|| group_color(schema, &gr.name));
                let (mx0, my0) = xf.world_to_map(x0 - pad, y0 - pad - 8.);
                let (mx1, my1) = xf.world_to_map(x1 + pad, y1 + pad);
                mm_groups.push((mx0, my0, (mx1 - mx0).max(2.), (my1 - my0).max(2.), c));
            }
            let mut mm_tables: Vec<(f32, f32, f32, f32, Hsla, bool)> =
                Vec::with_capacity(schema.tables.len());
            for t in &schema.tables {
                let q = t.qualified_name();
                let (x, y) = self
                    .pos
                    .get(&q)
                    .copied()
                    .unwrap_or(crate::constants::DEFAULT_CARD_POS);
                let grp = schema.group_of(&q);
                let tint = t
                    .color
                    .as_deref()
                    .or_else(|| grp.and_then(|gr| gr.color.as_deref()))
                    .and_then(hex)
                    .or_else(|| grp.map(|gr| group_color(schema, &gr.name)))
                    .unwrap_or(g.text_muted);
                let (mx0, my0) = xf.world_to_map(x, y);
                let (mx1, my1) = xf.world_to_map(x + CARD_W, y + card_h(t.columns.len()));
                mm_tables.push((
                    mx0,
                    my0,
                    (mx1 - mx0).max(3.),
                    (my1 - my0).max(3.),
                    tint,
                    self.is_selected(&q),
                ));
            }
            let mut mm_enums: Vec<(f32, f32, f32, f32)> = Vec::with_capacity(schema.enums.len());
            for en in &schema.enums {
                let key = enum_key(en);
                if let Some(&(x, y)) = self.pos.get(&key) {
                    let (mx0, my0) = xf.world_to_map(x, y);
                    let (mx1, my1) = xf.world_to_map(x + ENUM_W, y + enum_h(en));
                    mm_enums.push((mx0, my0, (mx1 - mx0).max(3.), (my1 - my0).max(3.)));
                }
            }
            let mm_ghosts: Vec<(f32, f32, f32, f32)> = self
                .diff
                .as_ref()
                .map(DiffSession::ghost_rects)
                .unwrap_or_default()
                .into_iter()
                .map(|r| {
                    let (mx0, my0) = xf.world_to_map(r.x, r.y);
                    let (mx1, my1) = xf.world_to_map(r.x + r.w, r.y + r.h);
                    (mx0, my0, (mx1 - mx0).max(3.), (my1 - my0).max(3.))
                })
                .collect();
            let ghost_color = diff_color(g, DiffStatus::Removed);
            let (vp_left, vp_top) = xf.world_to_map(vp_world.0, vp_world.1);
            let (vp_right, vp_bottom) =
                xf.world_to_map(vp_world.0 + vp_world.2, vp_world.1 + vp_world.3);
            let vp_rect = (
                vp_left,
                vp_top,
                (vp_right - vp_left).max(8.),
                (vp_bottom - vp_top).max(6.),
            );
            let accent = g.accent;
            let muted = g.text_faint;
            let entity = cx.entity();
            out.push(
                float(div().id("canvas-minimap"))
                    .role(Role::Region)
                    .aria_label(crate::i18n::t("Minimapa"))
                    .absolute()
                    .bottom(px(56.))
                    .right(px(12.))
                    .w(px(map_w))
                    .h(px(map_h))
                    .overflow_hidden()
                    .cursor_pointer()
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.interrupt_motion();
                            window.focus(&this.focus, cx);
                            let Some(origin) = this.minimap_origin.get() else {
                                return;
                            };
                            let mx = f32::from(e.position.x - origin.x);
                            let my = f32::from(e.position.y - origin.y);
                            let inside_vp = mx >= vp_rect.0
                                && mx <= vp_rect.0 + vp_rect.2
                                && my >= vp_rect.1
                                && my <= vp_rect.1 + vp_rect.3;
                            let grab = if inside_vp {
                                (
                                    mx - (vp_rect.0 + vp_rect.2 * 0.5),
                                    my - (vp_rect.1 + vp_rect.3 * 0.5),
                                )
                            } else {
                                (0.0, 0.0)
                            };
                            if !inside_vp {
                                this.pan_from_minimap(e.position, grab);
                            }
                            this.drag = Some(Drag::Minimap { grab });
                            cx.notify();
                        }),
                    )
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, (), window, cx| {
                                let d = entity.read(cx);
                                d.minimap_origin.set(Some(bounds.origin));
                                if let Some(map) = &d.tour_bounds {
                                    crate::shell::guide_ui::record_tour_bounds(
                                        map,
                                        crate::guide::TourTarget::Minimap,
                                        bounds,
                                        window,
                                    );
                                }
                                for &(x, y, w, h, c) in &mm_groups {
                                    window.paint_quad(
                                        fill(
                                            Bounds::new(
                                                bounds.origin + point(px(x), px(y)),
                                                size(px(w), px(h)),
                                            ),
                                            c.opacity(if dark { 0.14 } else { 0.16 }),
                                        )
                                        .corner_radii(px(3.))
                                        .border_widths(px(1.))
                                        .border_color(c.opacity(0.35)),
                                    );
                                }
                                for &(x, y, w, h) in &mm_enums {
                                    window.paint_quad(
                                        fill(
                                            Bounds::new(
                                                bounds.origin + point(px(x), px(y)),
                                                size(px(w), px(h)),
                                            ),
                                            muted.opacity(0.45),
                                        )
                                        .corner_radii(px(2.)),
                                    );
                                }
                                for &(x, y, w, h, c, sel) in &mm_tables {
                                    window.paint_quad(
                                        fill(
                                            Bounds::new(
                                                bounds.origin + point(px(x), px(y)),
                                                size(px(w), px(h)),
                                            ),
                                            if sel { accent } else { c.opacity(0.75) },
                                        )
                                        .corner_radii(px(2.)),
                                    );
                                }
                                for &(x, y, w, h) in &mm_ghosts {
                                    window.paint_quad(
                                        fill(
                                            Bounds::new(
                                                bounds.origin + point(px(x), px(y)),
                                                size(px(w), px(h)),
                                            ),
                                            ghost_color.opacity(DIFF_MINIMAP_GHOST_ALPHA),
                                        )
                                        .corner_radii(px(2.)),
                                    );
                                }
                                window.paint_quad(
                                    fill(
                                        Bounds::new(
                                            bounds.origin + point(px(vp_rect.0), px(vp_rect.1)),
                                            size(px(vp_rect.2), px(vp_rect.3)),
                                        ),
                                        accent.opacity(0.14),
                                    )
                                    .corner_radii(px(3.))
                                    .border_widths(px(1.5))
                                    .border_color(accent),
                                );
                            },
                        )
                        .size_full(),
                    )
                    .into_any_element(),
            );
        }

        // Legenda acima da ilha de zoom (ou acima do minimapa quando visível).
        let legend_bottom = if minimap_visible { 202. } else { 56. };
        if legend_progress > 0. {
            let row = |swatch: AnyElement, text: &'static str| {
                div()
                    .h(px(28.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .text_color(g.text_muted)
                    .child(
                        div()
                            .w(px(30.))
                            .flex_none()
                            .flex()
                            .justify_center()
                            .child(swatch),
                    )
                    .child(text)
            };
            let bar = |h: f32, c: Hsla| {
                div()
                    .w(px(26.))
                    .h(px(h))
                    .rounded(px(2.))
                    .bg(c)
                    .into_any_element()
            };
            out.push(
                float(div().id("canvas-legend"))
                    .absolute()
                    .bottom(px(legend_bottom - 6. * (1. - legend_progress)))
                    .opacity(legend_progress)
                    .right(px(12.))
                    .w(px(256.))
                    .p(px(6.))
                    .bg(g.modal)
                    .flex()
                    .flex_col()
                    .text_size(px(12.5))
                    .child(
                        div()
                            .h(px(30.))
                            .pl(px(8.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .text_size(TEXT_XS)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(g.text)
                            .child(crate::i18n::t("LEGENDA"))
                            .child(
                                Button::new("legend-close")
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::X)
                                    .tooltip_with_action(
                                        crate::i18n::t("Ocultar legenda"),
                                        &ToggleLegend,
                                        Some(actions::CTX),
                                    )
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(Box::new(ToggleLegend), cx);
                                    }),
                            ),
                    )
                    .child(row(
                        Icon::new(IconName::KeyRound)
                            .size(px(12.))
                            .text_color(g.accent)
                            .into_any_element(),
                        crate::i18n::t("Chave primária"),
                    ))
                    .child(row(
                        Icon::new(IconName::Link)
                            .size(px(12.))
                            .text_color(g.link)
                            .into_any_element(),
                        crate::i18n::t("Chave estrangeira"),
                    ))
                    .child(row(
                        badge("NN", gpui_kit::transparent_black(), g.text_faint, 1.)
                            .into_any_element(),
                        crate::i18n::t("Não nulo · UQ único"),
                    ))
                    .child(row(
                        bar(1.5, g.rel_line),
                        crate::i18n::t("Relação (coluna → coluna)"),
                    ))
                    .child(row(
                        badge("N·1", gpui_kit::transparent_black(), g.text_muted, 1.)
                            .into_any_element(),
                        crate::i18n::t("Cardinalidade: muitos · um"),
                    ))
                    .child(row(
                        bar(2.5, g.accent),
                        crate::i18n::t("Tabela selecionada e suas relações"),
                    ))
                    .into_any_element(),
            );
        }

        // Ações para começar um arquivo vazio.
        if self.schema.tables.is_empty() && self.schema.enums.is_empty() {
            let action_btn = |id: &'static str,
                              icon: IconName,
                              label: &'static str,
                              keys: &'static str,
                              action: Box<dyn Action>| {
                div()
                    .id(id)
                    .role(Role::Button)
                    .aria_label(label)
                    .h(px(34.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(8.))
                    .text_size(TEXT_SM)
                    .text_color(g.text)
                    .hover({
                        let h = g.hover;
                        move |st| st.bg(h)
                    })
                    .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
                    .child(Icon::new(icon).size(px(16.)).text_color(g.accent))
                    .child(div().flex_1().child(label))
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .text_size(px(11.))
                            .text_color(g.text_faint)
                            .child(keys),
                    )
            };
            out.push(
                div()
                    .absolute()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("canvas-empty")
                            .occlude()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .w(px(300.))
                            .flex()
                            .flex_col()
                            .gap(px(14.))
                            .text_left()
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(g.text)
                                    .child(crate::i18n::t("Nenhuma tabela ainda")),
                            )
                            .child(
                                div()
                                    .text_size(TEXT_SM)
                                    .line_height(relative(1.5))
                                    .text_color(g.text_muted)
                                    .child(
                                        crate::i18n::t("Escreva seu esquema DBML no editor ou abra um arquivo existente."),
                                    ),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .mt(px(10.))
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .text_left()
                                    .child(action_btn(
                                        "empty-editor",
                                        IconName::Braces,
                                        crate::i18n::t("Abrir editor DBML"),
                                        "Ctrl E",
                                        Box::new(ToggleEditor),
                                    ))
                                    .child(action_btn(
                                        "empty-open",
                                        IconName::FolderOpen,
                                        crate::i18n::t("Abrir arquivo"),
                                        "Ctrl O",
                                        Box::new(OpenFile),
                                    ))
                                    .child(action_btn(
                                        "empty-import",
                                        IconName::FileInput,
                                        crate::i18n::t("Importar SQL DDL"),
                                        "",
                                        Box::new(ImportSqlDdl),
                                    ))
                                    .child(action_btn(
                                        "empty-connect",
                                        IconName::Database,
                                        crate::i18n::t("Conectar ao banco"),
                                        "",
                                        Box::new(ConnectDatabase),
                                    )),
                            ),
                    )
                    .into_any_element(),
            );
        }
        let _ = cx;
        out
    }
}

/// Raio do cartão de tabela/enum.
const CARD_R: f32 = 12.;

/// Anel do cartão selecionado.
fn sel_ring(g: &Glass) -> BoxShadow {
    BoxShadow {
        color: g.accent.opacity(0.18),
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(3.),
        inset: false,
    }
}

/// Halo do cartão alterado no modo Comparar (cor do status).
fn diff_halo(color: Hsla) -> BoxShadow {
    BoxShadow {
        color: color.opacity(DIFF_HALO_ALPHA),
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(DIFF_HALO_SPREAD_PX),
        inset: false,
    }
}

/// Linha fantasma (coluna/valor removido): nome riscado e tipo, sobre o tom de "removido".
fn ghost_row(g: &Glass, name: &str, ty: &str, k: f32) -> Div {
    div()
        .h(px(ROW_H * k))
        .px(px(12. * k))
        .flex()
        .items_center()
        .gap(px(8. * k))
        .text_size(px(12.5 * k))
        .bg(g.danger.opacity(DIFF_ROW_TINT))
        .opacity(DIFF_GHOST_OPACITY)
        .child(
            div()
                .flex_1()
                .truncate()
                .line_through()
                .text_color(g.text_muted)
                .child(name.to_owned()),
        )
        .child(
            div()
                .text_size(px(11. * k))
                .font_family(FONT_MONO)
                .text_color(g.text_faint)
                .child(ty.to_owned()),
        )
}

/// Selo mono 600 9.5px (FK, NN, UQ, IDX, ENUM). `k` escala com o zoom (1 fora do mundo).
fn badge(label: &'static str, bg: Hsla, fg: Hsla, k: f32) -> Div {
    div()
        .flex_none()
        .h(px(15. * k))
        .flex()
        .items_center()
        .rounded(px(4. * k))
        .bg(bg)
        .text_color(fg)
        .font_family(FONT_MONO)
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(px(9. * k))
        .child(label)
}

/// Botão da ilha de ferramentas: (id, ícone, dica, ação, divisor antes).
type ToolItem = (&'static str, IconName, &'static str, Box<dyn Action>, bool);

/// Ilha de ferramentas na ordem do protótipo v5.
fn tool_items() -> Vec<ToolItem> {
    vec![
        (
            "tool-select",
            IconName::MousePointer2,
            crate::i18n::t("Selecionar"),
            Box::new(ToolSelect),
            false,
        ),
        (
            "tool-hand",
            IconName::Hand,
            crate::i18n::t("Mão: mover a vista"),
            Box::new(ToolHand),
            false,
        ),
        (
            "tool-table",
            IconName::SquarePlus,
            crate::i18n::t("Nova tabela"),
            Box::new(NewTable),
            true,
        ),
        (
            "tool-enum",
            IconName::ListPlus,
            crate::i18n::t("Novo enum"),
            Box::new(NewEnum),
            false,
        ),
        (
            "tool-group",
            IconName::Group,
            crate::i18n::t("Agrupar seleção"),
            Box::new(GroupSelection),
            false,
        ),
        (
            "tool-layout",
            IconName::LayoutGrid,
            "Auto-layout",
            Box::new(Relayout),
            true,
        ),
        (
            "tool-focus",
            IconName::Scan,
            crate::i18n::t("Modo Foco (vizinhança FK)"),
            Box::new(ToggleFocusMode),
            false,
        ),
        (
            ID_TOOL_DIFF,
            IconName::GitCompare,
            crate::i18n::t("Comparar com HEAD (git)"),
            Box::new(ToggleDiffMode),
            false,
        ),
        (
            "tool-editor",
            IconName::Braces,
            "Editor DBML",
            Box::new(ToggleEditor),
            false,
        ),
    ]
}

/// Item do menu de contexto: (rótulo, atalho exibido, ação, destrutivo). `None` = separador.
type CtxItem = Option<(&'static str, &'static str, Box<dyn Action>, bool)>;

/// Itens do menu de contexto do canvas, sobre uma tabela, uma relação ou o fundo. Ponto de
/// extensão: itens novos (ex.: "Duplicate", "View in DBML") entram só nesta lista.
fn context_menu_items(on: CtxOn) -> Vec<CtxItem> {
    let it = |label, keys, action: Box<dyn Action>| Some((label, keys, action, false));
    if on == CtxOn::Edge {
        return vec![
            it(
                crate::i18n::t("Relação: reta/curva"),
                "",
                Box::new(ToggleEdgeStraight),
            ),
            it(
                crate::i18n::t("Relação: restaurar curva automática"),
                "",
                Box::new(ResetEdgeCurve),
            ),
        ];
    }
    if on == CtxOn::Table {
        vec![
            it(crate::i18n::t("Renomear"), "F2", Box::new(RenameSelected)),
            it(
                crate::i18n::t("Duplicar"),
                "Ctrl+D",
                Box::new(DuplicateTable),
            ),
            it(
                crate::i18n::t("Ver no DBML"),
                "Ctrl+Alt+D",
                Box::new(RevealInDbml),
            ),
            it(
                crate::i18n::t("Agrupar tabelas selecionadas"),
                "Ctrl+G",
                Box::new(GroupSelection),
            ),
            it(
                crate::i18n::t("Desagrupar"),
                "Ctrl+Shift+G",
                Box::new(UngroupSelection),
            ),
            None,
            Some((
                crate::i18n::t("Excluir tabela"),
                "Delete",
                Box::new(DeleteSelection),
                true,
            )),
        ]
    } else {
        vec![
            it(crate::i18n::t("Nova tabela"), "T", Box::new(NewTable)),
            it(crate::i18n::t("Novo enum"), "E", Box::new(NewEnum)),
            it("Auto-layout", "Ctrl+L", Box::new(Relayout)),
            it(crate::i18n::t("Ajustar à tela"), "", Box::new(FitView)),
            it(
                crate::i18n::t("Alternar editor DBML"),
                "Ctrl+E",
                Box::new(ToggleEditor),
            ),
        ]
    }
}

/// Cor padrão de um grupo sem `color`: paleta fixa pela posição do grupo.
pub fn group_color(schema: &Schema, group: &str) -> Hsla {
    let i = schema
        .groups
        .iter()
        .position(|g| g.name == group)
        .unwrap_or(0);
    hex(TABLE_COLORS[i % TABLE_COLORS.len()].1).unwrap_or_default()
}

fn bezier(segs: &[[Point<Pixels>; 4]], width: f32) -> Option<Path<Pixels>> {
    let mut pb = PathBuilder::stroke(px(width));
    pb.move_to(segs.first()?[0]);
    for pts in segs {
        pb.cubic_bezier_to(pts[3], pts[1], pts[2]);
    }
    pb.build().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[::core::prelude::v1::test]
    fn headless_motion_preserves_targets_and_direct_manipulation() {
        gpui_kit::platform::headless()
            .with_assets(crate::AppAssets)
            .run(|cx| {
                gpui_kit::init(cx);
                cx.set_reduce_motion(motion::disabled());
                cx.text_system()
                    .add_fonts(
                        crate::FONTS
                            .iter()
                            .map(|f| std::borrow::Cow::Borrowed(*f))
                            .collect(),
                    )
                    .unwrap();
                let (handle, diagram) = gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                    cx.new(|cx| {
                        let mut d = Diagram::new(
                            dbml_analysis::analyze("Table a {\n id int\n}\nTable b {\n id int\n}"),
                            cx,
                        );
                        d.glass = Some(Glass::of(Mode::Dark, false));
                        d
                    })
                })
                .unwrap();
                let draw = |cx: &mut App| {
                    cx.update_window(handle, |_, window, cx| {
                        window.draw(cx).clear(cx);
                    })
                    .unwrap();
                };
                draw(cx);
                draw(cx);
                diagram.update(cx, |d, cx| {
                    assert!(!d.needs_fit);
                    assert!(d.viewport_motion.is_none());
                    d.selected = Some("a".into());
                    let before_resize = d.pan;
                    let old_size = d.size;
                    d.resize_viewport(d.origin, size(old_size.width - px(100.), old_size.height - px(40.)));
                    assert_eq!(d.pan, (before_resize.0 - 50., before_resize.1 - 20.));
                    d.resize_viewport(d.origin, old_size);
                    assert_eq!(d.pan, before_resize);
                    let before = (d.pan, d.zoom);
                    d.zoom_by(1.25);
                    d.zoom_by(1.25);
                    assert!((d.viewport().0 - before.1 * 1.25 * 1.25).abs() < 0.0001);
                    if !motion::disabled() {
                        assert!(d.viewport_motion.is_some());
                    }
                    let target = d.viewport();
                    d.advance_motion(Instant::now() + motion::SLOW, false);
                    assert_eq!((d.zoom, d.pan.0, d.pan.1), target);
                    d.fit();
                    d.zoom_at((200., 100.), 1.1);
                    assert!(d.viewport_motion.is_none());
                    d.pos.get_mut("a").unwrap().0 += 300.;
                    d.relayout();
                    assert_eq!(*d.positions(), auto_layout(&d.schema));
                    d.interrupt_motion();
                    assert!(d.layout_motion.is_none());
                    assert_eq!(d.positions(), &d.pos);
                    let added = dbml_analysis::analyze(
                        "Table a {\n id int\n}\nTable b {\n id int\n}\nTable c {\n id int\n}",
                    );
                    d.set_schema(added.clone());
                    d.apply_snapshot(added, d.pos.clone(), Some("c".into()), None, Vec::new());
                    assert_eq!(d.appearing.contains_key("c"), !motion::disabled());
                    // posições do sidecar: respeita as salvas, completa as ausentes, ignora as órfãs
                    d.seed_positions(HashMap::from([("a".to_string(), (500., 600.)), ("ghost".to_string(), (1., 1.))]));
                    assert_eq!(d.pos["a"], (500., 600.));
                    assert!(!d.pos.contains_key("ghost"));
                    assert!(d.pos.contains_key("b") && d.pos.contains_key("c"));
                    // tabela ausente de `pos` nunca entra em pânico (#94)
                    let saved_a = d.pos.remove("a");
                    d.fit();
                    let _ = d.column_at((0., 0.));
                    let _ = d.table_at((0., 0.));
                    if let Some(p) = saved_a {
                        d.pos.insert("a".to_string(), p);
                    }
                    // cache de rotas: reaproveita sem mudança (pan/zoom não contam), recalcula ao mover
                    d.set_schema(dbml_analysis::analyze(
                        "Table a {\n id int\n}\nTable b {\n id int\n}\nTable c {\n id int\n}\nRef: a.id > b.id",
                    ));
                    d.validate_routes();
                    let first = d.ref_geom(0).unwrap();
                    d.pan.0 += 50.;
                    d.zoom *= 1.1;
                    d.validate_routes();
                    assert!(Rc::ptr_eq(&first, &d.ref_geom(0).unwrap()));
                    assert_eq!(d.ref_at((-1e6, -1e6)), None);
                    d.pos.get_mut("c").unwrap().0 += 10.;
                    d.validate_routes();
                    assert!(!Rc::ptr_eq(&first, &d.ref_geom(0).unwrap()));
                    // modo Comparar: base lida em segundo plano; só o último pedido é aceito
                    let stale = d.begin_diff();
                    let ticket = d.begin_diff();
                    assert!(d.diff_waiting_for(ticket) && !d.diff_waiting_for(stale));
                    assert!(!d.finish_diff(stale, Ok(Schema::default())));
                    let base = dbml_analysis::analyze(
                        "Table a {\n id int\n}\nTable b {\n id int\n}\nTable c {\n id int\n}\nTable old {\n id int\n}",
                    );
                    assert!(d.finish_diff(ticket, Ok(base)));
                    assert!(!d.diff_waiting_for(ticket));
                    // o cartão fantasma (abaixo do diagrama) cabe no "ajustar à tela"
                    let ghost = d.diff.as_ref().unwrap().ghost_rects()[0];
                    d.do_fit();
                    let (vw, vh) = (f32::from(d.size.width), f32::from(d.size.height));
                    let (gx0, gy0) = (ghost.x * d.zoom + d.pan.0, ghost.y * d.zoom + d.pan.1);
                    let (gx1, gy1) = (gx0 + ghost.w * d.zoom, gy0 + ghost.h * d.zoom);
                    assert!(gx0 >= 0. && gy0 >= 0. && gx1 <= vw && gy1 <= vh, "{ghost:?}");
                    assert!(d.minimap_has_cards());
                    d.exit_diff();
                    assert!(!d.finish_diff(ticket, Ok(Schema::default())));
                    assert!(!d.diff_active());
                    cx.notify();
                });
                draw(cx);
                cx.update_window(handle, |_, window, cx| {
                    diagram.update(cx, |d, cx| {
                        d.open_context_menu(point(px(50.), px(50.)), window, cx);
                    });
                })
                .unwrap();
                draw(cx);
                cx.spawn(async |cx| cx.update(|cx| cx.quit())).detach();
            });
    }

    #[::core::prelude::v1::test]
    fn sample_layout_respects_groups_and_does_not_overlap() {
        let s = dbml_analysis::analyze(include_str!("../assets/sample.dbml"));
        let p = auto_layout(&s);
        let x = |n: &str| p[n].0;
        // dentro de cada grupo, quem referencia fica à direita
        assert!(x("orders") < x("order_items"), "{p:?}");
        assert!(x("products") < x("reviews"), "{p:?}");
        // nenhum par de cartões se sobrepõe
        for a in &s.tables {
            for b in &s.tables {
                if a.name >= b.name {
                    continue;
                }
                let (pa, pb) = (p[&a.name], p[&b.name]);
                let (ha, hb) = (card_h(a.columns.len()), card_h(b.columns.len()));
                let apart = pa.0 + CARD_W <= pb.0
                    || pb.0 + CARD_W <= pa.0
                    || pa.1 + ha <= pb.1
                    || pb.1 + hb <= pa.1;
                assert!(apart, "{} e {} se sobrepõem: {p:?}", a.name, b.name);
            }
        }
    }
    /// Importação de SQL com enums: nenhum cartão (tabela ou enum) pode ficar sobre outro, e
    /// enum com schema precisa receber posição sob a chave qualificada usada na renderização.
    #[::core::prelude::v1::test]
    fn imported_schema_with_enums_has_no_overlapping_cards() {
        use std::fmt::Write as _;
        let mut dbml = String::new();
        for i in 0..24 {
            let _ = write!(dbml, "Table t{i} {{\n  id int [pk]\n");
            for c in 0..(i * 7 % 23) {
                let _ = writeln!(dbml, "  c{c} int");
            }
            if i > 0 {
                let _ = writeln!(dbml, "  parent int [ref: > t{}.id]", i / 3);
            }
            dbml.push_str("}\n");
        }
        for i in 0..6 {
            let _ = write!(
                dbml,
                "Enum app.kind{i} {{\n  a\n  b\n  c\n  d\n  e\n  f\n  g\n  h\n}}\n"
            );
        }
        let s = dbml_analysis::analyze(&dbml);
        assert_eq!((s.tables.len(), s.enums.len()), (24, 6));
        let p = auto_layout(&s);
        let mut rects: Vec<layout::Rect> = Vec::new();
        for t in &s.tables {
            let (x, y) = p[&t.qualified_name()];
            rects.push(layout::Rect {
                x,
                y,
                w: CARD_W,
                h: card_h(t.columns.len()),
            });
        }
        for e in &s.enums {
            let (x, y) = p[&format!("enum:app.{}", e.name)];
            rects.push(layout::Rect {
                x,
                y,
                w: ENUM_W,
                h: enum_h(e),
            });
        }
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                assert!(!a.intersects(b), "{a:?} sobrepõe {b:?}");
            }
        }
    }

    /// Ações e divisores da ilha de ferramentas.
    #[::core::prelude::v1::test]
    fn tool_island_order_and_separators() {
        let want: [(&str, &dyn Action, bool); 9] = [
            ("tool-select", &ToolSelect, false),
            ("tool-hand", &ToolHand, false),
            ("tool-table", &NewTable, true),
            ("tool-enum", &NewEnum, false),
            ("tool-group", &GroupSelection, false),
            ("tool-layout", &Relayout, true),
            ("tool-focus", &ToggleFocusMode, false),
            (ID_TOOL_DIFF, &ToggleDiffMode, false),
            ("tool-editor", &ToggleEditor, false),
        ];
        let got = tool_items();
        assert_eq!(got.len(), want.len());
        for ((id, _, _, a, sep), (wid, wa, wsep)) in got.iter().zip(want) {
            assert_eq!((*id, *sep), (wid, wsep));
            assert!(a.partial_eq(wa), "{id}: ação errada");
        }
    }

    fn diff_session(base: &str, head: &Schema) -> DiffSession {
        let base = dbml_analysis::analyze(base);
        let view = crate::diff_view::build_diff_view(&schema::diff_schemas(&base, head));
        DiffSession {
            base: BaseState::Ready(base),
            view: Rc::new(view),
            ghost_tables: HashMap::from([("old".to_string(), (0., 900.))]),
            ghost_enums: HashMap::from([("gone".to_string(), (400., 900.))]),
            cursor: None,
            saved: DiffSaved {
                pan: (0., 0.),
                zoom: 1.,
                selected: None,
                selected_enum: None,
                multi: Vec::new(),
                selected_ref: None,
                focus_mode: false,
                highlighted_column: None,
            },
        }
    }

    /// Cartões fantasma (removidos) entram nos limites do "ajustar à tela" e do minimapa.
    #[::core::prelude::v1::test]
    fn ghost_cards_extend_the_diagram_bounds() {
        let head = dbml_analysis::analyze("Table a {\n id int\n}\n");
        let s = diff_session(
            "Table a {\n id int\n}\nTable old {\n id int\n x int\n}\nEnum gone {\n  v\n}\n",
            &head,
        );
        let ghosts = s.ghost_rects();
        let old_h = card_h(2);
        assert_eq!(ghosts.len(), 2);
        assert!(ghosts.contains(&layout::Rect {
            x: 0.,
            y: 900.,
            w: CARD_W,
            h: old_h
        }));
        let gone = &dbml_analysis::analyze("Enum gone {\n  v\n}\n").enums[0];
        assert!(ghosts.contains(&layout::Rect {
            x: 400.,
            y: 900.,
            w: ENUM_W,
            h: enum_h(gone)
        }));
        let head_card = layout::Rect {
            x: 0.,
            y: 0.,
            w: CARD_W,
            h: card_h(1),
        };
        let b = crate::canvas_geom::world_bounds(std::iter::once(head_card).chain(ghosts));
        assert_eq!(
            b,
            Some((0., 0., 400. + ENUM_W, 900. + old_h.max(enum_h(gone))))
        );
        // base ainda carregando: nenhum fantasma
        let mut loading = diff_session("Table old {}\n", &head);
        loading.base = BaseState::Loading(1);
        loading.view = Rc::default();
        assert_eq!(loading.ghost_rects(), Vec::new());
    }

    /// Enum v4: cabeçalho + linhas de 26px + 6px de respiro em cima e embaixo.
    #[::core::prelude::v1::test]
    fn enum_card_height_matches_v4_rows() {
        let s = dbml_analysis::analyze("Enum e {\n  a\n  b\n}\n");
        assert_eq!(enum_h(&s.enums[0]), HEADER_H + 2. * 26. + 12.);
        assert_eq!(ENUM_W, 220.);
    }

    /// Menu de contexto: itens e ações por alvo (tabela × fundo), na ordem exibida.
    #[::core::prelude::v1::test]
    fn context_menu_items_per_target() {
        let check = |on: CtxOn, want: &[Option<(&str, &dyn Action, bool)>]| {
            let got = context_menu_items(on);
            assert_eq!(got.len(), want.len());
            for (g, w) in got.iter().zip(want) {
                match (g, w) {
                    (None, None) => {}
                    (Some((l, _, a, d)), Some((wl, wa, wd))) => {
                        assert_eq!((*l, *d), (*wl, *wd));
                        assert!(a.partial_eq(*wa), "{l}: ação errada");
                    }
                    _ => panic!("separador fora do lugar"),
                }
            }
        };
        check(
            CtxOn::Table,
            &[
                Some(("Renomear", &RenameSelected, false)),
                Some((crate::i18n::t("Duplicar"), &DuplicateTable, false)),
                Some((crate::i18n::t("Ver no DBML"), &RevealInDbml, false)),
                Some(("Agrupar tabelas selecionadas", &GroupSelection, false)),
                Some((crate::i18n::t("Desagrupar"), &UngroupSelection, false)),
                None,
                Some(("Excluir tabela", &DeleteSelection, true)),
            ],
        );
        check(
            CtxOn::Background,
            &[
                Some(("Nova tabela", &NewTable, false)),
                Some(("Novo enum", &NewEnum, false)),
                Some(("Auto-layout", &Relayout, false)),
                Some(("Ajustar à tela", &FitView, false)),
                Some(("Alternar editor DBML", &ToggleEditor, false)),
            ],
        );
        check(
            CtxOn::Edge,
            &[
                Some(("Relação: reta/curva", &ToggleEdgeStraight, false)),
                Some((
                    "Relação: restaurar curva automática",
                    &ResetEdgeCurve,
                    false,
                )),
            ],
        );
    }
}
