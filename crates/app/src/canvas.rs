//! Canvas do diagrama: entidade com pan/zoom/arrastar, tabelas em vidro e relações em bezier.
//! Coordenadas de mundo → tela: `tela = mundo * zoom + pan` (relativo à origem do canvas).

use crate::actions::{self, *};
use crate::glass::{
    Glass, RADIUS_CARD, RADIUS_ISLAND, TABLE_COLORS, TEXT_XS, card_shadow, elevation, hex,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use schema::Schema;
use std::collections::HashMap;

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
    Pan {
        last: Point<Pixels>,
    },
}

pub type RefPair = ((String, String), (String, String));

pub struct Diagram {
    schema: Schema,
    /// Posição de mundo por nome de tabela (sobrevive a reparse).
    pos: HashMap<String, (f32, f32)>,
    pan: (f32, f32),
    zoom: f32,
    origin: Point<Pixels>,
    size: Size<Pixels>,
    drag: Option<Drag>,
    /// Último cartão tocado; desenhado por cima dos demais.
    top: Option<String>,
    selected: Option<String>,
    /// relação selecionada no canvas `((from_table, from_col), (to_table, to_col))`
    selected_ref: Option<RefPair>,
    /// seleção extra (Shift+clique); a seleção total é `selected` + `multi`
    multi: Vec<String>,
    pub tool: Tool,
    /// barra de espaço pressionada: arrastar move a vista (temporariamente)
    space: bool,
    pub legend_open: bool,
    focus: FocusHandle,
    /// tema atual (o shell atualiza ao trocar); o canvas é uma view própria
    pub glass: Option<Glass>,
    pub translucent_cards: bool,
    needs_fit: bool,
}

impl Diagram {
    pub fn new(schema: Schema, cx: &mut Context<Self>) -> Self {
        let mut d = Self {
            schema: Schema::default(),
            pos: HashMap::new(),
            pan: (0., 0.),
            zoom: 1.,
            origin: Point::default(),
            size: Size::default(),
            drag: None,
            top: None,
            selected: None,
            selected_ref: None,
            multi: Vec::new(),
            tool: Tool::Select,
            space: false,
            legend_open: false,
            focus: cx.focus_handle(),
            glass: None,
            translucent_cards: false,
            needs_fit: true,
        };
        d.set_schema(schema);
        d
    }

    /// Troca o esquema mantendo a posição das tabelas que já existiam; novas ganham auto-layout.
    pub fn set_schema(&mut self, schema: Schema) {
        let auto = auto_layout(&schema);
        let old = std::mem::take(&mut self.pos);
        for t in &schema.tables {
            let p = old
                .get(&t.name)
                .or_else(|| auto.get(&t.name))
                .copied()
                .unwrap_or((0., 0.));
            self.pos.insert(t.name.clone(), p);
        }
        self.schema = schema;
    }

    pub fn relayout(&mut self) {
        self.pos = auto_layout(&self.schema);
        self.needs_fit = true;
    }

    pub fn selected(&self) -> Option<&schema::Table> {
        self.schema.table(self.selected.as_deref()?)
    }

    /// Relações em que a tabela participa (como origem ou destino).
    pub fn refs_of(&self, table: &str) -> Vec<&schema::Ref> {
        self.schema
            .refs
            .iter()
            .filter(|r| r.from.table == table || r.to.table == table)
            .collect()
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    pub fn viewport(&self) -> (f32, f32, f32) {
        (self.zoom, self.pan.0, self.pan.1)
    }

    pub fn set_viewport(&mut self, zoom: f32, pan: (f32, f32)) {
        if zoom.is_finite() && zoom > 0.0 && pan.0.is_finite() && pan.1.is_finite() {
            self.zoom = zoom;
            self.pan = pan;
            self.needs_fit = false;
        }
    }

    /// Seleciona a tabela e centraliza a vista nela.
    pub fn focus_table(&mut self, name: &str) {
        let Some(t) = self.schema.table(name) else {
            return;
        };
        let (x, y) = self.pos[name];
        let (w, h) = (f32::from(self.size.width), f32::from(self.size.height));
        let (cx_, cy_) = (x + CARD_W / 2., y + card_h(t.columns.len()) / 2.);
        self.pan = (w / 2. - cx_ * self.zoom, h / 2. - cy_ * self.zoom);
        self.selected = Some(name.to_string());
        self.top = Some(name.to_string());
    }

    /// Frase curta para a barra de status: o que o usuário pode fazer agora.
    pub fn hint(&self) -> String {
        match (&self.drag, self.tool) {
            (Some(Drag::Cards { .. }), _) => match self.multi.len() {
                0 => format!("Moving {}", self.selected.clone().unwrap_or_default()),
                n => format!("Moving {} tables", n + 1),
            },
            (Some(Drag::Link { from, .. }), _) => {
                format!("Relation from {}.{}: drop on target column", from.0, from.1)
            }
            (Some(Drag::Marquee { .. }), _) => "Selecting: release to select touched tables".into(),
            (Some(Drag::Pan { .. }), _) => "Panning view".into(),
            (Some(Drag::Group { name, .. }), _) => format!("Moving group {name}"),
            (None, Tool::Hand) => "Hand: drag to pan view · V returns to Select".into(),
            (None, Tool::Select) if !self.multi.is_empty() => {
                format!("{} tables selected · Ctrl+G groups", self.multi.len() + 1)
            }
            (None, Tool::Select) if self.selected_ref.is_some() => {
                "Relation selected · Delete removes line and FK column · Esc clears selection"
                    .into()
            }
            (None, Tool::Select) if self.selected.is_some() => {
                "Drag to move table · Esc clears selection".into()
            }
            (None, Tool::Select) => {
                "Click a table to inspect details · drag background to pan · scroll to zoom".into()
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
        self.selected_ref = None;
    }

    pub fn zoom_by(&mut self, factor: f32) {
        let c = (
            f32::from(self.size.width) / 2.,
            f32::from(self.size.height) / 2.,
        );
        self.zoom_at(c, factor);
    }

    pub fn fit(&mut self) {
        self.needs_fit = true;
    }

    fn do_fit(&mut self) {
        let (w, h) = (f32::from(self.size.width), f32::from(self.size.height));
        if self.schema.tables.is_empty() || w < 1. || h < 1. {
            return;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for t in &self.schema.tables {
            let (x, y) = self.pos[&t.name];
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + CARD_W);
            y1 = y1.max(y + card_h(t.columns.len()));
        }
        let pad = 48.;
        let z = ((w - 2. * pad) / (x1 - x0))
            .min((h - 2. * pad) / (y1 - y0))
            .clamp(0.5, 1.1);
        self.zoom = z;
        // centraliza; se não couber, alinha ao início para mostrar as primeiras tabelas inteiras
        let fit_x = (w - (x1 - x0) * z) / 2.;
        let fit_y = (h - (y1 - y0) * z) / 2.;
        self.pan = (fit_x.max(pad) - x0 * z, fit_y.max(pad) - y0 * z);
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
            let (x, y) = self.pos[&t.name];
            let inside =
                p.0 >= x && p.0 <= x + CARD_W && p.1 >= y && p.1 <= y + card_h(t.columns.len());
            let row = ((p.1 - y - HEADER_H - 4.) / ROW_H).floor();
            (inside && row >= 0.)
                .then(|| t.columns.get(row as usize))
                .flatten()
                .map(|c| (t.name.clone(), c.name.clone()))
        })
    }

    /// Tabela sob o ponto (mundo), para soltar uma relação em qualquer parte do cartão.
    fn table_at(&self, p: (f32, f32)) -> Option<(String, String)> {
        self.schema.tables.iter().find_map(|t| {
            let (x, y) = self.pos[&t.name];
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
            Some((t.name.clone(), pk_col))
        })
    }

    /// Relação (`Ref`) sob o ponto (mundo), verificando distância até os segmentos ortogonais.
    fn ref_at(&self, p: (f32, f32)) -> Option<RefPair> {
        let schema = &self.schema;
        for r in &schema.refs {
            let Some(ft) = schema.table(&r.from.table) else {
                continue;
            };
            let Some(tt) = schema.table(&r.to.table) else {
                continue;
            };
            let Some(fc) = ft.columns.iter().position(|c| c.name == r.from.column) else {
                continue;
            };
            let Some(tc) = tt.columns.iter().position(|c| c.name == r.to.column) else {
                continue;
            };
            let (ax, ay) = self.pos[&ft.name];
            let (bx, by) = self.pos[&tt.name];
            let ay = ay + HEADER_H + 4. + ROW_H * (fc as f32 + 0.5);
            let by = by + HEADER_H + 4. + ROW_H * (tc as f32 + 0.5);
            let (sa, sb) = if ax + CARD_W < bx {
                (1., -1.)
            } else if bx + CARD_W < ax {
                (-1., 1.)
            } else {
                (1., 1.)
            };
            let (ax, bx) = (
                if sa > 0. { ax + CARD_W } else { ax },
                if sb > 0. { bx + CARD_W } else { bx },
            );
            let obstacles: Vec<layout::Rect> = schema
                .tables
                .iter()
                .filter(|t| t.name != ft.name && t.name != tt.name)
                .map(|t| {
                    let (x, y) = self.pos[&t.name];
                    layout::Rect {
                        x,
                        y,
                        w: CARD_W,
                        h: card_h(t.columns.len()),
                    }
                })
                .collect();
            let pts = layout::route(
                layout::Pos { x: ax, y: ay },
                sa,
                layout::Pos { x: bx, y: by },
                sb,
                &obstacles,
                24.,
            );
            if crate::canvas_geom::point_hits_polyline(p, &pts, 8. / self.zoom.max(0.4)) {
                return Some((
                    (r.from.table.clone(), r.from.column.clone()),
                    (r.to.table.clone(), r.to.column.clone()),
                ));
            }
        }
        None
    }

    /// Posiciona uma tabela recém-criada no centro da vista e a seleciona.
    pub fn place_new(&mut self, name: &str) {
        let (w, h) = (f32::from(self.size.width), f32::from(self.size.height));
        let at = (
            (w / 2. - self.pan.0) / self.zoom - CARD_W / 2.,
            (h / 2. - self.pan.1) / self.zoom - 60.,
        );
        self.pos.insert(name.to_string(), at);
        self.selected = Some(name.to_string());
        self.multi.clear();
        self.top = Some(name.to_string());
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
        if let Some(Drag::Link { from, now }) = &self.drag
            && let Some(to) = self.column_at(*now).or_else(|| self.table_at(*now))
            && to.0 != from.0
        {
            cx.emit(DiagramEvent::CreateRef {
                from: from.clone(),
                to,
            });
        }
        if let Some(Drag::Marquee { start, now }) = self.drag.take() {
            let (x0, x1) = (start.0.min(now.0), start.0.max(now.0));
            let (y0, y1) = (start.1.min(now.1), start.1.max(now.1));
            if x1 - x0 > 2. || y1 - y0 > 2. {
                for t in &self.schema.tables {
                    let (x, y) = self.pos[&t.name];
                    let hit =
                        x < x1 && x + CARD_W > x0 && y < y1 && y + card_h(t.columns.len()) > y0;
                    if hit && !self.is_selected(&t.name) {
                        if self.selected.is_none() {
                            self.selected = Some(t.name.clone());
                        } else {
                            self.multi.push(t.name.clone());
                        }
                    }
                }
            }
        }
        self.drag = None;
        cx.notify();
    }

    fn to_local(&self, p: Point<Pixels>) -> (f32, f32) {
        (
            f32::from(p.x - self.origin.x),
            f32::from(p.y - self.origin.y),
        )
    }

    fn zoom_at(&mut self, cursor: (f32, f32), factor: f32) {
        let z = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let k = z / self.zoom;
        self.pan = (
            cursor.0 - (cursor.0 - self.pan.0) * k,
            cursor.1 - (cursor.1 - self.pan.1) * k,
        );
        self.zoom = z;
    }
}

fn auto_layout(schema: &Schema) -> HashMap<String, (f32, f32)> {
    let col_counts: Vec<usize> = schema.tables.iter().map(|t| t.columns.len()).collect();
    let idx = |n: &str| schema.tables.iter().position(|t| t.name == n);
    let edges: Vec<_> = schema
        .refs
        .iter()
        .filter_map(|r| Some((idx(&r.from.table)?, idx(&r.to.table)?)))
        .collect();
    let clusters = (!schema.groups.is_empty()).then(|| {
        let mut c: Vec<Vec<usize>> = schema
            .groups
            .iter()
            .map(|g| g.tables.iter().filter_map(|t| idx(t)).collect())
            .collect();
        c.push(
            (0..schema.tables.len())
                .filter(|&i| schema.group_of(&schema.tables[i].name).is_none())
                .collect(),
        );
        c
    });
    let pos = layout::compute_diagram_positions(&col_counts, &edges, clusters.as_deref());
    schema
        .tables
        .iter()
        .zip(pos)
        .map(|(t, p)| (t.name.clone(), (p.x, p.y)))
        .collect()
}

/// O canvas renderiza sozinho: arrastar/zoom não redesenha a janela inteira.
impl Render for Diagram {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.glass {
            Some(g) => self.view(&g, cx),
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
            (self.zoom * 100.).round() as i32,
            self.hint(),
            self.schema.tables.len(),
            self.schema.refs.len() + self.schema.groups.len(),
        )
    }

    pub fn view(&mut self, g: &Glass, cx: &mut Context<Self>) -> AnyElement {
        struct Edge {
            a: Point<Pixels>,
            b: Point<Pixels>,
            sa: f32,
            sb: f32,
            pts: Vec<Point<Pixels>>,
            lit: bool,
            ends: (&'static str, &'static str),
        }
        if self.needs_fit {
            self.do_fit();
        }
        let z = self.zoom;
        let (pan_x, pan_y) = self.pan;
        let s = move |v: f32| px(v * z);
        let schema = &self.schema;

        // arestas coluna→coluna: âncoras locais ao canvas + cardinalidade + destaque
        let sel = self.selected.clone();
        let edges: Vec<Edge> = schema
            .refs
            .iter()
            .filter_map(|r| {
                let ft = schema.table(&r.from.table)?;
                let tt = schema.table(&r.to.table)?;
                let fc = ft.columns.iter().position(|c| c.name == r.from.column)?;
                let tc = tt.columns.iter().position(|c| c.name == r.to.column)?;
                let (ax, ay) = self.pos[&ft.name];
                let (bx, by) = self.pos[&tt.name];
                let ay = ay + HEADER_H + 4. + ROW_H * (fc as f32 + 0.5);
                let by = by + HEADER_H + 4. + ROW_H * (tc as f32 + 0.5);
                // lado de saída/entrada: +1 borda direita, -1 esquerda
                let (sa, sb) = if ax + CARD_W < bx {
                    (1., -1.)
                } else if bx + CARD_W < ax {
                    (-1., 1.)
                } else {
                    (1., 1.)
                };
                let (ax, bx) = (
                    if sa > 0. { ax + CARD_W } else { ax },
                    if sb > 0. { bx + CARD_W } else { bx },
                );
                let obstacles: Vec<layout::Rect> = schema
                    .tables
                    .iter()
                    .filter(|t| t.name != ft.name && t.name != tt.name)
                    .map(|t| {
                        let (x, y) = self.pos[&t.name];
                        layout::Rect {
                            x,
                            y,
                            w: CARD_W,
                            h: card_h(t.columns.len()),
                        }
                    })
                    .collect();
                let pts = layout::route(
                    layout::Pos { x: ax, y: ay },
                    sa,
                    layout::Pos { x: bx, y: by },
                    sb,
                    &obstacles,
                    24.,
                )
                .into_iter()
                .map(|p| point(px(p.x * z + pan_x), px(p.y * z + pan_y)))
                .collect();
                let ends = match r.kind {
                    schema::RefKind::ManyToOne => ("N", "1"),
                    schema::RefKind::OneToMany => ("1", "N"),
                    schema::RefKind::OneToOne => ("1", "1"),
                    schema::RefKind::ManyToMany => ("N", "N"),
                };
                let is_sel_ref = self.selected_ref.as_ref().is_some_and(|(f, t)| {
                    f.0 == r.from.table
                        && f.1 == r.from.column
                        && t.0 == r.to.table
                        && t.1 == r.to.column
                });
                let lit =
                    is_sel_ref || self.is_selected(&r.from.table) || self.is_selected(&r.to.table);
                Some(Edge {
                    a: point(px(ax * z + pan_x), px(ay * z + pan_y)),
                    b: point(px(bx * z + pan_x), px(by * z + pan_y)),
                    sa,
                    sb,
                    pts,
                    lit,
                    ends,
                })
            })
            .collect();
        let has_sel = sel.is_some() || !self.multi.is_empty() || self.selected_ref.is_some();
        let ink = g.text_muted;
        let labels: Vec<(Point<Pixels>, &'static str)> = edges
            .iter()
            .flat_map(|e| {
                [
                    (
                        point(e.a.x + px(e.sa * 10. * z) - px(4. * z), e.a.y - px(15. * z)),
                        e.ends.0,
                    ),
                    (
                        point(e.b.x + px(e.sb * 10. * z) - px(4. * z), e.b.y - px(15. * z)),
                        e.ends.1,
                    ),
                ]
            })
            .collect();
        // relação sendo arrastada: linha provisória da coluna de origem até o cursor
        let mut edges = edges;
        if let Some(Drag::Link { from, now }) = &self.drag
            && let (Some(t), Some(&(x, y))) = (schema.table(&from.0), self.pos.get(&from.0))
            && let Some(row) = t.columns.iter().position(|c| c.name == from.1)
        {
            let a = point(
                px((x + CARD_W) * z + pan_x),
                px((y + HEADER_H + 4. + ROW_H * (row as f32 + 0.5)) * z + pan_y),
            );
            let b = point(px(now.0 * z + pan_x), px(now.1 * z + pan_y));
            edges.push(Edge {
                a,
                b,
                sa: 1.,
                sb: -1.,
                pts: vec![a, b],
                lit: true,
                ends: ("", ""),
            });
        }
        let stroke = g.accent;
        let dot = g.grid;
        let entity = cx.entity();
        let mut root = div()
            .id("diagram")
            .role(Role::Group)
            .aria_label("Diagram")
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
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "space" && !this.space {
                    this.space = true;
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
                    window.focus(&this.focus, cx);
                    this.drag = Some(Drag::Pan { last: e.position });
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    if this.tool == Tool::Hand || this.space {
                        this.drag = Some(Drag::Pan { last: e.position });
                    } else {
                        let at = this.to_world(e.position);
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
                    Some(Drag::Marquee { now: n, .. } | Drag::Link { now: n, .. }) => *n = now,
                    Some(Drag::Pan { last }) => {
                        this.pan.0 += f32::from(e.position.x - last.x);
                        this.pan.1 += f32::from(e.position.y - last.y);
                        *last = e.position;
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
            );

        root = root.child(
            canvas(
                |_, _, _| (),
                move |bounds, (), window, cx| {
                    // informa origem/tamanho reais ao estado (usados por zoom e fit)
                    entity.update(cx, |d, cx| {
                        if d.origin != bounds.origin || d.size != bounds.size {
                            d.origin = bounds.origin;
                            d.size = bounds.size;
                            cx.notify();
                        }
                        // ajuste à tela assim que o tamanho real é conhecido (avisa o shell)
                        if d.needs_fit {
                            d.do_fit();
                            cx.notify();
                        }
                    });
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
                        let pts: Vec<_> = e.pts.iter().map(|p| bounds.origin + *p).collect();
                        let color = if e.lit {
                            stroke
                        } else if has_sel {
                            ink.opacity(0.2)
                        } else {
                            ink.opacity(0.55)
                        };
                        let w = if e.lit { 2. } else { 1.5 };
                        if let Some(path) = polyline(&pts, z, w) {
                            window.paint_path(path, color);
                        }
                        // portas: pontos nas extremidades, como num conector
                        let r = (3. * z).max(2.);
                        for p in [a, b] {
                            let at = p - point(px(r), px(r));
                            window.paint_quad(
                                fill(Bounds::new(at, size(px(2. * r), px(2. * r))), color)
                                    .corner_radii(px(r)),
                            );
                        }
                    }
                },
            )
            .absolute()
            .size_full(),
        );

        for (at, text) in labels {
            root = root.child(
                div()
                    .absolute()
                    .left(at.x)
                    .top(at.y)
                    .text_size(s(10.))
                    .text_color(ink)
                    .child(text),
            );
        }

        // molduras de grupo (abaixo dos cartões): caixa que envolve as tabelas do grupo
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
            let name = gr.name.clone();
            root = root.child(
                div()
                    .id(SharedString::from(format!("group-{}", gr.name)))
                    .role(Role::Group)
                    .aria_label(format!("Group {}", gr.name))
                    .absolute()
                    .left(s(x0 - pad) + px(pan_x))
                    .top(s(y0 - pad - 8.) + px(pan_y))
                    .w(s(x1 - x0 + 2. * pad))
                    .h(s(y1 - y0 + 2. * pad + 8.))
                    .rounded(s(14.))
                    .border_1()
                    .border_color(c.opacity(0.4))
                    .bg(c.opacity(0.06))
                    .child(
                        div()
                            .id(SharedString::from(format!("group-label-{}", gr.name)))
                            .absolute()
                            .top(s(8.))
                            .left(s(10.))
                            .px(s(8.))
                            .h(s(22.))
                            .rounded(s(6.))
                            .flex()
                            .items_center()
                            .gap(s(6.))
                            .bg(c.opacity(0.16))
                            .text_size(s(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(c)
                            .when(self.tool == Tool::Select, gpui_kit::Styled::cursor_move)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                                    if this.tool == Tool::Hand {
                                        return;
                                    }
                                    window.focus(&this.focus, cx);
                                    let (lx, ly) = this.to_local(e.position);
                                    let at = (
                                        (lx - this.pan.0) / this.zoom,
                                        (ly - this.pan.1) / this.zoom,
                                    );
                                    this.drag = Some(Drag::Group {
                                        name: name.clone(),
                                        last: at,
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            )
                            .child(div().size(s(7.)).rounded_full().bg(c))
                            .child(gr.name.clone())
                            .child(
                                div()
                                    .font_weight(FontWeight::NORMAL)
                                    .opacity(0.7)
                                    .child(format!("{}", members.len())),
                            ),
                    ),
            );
        }

        // colunas que são origem de alguma relação (FK)
        let fks: std::collections::HashSet<(&str, &str)> = schema
            .refs
            .iter()
            .map(|r| (r.from.table.as_str(), r.from.column.as_str()))
            .collect();
        let mut order: Vec<_> = schema.tables.iter().collect();
        order.sort_by_key(|t| Some(&t.name) == self.top.as_ref());
        let (vw, vh) = (f32::from(self.size.width), f32::from(self.size.height));
        for t in order {
            let (x, y) = self.pos[&t.name];
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
            let name = t.name.clone();
            let on = self.is_selected(&t.name);
            // cor: a da tabela (headercolor) ou a do grupo
            let tint = t
                .color
                .as_deref()
                .or_else(|| schema.group_of(&t.name).and_then(|gr| gr.color.as_deref()))
                .and_then(hex)
                .or_else(|| {
                    schema
                        .group_of(&t.name)
                        .map(|gr| group_color(schema, &gr.name))
                });
            let mut rows: Vec<AnyElement> = Vec::new();
            for (i, c) in t.columns.iter().enumerate() {
                let fk = c.fk || fks.contains(&(t.name.as_str(), c.name.as_str()));
                let icon = if c.pk {
                    Some(Icon::new(IconName::KeyRound).text_color(g.accent))
                } else if fk {
                    Some(Icon::new(IconName::Link2).text_color(g.text_muted))
                } else {
                    None
                };
                let from = (t.name.clone(), c.name.clone());
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
                        .hover({
                            let h = g.hover;
                            move |st| st.bg(h)
                        })
                        // trilho fixo do ícone: nomes alinhados com ou sem ícone
                        .child(
                            div()
                                .w(s(14.))
                                .flex()
                                .justify_center()
                                .children(icon.map(|i| i.size(s(12. * g.ui_scale)))),
                        )
                        .child(
                            div()
                                .flex_1()
                                .truncate()
                                .text_size(s(13. * g.ui_scale))
                                .text_color(g.text)
                                .when(c.pk || fk, |d| d.font_weight(FontWeight::MEDIUM))
                                .child(c.name.clone()),
                        )
                        .child(
                            div()
                                .text_size(s(11.5 * g.ui_scale))
                                .text_color(g.text_muted)
                                .child(c.ty.clone()),
                        )
                        .when(fk, |d| {
                            d.child(
                                div()
                                    .px(s(4.))
                                    .rounded(s(3.))
                                    .bg(g.selected)
                                    .text_size(s(9.5 * g.ui_scale))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(g.accent)
                                    .child("FK"),
                            )
                        })
                        .when(c.not_null, |d| {
                            d.child(
                                div()
                                    .text_size(s(10. * g.ui_scale))
                                    .text_color(g.text_muted)
                                    .child("NN"),
                            )
                        })
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
                                .group_hover("col", gpui_kit::Styled::visible)
                                .cursor_crosshair()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, e: &MouseDownEvent, window, cx| {
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
            root = root.child(
                div()
                    .id(SharedString::from(format!("table-{}", t.name)))
                    .role(Role::Group)
                    .aria_label(format!("Table {}", t.name))
                    .absolute()
                    .left(s(x) + px(pan_x))
                    .top(s(y) + px(pan_y))
                    .w(s(CARD_W))
                    .h(s(card_h(t.columns.len())))
                    .rounded(RADIUS_CARD * z)
                    .group("card")
                    .border_1()
                    .border_color(if on { g.accent } else { g.edge })
                    // sombra suave sempre; selecionado ganha um halo do destaque
                    .shadow({
                        let mut sh = vec![card_shadow(g.mode)];
                        if on {
                            sh.push(BoxShadow {
                                color: g.accent.opacity(0.25),
                                offset: point(px(0.), px(0.)),
                                blur_radius: px(0.),
                                spread_radius: px(3.),
                                inset: false,
                            });
                        }
                        sh
                    })
                    .bg(if self.translucent_cards {
                        g.card.opacity(0.55)
                    } else {
                        g.card
                    })
                    .when(self.tool == Tool::Select, gpui_kit::Styled::cursor_move)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                            if this.tool == Tool::Hand || this.space {
                                return;
                            }
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
                            let (px_, py_) = this.pos[&name];
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
                            cx.emit(DiagramEvent::Select(name.clone()));
                            let _ = grab;
                            this.drag = Some(Drag::Cards {
                                last: this.to_world(e.position),
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .children(tint.map(|c| {
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .w_full()
                            .h(s(3.))
                            .rounded_t(RADIUS_CARD * z)
                            .bg(c)
                    }))
                    .child(
                        div()
                            .h(s(HEADER_H))
                            .px(s(12.))
                            .flex()
                            .items_center()
                            .gap(s(8.))
                            .rounded_t(RADIUS_CARD * z)
                            .border_b_1()
                            .border_color(g.edge_soft)
                            // sem fundo próprio: só um véu da cor da tabela, se houver
                            .when_some(tint, |d, c| d.bg(c.opacity(0.08)))
                            .child(
                                div()
                                    .size(s(22.))
                                    .rounded(s(6.))
                                    .bg(tint.map_or(g.selected, |c| c.opacity(0.2)))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        Icon::new(IconName::Table2)
                                            .size(s(13. * g.ui_scale))
                                            .text_color(tint.unwrap_or(g.accent)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(s(14. * g.ui_scale))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(g.text)
                                    .child(t.name.clone()),
                            )
                            .child(
                                div()
                                    .px(s(6.))
                                    .rounded(s(5.))
                                    .border_1()
                                    .border_color(g.edge)
                                    .text_size(s(10.5 * g.ui_scale))
                                    .text_color(g.text_muted)
                                    .child(t.columns.len().to_string()),
                            ),
                    )
                    .child(div().pt(s(4.)).children(rows))
                    .child({
                        let table = t.name.clone();
                        div()
                            .id(SharedString::from(format!("addcol-{}", t.name)))
                            .role(Role::Button)
                            .aria_label("Add column")
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
                    }),
            );
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
        root = root.children(self.overlays(g, cx));
        root.into_any_element()
    }
}

impl Diagram {
    /// Camadas flutuantes do canvas: ferramentas, legenda, zoom, ajuda e estado vazio.
    fn overlays(&self, g: &Glass, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let island = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .role(Role::Toolbar)
                .aria_label(label)
                .flex()
                .items_center()
                .gap(px(2.))
                .p(px(4.))
                .rounded(RADIUS_ISLAND)
                .border_1()
                .border_color(g.edge)
                .bg(g.island)
                .shadow(elevation(g.mode))
                // clique nas ilhas não inicia pan/seleção no fundo
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };
        let sep = || div().w(px(1.)).h(px(18.)).mx(px(4.)).bg(g.edge);
        // botão de ferramenta: a ativa fica numa "bolha" de vidro (fundo claro + aresta realçada)
        let tool = |id: &'static str,
                    icon: IconName,
                    tip: &'static str,
                    action: Box<dyn Action>,
                    on: bool| {
            let tip_action = action.boxed_clone();
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(tip)
                .size(px(32.))
                .rounded(px(8.))
                .flex()
                .items_center()
                .justify_center()
                .border_1()
                .border_color(if on {
                    g.island_sheen
                } else {
                    gpui_kit::transparent_black()
                })
                .text_color(if on { g.text } else { g.text_muted })
                .when(on, |d| d.bg(g.hover))
                .hover({
                    let (h, t) = (g.hover, g.text);
                    move |st| st.bg(h).text_color(t)
                })
                .tooltip(move |window, cx| {
                    Tooltip::new(tip)
                        .action(tip_action.as_ref(), Some(actions::CANVAS))
                        .build(window, cx)
                })
                .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
                .child(Icon::new(icon).size(px(17.)))
        };
        let mut out = Vec::new();

        // ferramentas (topo, centro)
        out.push(
            div()
                .absolute()
                .top(px(12.))
                .w_full()
                .flex()
                .justify_center()
                .child(
                    island("canvas-tools", "Tools")
                        .occlude()
                        .child(tool(
                            "tool-select",
                            IconName::MousePointer2,
                            "Select",
                            Box::new(ToolSelect),
                            self.tool == Tool::Select,
                        ))
                        .child(tool(
                            "tool-hand",
                            IconName::Hand,
                            "Hand: pan view",
                            Box::new(ToolHand),
                            self.tool == Tool::Hand,
                        ))
                        .child(sep())
                        .child(tool(
                            "tool-table",
                            IconName::Plus,
                            "New Table",
                            Box::new(NewTable),
                            false,
                        ))
                        .child(sep())
                        .child(tool(
                            "tool-layout",
                            IconName::LayoutGrid,
                            "Auto-Layout",
                            Box::new(Relayout),
                            false,
                        ))
                        .child(tool(
                            "tool-fit",
                            IconName::Scan,
                            "Fit to Screen",
                            Box::new(FitView),
                            false,
                        ))
                        .child(sep())
                        .child(tool(
                            "tool-editor",
                            IconName::Braces,
                            "DBML Editor",
                            Box::new(ToggleEditor),
                            false,
                        )),
                )
                .into_any_element(),
        );

        // zoom (base, esquerda)
        let pct = format!("{}%", (self.zoom * 100.).round() as i32);
        out.push(
            div()
                .absolute()
                .bottom(px(12.))
                .left(px(12.))
                .child(
                    island("canvas-zoom", "Zoom")
                        .occlude()
                        .child(tool(
                            "zoom-out",
                            IconName::ZoomOut,
                            "Zoom Out",
                            Box::new(ZoomOut),
                            false,
                        ))
                        .child(
                            Button::new("zoom-pct")
                                .ghost()
                                .small()
                                .label(pct)
                                .tooltip_with_action(
                                    "Fit to Screen",
                                    &FitView,
                                    Some(actions::CANVAS),
                                )
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(FitView), cx);
                                }),
                        )
                        .child(tool(
                            "zoom-in",
                            IconName::ZoomIn,
                            "Zoom In",
                            Box::new(ZoomIn),
                            false,
                        )),
                )
                .into_any_element(),
        );

        // ajuda (base, direita)
        out.push(
            div()
                .absolute()
                .bottom(px(12.))
                .right(px(12.))
                .child(island("canvas-help", "Legend").occlude().child(tool(
                    "help-legend",
                    IconName::Info,
                    "Legend",
                    Box::new(ToggleLegend),
                    self.legend_open,
                )))
                .into_any_element(),
        );

        // legenda (base, esquerda)
        let row = |swatch: AnyElement, text: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(TEXT_XS)
                .text_color(g.text_muted)
                .child(div().w(px(22.)).flex().justify_center().child(swatch))
                .child(text)
        };
        let legend = {
            island("canvas-legend", "Legend")
                .flex_col()
                .items_start()
                .gap(px(6.))
                .p(px(10.))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .text_size(TEXT_XS)
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(g.text)
                                .child("Legend"),
                        )
                        .child(
                            Button::new("legend-close")
                                .ghost()
                                .xsmall()
                                .icon(IconName::X)
                                .tooltip_with_action(
                                    "Hide legend",
                                    &ToggleLegend,
                                    Some(actions::CTX),
                                )
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(ToggleLegend), cx);
                                }),
                        ),
                )
                .child(row(
                    div().text_size(TEXT_XS).child("PK").into_any_element(),
                    "Primary key",
                ))
                .child(row(
                    div()
                        .text_size(TEXT_XS)
                        .text_color(g.accent)
                        .child("FK")
                        .into_any_element(),
                    "Foreign key (id_table)",
                ))
                .child(row(
                    div()
                        .w(px(20.))
                        .h(px(1.5))
                        .bg(g.text_muted)
                        .into_any_element(),
                    "Relation (column → column)",
                ))
                .child(row(
                    div().text_size(TEXT_XS).child("N · 1").into_any_element(),
                    "Cardinality: many · one",
                ))
                .child(row(
                    div().w(px(20.)).h(px(2.)).bg(g.accent).into_any_element(),
                    "Selected table and its relations",
                ))
        };
        if self.legend_open {
            out.push(
                div()
                    .absolute()
                    .bottom(px(52.))
                    .right(px(12.))
                    .max_w(px(280.))
                    .child(legend.occlude())
                    .into_any_element(),
            );
        }
        // estado vazio
        if self.schema.tables.is_empty() {
            out.push(
                div()
                    .absolute()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(crate::widgets::empty_state(
                        g,
                        IconName::Database,
                        "No tables yet",
                        "Write your DBML schema in the editor or open an existing file.",
                        crate::widgets::EmptyAction {
                            id: "empty-editor",
                            icon: IconName::Braces,
                            label: "Open DBML Editor",
                            keys: "Ctrl+E",
                            action: Box::new(ToggleEditor),
                        },
                        crate::widgets::EmptyAction {
                            id: "empty-open",
                            icon: IconName::FolderOpen,
                            label: "Open File",
                            keys: "Ctrl+O",
                            action: Box::new(OpenFile),
                        },
                    ))
                    .into_any_element(),
            );
        }
        let _ = cx;
        out
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

/// Polilinha ortogonal com cantos arredondados (raio limitado pela metade dos segmentos).
fn polyline(pts: &[Point<Pixels>], z: f32, width: f32) -> Option<Path<Pixels>> {
    let f = |p: Point<Pixels>| (f32::from(p.x), f32::from(p.y));
    let mut pb = PathBuilder::stroke(px(width));
    pb.move_to(*pts.first()?);
    for w in pts.windows(3) {
        let (p, c, n) = (f(w[0]), f(w[1]), f(w[2]));
        let (l1, l2) = (
            (c.0 - p.0).abs() + (c.1 - p.1).abs(),
            (n.0 - c.0).abs() + (n.1 - c.1).abs(),
        );
        let r = (8. * z).min(l1 / 2.).min(l2 / 2.);
        if r < 0.5 {
            pb.line_to(w[1]);
            continue;
        }
        let d_in = (((c.0 - p.0) / l1.max(0.001)), ((c.1 - p.1) / l1.max(0.001)));
        let d_out = (((n.0 - c.0) / l2.max(0.001)), ((n.1 - c.1) / l2.max(0.001)));
        pb.line_to(point(px(c.0 - d_in.0 * r), px(c.1 - d_in.1 * r)));
        pb.curve_to(point(px(c.0 + d_out.0 * r), px(c.1 + d_out.1 * r)), w[1]);
    }
    pb.line_to(*pts.last()?);
    pb.build().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
