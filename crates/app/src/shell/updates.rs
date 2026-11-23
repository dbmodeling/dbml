//! Ligação GPUI do auto-update: ações, controle único da title bar, saída/relançamento e o
//! aviso "Atualizado para X". A decisão do que mostrar e fazer está em `updater_ui.rs` (puro,
//! testado); aqui só se desenha e se despacha.
//!
//! Limitação do GPUI fixado (`gpui-pre` 0.3.9): o `aria` do `div` tem rótulo, descrição, estado
//! alternado etc., mas **não tem região viva** (`aria-live`). O controle usa `Role::Status`
//! (informativo) ou `Role::Button` (acionável) com o rótulo acessível atualizado a cada estado;
//! o anúncio automático da mudança fica por conta do leitor de tela.

use super::*;
use crate::constants::{
    ID_UPDATE_CONTROL, RELEASES_URL, UPDATE_CONTROL_FONT_PX, UPDATE_CONTROL_GAP_PX,
    UPDATE_CONTROL_H_PX, UPDATE_CONTROL_ICON_PX, UPDATE_CONTROL_MAX_W_PX, UPDATE_CONTROL_PAD_X_PX,
    UPDATE_CONTROL_RADIUS_PX,
};
use crate::updater_ui::{UpdateView, ViewAction};

/// Como o app termina: sair (`Quit`) ou reiniciar para concluir a atualização.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ExitKind {
    /// Só sai.
    Quit,
    /// Grava a sessão, abre o processo de relançamento e sai.
    Relaunch,
}

impl Shell {
    /// Espera mudanças do updater (canal `watch`) e repinta; sem temporizador de polling.
    pub(super) fn spawn_update_observer(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let mut changes = self.updates.subscribe();
        cx.spawn_in(window, async move |this, cx| {
            while changes.changed().await.is_ok() {
                if this
                    .update_in(cx, |this, _, cx| this.on_update_changed(cx))
                    .is_err()
                {
                    return;
                }
            }
        })
    }

    /// Estado do updater mudou: repinta e arma o temporizador de "Atualizado" (some em 4 s).
    fn on_update_changed(&mut self, cx: &mut Context<Self>) {
        self.update_dismiss = self.updates.view().visible_for().map(|after| {
            let bg = cx.background_executor().clone();
            cx.spawn(async move |this, cx| {
                bg.timer(after).await;
                this.update(cx, |this, cx| {
                    this.updates.dismiss_transient();
                    cx.notify();
                })
                .ok();
            })
        });
        cx.notify();
    }

    pub(super) fn check_for_updates(
        &mut self,
        _: &CheckForUpdates,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.updates.check_now();
        cx.notify();
    }

    pub(super) fn restart_to_update(
        &mut self,
        _: &RestartToUpdate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.updates.restart_ready().is_some() {
            self.request_exit(ExitKind::Relaunch, window, cx);
        }
    }

    pub(super) fn toggle_auto_update(
        &mut self,
        _: &ToggleAutoUpdate,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.auto_update = !self.auto_update;
        self.updates.set_auto_update(self.auto_update);
        self.save_prefs();
        cx.notify();
    }

    /// Último passo de "sair" e de "reiniciar para atualizar", depois do fluxo de alterações
    /// não salvas. Reiniciar grava a sessão **antes** de abrir o processo de relançamento; este
    /// espera o app sair (e a sessão estar em disco) antes de abrir o novo.
    pub(super) fn finish_exit(&mut self, kind: ExitKind, cx: &mut Context<Self>) {
        match kind {
            ExitKind::Quit => cx.quit(),
            ExitKind::Relaunch => {
                self.save_prefs();
                self.save_session(cx);
                match crate::relaunch::relaunch_current() {
                    Ok(()) => cx.quit(),
                    Err(error) => {
                        // o app segue aberto e a versão nova já está instalada
                        tracing::error!("{error}. {}", crate::constants::RELAUNCH_MANUAL_HINT);
                        self.updates.report_restart_failed();
                        cx.notify();
                    }
                }
            }
        }
    }

    /// Depois de reiniciar: "Atualizado para X" com o link das notas. As novidades da versão já
    /// abrem pelo fluxo existente (`last_seen_version`); aqui só se acrescenta o aviso.
    pub(super) fn announce_update(window: &mut Window, cx: &mut App) {
        let Some(dir) = crate::session::config_dir() else {
            return;
        };
        let Some(notice) = crate::update_notice::take(&dir, env!("CARGO_PKG_VERSION")) else {
            return;
        };
        let title = crate::i18n::t("Atualizado para {}").replace("{}", &notice.version);
        let url = notice.notes_url;
        window.push_notification(
            gpui_kit::component::notification::Notification::info(crate::i18n::t(
                "Clique para ver as notas da versão",
            ))
            .title(title)
            .autohide(false)
            .on_click(move |_, _, cx| cx.open_url(&url)),
            cx,
        );
    }

    /// Ativar o controle (clique, Enter ou Espaço): o que o estado atual oferece.
    fn activate_update_control(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.updates.view().action() {
            ViewAction::Restart => window.dispatch_action(Box::new(RestartToUpdate), cx),
            ViewAction::Retry => window.dispatch_action(Box::new(CheckForUpdates), cx),
            ViewAction::OpenReleases => {
                cx.open_url(RELEASES_URL);
                self.updates.dismiss();
            }
            ViewAction::Dismiss => self.updates.dismiss(),
            ViewAction::None => {}
        }
        cx.notify();
    }

    /// O controle único da title bar; `None` quando não há nada a mostrar.
    pub(super) fn update_control(
        &self,
        g: &Glass,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let view = self.updates.view();
        let label: SharedString = view.label()?.into();
        let aria = view.aria_label().unwrap_or_else(|| label.to_string());
        let action = view.action();
        let (icon, tint) = match &view {
            UpdateView::Hidden => return None,
            UpdateView::Checking | UpdateView::Downloading | UpdateView::Installing => {
                (IconName::RefreshCw, g.text_muted)
            }
            UpdateView::AnotherInstance => (IconName::RefreshCw, g.text_muted),
            UpdateView::UpToDate => (IconName::CircleCheck, g.ok),
            UpdateView::Restart(_) => (IconName::RotateCw, g.accent),
            UpdateView::Failed | UpdateView::RestartFailed | UpdateView::Disabled(_) => {
                (IconName::TriangleAlert, g.warning)
            }
        };
        let actionable = action != ViewAction::None;
        let (hover, text, accent) = (g.hover, g.text, g.accent);
        let tooltip: SharedString = label.clone();
        Some(
            div()
                .id(ID_UPDATE_CONTROL)
                .role(if actionable {
                    Role::Button
                } else {
                    Role::Status
                })
                .aria_label(aria)
                .flex_none()
                .flex()
                .items_center()
                .gap(px(UPDATE_CONTROL_GAP_PX))
                .h(px(UPDATE_CONTROL_H_PX))
                .px(px(UPDATE_CONTROL_PAD_X_PX))
                .max_w(px(UPDATE_CONTROL_MAX_W_PX))
                .rounded(px(UPDATE_CONTROL_RADIUS_PX))
                .text_size(px(UPDATE_CONTROL_FONT_PX))
                .text_color(tint)
                .child(Icon::new(icon).size(px(UPDATE_CONTROL_ICON_PX)))
                // janela estreita: só o ícone (o texto vai no tooltip e no rótulo acessível)
                .when(!compact, |d| d.child(div().truncate().child(label)))
                .when(compact, |d| {
                    d.tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
                })
                .when(actionable, |d| {
                    d.focusable()
                        .tab_stop(true)
                        .cursor_pointer()
                        .border_1()
                        .border_color(gpui_kit::transparent_black())
                        .hover(move |s| s.bg(hover).text_color(text))
                        .focus_visible(move |s| s.border_color(accent))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.activate_update_control(window, cx);
                        }))
                }),
        )
    }
}
