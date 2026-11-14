//! Peças de UI compartilhadas entre shell e canvas.

use crate::glass::{Glass, SP_SM, TEXT_SM, TEXT_XS};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

pub struct EmptyAction {
    pub id: &'static str,
    pub icon: IconName,
    pub label: &'static str,
    pub keys: &'static str,
    pub action: Box<dyn Action>,
}

/// Estado vazio padrão: ícone, título, explicação, ação primária, "ou", ação secundária.
/// O atalho fica visível em cada botão (descoberta pelo teclado).
pub fn empty_state(
    g: &Glass,
    icon: IconName,
    title: &'static str,
    body: &'static str,
    primary: EmptyAction,
    secondary: EmptyAction,
    tertiary: EmptyAction,
) -> Div {
    let button = |a: EmptyAction, is_primary: bool| {
        let action = a.action;
        let b = Button::new(a.id)
            .small()
            .w_full()
            .icon(a.icon)
            .label(a.label)
            .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx));
        let b = if is_primary { b.primary() } else { b };
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.))
            .child(b)
            .when(!a.keys.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(TEXT_XS)
                        .text_color(g.text_faint)
                        .child(a.keys),
                )
            })
    };
    let rule = || div().flex_1().h(px(1.)).bg(g.edge_soft);
    div()
        .w(px(240.))
        .flex()
        .flex_col()
        .items_center()
        .gap(SP_SM)
        .child(Icon::new(icon).large().text_color(g.text_faint))
        .child(
            div()
                .text_size(TEXT_SM)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(g.text)
                .child(title),
        )
        .child(
            div()
                .text_size(TEXT_XS)
                .text_color(g.text_muted)
                .text_center()
                .child(body),
        )
        .child(div().h(px(4.)))
        .child(button(primary, true))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(SP_SM)
                .text_size(TEXT_XS)
                .text_color(g.text_faint)
                .child(rule())
                .child("or")
                .child(rule()),
        )
        .child(button(secondary, false))
        .child(button(tertiary, false))
}
