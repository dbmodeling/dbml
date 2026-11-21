//! Overlay do Tour Guiado (*Spotlight Tour*) e Modal de Novidades (*What's New*) no Desktop GPUI.

use super::{Pane, Shell, dialog_bare};
use crate::actions::{QuickSearch, ShowTour, ShowWhatsNew, ToggleMinimap};
use crate::constants::{
    ENV_NO_GUIDE, ENV_PRESELECT, GUIDE_ACTION_QUICK_SEARCH, GUIDE_ACTION_TOGGLE_MINIMAP,
    TOUR_ARROW_BADGE_SIZE_PX, TOUR_ARROW_HALF_PX, TOUR_ARROW_INSET_PX, TOUR_ARROW_MIN_OFFSET_PX,
    TOUR_ARROW_OVERHANG_PX, TOUR_BOUNDS_EPSILON_PX, TOUR_CARD_GAP_PX, TOUR_CARD_H_PX,
    TOUR_CARD_W_PX, TOUR_HOLE_PAD_PX, TOUR_MIN_VIEWPORT_H_PX, TOUR_MIN_VIEWPORT_W_PX,
    TOUR_VIEWPORT_MARGIN_PX, WHATS_NEW_DIALOG_MAX_H_PX, WHATS_NEW_DIALOG_W_PX,
};
use crate::glass::Glass;
use crate::guide::{
    HighlightKind, StartupGuide, TourPlacement, TourTarget, decide_startup_guide, product_guide,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, WindowExt};
use gpui_kit::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Default)]
pub struct TourBoundsState {
    pub rects: HashMap<TourTarget, BoxRect>,
    pub active_target: Option<TourTarget>,
}

pub type TourBoundsMap = Rc<RefCell<TourBoundsState>>;

/// Registra o `Bounds<Pixels>` real medido pelo layout do GPUI para um alvo do tour.
pub fn record_tour_bounds(
    map: &TourBoundsMap,
    target: TourTarget,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    let rect = BoxRect {
        x: f32::from(bounds.origin.x),
        y: f32::from(bounds.origin.y),
        w: f32::from(bounds.size.width),
        h: f32::from(bounds.size.height),
    };
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    let mut state = map.borrow_mut();
    let changed = state.rects.get(&target).is_none_or(|prev| {
        (prev.x - rect.x).abs() > TOUR_BOUNDS_EPSILON_PX
            || (prev.y - rect.y).abs() > TOUR_BOUNDS_EPSILON_PX
            || (prev.w - rect.w).abs() > TOUR_BOUNDS_EPSILON_PX
            || (prev.h - rect.h).abs() > TOUR_BOUNDS_EPSILON_PX
    });
    if changed {
        state.rects.insert(target, rect);
        if state.active_target == Some(target) {
            window.refresh();
        }
    }
}

/// Elemento invisível anexado como filho de um controle real para capturar seu `Bounds<Pixels>`.
pub fn track_tour_anchor(target: TourTarget, map: TourBoundsMap) -> AnyElement {
    canvas(
        move |bounds, window, _| {
            record_tour_bounds(&map, target, bounds, window);
        },
        |_, (), _, _| {},
    )
    .absolute()
    .inset_0()
    .into_any_element()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ArrowSide {
    Top,
    Bottom,
    Left,
    Right,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct CardLayout {
    pub x: f32,
    pub y: f32,
    pub arrow: ArrowSide,
    pub arrow_offset: f32,
}

/// Calcula a posição do card popover e o lado da seta sem sair dos limites da janela.
pub(super) fn compute_card_layout(
    target: Option<BoxRect>,
    preferred: TourPlacement,
    vp_w: f32,
    vp_h: f32,
) -> CardLayout {
    let clamp = |v: f32, lo: f32, hi: f32| v.clamp(lo, hi.max(lo));
    let Some(t) = target.filter(|r| r.w > 0.0 && r.h > 0.0 && preferred != TourPlacement::Center)
    else {
        return CardLayout {
            x: clamp(
                (vp_w - TOUR_CARD_W_PX) * 0.5,
                TOUR_VIEWPORT_MARGIN_PX,
                vp_w - TOUR_CARD_W_PX - TOUR_VIEWPORT_MARGIN_PX,
            ),
            y: clamp(
                (vp_h - TOUR_CARD_H_PX) * 0.5,
                TOUR_VIEWPORT_MARGIN_PX,
                vp_h - TOUR_CARD_H_PX - TOUR_VIEWPORT_MARGIN_PX,
            ),
            arrow: ArrowSide::None,
            arrow_offset: 0.0,
        };
    };

    let cx = t.x + t.w * 0.5;
    let cy = t.y + t.h * 0.5;

    let fits = |side: TourPlacement| match side {
        TourPlacement::Bottom => {
            t.y + t.h
                + TOUR_HOLE_PAD_PX
                + TOUR_CARD_GAP_PX
                + TOUR_CARD_H_PX
                + TOUR_VIEWPORT_MARGIN_PX
                <= vp_h
        }
        TourPlacement::Top => {
            t.y - TOUR_HOLE_PAD_PX - TOUR_CARD_GAP_PX - TOUR_CARD_H_PX - TOUR_VIEWPORT_MARGIN_PX
                >= 0.0
        }
        TourPlacement::Right => {
            t.x + t.w
                + TOUR_HOLE_PAD_PX
                + TOUR_CARD_GAP_PX
                + TOUR_CARD_W_PX
                + TOUR_VIEWPORT_MARGIN_PX
                <= vp_w
        }
        TourPlacement::Left => {
            t.x - TOUR_HOLE_PAD_PX - TOUR_CARD_GAP_PX - TOUR_CARD_W_PX - TOUR_VIEWPORT_MARGIN_PX
                >= 0.0
        }
        TourPlacement::Center => true,
    };

    let order = match preferred {
        TourPlacement::Bottom => [
            TourPlacement::Bottom,
            TourPlacement::Top,
            TourPlacement::Right,
            TourPlacement::Left,
        ],
        TourPlacement::Top => [
            TourPlacement::Top,
            TourPlacement::Bottom,
            TourPlacement::Right,
            TourPlacement::Left,
        ],
        TourPlacement::Right => [
            TourPlacement::Right,
            TourPlacement::Left,
            TourPlacement::Bottom,
            TourPlacement::Top,
        ],
        TourPlacement::Left | TourPlacement::Center => [
            TourPlacement::Left,
            TourPlacement::Right,
            TourPlacement::Bottom,
            TourPlacement::Top,
        ],
    };

    let side = order.into_iter().find(|&s| fits(s)).unwrap_or(order[0]);
    match side {
        TourPlacement::Bottom | TourPlacement::Top => {
            let x = clamp(
                cx - TOUR_CARD_W_PX * 0.5,
                TOUR_VIEWPORT_MARGIN_PX,
                vp_w - TOUR_CARD_W_PX - TOUR_VIEWPORT_MARGIN_PX,
            );
            let y = if side == TourPlacement::Bottom {
                clamp(
                    t.y + t.h + TOUR_HOLE_PAD_PX + TOUR_CARD_GAP_PX,
                    TOUR_VIEWPORT_MARGIN_PX,
                    vp_h - TOUR_CARD_H_PX - TOUR_VIEWPORT_MARGIN_PX,
                )
            } else {
                clamp(
                    t.y - TOUR_HOLE_PAD_PX - TOUR_CARD_GAP_PX - TOUR_CARD_H_PX,
                    TOUR_VIEWPORT_MARGIN_PX,
                    vp_h - TOUR_CARD_H_PX - TOUR_VIEWPORT_MARGIN_PX,
                )
            };
            CardLayout {
                x,
                y,
                arrow: if side == TourPlacement::Bottom {
                    ArrowSide::Top
                } else {
                    ArrowSide::Bottom
                },
                arrow_offset: clamp(
                    cx - x,
                    TOUR_ARROW_MIN_OFFSET_PX,
                    TOUR_CARD_W_PX - TOUR_ARROW_MIN_OFFSET_PX,
                ),
            }
        }
        _ => {
            let x = if side == TourPlacement::Right {
                clamp(
                    t.x + t.w + TOUR_HOLE_PAD_PX + TOUR_CARD_GAP_PX,
                    TOUR_VIEWPORT_MARGIN_PX,
                    vp_w - TOUR_CARD_W_PX - TOUR_VIEWPORT_MARGIN_PX,
                )
            } else {
                clamp(
                    t.x - TOUR_HOLE_PAD_PX - TOUR_CARD_GAP_PX - TOUR_CARD_W_PX,
                    TOUR_VIEWPORT_MARGIN_PX,
                    vp_w - TOUR_CARD_W_PX - TOUR_VIEWPORT_MARGIN_PX,
                )
            };
            let y = clamp(
                cy - TOUR_CARD_H_PX * 0.5,
                TOUR_VIEWPORT_MARGIN_PX,
                vp_h - TOUR_CARD_H_PX - TOUR_VIEWPORT_MARGIN_PX,
            );
            CardLayout {
                x,
                y,
                arrow: if side == TourPlacement::Right {
                    ArrowSide::Left
                } else {
                    ArrowSide::Right
                },
                arrow_offset: clamp(
                    cy - y,
                    TOUR_ARROW_MIN_OFFSET_PX,
                    TOUR_CARD_H_PX - TOUR_ARROW_MIN_OFFSET_PX,
                ),
            }
        }
    }
}

fn dispatch_named_action(name: &str, window: &mut Window, cx: &mut App) {
    match name {
        GUIDE_ACTION_TOGGLE_MINIMAP => window.dispatch_action(Box::new(ToggleMinimap), cx),
        GUIDE_ACTION_QUICK_SEARCH => window.dispatch_action(Box::new(QuickSearch), cx),
        _ => {}
    }
}

impl Shell {
    pub(super) fn maybe_start_guide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if crate::harness::var(ENV_NO_GUIDE).is_some()
            || crate::harness::var(ENV_PRESELECT).is_some()
        {
            return;
        }
        let current = env!("CARGO_PKG_VERSION");
        match decide_startup_guide(
            current,
            self.onboarding_done,
            self.last_seen_version.as_deref(),
        ) {
            StartupGuide::Tour => {
                self.start_tour(0, cx);
            }
            StartupGuide::WhatsNew => {
                self.open_whats_new(window, cx);
            }
            StartupGuide::None => {}
        }
    }

    pub(super) fn show_tour(&mut self, _: &ShowTour, _window: &mut Window, cx: &mut Context<Self>) {
        self.start_tour(0, cx);
    }

    pub(super) fn show_whats_new(
        &mut self,
        _: &ShowWhatsNew,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_whats_new(window, cx);
    }

    fn start_tour(&mut self, step: usize, cx: &mut Context<Self>) {
        let guide = product_guide();
        if guide.tour.is_empty() {
            return;
        }
        let idx = step.min(guide.tour.len().saturating_sub(1));
        let target = guide.tour[idx].target;
        self.tour_bounds.borrow_mut().active_target = Some(target);
        self.ensure_tour_target(target, cx);
        self.tour_step = Some(idx);
        cx.notify();
    }

    fn ensure_tour_target(&mut self, target: TourTarget, cx: &mut Context<Self>) {
        match target {
            TourTarget::Sidebar => {
                self.sidebar_open = true;
                self.pane = Pane::Schema;
            }
            TourTarget::Inspector => {
                self.inspector_open = true;
                self.with_diagram(cx, |d| {
                    if d.selected().is_none()
                        && d.selected_enum().is_none()
                        && d.selected_ref().is_none()
                        && let Some(first) = d.schema().tables.first()
                    {
                        let qname = first.qualified_name();
                        d.select(&qname);
                    }
                });
            }
            TourTarget::Minimap => {
                self.with_diagram(cx, |d| {
                    d.minimap_open = true;
                });
            }
            TourTarget::TopbarAdd | TourTarget::Canvas | TourTarget::ExportBtn => {}
        }
    }

    fn finish_tour(&mut self, cx: &mut Context<Self>) {
        self.tour_step = None;
        self.tour_bounds.borrow_mut().active_target = None;
        self.onboarding_done = true;
        self.last_seen_version = Some(env!("CARGO_PKG_VERSION").to_string());
        self.save_prefs();
        cx.notify();
    }

    fn step_tour(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(cur) = self.tour_step else { return };
        let total = product_guide().tour.len();
        if total == 0 {
            self.finish_tour(cx);
            return;
        }
        let next = cur as isize + delta;
        if next < 0 {
            return;
        }
        if (next as usize) >= total {
            self.finish_tour(cx);
        } else {
            self.start_tour(next as usize, cx);
        }
    }

    fn open_whats_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tour_step = None;
        self.last_seen_version = Some(env!("CARGO_PKG_VERSION").to_string());
        self.save_prefs();

        let g = Glass::of_scaled(self.mode, self.translucent, self.ui_scale);
        let lang = crate::i18n::current();
        let guide = product_guide();
        let version = env!("CARGO_PKG_VERSION");

        window.open_dialog(cx, move |dialog, _, _| {
            dialog_bare(dialog, &g, WHATS_NEW_DIALOG_W_PX).content(move |content, _, _| {
                let mut list = div().flex().flex_col().gap(px(14.)).px(px(18.)).py(px(12.));
                for rel in &guide.releases {
                    let mut section = div().flex().flex_col().gap(px(8.)).child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .px(px(7.))
                                    .py(px(2.))
                                    .rounded(px(6.))
                                    .bg(g.accent.opacity(0.16))
                                    .text_color(g.accent)
                                    .font_family(crate::glass::FONT_MONO)
                                    .text_size(px(11.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format!("v{}", rel.version)),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(g.text_faint)
                                    .child(rel.date.clone()),
                            ),
                    );
                    for (hi, h) in rel.highlights.iter().enumerate() {
                        let (badge_txt, badge_color) = match h.kind {
                            HighlightKind::Added => (crate::i18n::t("Novo"), g.ok),
                            HighlightKind::Improved => (crate::i18n::t("Melhorado"), g.accent),
                            HighlightKind::Fixed => (crate::i18n::t("Corrigido"), g.warning),
                        };
                        let mut row = div()
                            .flex()
                            .items_start()
                            .gap(px(10.))
                            .p(px(10.))
                            .rounded(px(9.))
                            .bg(g.input)
                            .border_1()
                            .border_color(g.edge_soft)
                            .child(
                                div()
                                    .px(px(7.))
                                    .py(px(2.))
                                    .rounded(px(99.))
                                    .bg(badge_color.opacity(0.18))
                                    .text_color(badge_color)
                                    .text_size(px(10.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(badge_txt),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(3.))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(g.text)
                                            .child(h.title.pick(lang).to_string()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(g.text_muted)
                                            .child(h.detail.pick(lang).to_string()),
                                    ),
                            );
                        if let Some(action_name) = h.action.clone() {
                            row = row.child(
                                div()
                                    .id(("wn-try", hi))
                                    .px(px(9.))
                                    .py(px(4.))
                                    .rounded(px(7.))
                                    .border_1()
                                    .border_color(g.edge_strong)
                                    .text_size(px(11.5))
                                    .text_color(g.text)
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(g.hover))
                                    .on_click(move |_, window, cx| {
                                        window.close_dialog(cx);
                                        dispatch_named_action(&action_name, window, cx);
                                    })
                                    .child(crate::i18n::t("Experimentar")),
                            );
                        }
                        section = section.child(row);
                    }
                    list = list.child(section);
                }

                content
                    .flex()
                    .flex_col()
                    .max_h(px(WHATS_NEW_DIALOG_MAX_H_PX))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .px(px(18.))
                            .pt(px(16.))
                            .pb(px(10.))
                            .border_b_1()
                            .border_color(g.edge_soft)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .text_size(px(15.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(g.text)
                                            .child(
                                                crate::i18n::t("Novidades da versão {}")
                                                    .replace("{}", version),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(g.text_muted)
                                            .child(crate::i18n::t("Histórico de atualizações")),
                                    ),
                            )
                            .child(
                                div()
                                    .id("wn-close")
                                    .size(px(26.))
                                    .rounded(px(6.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .text_color(g.text_muted)
                                    .hover(move |s| s.bg(g.hover))
                                    .on_click(|_, window, cx| window.close_dialog(cx))
                                    .child(Icon::new(IconName::X).size(px(14.))),
                            ),
                    )
                    .child(
                        div()
                            .id("wn-scroll")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(list),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .px(px(18.))
                            .py(px(12.))
                            .border_t_1()
                            .border_color(g.edge_soft)
                            .child(
                                div()
                                    .id("wn-tour")
                                    .px(px(10.))
                                    .py(px(6.))
                                    .rounded(px(7.))
                                    .text_size(px(12.))
                                    .text_color(g.text_muted)
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(g.hover).text_color(g.text))
                                    .on_click(|_, window, cx| {
                                        window.close_dialog(cx);
                                        window.dispatch_action(Box::new(ShowTour), cx);
                                    })
                                    .child(crate::i18n::t("Tour guiado")),
                            )
                            .child(
                                div()
                                    .id("wn-ok")
                                    .px(px(14.))
                                    .py(px(6.))
                                    .rounded(px(7.))
                                    .bg(g.accent)
                                    .text_color(gpui_kit::white())
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .on_click(|_, window, cx| window.close_dialog(cx))
                                    .child(crate::i18n::t("Entendi")),
                            ),
                    )
            })
        });
    }

    pub(super) fn render_tour_overlay(
        &self,
        g: &Glass,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let idx = self.tour_step?;
        let guide = product_guide();
        let step = guide.tour.get(idx)?;
        let total = guide.tour.len();
        let is_last = idx + 1 >= total;
        let lang = crate::i18n::current();

        let vp = window.viewport_size();
        let vp_w = f32::from(vp.width).max(TOUR_MIN_VIEWPORT_W_PX);
        let vp_h = f32::from(vp.height).max(TOUR_MIN_VIEWPORT_H_PX);

        let rect = if step.placement == TourPlacement::Center {
            None
        } else {
            self.tour_bounds.borrow().rects.get(&step.target).copied()
        };
        let card_pos = compute_card_layout(rect, step.placement, vp_w, vp_h);

        let mut layer = div()
            .id("tour-overlay")
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(
                |this, e: &KeyDownEvent, _, cx| match e.keystroke.key.as_str() {
                    "escape" => this.finish_tour(cx),
                    "right" | "enter" => this.step_tour(1, cx),
                    "left" => this.step_tour(-1, cx),
                    _ => {}
                },
            ));

        // Fundo escurecido: 4 retângulos ao redor do alvo (efeito spotlight) ou scrim único.
        if let Some(r) = rect {
            let hx = (r.x - TOUR_HOLE_PAD_PX).max(0.0);
            let hy = (r.y - TOUR_HOLE_PAD_PX).max(0.0);
            let hw = (r.w + TOUR_HOLE_PAD_PX * 2.0).min(vp_w - hx).max(0.0);
            let hh = (r.h + TOUR_HOLE_PAD_PX * 2.0).min(vp_h - hy).max(0.0);
            let right_x = (hx + hw).min(vp_w);
            let bottom_y = (hy + hh).min(vp_h);

            layer = layer
                .child(
                    div()
                        .absolute()
                        .left(px(0.))
                        .top(px(0.))
                        .w(px(vp_w))
                        .h(px(hy))
                        .bg(g.scrim),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(0.))
                        .top(px(bottom_y))
                        .w(px(vp_w))
                        .h(px((vp_h - bottom_y).max(0.)))
                        .bg(g.scrim),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(0.))
                        .top(px(hy))
                        .w(px(hx))
                        .h(px(hh))
                        .bg(g.scrim),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(right_x))
                        .top(px(hy))
                        .w(px((vp_w - right_x).max(0.)))
                        .h(px(hh))
                        .bg(g.scrim),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(hx))
                        .top(px(hy))
                        .w(px(hw))
                        .h(px(hh))
                        .rounded(px(10.))
                        .border_2()
                        .border_color(g.accent),
                );
        } else {
            layer = layer.child(div().absolute().inset_0().bg(g.scrim));
        }

        // Indicador visual da seta apontando do card para o alvo
        let arrow_icon = match card_pos.arrow {
            ArrowSide::Top => Some((
                IconName::ChevronUp,
                card_pos.x + card_pos.arrow_offset - TOUR_ARROW_HALF_PX,
                card_pos.y - TOUR_ARROW_OVERHANG_PX,
            )),
            ArrowSide::Bottom => Some((
                IconName::ChevronDown,
                card_pos.x + card_pos.arrow_offset - TOUR_ARROW_HALF_PX,
                card_pos.y + TOUR_CARD_H_PX - TOUR_ARROW_INSET_PX,
            )),
            ArrowSide::Left => Some((
                IconName::ChevronLeft,
                card_pos.x - TOUR_ARROW_OVERHANG_PX,
                card_pos.y + card_pos.arrow_offset - TOUR_ARROW_HALF_PX,
            )),
            ArrowSide::Right => Some((
                IconName::ChevronRight,
                card_pos.x + TOUR_CARD_W_PX - TOUR_ARROW_INSET_PX,
                card_pos.y + card_pos.arrow_offset - TOUR_ARROW_HALF_PX,
            )),
            ArrowSide::None => None,
        };
        if let Some((ic, ax, ay)) = arrow_icon {
            layer = layer.child(
                div()
                    .absolute()
                    .left(px(ax))
                    .top(px(ay))
                    .size(px(TOUR_ARROW_BADGE_SIZE_PX))
                    .rounded(px(99.))
                    .bg(g.accent)
                    .text_color(gpui_kit::white())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Icon::new(ic).size(px(12.))),
            );
        }

        let progress = crate::i18n::t("Passo {} de {}")
            .replacen("{}", &(idx + 1).to_string(), 1)
            .replacen("{}", &total.to_string(), 1);

        let mut dots = div().flex().items_center().gap(px(5.));
        for i in 0..total {
            dots = dots.child(
                div()
                    .h(px(6.))
                    .w(px(if i == idx { 16. } else { 6. }))
                    .rounded(px(99.))
                    .bg(if i == idx { g.accent } else { g.edge_strong }),
            );
        }

        let g_hover = g.hover;
        let g_text = g.text;
        let g_muted = g.text_muted;
        let g_edge_strong = g.edge_strong;

        let mut btns = div().flex().items_center().gap(px(6.)).child(
            div()
                .id("tour-skip")
                .px(px(9.))
                .py(px(5.))
                .rounded(px(7.))
                .text_size(px(12.))
                .text_color(g_muted)
                .cursor_pointer()
                .hover(move |s| s.bg(g_hover).text_color(g_text))
                .on_click(cx.listener(|this, _, _, cx| this.finish_tour(cx)))
                .child(crate::i18n::t("Pular")),
        );
        if idx > 0 {
            btns = btns.child(
                div()
                    .id("tour-prev")
                    .px(px(9.))
                    .py(px(5.))
                    .rounded(px(7.))
                    .border_1()
                    .border_color(g_edge_strong)
                    .text_size(px(12.))
                    .text_color(g_text)
                    .cursor_pointer()
                    .hover(move |s| s.bg(g_hover))
                    .on_click(cx.listener(|this, _, _, cx| this.step_tour(-1, cx)))
                    .child(crate::i18n::t("Anterior")),
            );
        }
        btns = btns.child(
            div()
                .id("tour-next")
                .px(px(12.))
                .py(px(5.))
                .rounded(px(7.))
                .bg(g.accent)
                .text_color(gpui_kit::white())
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| this.step_tour(1, cx)))
                .child(crate::i18n::t(if is_last {
                    "Concluir"
                } else {
                    "Próximo"
                })),
        );

        let card = div()
            .id("tour-card")
            .absolute()
            .left(px(card_pos.x))
            .top(px(card_pos.y))
            .w(px(TOUR_CARD_W_PX))
            .p(px(16.))
            .rounded(px(12.))
            .bg(g.modal.alpha(1.))
            .border_1()
            .border_color(g.edge_strong)
            .shadow_lg()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(g.accent)
                            .child(progress.to_uppercase()),
                    )
                    .child(
                        div()
                            .id("tour-x")
                            .size(px(22.))
                            .rounded(px(6.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .text_color(g_muted)
                            .hover(move |s| s.bg(g_hover))
                            .on_click(cx.listener(|this, _, _, cx| this.finish_tour(cx)))
                            .child(Icon::new(IconName::X).size(px(13.))),
                    ),
            )
            .child(
                div()
                    .text_size(px(14.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(g.text)
                    .child(step.title.pick(lang).to_string()),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(g.text_muted)
                    .child(step.body.pick(lang).to_string()),
            )
            .child(
                div()
                    .pt(px(6.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(dots)
                    .child(btns),
            );

        Some(layer.child(card).into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::{ArrowSide, BoxRect, compute_card_layout};
    use crate::constants::TOUR_CARD_W_PX;
    use crate::guide::TourPlacement;

    #[test]
    fn card_centers_when_no_target() {
        let pos = compute_card_layout(None, TourPlacement::Bottom, 1200.0, 800.0);
        assert_eq!(pos.arrow, ArrowSide::None);
        assert!((pos.x - (1200.0 - TOUR_CARD_W_PX) * 0.5).abs() < 1.0);
    }

    #[test]
    fn card_flips_up_near_bottom_edge() {
        let r = BoxRect {
            x: 500.0,
            y: 720.0,
            w: 100.0,
            h: 40.0,
        };
        let pos = compute_card_layout(Some(r), TourPlacement::Bottom, 1200.0, 800.0);
        assert_eq!(pos.arrow, ArrowSide::Bottom);
        assert!(pos.y < r.y);
    }
}
