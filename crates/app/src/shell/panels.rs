use super::*;

impl Shell {
    // ---- partes da tela ----

    pub(super) fn project_name(&self) -> String {
        self.project
            .as_deref()
            .or_else(|| self.doc().and_then(|d| d.path.as_deref()?.parent()))
            .and_then(|p| p.file_name())
            .map_or_else(
                || "dbm-proj".to_string(),
                |n| n.to_string_lossy().into_owned(),
            )
    }

    pub(super) fn titlebar(
        &self,
        g: &Glass,
        maximized: bool,
        compact: bool,
        inspector: (bool, bool),
        cx: &mut Context<Self>,
    ) -> Div {
        let (inspector_shown, has_selection) = inspector;
        let has_file = !self.docs.is_empty();
        let left_open = self.sidebar_open || self.editor_open;
        let mut tabs = div()
            .flex()
            .items_center()
            .gap(px(2.))
            .min_w_0()
            .flex_shrink(1.0)
            .overflow_x_hidden();
        for (i, doc) in self.docs.iter().enumerate() {
            let on = i == self.active;
            tabs = tabs.child(
                div()
                    .id(("tab", i))
                    .role(Role::Tab)
                    .aria_label(doc.title.clone())
                    .group("tab")
                    .h(px(30.))
                    .max_w(px(220.))
                    .min_w(px(70.))
                    .flex_shrink(1.0)
                    .pl(px(10.))
                    .pr(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(8.))
                    .text_size(px(12.5))
                    .text_color(if on { g.text } else { g.text_faint })
                    .when(on, |d| d.bg(g.sel).shadow(sel_shadow(g.mode)))
                    .when(!on, |d| {
                        let h = g.hover;
                        d.hover(move |s| s.bg(h))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.active = i;
                        this.save_session(cx);
                        cx.notify();
                    }))
                    // clique do meio fecha a aba (como em navegadores e no Zed)
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(move |this, _, window, cx| this.close_tab_at(i, window, cx)),
                    )
                    .child(
                        Icon::new(IconName::FileCode)
                            .size(px(14.))
                            .text_color(if on { g.accent } else { g.text_faint }),
                    )
                    .child(div().truncate().child(doc.title.clone()))
                    .child(
                        div()
                            .id(("tab-close", i))
                            .role(Role::Button)
                            .aria_label(crate::i18n::t("Fechar aba"))
                            .size(px(16.))
                            .rounded(px(4.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover({
                                let h = g.hover;
                                move |s| s.bg(h)
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.close_tab_at(i, window, cx);
                            }))
                            // alterado: ponto; senão o X (visível na aba ativa ou no hover)
                            .child(if doc.dirty {
                                div()
                                    .size(px(7.))
                                    .rounded_full()
                                    .bg(g.accent)
                                    .into_any_element()
                            } else {
                                div()
                                    .when(!on, |d| {
                                        d.invisible().group_hover("tab", gpui_kit::Styled::visible)
                                    })
                                    .child(
                                        Icon::new(IconName::X)
                                            .size(px(12.))
                                            .text_color(g.text_faint),
                                    )
                                    .into_any_element()
                            }),
                    ),
            );
        }
        tabs = tabs.child(
            div()
                .id("tab-new")
                .role(Role::Button)
                .aria_label(crate::i18n::t("Novo arquivo"))
                .size(px(28.))
                .rounded(px(7.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .text_color(g.text_faint)
                .hover({
                    let (h, t) = (g.hover, g.text);
                    move |s| s.bg(h).text_color(t)
                })
                .tooltip(|window, cx| {
                    Tooltip::new(crate::i18n::t("Novo arquivo"))
                        .action(&NewFile, Some(actions::CTX))
                        .build(window, cx)
                })
                .on_click(|_, window, cx| window.dispatch_action(Box::new(NewFile), cx))
                .child(Icon::new(IconName::Plus).size(px(14.))),
        );

        let proj_name = self.project_name();
        let main_menu = div().mr(px(10.)).flex_none().child(
            Button::new("main-menu")
                .ghost()
                .small()
                .h(px(30.))
                .pl(px(5.))
                .pr(px(8.))
                .rounded(px(8.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(7.))
                        .child(img("brand/icon-64.png").size(px(20.)).rounded(px(6.)))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(g.text)
                                .child(proj_name),
                        )
                        .child(
                            Icon::new(IconName::ChevronDown)
                                .size(px(12.))
                                .text_color(g.text_faint),
                        ),
                )
                .tooltip(crate::i18n::t("Menu do projeto"))
                .map(|button| {
                    motion::dropdown(
                        "project-menu-motion",
                        button,
                        Anchor::TopLeft,
                        self.focus.clone(),
                        main_menu_items,
                    )
                }),
        );
        let theme_icon = if self.mode == Mode::Dark {
            IconName::Moon
        } else {
            IconName::Sun
        };
        let divider = || div().flex_none().w(px(1.)).h(px(18.)).bg(g.edge_soft);
        let diag_active = has_file && !self.editor_open;
        let split_active = self.editor_open && !self.code_only;
        let mut bar = div()
            .h(TITLEBAR_H)
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(16.))
            .pr(px(10.))
            // translúcido: a raiz já pinta `chrome`; repetir aqui dobraria o alfa
            .when(!g.translucent, |d| d.bg(g.chrome))
            .border_b_1()
            .border_color(g.edge_soft)
            .child(
                div()
                    .id("title-sidebar")
                    .role(Role::Button)
                    .aria_label(crate::i18n::t("Alternar painel lateral"))
                    .size(px(28.))
                    .rounded(px(7.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_color(if left_open { g.text } else { g.text_faint })
                    .when(left_open, |d| d.bg(g.hover))
                    .hover({
                        let (h, t) = (g.hover, g.text);
                        move |s| s.bg(h).text_color(t)
                    })
                    .tooltip(|window, cx| {
                        Tooltip::new(crate::i18n::t("Alternar painel lateral"))
                            .action(&ToggleSidebar, Some(actions::CTX))
                            .build(window, cx)
                    })
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleSidebar), cx))
                    .child(Icon::new(IconName::PanelLeft).size(px(16.))),
            )
            .child(divider().mx(px(6.)))
            .child(main_menu)
            .child(tabs)
            .child(
                div()
                    .id("titlebar-drag")
                    .window_control_area(WindowControlArea::Drag)
                    .flex_1()
                    .h_full()
                    .when(cfg!(not(windows)), |d| {
                        d.on_double_click(|_, window, _| window.zoom_window())
                            .on_mouse_down_out(cx.listener(|this, _, _, _| {
                                this.dragging_window = false;
                            }))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, _| {
                                    this.dragging_window = true;
                                }),
                            )
                            .on_mouse_down(MouseButton::Right, |ev, window, _| {
                                window.show_window_menu(ev.position);
                            })
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _, _, _| {
                                    this.dragging_window = false;
                                }),
                            )
                            .on_mouse_move(cx.listener(|this, _, window, _| {
                                if this.dragging_window {
                                    this.dragging_window = false;
                                    window.start_window_move();
                                }
                            }))
                    }),
            )
            .child(
                div()
                    .id("palette-pill")
                    .role(Role::Button)
                    .aria_label(crate::i18n::t("Buscar tabelas, colunas, comandos (Ctrl+K)"))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(SP_SM)
                    .h(px(28.))
                    .w(if compact { px(28.) } else { px(250.) })
                    .rounded(if compact { px(7.) } else { px(8.) })
                    .when(compact, |d| {
                        d.justify_center().tooltip(|window, cx| {
                            Tooltip::new(crate::i18n::t("Buscar tabelas, colunas, comandos"))
                                .action(&OpenPalette, Some(actions::CTX))
                                .build(window, cx)
                        })
                    })
                    .when(!compact, |d| {
                        d.px(px(10.))
                            .border_1()
                            .border_color(g.edge_soft)
                            .bg(g.input)
                    })
                    .text_size(px(12.5))
                    .text_color(g.text_faint)
                    .cursor_pointer()
                    .hover({
                        let (h, t) = (g.hover, g.text);
                        move |s| s.bg(h).text_color(t)
                    })
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenPalette), cx))
                    .child(Icon::new(IconName::Search).size(if compact {
                        px(15.)
                    } else {
                        px(14.)
                    }))
                    .when(!compact, |d| {
                        d.child(
                            div()
                                .flex_1()
                                .truncate()
                                .child(crate::i18n::t("Buscar tabelas, colunas, comandos…")),
                        )
                        .child(
                            div()
                                .px(px(5.))
                                .py(px(1.))
                                .rounded(px(4.))
                                .border_1()
                                .border_color(g.edge_soft)
                                .font_family(FONT_MONO)
                                .text_size(px(10.5))
                                .child("Ctrl K"),
                        )
                    }),
            )
            .child(divider().mx(px(6.)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .p(px(2.))
                    .rounded(px(8.))
                    .bg(g.input)
                    .child(
                        div()
                            .id("view-diagram")
                            .role(Role::Button)
                            .aria_label(crate::i18n::t("Diagrama"))
                            .w(px(30.))
                            .h(px(24.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .cursor_pointer()
                            .when(diag_active, |d| d.bg(g.sel).shadow(sel_shadow(g.mode)))
                            .text_color(if diag_active { g.text } else { g.text_faint })
                            .hover({
                                let t = g.text;
                                move |s| s.text_color(t)
                            })
                            .tooltip(|window, cx| {
                                Tooltip::new(crate::i18n::t("Diagrama"))
                                    .action(&ShowDiagramView, Some(actions::CTX))
                                    .build(window, cx)
                            })
                            .on_click(
                                cx.listener(|this, _, _, cx| this.set_view(View::Diagram, cx)),
                            )
                            .child(Icon::new(IconName::Workflow).size(px(14.))),
                    )
                    .child(
                        div()
                            .id("view-split")
                            .role(Role::Button)
                            .aria_label(crate::i18n::t("Dividido · DBML"))
                            .w(px(30.))
                            .h(px(24.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .cursor_pointer()
                            .when(split_active, |d| d.bg(g.sel).shadow(sel_shadow(g.mode)))
                            .text_color(if split_active { g.text } else { g.text_faint })
                            .hover({
                                let t = g.text;
                                move |s| s.text_color(t)
                            })
                            .tooltip(|window, cx| {
                                Tooltip::new(crate::i18n::t("Dividido · DBML"))
                                    .action(&ToggleEditor, Some(actions::CTX))
                                    .build(window, cx)
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.set_view(View::Split, cx)))
                            .child(Icon::new(IconName::Columns2).size(px(14.))),
                    ),
            )
            // quatro ações: DBML, código, imagem, tema
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .ml(px(4.))
                    .child(
                        div()
                            .relative()
                            .flex()
                            .items_center()
                            .gap(px(2.))
                            .child(
                                Button::new("save-dbml-btn")
                                    .ghost()
                                    .small()
                                    .w(px(28.))
                                    .h(px(28.))
                                    .rounded(px(7.))
                                    .icon(
                                        Icon::new(IconName::FileDown)
                                            .size(px(15.))
                                            .text_color(g.text_faint),
                                    )
                                    .tooltip_with_action(
                                        crate::i18n::t("Exportar DBML"),
                                        &SaveFile,
                                        Some(actions::CTX),
                                    )
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(Box::new(SaveFile), cx);
                                    }),
                            )
                            .child(export_dropdown(
                                g,
                                &self.focus,
                                "export-code-btn",
                                IconName::DatabaseZap,
                                crate::i18n::t("Exportar código (SQL / Mermaid)…"),
                                actions::GROUP_EXPORT_CODE,
                            ))
                            .child(export_dropdown(
                                g,
                                &self.focus,
                                "export-image-btn",
                                IconName::ImageDown,
                                crate::i18n::t("Exportar imagem (PNG / JPEG / SVG)…"),
                                actions::GROUP_EXPORT_IMAGE,
                            ))
                            .child(crate::shell::guide_ui::track_tour_anchor(
                                crate::guide::TourTarget::ExportBtn,
                                self.tour_bounds.clone(),
                            )),
                    )
                    .child(
                        Button::new("theme-toggle")
                            .ghost()
                            .small()
                            .w(px(28.))
                            .h(px(28.))
                            .rounded(px(7.))
                            .icon(Icon::new(theme_icon).size(px(15.)).text_color(g.text_faint))
                            .tooltip_with_action(
                                crate::i18n::t("Alternar tema"),
                                &ToggleTheme,
                                Some(actions::CTX),
                            )
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(ToggleTheme), cx);
                            }),
                    ),
            )
            .child(
                div()
                    .id("title-inspector")
                    .role(Role::Button)
                    .aria_label(crate::i18n::t("Alternar inspector"))
                    .relative()
                    .ml(px(2.))
                    .size(px(28.))
                    .rounded(px(7.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_color(if inspector_shown {
                        g.text
                    } else {
                        g.text_faint
                    })
                    .when(inspector_shown, |d| d.bg(g.hover))
                    .hover({
                        let (h, t) = (g.hover, g.text);
                        move |s| s.bg(h).text_color(t)
                    })
                    .tooltip(|window, cx| {
                        Tooltip::new(crate::i18n::t("Alternar inspector"))
                            .action(&ToggleInspector, Some(actions::CTX))
                            .build(window, cx)
                    })
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(ToggleInspector), cx);
                    })
                    .child(Icon::new(IconName::PanelRight).size(px(16.)))
                    .when(has_selection && !inspector_shown && has_file, |d| {
                        d.child(
                            div()
                                .absolute()
                                .right(px(3.))
                                .top(px(3.))
                                .size(px(6.))
                                .rounded_full()
                                .bg(g.accent),
                        )
                    }),
            )
            .child(divider().ml(px(6.)).mr(px(4.)));
        #[cfg(windows)]
        {
            // botões nativos do Windows: glifos da fonte do sistema, 46 px, fechar vermelho
            let font = crate::platform::caption_font();
            let caption = |id: &'static str,
                           label: &'static str,
                           area: WindowControlArea,
                           glyph: &'static str,
                           close: bool| {
                let (hover, ink) = (if close { CAPTION_CLOSE } else { g.hover }, g.text_muted);
                let hover_ink = if close { gpui_kit::white() } else { g.text };
                div()
                    .id(id)
                    .role(Role::Button)
                    .aria_label(label)
                    .window_control_area(area)
                    .occlude()
                    .w(px(46.))
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_family(font)
                    .text_size(px(10.))
                    .text_color(ink)
                    .hover(move |s| s.bg(hover).text_color(hover_ink))
                    .active(move |s| s.bg(hover.opacity(0.8)))
                    .child(glyph)
            };
            let (max_label, max_glyph) = if maximized {
                (crate::i18n::t("Restaurar"), "\u{e923}")
            } else {
                (crate::i18n::t("Maximizar"), "\u{e922}")
            };
            bar = bar.child(
                div()
                    .h_full()
                    .flex_none()
                    .flex()
                    .child(caption(
                        "cap-min",
                        crate::i18n::t("Minimizar"),
                        WindowControlArea::Min,
                        "\u{e921}",
                        false,
                    ))
                    .child(caption(
                        "cap-max",
                        max_label,
                        WindowControlArea::Max,
                        max_glyph,
                        false,
                    ))
                    .child(caption(
                        "cap-close",
                        crate::i18n::t("Fechar"),
                        WindowControlArea::Close,
                        "\u{e8bb}",
                        true,
                    )),
            );
        }
        #[cfg(not(windows))]
        {
            let (max_label, max_icon) = if maximized {
                (crate::i18n::t("Restaurar"), IconName::WindowRestore)
            } else {
                (crate::i18n::t("Maximizar"), IconName::WindowMaximize)
            };

            bar = bar.child(
                div()
                    .h_full()
                    .flex_none()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .id("cap-min")
                            .role(Role::Button)
                            .aria_label(crate::i18n::t("Minimizar"))
                            .occlude()
                            .w(px(44.))
                            .h_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(g.text_muted)
                            .hover({
                                let h = g.hover;
                                let ink = g.text;
                                move |s| s.bg(h).text_color(ink)
                            })
                            .active({
                                let h = g.hover;
                                move |s| s.bg(h.opacity(0.8))
                            })
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                            })
                            .on_click(|_, window, cx| {
                                cx.stop_propagation();
                                window.minimize_window();
                            })
                            .child(Icon::new(IconName::WindowMinimize).small()),
                    )
                    .child(
                        div()
                            .id("cap-max")
                            .role(Role::Button)
                            .aria_label(max_label)
                            .occlude()
                            .w(px(44.))
                            .h_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(g.text_muted)
                            .hover({
                                let h = g.hover;
                                let ink = g.text;
                                move |s| s.bg(h).text_color(ink)
                            })
                            .active({
                                let h = g.hover;
                                move |s| s.bg(h.opacity(0.8))
                            })
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                            })
                            .on_click(|_, window, cx| {
                                cx.stop_propagation();
                                window.zoom_window();
                            })
                            .child(Icon::new(max_icon).small()),
                    )
                    .child(
                        div()
                            .id("cap-close")
                            .role(Role::Button)
                            .aria_label(crate::i18n::t("Fechar"))
                            .occlude()
                            .w(px(46.))
                            .h_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(g.text_muted)
                            .hover(move |s| s.bg(CAPTION_CLOSE).text_color(gpui_kit::white()))
                            .active(move |s| s.bg(CAPTION_CLOSE.opacity(0.8)))
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                            })
                            .on_click(|_, window, cx| {
                                cx.stop_propagation();
                                window.dispatch_action(Box::new(Quit), cx);
                            })
                            .child(Icon::new(IconName::WindowClose).small()),
                    ),
            );
        }
        bar
    }

    /// Barra de atividades (ícones à esquerda), como no VS Code / GitHub Desktop.
    pub(super) fn rail(&self, g: &Glass, cx: &mut Context<Self>) -> Div {
        let errors = self.doc().map_or(0, |d| d.counts().0);
        let item = |id: &'static str,
                    icon: IconName,
                    tip: &'static str,
                    action: Box<dyn Action>,
                    is_active: bool,
                    badge: usize| {
            let on = is_active;
            let tip_action = action.boxed_clone();
            // wrapper do tamanho do rail: a barrinha de ativo encosta na borda esquerda
            div()
                .relative()
                .w_full()
                .h(px(34.))
                .flex()
                .justify_center()
                .when(on, |d| {
                    d.child(
                        div()
                            .absolute()
                            .left_0()
                            .top(px(9.))
                            .w(px(2.))
                            .h(px(16.))
                            .rounded(px(2.))
                            .bg(g.accent),
                    )
                })
                .child(
                    div()
                        .id(id)
                        .role(Role::Tab)
                        .aria_label(tip)
                        .relative()
                        .size(px(34.))
                        .rounded(px(9.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(if on { g.text } else { g.text_faint })
                        .when(on, |d| d.bg(g.hover))
                        .hover({
                            let (h, t) = (g.hover, g.text);
                            move |s| s.bg(h).text_color(t)
                        })
                        .active({
                            let a = g.active;
                            move |s| s.bg(a)
                        })
                        .tooltip(move |window, cx| {
                            Tooltip::new(tip)
                                .action(tip_action.as_ref(), Some(actions::CTX))
                                .build(window, cx)
                        })
                        .on_click(move |_, window, cx| {
                            window.dispatch_action(action.boxed_clone(), cx);
                        })
                        .child(Icon::new(icon).size(px(17.)))
                        .when(badge > 0, |d| {
                            d.child(
                                div()
                                    .absolute()
                                    .top(px(2.))
                                    .right(px(2.))
                                    .min_w(px(14.))
                                    .h(px(14.))
                                    .px(px(3.))
                                    .rounded_full()
                                    .bg(g.danger)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .font_family(FONT_MONO)
                                    .text_size(px(9.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(gpui_kit::white())
                                    .child(badge.to_string()),
                            )
                        }),
                )
        };
        div()
            .w(RAIL_W)
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.))
            .py(px(8.))
            .bg(g.rail)
            .border_r_1()
            .border_color(g.edge_soft)
            .child(item(
                "rail-files",
                IconName::Folder,
                crate::i18n::t("Arquivos do projeto"),
                Box::new(ShowFiles),
                self.sidebar_open && !self.editor_open && self.pane == Pane::Files,
                0,
            ))
            .child(item(
                "rail-schema",
                IconName::Table2,
                crate::i18n::t("Tabelas"),
                Box::new(ShowSchema),
                self.sidebar_open && !self.editor_open && self.pane == Pane::Schema,
                0,
            ))
            .child(item(
                "rail-problems",
                IconName::TriangleAlert,
                crate::i18n::t("Problemas"),
                Box::new(ShowProblems),
                self.sidebar_open && !self.editor_open && self.pane == Pane::Problems,
                errors,
            ))
            .child(item(
                "rail-editor",
                IconName::Braces,
                "Editor DBML",
                Box::new(ToggleEditor),
                self.editor_open,
                0,
            ))
            .child(item(
                "rail-connect",
                IconName::Database,
                crate::i18n::t("Conectar ao banco"),
                Box::new(ConnectDatabase),
                false,
                0,
            ))
            .child(item(
                "rail-import",
                IconName::FileInput,
                crate::i18n::t("Importar SQL DDL"),
                Box::new(ImportSqlDdl),
                false,
                0,
            ))
            .child(div().flex_1())
            .child(self.appearance_popover(g, cx))
            .child(item(
                "rail-keys",
                IconName::Keyboard,
                crate::i18n::t("Atalhos de teclado"),
                Box::new(ShowShortcuts),
                false,
                0,
            ))
            .child(item(
                "rail-palette",
                IconName::Command,
                crate::i18n::t("Paleta de comandos"),
                Box::new(OpenPalette),
                false,
                0,
            ))
    }

    /// Botão Aparência do rail + popover de 310px: tema, escala da interface e painéis.
    fn appearance_popover(&self, g: &Glass, cx: &mut Context<Self>) -> Popover {
        let g = *g;
        let (mode, ui_scale, auto_hide) = (self.mode, self.ui_scale, self.inspector_auto_hide);
        let shell = cx.entity().downgrade();
        let on_open = shell.clone();
        let epoch = self.popover_epoch;
        Popover::new("rail-appearance-pop")
            .anchor(Anchor::BottomLeft)
            .offset(px(8.))
            .appearance(false)
            .on_open_change(move |open, _, cx| {
                if *open {
                    _ = on_open.update(cx, |s, cx| {
                        s.popover_epoch = s.popover_epoch.wrapping_add(1);
                        cx.notify();
                    });
                }
            })
            .trigger(
                // mesmo estilo dos demais botões do rail (34x34, raio 9, ícone 17px)
                Button::new("rail-appearance")
                    .ghost()
                    .w(px(34.))
                    .h(px(34.))
                    .rounded(px(9.))
                    .icon(
                        Icon::new(IconName::Palette)
                            .size(px(17.))
                            .text_color(g.text_faint),
                    )
                    .tooltip(crate::i18n::t("Aparência")),
            )
            .content(move |_, _, _| {
                // rótulo em caixa alta com dica de atalho à direita
                let label = |text: &'static str, hint: &'static str| {
                    div()
                        .flex()
                        .items_center()
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(g.text_faint)
                        .child(crate::i18n::t(text))
                        .child(div().flex_1())
                        .child(
                            div()
                                .font_family(FONT_MONO)
                                .text_size(px(10.5))
                                .font_weight(FontWeight::MEDIUM)
                                .child(hint),
                        )
                };
                // segmentado: item ativo em `sel` (tema, escala)
                let seg =
                    |id: &'static str, text: &'static str, icon: Option<IconName>, on: bool| {
                        div()
                            .id(id)
                            .role(Role::Button)
                            .h(px(28.))
                            .flex_1()
                            .rounded(px(6.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap(px(7.))
                            .text_size(px(12.5))
                            .text_color(g.text_faint)
                            .when(!on, gpui_kit::Styled::cursor_pointer)
                            .children(icon.map(|i| Icon::new(i).size(px(13.))))
                            .child(crate::i18n::t(text))
                    };
                let selected = |id: &'static str, on: bool, el| {
                    motion::animate(
                        (ElementId::from(id), "active"),
                        f32::from(on),
                        el,
                        move |el: Stateful<Div>, t| {
                            el.bg(g.sel.opacity(t))
                                .text_color(motion::color(g.text_faint, g.text, t))
                                .shadow(
                                    sel_shadow(g.mode)
                                        .into_iter()
                                        .map(|mut s| {
                                            s.color = s.color.opacity(t);
                                            s
                                        })
                                        .collect::<Vec<_>>(),
                                )
                        },
                    )
                };
                let track = || {
                    div()
                        .flex()
                        .gap(px(3.))
                        .p(px(3.))
                        .rounded(px(9.))
                        .bg(g.input)
                };
                let theme_opt = |id: &'static str,
                                 text: &'static str,
                                 icon: IconName,
                                 want: Mode,
                                 action: Box<dyn Action>| {
                    selected(
                        id,
                        mode == want,
                        seg(id, text, Some(icon), mode == want).on_click(move |_, window, cx| {
                            window.dispatch_action(action.boxed_clone(), cx);
                        }),
                    )
                };
                let scale = |id: &'static str, text: &'static str, value: f32| {
                    let shell = shell.clone();
                    let on = (ui_scale - value).abs() < 0.05;
                    selected(
                        id,
                        on,
                        seg(id, text, None, on).on_click(move |_, window, cx| {
                            shell
                                .update(cx, |s, cx| s.set_ui_scale(value, window, cx))
                                .ok();
                        }),
                    )
                };
                let toggle = |id: &'static str,
                              name: &'static str,
                              desc: &'static str,
                              on: bool,
                              action: Box<dyn Action>| {
                    div()
                        .id(id)
                        .role(Role::Switch)
                        .aria_label(crate::i18n::t(name))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .px(px(8.))
                        .py(px(7.))
                        .rounded(px(7.))
                        .cursor_pointer()
                        .hover(move |s| s.bg(g.hover))
                        .on_click(move |_, window, cx| {
                            window.dispatch_action(action.boxed_clone(), cx);
                        })
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(div().text_size(TEXT_SM).child(crate::i18n::t(name)))
                                .child(
                                    div()
                                        .text_size(px(11.5))
                                        .text_color(g.text_faint)
                                        .child(crate::i18n::t(desc)),
                                ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .relative()
                                .w(px(30.))
                                .h(px(18.))
                                .rounded(px(9.))
                                .bg(g.edge_strong)
                                .child(motion::animate(
                                    (ElementId::from(id), "knob"),
                                    f32::from(on),
                                    div()
                                        .absolute()
                                        .top(px(2.))
                                        .size(px(14.))
                                        .rounded_full()
                                        .bg(gpui_kit::white()),
                                    |knob, t| knob.left(px(2. + 12. * t)),
                                ))
                                .map(|track| {
                                    motion::animate(
                                        (ElementId::from(id), "track"),
                                        f32::from(on),
                                        track,
                                        move |track, t| {
                                            track.bg(motion::color(g.edge_strong, g.accent, t))
                                        },
                                    )
                                }),
                        )
                };
                motion::enter(
                    ("appearance-enter", epoch),
                    div()
                        .relative()
                        .p(px(6.))
                        .w(px(310.))
                        .rounded(px(12.))
                        .bg(g.modal.alpha(1.))
                        .border_1()
                        .border_color(g.edge_strong)
                        .shadow(elevation(g.mode))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.))
                                .pt(px(8.))
                                .pb(px(12.))
                                .border_b_1()
                                .border_color(g.edge_soft)
                                .child(
                                    div()
                                        .px(px(8.))
                                        .child(label(crate::i18n::t("TEMA"), "Ctrl T")),
                                )
                                .child(
                                    div().px(px(8.)).child(
                                        track()
                                            .child(theme_opt(
                                                "theme-light",
                                                crate::i18n::t("Claro"),
                                                IconName::Sun,
                                                Mode::Light,
                                                Box::new(ThemeLight),
                                            ))
                                            .child(theme_opt(
                                                "theme-dark",
                                                crate::i18n::t("Escuro"),
                                                IconName::Moon,
                                                Mode::Dark,
                                                Box::new(ThemeDark),
                                            )),
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.))
                                .px(px(8.))
                                .py(px(12.))
                                .border_b_1()
                                .border_color(g.edge_soft)
                                .child(label(crate::i18n::t("IDIOMA"), ""))
                                .child(
                                    track()
                                        .child(selected(
                                            "lang-pt",
                                            crate::i18n::current() == crate::i18n::Lang::PtBr,
                                            seg(
                                                "lang-pt",
                                                "Português",
                                                None,
                                                crate::i18n::current() == crate::i18n::Lang::PtBr,
                                            )
                                            .on_click(
                                                |_, window, cx| {
                                                    window.dispatch_action(
                                                        Box::new(LangPortuguese),
                                                        cx,
                                                    );
                                                },
                                            ),
                                        ))
                                        .child(selected(
                                            "lang-en",
                                            crate::i18n::current() == crate::i18n::Lang::En,
                                            seg(
                                                "lang-en",
                                                "English",
                                                None,
                                                crate::i18n::current() == crate::i18n::Lang::En,
                                            )
                                            .on_click(
                                                |_, window, cx| {
                                                    window
                                                        .dispatch_action(Box::new(LangEnglish), cx);
                                                },
                                            ),
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.))
                                .px(px(8.))
                                .py(px(12.))
                                .border_b_1()
                                .border_color(g.edge_soft)
                                .child(label(crate::i18n::t("ESCALA DA UI"), "Ctrl Shift = / -"))
                                .child(
                                    track()
                                        .child(scale("scale-100", "100%", 1.0))
                                        .child(scale("scale-115", "115%", 1.15))
                                        .child(scale("scale-130", "130%", 1.30)),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .pt(px(10.))
                                .pb(px(4.))
                                .child(
                                    div()
                                        .px(px(8.))
                                        .pb(px(4.))
                                        .child(label(crate::i18n::t("PAINÉIS"), "")),
                                )
                                .child(toggle(
                                    "pref-auto-collapse",
                                    crate::i18n::t("Recolher inspetor automaticamente"),
                                    crate::i18n::t("Esconde-o quando nada está selecionado"),
                                    auto_hide,
                                    Box::new(ToggleAutoCollapseInspector),
                                )),
                        ),
                )
            })
    }

    /// Cabeçalho de seção de painel lateral (30px): chevron 13px, título 11px semibold e pílula de contagem.
    fn list_section(
        g: &Glass,
        id: &'static str,
        text: impl Into<SharedString>,
        count: Option<usize>,
        collapsed: bool,
    ) -> Stateful<Div> {
        let text: SharedString = text.into();
        div()
            .id(id)
            .role(Role::Button)
            .flex()
            .items_center()
            .gap(px(6.))
            .h(px(30.))
            .pl(px(10.))
            .pr(px(12.))
            .cursor_pointer()
            .child(
                Icon::new(if collapsed {
                    IconName::ChevronRight
                } else {
                    IconName::ChevronDown
                })
                .size(px(13.))
                .text_color(g.text_faint),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(g.text_muted)
                    .child(text.to_uppercase()),
            )
            .children(count.map(|n| {
                div()
                    .px(px(6.))
                    .rounded(px(9.))
                    .bg(g.input)
                    .font_family(FONT_MONO)
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(g.text_faint)
                    .child(n.to_string())
            }))
    }

    /// Linha de lista sem comportamento (grupos, relações): 28px, margem 6px, padding 8px, raio 6px.
    fn plain_row(g: &Glass) -> Div {
        div()
            .mx(px(6.))
            .h(LIST_ROW_H)
            .px(px(8.))
            .rounded(px(6.))
            .flex()
            .items_center()
            .gap(SP_SM)
            .text_size(px(12.5))
            .text_color(g.text_muted)
    }

    /// Linha clicável; ativa usa `sel` + `sel_shadow` (sem brasa).
    fn row(g: &Glass, id: impl Into<ElementId>, on: bool) -> Stateful<Div> {
        div()
            .id(id)
            .role(Role::Button)
            .mx(px(6.))
            .h(LIST_ROW_H)
            .px(px(8.))
            .rounded(px(6.))
            .flex()
            .items_center()
            .gap(SP_SM)
            .text_size(px(12.5))
            .cursor_pointer()
            .text_color(if on { g.text } else { g.text_muted })
            .when(on, |d| d.bg(g.sel).shadow(sel_shadow(g.mode)))
            .when(!on, |d| {
                let h = g.hover;
                d.hover(move |s| s.bg(h))
            })
            .active({
                let a = g.active;
                move |s| s.bg(a)
            })
    }

    /// Botão de ícone de cabeçalho de painel (14px, padding 4px, sem fundo até o hover).
    fn pane_icon_btn(
        g: &Glass,
        id: &'static str,
        icon: IconName,
        tip: &'static str,
        action: Box<dyn Action>,
    ) -> Stateful<Div> {
        let tip_action = action.boxed_clone();
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(tip)
            .p(px(4.))
            .rounded(px(5.))
            .cursor_pointer()
            .text_color(g.text_faint)
            .hover({
                let (h, t) = (g.hover, g.text);
                move |s| s.bg(h).text_color(t)
            })
            .tooltip(move |window, cx| {
                Tooltip::new(tip)
                    .action(tip_action.as_ref(), Some(actions::CTX))
                    .build(window, cx)
            })
            .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
            .child(Icon::new(icon).size(px(14.)))
    }

    /// Linha de arquivo (28px) do painel Projeto: nome em mono 12.5px e contagem `"N tab"` em mono 11px.
    fn file_row(
        g: &Glass,
        id: impl Into<ElementId>,
        name: String,
        tables: usize,
        on: bool,
        dirty: bool,
    ) -> Stateful<Div> {
        Self::row(g, id, on)
            .aria_label(name.clone())
            .child(
                Icon::new(IconName::FileCode)
                    .size(px(14.))
                    .text_color(if on { g.accent } else { g.text_faint }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(FONT_MONO)
                    .text_size(px(12.5))
                    .child(name),
            )
            .child(
                div()
                    .font_family(FONT_MONO)
                    .text_size(px(11.))
                    .text_color(g.text_faint)
                    .child(format!("{tables} tab")),
            )
            .when(dirty, |r| {
                r.child(div().size(px(6.)).rounded_full().bg(g.accent))
            })
    }

    fn files_pane(&self, g: &Glass, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let active_path = self.doc().and_then(|d| d.path.clone());
        let root_dir = self.project.clone().or_else(|| {
            self.doc()
                .and_then(|d| d.path.as_ref()?.parent().map(Path::to_path_buf))
        });
        let folder_files: Vec<PathBuf> = if !self.files.is_empty() {
            self.files.clone()
        } else if root_dir
            .as_ref()
            .is_some_and(|dir| *dir == self.loose_files.0)
        {
            self.loose_files.1.clone()
        } else {
            Vec::new()
        };
        let folder_title = self.project_name().to_uppercase();
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from);
        let folder_path = root_dir
            .as_deref()
            .and_then(|d| d.parent().or(Some(d)))
            .map(|d| crate::session::short_dir(d, home.as_deref()))
            .unwrap_or_default();

        let count = if folder_files.is_empty() {
            self.docs.len()
        } else {
            folder_files.len()
        };

        let header = div()
            .h(PANEL_HEADER_H)
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .pl(px(14.))
            .pr(px(8.))
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(g.text)
            .child(crate::i18n::t("Projeto"))
            .child(div().flex_1())
            .child(Self::pane_icon_btn(
                g,
                "pane-new-file",
                IconName::FilePlus,
                crate::i18n::t("Novo arquivo"),
                Box::new(NewFile),
            ))
            .child(Self::pane_icon_btn(
                g,
                "pane-open-folder",
                IconName::FolderInput,
                crate::i18n::t("Abrir pasta"),
                Box::new(OpenFolder),
            ))
            .child(Self::pane_icon_btn(
                g,
                "pane-close-files",
                IconName::ChevronsLeft,
                crate::i18n::t("Fechar painel"),
                Box::new(ToggleSidebar),
            ));

        let collapsed = self.collapsed.contains("files:root");
        let open_progress = motion::value(
            "files-progress",
            f32::from(!collapsed),
            motion::BASE,
            window,
            cx,
        );
        let sub_header =
            Self::list_section(g, "files-sec-header", folder_title, Some(count), collapsed)
                .on_click(cx.listener(|this, _, _, cx| {
                    if !this.collapsed.remove("files:root") {
                        this.collapsed.insert("files:root".to_string());
                    }
                    cx.notify();
                }))
                .child(div().flex_1())
                .when(!folder_path.is_empty(), |d| {
                    d.child(
                        div()
                            .font_family(FONT_MONO)
                            .text_size(px(11.))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(g.text_faint)
                            .child(folder_path),
                    )
                });

        let mut sec = div()
            .flex_none()
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(g.edge_soft)
            .pb(px(6.))
            .child(sub_header);

        let mut items = div().flex().flex_col();
        if !collapsed || open_progress > 0. {
            if folder_files.is_empty() {
                for (i, d) in self.docs.iter().enumerate() {
                    let on = i == self.active;
                    let tables = d.diagram.read(cx).schema().tables.len();
                    items = items.child(
                        Self::file_row(g, ("open", i), d.title.clone(), tables, on, d.dirty)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.active = i;
                                cx.notify();
                            })),
                    );
                }
            } else {
                for (i, f) in folder_files.iter().enumerate() {
                    let on = active_path.as_ref() == Some(f);
                    let path = f.clone();
                    let open_doc = self.docs.iter().find(|d| d.path.as_ref() == Some(f));
                    let is_dirty = open_doc.is_some_and(|d| d.dirty);
                    let tables = open_doc.map_or_else(
                        || {
                            std::fs::read_to_string(f)
                                .ok()
                                .map_or(0, |t| dbml_analysis::analyze(&t).tables.len())
                        },
                        |d| d.diagram.read(cx).schema().tables.len(),
                    );
                    items = items.child(
                        Self::file_row(g, ("file", i), file_title(f), tables, on, is_dirty)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_path(path.clone(), window, cx);
                            })),
                    );
                }
            }
        }

        sec = sec.child(motion::reveal_at("files-reveal", open_progress, items, cx));

        div().size_full().flex().flex_col().child(header).child(
            div()
                .id("sidebar-body")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .child(sec)
                .child(div().flex_1().border_t_1().border_color(g.edge_soft)),
        )
    }

    fn schema_pane(&self, g: &Glass, cx: &mut Context<Self>) -> Div {
        let Some(doc) = self.doc() else { return div() };
        // filtro do explorador: substring sem diferenciar maiúsculas; vazio = tudo
        let filter_open = self.filter.is_some();
        let query = self
            .filter
            .as_ref()
            .map(|(i, _)| i.read(cx).value().trim().to_lowercase())
            .filter(|q| !q.is_empty());
        let hit = |text: &str| {
            query
                .as_ref()
                .is_none_or(|q| text.to_lowercase().contains(q))
        };
        let d = doc.diagram.read(cx);
        let sel = d.selection();
        let sel_enum = d.selected_enum().map(|e| e.name.clone());
        let schema = d.schema().clone();
        let row = |t: &schema::Table, tint: Option<Hsla>, cx: &mut Context<Self>| {
            let qname = t.qualified_name();
            let on = sel.contains(&qname);
            let name = qname.clone();
            Self::row(g, SharedString::from(format!("side-{qname}")), on)
                .aria_label(crate::i18n::t("Tabela {}").replace("{}", &qname))
                .on_click(cx.listener(move |this, _, _, cx| {
                    let n = name.clone();
                    this.with_diagram(cx, |d| d.focus_table(&n));
                    this.inspector_open = true;
                    this.save_prefs();
                    cx.notify();
                }))
                .child(
                    Icon::new(IconName::Table2)
                        .size(px(14.))
                        .text_color(tint.unwrap_or(g.accent)),
                )
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .font_family(FONT_MONO)
                        .text_size(px(12.5))
                        .child(qname),
                )
                .child(
                    div()
                        .font_family(FONT_MONO)
                        .text_size(px(11.))
                        .text_color(g.text_faint)
                        .child(t.columns.len().to_string()),
                )
        };
        let mut body = div().flex().flex_col();
        let mut shown_tables = 0;
        for gr in &schema.groups {
            let members: Vec<_> = gr
                .tables
                .iter()
                .filter_map(|n| schema.table(n))
                .filter(|t| hit(&t.qualified_name()) || hit(&gr.name))
                .collect();
            if query.is_some() && members.is_empty() {
                continue;
            }
            shown_tables += members.len();
            let c = gr
                .color
                .as_deref()
                .and_then(hex)
                .unwrap_or_else(|| crate::canvas::group_color(&schema, &gr.name));
            body = body.child(
                Self::plain_row(g)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(g.text)
                    .child(Icon::new(IconName::Layers).size(px(14.)).text_color(c))
                    .child(div().flex_1().truncate().child(gr.name.clone()))
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .text_size(px(11.))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(g.text_faint)
                            .child(gr.tables.len().to_string()),
                    ),
            );
            for t in members {
                body = body.child(row(t, Some(c), cx).pl(px(26.)));
            }
        }
        let loose: Vec<_> = schema
            .tables
            .iter()
            .filter(|t| schema.group_of(&t.qualified_name()).is_none())
            .filter(|t| hit(&t.qualified_name()))
            .collect();
        let loose_count = loose.len();
        shown_tables += loose_count;
        let mut loose_rows = div().flex().flex_col();
        for t in loose {
            loose_rows = loose_rows.child(row(t, t.color.as_deref().and_then(hex), cx).pl(px(8.)));
        }
        let mut enums_list = div().flex().flex_col();
        let enums: Vec<_> = schema.enums.iter().filter(|e| hit(&e.name)).collect();
        for en in &enums {
            let name = en.name.clone();
            let on = sel_enum.as_deref() == Some(name.as_str());
            enums_list = enums_list.child(
                Self::row(g, SharedString::from(format!("schema-enum-{name}")), on)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.with_diagram(cx, |d| d.focus_enum(&name));
                        this.inspector_open = true;
                        this.save_prefs();
                        cx.notify();
                    }))
                    .child(Icon::new(IconName::List).size(px(14.)).text_color(g.accent))
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .font_family(FONT_MONO)
                            .text_size(px(12.5))
                            .child(en.name.clone()),
                    )
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .text_size(px(11.))
                            .text_color(g.text_faint)
                            .child(en.values.len().to_string()),
                    ),
            );
        }
        let mut refs = div().flex().flex_col();
        let ref_label =
            |r: &schema::Ref| format!("{} → {}", r.from.qualified_table(), r.to.qualified_table());
        let shown_refs: Vec<_> = schema.refs.iter().filter(|r| hit(&ref_label(r))).collect();
        for r in &shown_refs {
            refs = refs.child(
                Self::plain_row(g)
                    .h(px(28.))
                    .child(
                        Icon::new(IconName::ArrowUpRight)
                            .size(px(14.))
                            .text_color(g.text_faint),
                    )
                    .child(
                        div()
                            .truncate()
                            .font_family(FONT_MONO)
                            .text_size(px(12.5))
                            .child(ref_label(r)),
                    ),
            );
        }
        // com filtro, grupos sem itens visíveis somem
        let filtering = query.is_some();
        let mut out = div().flex().flex_col();
        let make_sec = |id: &'static str,
                        key: &'static str,
                        title: &'static str,
                        n: usize,
                        force_collapsed: bool,
                        items: Div,
                        this_collapsed: &std::collections::HashSet<String>,
                        cx: &mut Context<Self>| {
            let user_toggled = this_collapsed.contains(key);
            let is_collapsed = if force_collapsed {
                !user_toggled
            } else {
                user_toggled || n == 0
            };
            let k = key.to_string();
            div()
                .flex_none()
                .flex()
                .flex_col()
                .border_t_1()
                .border_color(g.edge_soft)
                .pb(px(6.))
                .child(
                    Self::list_section(g, id, title, Some(n), is_collapsed).on_click(cx.listener(
                        move |this, _, _, cx| {
                            if !this.collapsed.remove(&k) {
                                this.collapsed.insert(k.clone());
                            }
                            cx.notify();
                        },
                    )),
                )
                .child(motion::reveal(
                    (ElementId::from(id), "body"),
                    !is_collapsed,
                    items,
                ))
        };
        if !filtering || shown_tables > 0 {
            if !schema.groups.is_empty() {
                out = out.child(make_sec(
                    "sec-groups",
                    "schema:groups",
                    crate::i18n::t("GRUPOS"),
                    schema.groups.len(),
                    false,
                    body,
                    &self.collapsed,
                    cx,
                ));
            }
            out = out.child(make_sec(
                "sec-tables",
                "schema:tables",
                if schema.groups.is_empty() {
                    crate::i18n::t("TABELAS")
                } else {
                    crate::i18n::t("SEM GRUPO")
                },
                loose_count,
                false,
                loose_rows,
                &self.collapsed,
                cx,
            ));
        }
        if !filtering || !enums.is_empty() {
            out = out.child(make_sec(
                "sec-enums",
                "schema:enums",
                "ENUMS",
                enums.len(),
                enums.is_empty(),
                enums_list,
                &self.collapsed,
                cx,
            ));
        }
        if !filtering || !shown_refs.is_empty() {
            let auto_collapse_refs = shown_refs.len() > 3 && !sel.is_empty();
            out = out.child(make_sec(
                "sec-refs",
                "schema:refs",
                crate::i18n::t("RELAÇÕES"),
                shown_refs.len(),
                auto_collapse_refs,
                refs,
                &self.collapsed,
                cx,
            ));
        }
        let filter_field = self.filter.as_ref().map(|(input, _)| {
            div().flex_none().px(px(10.)).pb(px(8.)).child(
                div()
                    .id("explorer-filter")
                    .role(Role::SearchInput)
                    .aria_label(crate::i18n::t("Filtrar tabelas"))
                    .h(px(26.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .bg(g.input)
                    .border_1()
                    .border_color(g.edge_soft)
                    .flex()
                    .items_center()
                    .on_action(cx.listener(|this, _: &Escape, window, cx| {
                        this.close_filter(window, cx);
                    }))
                    .child(Input::new(input).small().appearance(false)),
            )
        });
        let n_probs = doc.diags.len();
        let (errors, warnings) = doc.counts();
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(PANEL_HEADER_H)
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .pl(px(14.))
                    .pr(px(8.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(crate::i18n::t("Esquema"))
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("schema-filter")
                            .role(Role::Button)
                            .aria_label(crate::i18n::t("Filtrar tabelas"))
                            .p(px(4.))
                            .rounded(px(5.))
                            .cursor_pointer()
                            .text_color(if filter_open { g.text } else { g.text_faint })
                            .when(filter_open, |d| d.bg(g.hover))
                            .hover({
                                let (h, t) = (g.hover, g.text);
                                move |s| s.bg(h).text_color(t)
                            })
                            .tooltip(|window, cx| {
                                Tooltip::new(crate::i18n::t("Filtrar tabelas"))
                                    .action(&FilterExplorer, Some(actions::CTX))
                                    .build(window, cx)
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_filter_explorer(window, cx);
                            }))
                            .child(Icon::new(IconName::ListFilter).size(px(14.))),
                    )
                    .child(Self::pane_icon_btn(
                        g,
                        "schema-new",
                        IconName::Plus,
                        crate::i18n::t("Nova tabela"),
                        Box::new(NewTable),
                    ))
                    .child(Self::pane_icon_btn(
                        g,
                        "schema-close",
                        IconName::ChevronsLeft,
                        crate::i18n::t("Fechar painel"),
                        Box::new(ToggleSidebar),
                    )),
            )
            .children(filter_field)
            .child(
                div()
                    .id("sidebar-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .child(out)
                    .child(div().flex_1().border_t_1().border_color(g.edge_soft)),
            )
            .child(
                div()
                    .id("schema-validation")
                    .role(Role::Button)
                    .aria_label(crate::i18n::t("Mostrar problemas"))
                    .flex_none()
                    .border_t_1()
                    .border_color(g.edge_soft)
                    .px(px(14.))
                    .py(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(12.))
                    .text_color(g.text_muted)
                    .cursor_pointer()
                    .hover({
                        let h = g.hover;
                        move |s| s.bg(h)
                    })
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(ShowProblems), cx))
                    .child(
                        Icon::new(if n_probs == 0 {
                            IconName::CircleCheck
                        } else {
                            IconName::CircleX
                        })
                        .size(px(14.))
                        .text_color(if n_probs == 0 {
                            g.ok
                        } else {
                            g.danger
                        }),
                    )
                    .child(if errors == 0 && warnings > 0 {
                        format!(
                            "{warnings} {}",
                            crate::i18n::t(if warnings == 1 { "aviso" } else { "avisos" })
                        )
                    } else if n_probs == 0 {
                        crate::i18n::t("Modelo válido").to_string()
                    } else if n_probs == 1 {
                        crate::i18n::t("1 problema").to_string()
                    } else {
                        crate::i18n::t("{} problemas").replace("{}", &n_probs.to_string())
                    })
                    .child(div().flex_1())
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .text_size(px(11.))
                            .text_color(g.text_faint)
                            .child(format!(
                                "{} · {} · {}",
                                schema.tables.len(),
                                schema.tables.iter().map(|t| t.columns.len()).sum::<usize>(),
                                schema.refs.len()
                            )),
                    ),
            )
    }

    fn problems_pane(&self, g: &Glass, cx: &mut Context<Self>) -> Div {
        let Some(doc) = self.doc() else { return div() };
        let text = doc.editor.read(cx).value().to_string();
        let mut list = div().w_full().min_w_0().flex().flex_col();
        for (i, d) in doc.diags.iter().enumerate() {
            let (icon, color) = match d.severity {
                Severity::Error => (IconName::CircleX, g.danger),
                Severity::Warning => (IconName::TriangleAlert, g.warning),
                Severity::Info => (IconName::Info, g.text_faint),
            };
            let mut line_end = d.range.start.min(text.len());
            while !text.is_char_boundary(line_end) {
                line_end -= 1;
            }
            let line = text[..line_end].matches('\n').count() + 1;
            let at = d.range.start;
            let message = crate::i18n::diagnostic(&d.message);
            list = list.child(
                Self::row(g, ("problem", i), false)
                    .h_auto()
                    .min_w_0()
                    .pl(px(10.))
                    .pr(px(8.))
                    .py(px(7.))
                    .gap(px(10.))
                    .rounded(px(7.))
                    .items_start()
                    .aria_label(message.clone())
                    .on_click(cx.listener(move |this, _, window, cx| this.reveal(at, window, cx)))
                    .child(
                        div()
                            .mt(px(1.))
                            .flex_shrink_0()
                            .child(Icon::new(icon).size(px(14.)).text_color(color)),
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
                                    .w_full()
                                    .min_w_0()
                                    .text_size(px(12.5))
                                    .line_height(relative(1.35))
                                    .text_color(g.text)
                                    .child(message),
                            )
                            .child(
                                div()
                                    .font_family(FONT_MONO)
                                    .text_size(px(11.))
                                    .text_color(g.text_faint)
                                    .child(
                                        crate::i18n::t("{} · linha {}")
                                            .replacen("{}", &doc.title, 1)
                                            .replacen("{}", &line.to_string(), 1),
                                    ),
                            ),
                    ),
            );
        }
        let body = if doc.diags.is_empty() {
            div()
                .px(px(16.))
                .py(px(6.))
                .flex()
                .items_center()
                .gap(SP_SM)
                .text_size(px(12.5))
                .text_color(g.text_muted)
                .child(
                    Icon::new(IconName::CircleCheck)
                        .size(px(14.))
                        .text_color(g.ok),
                )
                .child(crate::i18n::t("Nenhum problema neste arquivo"))
        } else {
            list
        };
        let n_probs = doc.diags.len();
        let has_probs = n_probs > 0;
        div()
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(PANEL_HEADER_H)
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .pl(px(14.))
                    .pr(px(8.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(g.text)
                    .child(crate::i18n::t("Problemas"))
                    .child(
                        div()
                            .px(px(6.))
                            .rounded(px(9.))
                            .bg(if has_probs {
                                g.danger.opacity(0.10)
                            } else {
                                g.input
                            })
                            .font_family(FONT_MONO)
                            .text_size(px(11.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if has_probs { g.danger } else { g.text_faint })
                            .child(n_probs.to_string()),
                    )
                    .child(div().flex_1())
                    .child(Self::pane_icon_btn(
                        g,
                        "problems-close",
                        IconName::ChevronsLeft,
                        crate::i18n::t("Fechar painel"),
                        Box::new(ToggleSidebar),
                    )),
            )
            .child(
                div()
                    .id("sidebar-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .flex_col()
                            .border_t_1()
                            .border_color(g.edge_soft)
                            .py(px(6.))
                            .child(
                                div()
                                    .h(px(30.))
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .pl(px(10.))
                                    .pr(px(12.))
                                    .child(
                                        Icon::new(IconName::ChevronDown)
                                            .size(px(13.))
                                            .text_color(g.text_faint),
                                    )
                                    .child(
                                        div()
                                            .font_family(FONT_MONO)
                                            .text_size(px(11.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(g.text_muted)
                                            .child(doc.title.clone()),
                                    ),
                            )
                            .child(body),
                    )
                    .child(div().flex_1().border_t_1().border_color(g.edge_soft)),
            )
    }

    pub(super) fn sidebar(
        &self,
        g: &Glass,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let content = match self.pane {
            Pane::Files => self.files_pane(g, window, cx),
            Pane::Schema => self.schema_pane(g, cx),
            Pane::Problems => self.problems_pane(g, cx),
        };
        div()
            .id("sidebar")
            .role(Role::Region)
            .aria_label(crate::i18n::t("Barra lateral"))
            .size_full()
            .flex()
            .flex_col()
            .bg(g.sidebar)
            .border_r_1()
            .border_color(g.edge_soft)
            .child(content)
    }

    pub(super) fn inspector(
        &self,
        g: &Glass,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let (
            selection,
            selected,
            selected_enum,
            group,
            group_tint,
            all_groups,
            selected_ref,
            multi,
        ) = match self.doc() {
            Some(doc) => {
                let d = doc.diagram.read(cx);
                let sel = d.selection();
                let one = d.selected().map(|t| {
                    let qname = t.qualified_name();
                    (
                        t.clone(),
                        d.refs_of(&qname).into_iter().cloned().collect::<Vec<_>>(),
                    )
                });
                let one_enum = d.selected_enum().map(|en| {
                    let refs_by: Vec<(String, String)> = d
                        .schema()
                        .tables
                        .iter()
                        .flat_map(|t| {
                            t.columns
                                .iter()
                                .filter(|c| c.ty.rsplit('.').next().is_some_and(|ty| ty == en.name))
                                .map(move |c| (t.qualified_name(), c.name.clone()))
                        })
                        .collect();
                    (en.clone(), refs_by)
                });
                let group = one
                    .as_ref()
                    .and_then(|(t, _)| d.schema().group_of(&t.qualified_name()).cloned());
                let group_tint = group.as_ref().map(|gr| {
                    gr.color
                        .as_deref()
                        .and_then(hex)
                        .unwrap_or_else(|| crate::canvas::group_color(d.schema(), &gr.name))
                });
                let all_groups: Vec<(String, Hsla)> =
                    d.schema()
                        .groups
                        .iter()
                        .map(|gr| {
                            let c = gr.color.as_deref().and_then(hex).unwrap_or_else(|| {
                                crate::canvas::group_color(d.schema(), &gr.name)
                            });
                            (gr.name.clone(), c)
                        })
                        .collect();
                let sref = d.selected_ref().and_then(|(from, to)| {
                    d.schema()
                        .refs
                        .iter()
                        .find(|r| {
                            let fq = r.from.qualified_table();
                            let tq = r.to.qualified_table();
                            (fq == from.0
                                && r.from.column == from.1
                                && tq == to.0
                                && r.to.column == to.1)
                                || (fq == to.0
                                    && r.from.column == to.1
                                    && tq == from.0
                                    && r.to.column == from.1)
                        })
                        .cloned()
                });
                // seleção múltipla: (nome, cor do cartão, grupo)
                let multi: Vec<(String, Hsla, Option<String>)> = sel
                    .iter()
                    .map(|n| {
                        let gr = d.schema().group_of(n);
                        let color = d
                            .schema()
                            .tables
                            .iter()
                            .find(|t| t.qualified_name() == *n)
                            .and_then(|t| t.color.as_deref().and_then(hex))
                            .or_else(|| {
                                gr.map(|gr| {
                                    gr.color.as_deref().and_then(hex).unwrap_or_else(|| {
                                        crate::canvas::group_color(d.schema(), &gr.name)
                                    })
                                })
                            })
                            .unwrap_or(g.accent);
                        (n.clone(), color, gr.map(|gr| gr.name.clone()))
                    })
                    .collect();
                (
                    sel, one, one_enum, group, group_tint, all_groups, sref, multi,
                )
            }
            None => (
                Vec::new(),
                None,
                None,
                None,
                None,
                Vec::new(),
                None,
                Vec::new(),
            ),
        };
        let is_single_table = selection.len() <= 1 && selected.is_some();

        // Cabeçalho de seção do inspector (30px): chevron 13px, título 11px semibold e pílula opcional.
        let sec_header = |label: &'static str, count: Option<usize>, right: Option<AnyElement>| {
            div()
                .h(px(30.))
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(g.text_muted)
                .child(
                    Icon::new(IconName::ChevronDown)
                        .size(px(13.))
                        .text_color(g.text_faint),
                )
                .child(label)
                .children(count.map(|n| {
                    div()
                        .px(px(6.))
                        .rounded(px(9.))
                        .bg(g.input)
                        .font_family(FONT_MONO)
                        .text_size(px(11.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(g.text_faint)
                        .child(n.to_string())
                }))
                .child(div().flex_1())
                .children(right)
        };

        // Campo em edição: borda de acento + fundo sólido + anel de foco.
        let edit_box = |st: &Entity<InputState>| -> AnyElement {
            div()
                .w_full()
                .h(px(26.))
                .px(px(6.))
                .rounded(px(6.))
                .border_1()
                .border_color(g.accent)
                .bg(g.input_solid)
                .shadow(vec![ring(g)])
                .flex()
                .items_center()
                .child(Input::new(st).small().appearance(false))
                .into_any_element()
        };
        // Lixeira discreta (13px faint, vermelha no hover); o chamador liga o `on_click`.
        let trash = |id: SharedString, tip: &'static str| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(tip)
                .flex_none()
                .p(px(4.))
                .rounded(px(5.))
                .cursor_pointer()
                .text_color(g.text_faint)
                .hover({
                    let d = g.danger;
                    move |s| s.text_color(d)
                })
                .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
                .child(Icon::new(IconName::Trash).size(px(13.)))
        };
        // Chip de índice/relação (22px, raio 6, mono 11px): ligado = acento 16% com borda acento 40%.
        let chip_btn = |id: SharedString, label: SharedString, on: bool| {
            div()
                .id(id)
                .role(Role::Switch)
                .h(px(22.))
                .px(px(7.))
                .flex_none()
                .rounded(px(6.))
                .border_1()
                .flex()
                .items_center()
                .justify_center()
                .font_family(FONT_MONO)
                .text_size(px(11.))
                .cursor_pointer()
                .when(on, |d| {
                    d.bg(g.accent.opacity(0.16))
                        .border_color(g.accent.opacity(0.40))
                        .text_color(g.accent)
                })
                .when(!on, |d| {
                    let f = g.text_faint;
                    d.border_color(g.edge_strong)
                        .text_color(g.text_muted)
                        .hover(move |s| s.border_color(f))
                })
                .child(label)
        };
        // Botão de rodapé Excluir (`30px`, `12.5px`, `g.danger`, com tecla `Del` opcional).
        let footer_danger =
            |id: &'static str, label: String, tip: &'static str, show_del_key: bool| {
                let aria = label.clone();
                div()
                    .flex_none()
                    .px(px(14.))
                    .py(px(10.))
                    .border_t_1()
                    .border_color(g.edge_soft)
                    .flex()
                    .child(
                        div()
                            .id(id)
                            .role(Role::Button)
                            .aria_label(aria)
                            .flex_1()
                            .h(px(30.))
                            .rounded(px(7.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap(px(7.))
                            .text_size(px(12.5))
                            .text_color(g.danger)
                            .cursor_pointer()
                            .hover({
                                let d = g.danger.opacity(0.10);
                                move |s| s.bg(d)
                            })
                            .tooltip(move |window, cx| {
                                Tooltip::new(tip)
                                    .action(&DeleteSelection, Some(actions::CANVAS))
                                    .build(window, cx)
                            })
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(DeleteSelection), cx);
                            })
                            .child(Icon::new(IconName::Trash).size(px(13.)))
                            .child(label)
                            .when(show_del_key, |d| {
                                d.child(
                                    div()
                                        .ml(px(4.))
                                        .font_family(FONT_MONO)
                                        .text_size(px(10.5))
                                        .text_color(g.text_faint)
                                        .child("Del"),
                                )
                            }),
                    )
            };

        let (body, footer): (Div, Option<Div>) = if selection.len() > 1 {
            let n = selection.len();
            let mut list = div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(2.))
                .pt(px(4.))
                .pb(px(12.))
                .px(px(8.))
                .border_t_1()
                .border_color(g.edge_soft)
                .child(div().px(px(6.)).child(sec_header(
                    crate::i18n::t("TABELAS"),
                    Some(n),
                    None,
                )));
            for (i, (name, color, grp)) in multi.iter().enumerate() {
                let tbl_click = name.clone();
                list = list.child(
                    div()
                        .id(("multi-row", i))
                        .role(Role::Button)
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .h(px(30.))
                        .px(px(8.))
                        .rounded(px(7.))
                        .cursor_pointer()
                        .hover({
                            let h = g.hover;
                            move |s| s.bg(h)
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.with_diagram(cx, |d| d.focus_table(&tbl_click));
                        }))
                        .child(Icon::new(IconName::Table2).size(px(13.)).text_color(*color))
                        .child(
                            div()
                                .font_family(FONT_MONO)
                                .text_size(px(12.5))
                                .text_color(g.text)
                                .child(name.clone()),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .font_family(FONT)
                                .text_size(px(11.5))
                                .text_color(g.text_faint)
                                .child(
                                    grp.clone()
                                        .unwrap_or_else(|| crate::i18n::t("sem grupo").into()),
                                ),
                        ),
                );
            }
            let summary = div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(12.))
                .px(px(14.))
                .py(px(12.))
                .border_t_1()
                .border_color(g.edge_soft)
                .child(
                    div()
                        .relative()
                        .w(px(36.))
                        .h(px(30.))
                        .flex_none()
                        .child(
                            div()
                                .absolute()
                                .left(px(8.))
                                .top(px(0.))
                                .size(px(26.))
                                .rounded(px(8.))
                                .bg(g.input)
                                .border_1()
                                .border_color(g.edge_soft),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(px(0.))
                                .top(px(4.))
                                .size(px(26.))
                                .rounded(px(8.))
                                .bg(g.accent.opacity(0.16))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    Icon::new(IconName::Copy).size(px(13.)).text_color(g.accent),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(g.text)
                                .child(crate::i18n::t("{} tabelas").replace("{}", &n.to_string())),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(g.text_faint)
                                .child(crate::i18n::t("Shift+clique adiciona ou remove")),
                        ),
                );
            let acts = div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(6.))
                .p(px(14.))
                .border_t_1()
                .border_color(g.edge_soft)
                .child(
                    div()
                        .id("insp-group")
                        .role(Role::Button)
                        .aria_label(crate::i18n::t("Agrupar seleção"))
                        .h(px(32.))
                        .rounded(px(8.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(8.))
                        .bg(g.accent)
                        .text_size(px(12.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(gpui_kit::white())
                        .cursor_pointer()
                        .hover({
                            let a = g.accent.opacity(0.88);
                            move |s| s.bg(a)
                        })
                        .tooltip(|window, cx| {
                            Tooltip::new(crate::i18n::t("Agrupar tabelas selecionadas"))
                                .action(&GroupSelection, Some(actions::CTX))
                                .build(window, cx)
                        })
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(GroupSelection), cx);
                        })
                        .child(Icon::new(IconName::Layers).size(px(14.)))
                        .child(crate::i18n::t("Agrupar seleção"))
                        .child(
                            div()
                                .font_family(FONT_MONO)
                                .text_size(px(10.5))
                                .opacity(0.75)
                                .child("Ctrl G"),
                        ),
                );
            (
                div()
                    .flex()
                    .flex_col()
                    .child(summary)
                    .child(list)
                    .child(acts),
                Some(footer_danger(
                    "insp-del-multi",
                    crate::i18n::t("Excluir {} tabelas").replace("{}", &n.to_string()),
                    crate::i18n::t("Excluir tabelas selecionadas"),
                    false,
                )),
            )
        } else if let Some((t, refs)) = selected {
            let editing = self
                .editing
                .as_ref()
                .map(|(t, st, _)| (t.clone(), st.clone()));
            let is_editing = |target: &EditTarget| matches!(&editing, Some((t, _)) if t == target);
            let t_qname = t.qualified_name();

            let open_col_name: Option<String> = match &editing {
                Some((
                    EditTarget::ColName(tb, col)
                    | EditTarget::ColType(tb, col)
                    | EditTarget::ColDefault(tb, col)
                    | EditTarget::ColCheck(tb, col)
                    | EditTarget::ColNote(tb, col),
                    _,
                )) if tb == &t_qname => Some(col.clone()),
                _ => {
                    if let Some((tb, col_opt)) = &self.open_col
                        && tb == &t_qname
                        && col_opt
                            .as_ref()
                            .is_none_or(|cn| t.columns.iter().any(|c| &c.name == cn))
                    {
                        col_opt.clone()
                    } else if is_editing(&EditTarget::Table(t_qname.clone()))
                        && t.columns.len() <= 1
                    {
                        None
                    } else {
                        t.columns
                            .get(1)
                            .or_else(|| t.columns.first())
                            .map(|c| c.name.clone())
                    }
                }
            };

            let tile_color = t
                .color
                .as_deref()
                .and_then(hex)
                .or(group_tint)
                .unwrap_or(g.accent);
            let table_target = EditTarget::Table(t_qname.clone());
            let title_el: AnyElement = match &editing {
                Some((tg, st)) if *tg == table_target => div()
                    .h(px(26.))
                    .px(px(8.))
                    .ml(px(-8.))
                    .rounded(px(7.))
                    .bg(g.input_solid)
                    .border_1()
                    .border_color(g.accent)
                    .shadow(vec![ring(g)])
                    .flex()
                    .items_center()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(Input::new(st).small().appearance(false))
                    .into_any_element(),
                _ => {
                    let val = t_qname.clone();
                    div()
                        .id("insp-table-name")
                        .role(Role::Button)
                        .aria_label(crate::i18n::t("Renomear tabela {}").replace("{}", &val))
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(g.text)
                        .truncate()
                        .cursor_text()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.start_edit(table_target.clone(), val.clone(), window, cx);
                        }))
                        .child(t_qname.clone())
                        .into_any_element()
                }
            };
            let group_sub = group.as_ref().map_or_else(
                || crate::i18n::t("sem grupo").to_string(),
                |gr| gr.name.clone(),
            );
            let ident = div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(10.))
                .px(px(14.))
                .py(px(12.))
                .border_t_1()
                .border_color(g.edge_soft)
                .child(
                    div()
                        .size(px(30.))
                        .rounded(px(8.))
                        .bg(tile_color.opacity(0.16))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Icon::new(IconName::Table2)
                                .size(px(15.))
                                .text_color(tile_color),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(title_el)
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(g.text_faint)
                                .truncate()
                                .child(
                                    crate::i18n::t("Tabela · {} · F2 para renomear")
                                        .replace("{}", &group_sub),
                                ),
                        ),
                );

            let group_el: AnyElement = {
                let has_group = group.is_some();
                let gr_name = group.as_ref().map_or_else(
                    || crate::i18n::t("Sem grupo").to_string(),
                    |gr| gr.name.clone(),
                );
                let gc = group_tint.unwrap_or(g.accent);
                let tbl = t_qname.clone();
                let groups_for_menu = all_groups;
                let shell_weak = cx.entity().downgrade();
                let g_menu = *g;
                let epoch = self.popover_epoch;
                let on_open = shell_weak.clone();
                Popover::new("insp-group-popover")
                    .anchor(Anchor::TopLeft)
                    .offset(px(4.))
                    .appearance(false)
                    .on_open_change(move |open, _, cx| {
                        if *open {
                            _ = on_open.update(cx, |s, cx| {
                                s.popover_epoch = s.popover_epoch.wrapping_add(1);
                                cx.notify();
                            });
                        }
                    })
                    .trigger(
                        Button::new("insp-group-sel")
                            .ghost()
                            .small()
                            .w_full()
                            .h(px(28.))
                            .px(px(10.))
                            .rounded(px(7.))
                            .when(has_group, |b| b.bg(g.input))
                            .border_1()
                            .when(!has_group, Styled::border_dashed)
                            .border_color(if has_group {
                                g.edge_soft
                            } else {
                                g.edge_strong
                            })
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .when(has_group, |d| {
                                        d.child(
                                            Icon::new(IconName::Layers)
                                                .size(px(13.))
                                                .text_color(gc),
                                        )
                                    })
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .text_size(px(13.))
                                            .text_color(g.text)
                                            .child(gr_name),
                                    )
                                    .child(
                                        Icon::new(IconName::ChevronsUpDown)
                                            .size(px(12.))
                                            .text_color(g.text_faint),
                                    ),
                            ),
                    )
                    .content(move |_, _, _| {
                        let mut col = div().flex().flex_col().gap(px(2.));
                        for (gname, gcol) in &groups_for_menu {
                            let gname_click = gname.clone();
                            let tbl_click = tbl.clone();
                            let shell = shell_weak.clone();
                            let h = g_menu.hover;
                            col = col.child(
                                div()
                                    .id(SharedString::from(format!("grp-opt-{gname}")))
                                    .role(Role::MenuItem)
                                    .h(px(30.))
                                    .px(px(10.))
                                    .rounded(px(7.))
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .text_size(px(13.))
                                    .text_color(g_menu.text)
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(h))
                                    .on_click(move |_, window, cx| {
                                        let _ = shell.update(cx, |s, cx| {
                                            s.assign_group(&tbl_click, &gname_click, window, cx);
                                        });
                                    })
                                    .child(
                                        Icon::new(IconName::Layers).size(px(14.)).text_color(*gcol),
                                    )
                                    .child(gname.clone()),
                            );
                        }
                        if !groups_for_menu.is_empty() {
                            col = col
                                .child(div().h(px(1.)).my(px(4.)).mx(px(6.)).bg(g_menu.edge_soft));
                        }
                        let h = g_menu.hover;
                        motion::enter(
                            ("group-menu-enter", epoch),
                            col.relative()
                                .p(px(5.))
                                .w(px(200.))
                                .rounded(px(11.))
                                .bg(g_menu.modal.alpha(1.))
                                .border_1()
                                .border_color(g_menu.edge_strong)
                                .shadow(elevation(g_menu.mode))
                                .child(
                                    div()
                                        .id("grp-opt-none")
                                        .role(Role::MenuItem)
                                        .h(px(30.))
                                        .px(px(10.))
                                        .rounded(px(7.))
                                        .flex()
                                        .items_center()
                                        .gap(px(10.))
                                        .text_size(px(13.))
                                        .text_color(g_menu.text)
                                        .cursor_pointer()
                                        .hover(move |s| s.bg(h))
                                        .on_click(move |_, window, cx| {
                                            window.dispatch_action(
                                                if has_group {
                                                    Box::new(UngroupSelection)
                                                } else {
                                                    Box::new(GroupSelection)
                                                },
                                                cx,
                                            );
                                        })
                                        .child(
                                            Icon::new(if has_group {
                                                IconName::X
                                            } else {
                                                IconName::Plus
                                            })
                                            .size(px(14.))
                                            .text_color(g_menu.text_faint),
                                        )
                                        .child(if has_group {
                                            crate::i18n::t("Sem grupo")
                                        } else {
                                            crate::i18n::t("Criar grupo")
                                        })
                                        .child(div().flex_1())
                                        .child(
                                            div()
                                                .font_family(FONT_MONO)
                                                .text_size(px(11.))
                                                .text_color(g_menu.text_faint)
                                                .child(if has_group {
                                                    "Ctrl Shift G"
                                                } else {
                                                    "Ctrl G"
                                                }),
                                        ),
                                ),
                        )
                    })
                    .into_any_element()
            };

            let current_color = t.color.clone().unwrap_or_default().to_ascii_lowercase();
            let mut swatches = div().flex().items_center().gap(px(6.));
            for (i, (label, hexv)) in TABLE_COLORS.iter().enumerate() {
                let on = current_color == *hexv;
                let table = t_qname.clone();
                let color = hex(hexv).unwrap_or_default();
                let shadow = |c: Hsla, spread: f32| BoxShadow {
                    color: c,
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(spread),
                    inset: false,
                };
                swatches = swatches.child(
                    div()
                        .id(("swatch", i))
                        .role(Role::Button)
                        .aria_label(crate::i18n::t("Cor {}").replace("{}", label))
                        .tooltip(move |window, cx| Tooltip::new(*label).build(window, cx))
                        .size(px(18.))
                        .flex_none()
                        .rounded(px(6.))
                        .bg(color)
                        .cursor_pointer()
                        .when(on, |d| {
                            d.shadow(vec![shadow(color, 3.5), shadow(g.swatch_ring, 2.)])
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.set_color(&table, Some(hexv), window, cx);
                        })),
                );
            }
            let table_for_none = t_qname.clone();
            swatches = swatches.child(
                div()
                    .id("swatch-none")
                    .role(Role::Button)
                    .aria_label(crate::i18n::t("Sem cor"))
                    .tooltip(|window, cx| Tooltip::new(crate::i18n::t("Sem cor")).build(window, cx))
                    .size(px(18.))
                    .flex_none()
                    .rounded(px(6.))
                    .border_1()
                    .border_dashed()
                    .border_color(g.edge_strong)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_color(&table_for_none, None, window, cx);
                    })),
            );

            let note_target = EditTarget::TableNote(t_qname.clone());
            let show_note =
                t.note.as_deref().is_some_and(|n| !n.is_empty()) || is_editing(&note_target);
            let mut general_sec = div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(8.))
                .pt(px(4.))
                .pb(px(14.))
                .px(px(14.))
                .border_t_1()
                .border_color(g.edge_soft)
                .child(sec_header(crate::i18n::t("GERAL"), None, None))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .w(px(64.))
                                .flex_none()
                                .text_color(g.text_faint)
                                .child(crate::i18n::t("Grupo")),
                        )
                        .child(div().flex_1().min_w_0().child(group_el)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .w(px(64.))
                                .flex_none()
                                .text_color(g.text_faint)
                                .child(crate::i18n::t("Cor")),
                        )
                        .child(swatches),
                );
            if show_note {
                let note_el: AnyElement = match &editing {
                    Some((tg, st)) if *tg == note_target => edit_box(st),
                    _ => {
                        let note = t.note.clone().unwrap_or_default();
                        div()
                            .id("insp-table-note")
                            .role(Role::Button)
                            .aria_label(crate::i18n::t("Editar nota da tabela"))
                            .w_full()
                            .h(px(26.))
                            .px(px(8.))
                            .rounded(px(6.))
                            .bg(g.input)
                            .border_1()
                            .border_color(g.edge_soft)
                            .flex()
                            .items_center()
                            .text_size(px(12.))
                            .text_color(g.text_muted)
                            .cursor_text()
                            .child(div().truncate().child(note.clone()))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.start_edit(note_target.clone(), note.clone(), window, cx);
                            }))
                            .into_any_element()
                    }
                };
                general_sec = general_sec.child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .w(px(64.))
                                .flex_none()
                                .text_color(g.text_faint)
                                .child(crate::i18n::t("Nota")),
                        )
                        .child(div().flex_1().min_w_0().child(note_el)),
                );
            }

            let step = |id: SharedString, icon: IconName, tip: &'static str, enabled: bool| {
                div()
                    .id(id)
                    .role(Role::Button)
                    .aria_label(tip)
                    .flex_none()
                    .p(px(3.))
                    .rounded(px(4.))
                    .text_color(g.text_faint)
                    .when(enabled, |d| {
                        let t = g.text;
                        d.cursor_pointer().hover(move |s| s.text_color(t))
                    })
                    .when(!enabled, |d| d.opacity(0.35))
                    .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
                    .child(Icon::new(icon).size(px(12.)))
            };
            let n_cols = t.columns.len();
            let mut cols = div().flex().flex_col().gap(px(2.));
            for (ci, c) in t.columns.iter().enumerate() {
                let qname = t_qname.clone();
                let open = open_col_name.as_deref() == Some(c.name.as_str());
                let reveal_id = SharedString::from(format!("column-reveal:{qname}:{}", c.name));
                let open_progress =
                    motion::value(reveal_id.clone(), f32::from(open), motion::BASE, window, cx);
                let is_fk = c.fk
                    || refs
                        .iter()
                        .any(|r| r.from.qualified_table() == qname && r.from.column == c.name);
                let (col_icon, col_icon_color) = if c.pk {
                    (IconName::KeyRound, g.accent)
                } else if is_fk {
                    (IconName::Link, g.link)
                } else {
                    (IconName::Dot, g.text_faint)
                };
                let marks = [c.not_null.then_some("NN"), c.unique.then_some("UQ")]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" ");

                let target_name = EditTarget::ColName(qname.clone(), c.name.clone());
                let target_type = EditTarget::ColType(qname.clone(), c.name.clone());
                let target_def = EditTarget::ColDefault(qname.clone(), c.name.clone());
                let target_chk = EditTarget::ColCheck(qname.clone(), c.name.clone());
                let target_note = EditTarget::ColNote(qname.clone(), c.name.clone());
                let (ed_name, ed_type, ed_def, ed_chk) = (
                    is_editing(&target_name),
                    is_editing(&target_type),
                    is_editing(&target_def),
                    is_editing(&target_chk),
                );

                let name_cell: AnyElement = if ed_name {
                    if let Some((_, st)) = &editing {
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(edit_box(st))
                            .into_any_element()
                    } else {
                        div().into_any_element()
                    }
                } else if open {
                    let cname = c.name.clone();
                    let t_edit = target_name.clone();
                    div()
                        .id(SharedString::from(format!("edit-col-name-{cname}")))
                        .role(Role::Button)
                        .aria_label(crate::i18n::t("Renomear coluna {}").replace("{}", &cname))
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_family(FONT_MONO)
                        .text_size(px(12.5))
                        .text_color(g.text)
                        .cursor_text()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.start_edit(t_edit.clone(), cname.clone(), window, cx);
                        }))
                        .child(c.name.clone())
                        .into_any_element()
                } else {
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_family(FONT_MONO)
                        .text_size(px(12.5))
                        .text_color(g.text)
                        .child(c.name.clone())
                        .into_any_element()
                };

                let (tbl_tog, col_tog) = (qname.clone(), c.name.clone());
                let top_row =
                    div()
                        .id(SharedString::from(format!("col-top-{}", c.name)))
                        .role(Role::Button)
                        .h(px(30.))
                        .px(px(8.))
                        .rounded(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .cursor_pointer()
                        .when(!open, |d| {
                            let h = g.hover;
                            d.hover(move |s| s.bg(h))
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_col = Some((
                                tbl_tog.clone(),
                                if open { None } else { Some(col_tog.clone()) },
                            ));
                            cx.notify();
                        }))
                        .child(
                            Icon::new(IconName::GripVertical)
                                .size(px(12.))
                                .text_color(g.text_faint),
                        )
                        .child(
                            div().w(px(14.)).flex_none().flex().justify_center().child(
                                Icon::new(col_icon).size(px(12.)).text_color(col_icon_color),
                            ),
                        )
                        .child(name_cell)
                        .when(!marks.is_empty(), |d| {
                            d.child(
                                div()
                                    .flex_none()
                                    .font_family(FONT_MONO)
                                    .text_size(px(9.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(g.text_faint)
                                    .child(marks),
                            )
                        })
                        .child(
                            div()
                                .flex_none()
                                .font_family(FONT_MONO)
                                .text_size(px(11.))
                                .text_color(g.text_muted)
                                .child(c.ty.clone()),
                        );

                let mut card = div()
                    .flex()
                    .flex_col()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(g.edge_soft.opacity(open_progress))
                    .bg(g.hover.opacity(open_progress))
                    .child(top_row);

                if open || open_progress > 0. {
                    let row_lbl = |txt: &'static str| {
                        div()
                            .w(px(48.))
                            .flex_none()
                            .text_size(px(12.))
                            .text_color(g.text_faint)
                            .child(txt)
                    };

                    let ty_box: AnyElement = match &editing {
                        Some((tg, st)) if *tg == target_type => div()
                            .flex_1()
                            .min_w_0()
                            .h(px(26.))
                            .px(px(8.))
                            .rounded(px(6.))
                            .bg(g.input_solid)
                            .border_1()
                            .border_color(g.accent)
                            .shadow(vec![ring(g)])
                            .flex()
                            .items_center()
                            .font_family(FONT_MONO)
                            .text_size(px(12.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(Input::new(st).small().appearance(false)),
                            )
                            .child(
                                Icon::new(IconName::ChevronDown)
                                    .size(px(11.))
                                    .text_color(g.text_faint),
                            )
                            .into_any_element(),
                        _ => {
                            let val = c.ty.clone();
                            let t_ty = target_type.clone();
                            div()
                                .id(SharedString::from(format!("col-ty-{}", c.name)))
                                .role(Role::Button)
                                .aria_label(crate::i18n::t("Editar tipo {}").replace("{}", &c.ty))
                                .flex_1()
                                .min_w_0()
                                .h(px(26.))
                                .px(px(8.))
                                .rounded(px(6.))
                                .bg(g.input)
                                .border_1()
                                .border_color(g.edge_soft)
                                .flex()
                                .items_center()
                                .font_family(FONT_MONO)
                                .text_size(px(12.))
                                .text_color(g.text)
                                .cursor_text()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.start_edit(t_ty.clone(), val.clone(), window, cx);
                                }))
                                .child(div().flex_1().min_w_0().truncate().child(c.ty.clone()))
                                .child(
                                    Icon::new(IconName::ChevronDown)
                                        .size(px(11.))
                                        .text_color(g.text_faint),
                                )
                                .into_any_element()
                        }
                    };

                    let flag_seg = |id: String,
                                    label: &'static str,
                                    flag: &'static str,
                                    on: bool,
                                    cx: &mut Context<Self>| {
                        let (table, col) = (qname.clone(), c.name.clone());
                        let motion_id =
                            SharedString::from(format!("flag:{qname}:{}:{flag}", c.name));
                        let g = *g;
                        div()
                            .id(SharedString::from(id))
                            .role(Role::Switch)
                            .aria_label(
                                crate::i18n::t("{} em {}")
                                    .replacen("{}", label, 1)
                                    .replacen("{}", &c.name, 1),
                            )
                            .flex_1()
                            .h(px(22.))
                            .rounded(px(5.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .font_family(FONT_MONO)
                            .text_size(px(10.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .when(!on, |d| {
                                let text = g.text;
                                d.hover(move |s| s.text_color(text))
                            })
                            .tooltip(move |window, cx| {
                                Tooltip::new(match flag {
                                    "pk" => crate::i18n::t("Chave primária"),
                                    "fk" => crate::i18n::t("Chave estrangeira (FK)"),
                                    "not null" => crate::i18n::t("Obrigatório (not null)"),
                                    _ => crate::i18n::t("Valor único"),
                                })
                                .build(window, cx)
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.toggle_flag(&table, &col, flag, !on, window, cx);
                            }))
                            .child(label)
                            .map(|el| {
                                motion::animate(motion_id, f32::from(on), el, move |el, t| {
                                    el.bg(g.sel.opacity(t))
                                        .text_color(motion::color(g.text_faint, g.accent, t))
                                        .shadow(
                                            sel_shadow(g.mode)
                                                .into_iter()
                                                .map(|mut s| {
                                                    s.color = s.color.opacity(t);
                                                    s
                                                })
                                                .collect::<Vec<_>>(),
                                        )
                                })
                            })
                    };
                    let flags_track = div()
                        .flex_1()
                        .flex()
                        .gap(px(2.))
                        .p(px(2.))
                        .rounded(px(7.))
                        .bg(g.input)
                        .child(flag_seg(format!("pk-{}", c.name), "PK", "pk", c.pk, cx))
                        .child(flag_seg(format!("fk-{}", c.name), "FK", "fk", is_fk, cx))
                        .child(flag_seg(
                            format!("nn-{}", c.name),
                            "NN",
                            "not null",
                            c.not_null,
                            cx,
                        ))
                        .child(flag_seg(
                            format!("uq-{}", c.name),
                            "UQ",
                            "unique",
                            c.unique,
                            cx,
                        ));

                    let def_box: AnyElement = match &editing {
                        Some((tg, st)) if *tg == target_def => div()
                            .flex_1()
                            .min_w_0()
                            .child(edit_box(st))
                            .into_any_element(),
                        _ => {
                            let val = c.default_value.clone().unwrap_or_default();
                            let has = !val.is_empty();
                            let display = if has {
                                val.clone()
                            } else {
                                crate::i18n::t("nenhum").to_string()
                            };
                            let t_def = target_def.clone();
                            div()
                                .id(SharedString::from(format!("col-def-{}", c.name)))
                                .role(Role::Button)
                                .aria_label(crate::i18n::t("Editar valor padrão"))
                                .flex_1()
                                .min_w_0()
                                .h(px(26.))
                                .px(px(8.))
                                .rounded(px(6.))
                                .border_1()
                                .border_color(g.edge_soft)
                                .flex()
                                .items_center()
                                .font_family(FONT_MONO)
                                .text_size(px(12.))
                                .text_color(if has { g.text } else { g.text_faint })
                                .cursor_text()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.start_edit(t_def.clone(), val.clone(), window, cx);
                                }))
                                .child(div().truncate().child(display))
                                .into_any_element()
                        }
                    };

                    let chk_box: AnyElement = match &editing {
                        Some((tg, st)) if *tg == target_chk => div()
                            .flex_1()
                            .min_w_0()
                            .child(edit_box(st))
                            .into_any_element(),
                        _ => {
                            let val = c.check.clone().unwrap_or_default();
                            let has = !val.is_empty();
                            let display = if has {
                                val.clone()
                            } else {
                                crate::i18n::t("expressão…").to_string()
                            };
                            let t_chk = target_chk.clone();
                            div()
                                .id(SharedString::from(format!("col-chk-{}", c.name)))
                                .role(Role::Button)
                                .aria_label(crate::i18n::t("Editar expressão check"))
                                .flex_1()
                                .min_w_0()
                                .h(px(26.))
                                .px(px(8.))
                                .rounded(px(6.))
                                .border_1()
                                .border_color(g.edge_soft)
                                .flex()
                                .items_center()
                                .font_family(FONT_MONO)
                                .text_size(px(12.))
                                .text_color(if has { g.text } else { g.text_faint })
                                .cursor_text()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.start_edit(t_chk.clone(), val.clone(), window, cx);
                                }))
                                .child(div().truncate().child(display))
                                .into_any_element()
                        }
                    };

                    let col_note_box: AnyElement = match &editing {
                        Some((tg, st)) if *tg == target_note => div()
                            .flex_1()
                            .min_w_0()
                            .child(edit_box(st))
                            .into_any_element(),
                        _ => {
                            let val = c.note.clone().unwrap_or_default();
                            let has = !val.is_empty();
                            let display = if has {
                                val.clone()
                            } else {
                                crate::i18n::t("adicionar nota…").to_string()
                            };
                            let t_note = target_note.clone();
                            div()
                                .id(SharedString::from(format!("col-note-edit-{}", c.name)))
                                .role(Role::Button)
                                .aria_label(crate::i18n::t("Editar nota da coluna"))
                                .flex_1()
                                .min_w_0()
                                .h(px(26.))
                                .px(px(8.))
                                .rounded(px(6.))
                                .border_1()
                                .border_color(g.edge_soft)
                                .flex()
                                .items_center()
                                .text_size(px(12.))
                                .text_color(if has { g.text } else { g.text_faint })
                                .cursor_text()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.start_edit(t_note.clone(), val.clone(), window, cx);
                                }))
                                .child(div().truncate().child(display))
                                .into_any_element()
                        }
                    };

                    let (table_mv, col_mv) = (qname.clone(), c.name.clone());
                    let mut up = step(
                        SharedString::from(format!("up-{}", c.name)),
                        IconName::ChevronUp,
                        crate::i18n::t("Mover coluna para cima"),
                        ci > 0,
                    );
                    let mut down = step(
                        SharedString::from(format!("down-{}", c.name)),
                        IconName::ChevronDown,
                        crate::i18n::t("Mover coluna para baixo"),
                        ci + 1 < n_cols,
                    );
                    if ci > 0 {
                        let (t2, c2) = (table_mv.clone(), col_mv.clone());
                        up = up.on_click(cx.listener(move |this, _, window, cx| {
                            this.move_column(&t2, &c2, -1, window, cx);
                        }));
                    }
                    if ci + 1 < n_cols {
                        let (t2, c2) = (table_mv.clone(), col_mv.clone());
                        down = down.on_click(cx.listener(move |this, _, window, cx| {
                            this.move_column(&t2, &c2, 1, window, cx);
                        }));
                    }
                    let (t_del, c_del) = (table_mv, col_mv);
                    let del_col_btn = trash(
                        SharedString::from(format!("del-{}", c.name)),
                        crate::i18n::t("Excluir coluna"),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let (t2, c2) = (t_del.clone(), c_del.clone());
                        this.edit_text(false, window, cx, |text| {
                            dbml_edit::delete_column(text, &t2, &c2)
                        });
                    }));

                    let mut open_body = div()
                        .pt(px(4.))
                        .pb(px(10.))
                        .pl(px(30.))
                        .pr(px(10.))
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(row_lbl(crate::i18n::t("Tipo")))
                                .child(ty_box),
                        );

                    if ed_type {
                        let query = if let Some((_, st)) = &editing {
                            st.read(cx).value().trim().to_ascii_lowercase()
                        } else {
                            String::new()
                        };
                        let doc_text = self
                            .doc()
                            .map(|d| d.editor.read(cx).value().to_string())
                            .unwrap_or_default();
                        let enums = extract_enums_from_dbml(&doc_text);
                        let mut suggestions: Vec<(&str, &str)> = Vec::new();
                        for &(ty, desc) in STANDARD_TYPES {
                            if query.is_empty() || ty.to_ascii_lowercase().contains(&query) {
                                suggestions.push((ty, crate::i18n::t(desc)));
                            }
                        }
                        for en in &enums {
                            if query.is_empty() || en.to_ascii_lowercase().contains(&query) {
                                suggestions.push((en.as_str(), crate::i18n::t("enumeração")));
                            }
                        }
                        suggestions.truncate(4);
                        if !suggestions.is_empty() {
                            let mut sugg_box = div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .p(px(4.))
                                .rounded(px(9.))
                                .bg(g.modal.alpha(1.))
                                .border_1()
                                .border_color(g.edge_strong)
                                .shadow_md()
                                .child(
                                    div()
                                        .h(px(22.))
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .px(px(6.))
                                        .text_size(px(10.5))
                                        .text_color(g.text_faint)
                                        .child(crate::i18n::t("Sugestões"))
                                        .child("Esc"),
                                );
                            for (si, (s_type, s_desc)) in suggestions.into_iter().enumerate() {
                                let opt_type = s_type.to_string();
                                let (tbl, cl) = (qname.clone(), c.name.clone());
                                sugg_box = sugg_box.child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "type-opt-{tbl}-{cl}-{opt_type}"
                                        )))
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .h(px(28.))
                                        .px(px(6.))
                                        .rounded(px(6.))
                                        .cursor_pointer()
                                        .when(si == 0, |d| d.bg(g.hover))
                                        .hover(|s| s.bg(g.hover))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |this, _, window, cx| {
                                                this.apply_column_type(
                                                    &tbl, &cl, &opt_type, window, cx,
                                                );
                                            }),
                                        )
                                        .child(
                                            div()
                                                .font_family(FONT_MONO)
                                                .text_size(px(12.))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(g.text)
                                                .child(s_type.to_string()),
                                        )
                                        .child(div().flex_1())
                                        .child(
                                            div()
                                                .truncate()
                                                .text_size(px(11.))
                                                .text_color(g.text_faint)
                                                .child(s_desc.to_string()),
                                        ),
                                );
                            }
                            open_body = open_body.child(
                                div()
                                    .flex()
                                    .gap(px(8.))
                                    .child(div().w(px(48.)).flex_none())
                                    .child(motion::enter(
                                        (ElementId::from(reveal_id.clone()), "suggestions"),
                                        sugg_box.relative(),
                                    )),
                            );
                        }
                    }

                    let _ = (ed_def, ed_chk);
                    open_body = open_body
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(row_lbl("Flags"))
                                .child(flags_track),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(row_lbl(crate::i18n::t("Padrão")))
                                .child(def_box),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(row_lbl("Check"))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .items_center()
                                        .gap(px(2.))
                                        .child(chk_box)
                                        .child(up)
                                        .child(down)
                                        .child(del_col_btn),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(row_lbl(crate::i18n::t("Nota")))
                                .child(col_note_box),
                        );
                    card = card.child(motion::reveal_at(reveal_id, open_progress, open_body, cx));
                }
                cols = cols.child(card);
            }

            let presets_el = div()
                .pt(px(8.))
                .px(px(6.))
                .flex()
                .flex_wrap()
                .gap(px(4.))
                .children(dbml_edit::ColumnPreset::all().into_iter().map(|preset| {
                    div()
                        .id(SharedString::from(format!("insp-preset-{}", preset.id())))
                        .role(Role::Button)
                        .aria_label(preset.label())
                        .h(px(24.))
                        .px(px(8.))
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .border_1()
                        .border_color(g.edge_soft)
                        .text_size(px(11.5))
                        .text_color(g.text_muted)
                        .cursor_pointer()
                        .hover({
                            let (h, t) = (g.hover, g.text);
                            move |s| s.bg(h).text_color(t)
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.apply_column_preset(preset, window, cx);
                        }))
                        .child(
                            Icon::new(IconName::Plus)
                                .size(px(11.))
                                .text_color(g.text_faint),
                        )
                        .child(preset.label())
                }));

            let add_col_table = t_qname.clone();
            let add_col_btn = div()
                .id("insp-add-col")
                .role(Role::Button)
                .aria_label(crate::i18n::t("Adicionar coluna"))
                .p(px(4.))
                .rounded(px(5.))
                .cursor_pointer()
                .text_color(g.text_faint)
                .hover({
                    let (h, t) = (g.hover, g.text);
                    move |s| s.bg(h).text_color(t)
                })
                .tooltip(|window, cx| {
                    Tooltip::new(crate::i18n::t("Adicionar coluna")).build(window, cx)
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.add_column_to(add_col_table.clone(), window, cx);
                }))
                .child(Icon::new(IconName::Plus).size(px(14.)))
                .into_any_element();

            let cols_sec = div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(2.))
                .pt(px(4.))
                .pb(px(10.))
                .px(px(8.))
                .border_t_1()
                .border_color(g.edge_soft)
                .child(div().px(px(6.)).child(sec_header(
                    crate::i18n::t("COLUNAS"),
                    Some(n_cols),
                    Some(add_col_btn),
                )))
                .child(cols)
                .child(presets_el);

            // ── ÍNDICES ────────────────────────────────────────────────
            let n_idx = t.indexes.len();
            let add_idx_table = t_qname.clone();
            let add_idx_col = t.columns.first().map(|c| c.name.clone());
            let add_idx_btn = div()
                .id("insp-add-index")
                .role(Role::Button)
                .aria_label(crate::i18n::t("Adicionar índice"))
                .p(px(4.))
                .rounded(px(5.))
                .cursor_pointer()
                .text_color(g.text_faint)
                .hover({
                    let (h, t) = (g.hover, g.text);
                    move |s| s.bg(h).text_color(t)
                })
                .tooltip(|window, cx| {
                    Tooltip::new(crate::i18n::t("Adicionar índice")).build(window, cx)
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    let Some(first_col) = add_idx_col.clone() else {
                        return;
                    };
                    let tbl = add_idx_table.clone();
                    let new_ix = schema::IndexDef {
                        columns: vec![schema::IndexColumn::Column(first_col)],
                        unique: false,
                        pk: false,
                        name: None,
                        index_type: None,
                    };
                    this.edit_text(false, window, cx, |text| {
                        dbml_edit::add_index(text, &tbl, &new_ix)
                    });
                }))
                .child(Icon::new(IconName::Plus).size(px(14.)))
                .into_any_element();

            let mut indexes_el = div().flex().flex_col().gap(px(6.));
            if t.indexes.is_empty() {
                indexes_el = indexes_el.child(
                    div()
                        .text_size(px(11.5))
                        .line_height(px(17.))
                        .text_color(g.text_faint)
                        .child(crate::i18n::t(
                            "Sem índices · a chave primária é indexada automaticamente",
                        )),
                );
            } else {
                for (idx, ix) in t.indexes.iter().enumerate() {
                    let idx_target = EditTarget::IndexName(t_qname.clone(), idx);
                    let default_name = ix.name.clone().unwrap_or_else(|| {
                        let cols_str = ix
                            .columns
                            .iter()
                            .filter_map(schema::IndexColumn::column_name)
                            .collect::<Vec<_>>()
                            .join("_");
                        if cols_str.is_empty() {
                            format!("idx_{}_{idx}", t.name)
                        } else {
                            format!("idx_{}_{cols_str}", t.name)
                        }
                    });
                    let name_el = match &editing {
                        Some((tg, st)) if *tg == idx_target => div()
                            .flex_1()
                            .min_w_0()
                            .child(edit_box(st))
                            .into_any_element(),
                        _ => {
                            let val = ix.name.clone().unwrap_or_default();
                            div()
                                .id(SharedString::from(format!("idx-name-{idx}")))
                                .role(Role::Button)
                                .aria_label(crate::i18n::t("Nome do índice"))
                                .flex_1()
                                .min_w_0()
                                .h(px(26.))
                                .px(px(8.))
                                .rounded(px(6.))
                                .bg(g.input)
                                .border_1()
                                .border_color(g.edge_soft)
                                .flex()
                                .items_center()
                                .font_family(FONT_MONO)
                                .text_size(px(11.5))
                                .text_color(g.text)
                                .cursor_text()
                                .child(div().truncate().child(default_name))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.start_edit(idx_target.clone(), val.clone(), window, cx);
                                }))
                                .into_any_element()
                        }
                    };
                    let tbl_del = t_qname.clone();
                    let top_row = div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .child(name_el)
                        .child(
                            trash(
                                SharedString::from(format!("del-idx-{idx}")),
                                crate::i18n::t("Excluir índice"),
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    let tbl = tbl_del.clone();
                                    this.edit_text(false, window, cx, |text| {
                                        dbml_edit::delete_index(text, &tbl, idx)
                                    });
                                },
                            )),
                        );

                    let chip_row_lbl = |text: &'static str| {
                        div()
                            .w(px(40.))
                            .flex_none()
                            .text_size(px(11.))
                            .text_color(g.text_muted)
                            .child(crate::i18n::t(text))
                    };

                    let mut cols_chips = div().flex_1().min_w_0().flex().flex_wrap().gap(px(4.));
                    for col_def in &t.columns {
                        let active = ix.references_column(&col_def.name);
                        let (tbl_col, col_name, ix_col) =
                            (t_qname.clone(), col_def.name.clone(), ix.clone());
                        cols_chips = cols_chips.child(
                            chip_btn(
                                SharedString::from(format!("idx-col-{idx}-{}", col_def.name)),
                                col_def.name.clone().into(),
                                active,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    let mut next = ix_col.clone();
                                    if let Some(pos) = next
                                        .columns
                                        .iter()
                                        .position(|ic| ic.column_name() == Some(col_name.as_str()))
                                    {
                                        if next.columns.len() > 1 {
                                            next.columns.remove(pos);
                                        }
                                    } else {
                                        next.columns
                                            .push(schema::IndexColumn::Column(col_name.clone()));
                                    }
                                    let tbl = tbl_col.clone();
                                    this.edit_text(false, window, cx, |text| {
                                        dbml_edit::update_index(text, &tbl, idx, &next)
                                    });
                                },
                            )),
                        );
                    }
                    let cols_row = div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(chip_row_lbl("Cols"))
                        .child(cols_chips);

                    let mut type_chips = div().flex_1().min_w_0().flex().flex_wrap().gap(px(4.));
                    let type_choices: [(Option<schema::IndexType>, &'static str); 5] = [
                        (None, "default"),
                        (Some(schema::IndexType::BTree), "btree"),
                        (Some(schema::IndexType::Hash), "hash"),
                        (Some(schema::IndexType::Gin), "gin"),
                        (Some(schema::IndexType::Gist), "gist"),
                    ];
                    for (opt_ty, name) in type_choices {
                        let active = ix.index_type == opt_ty;
                        let (tbl_ty, ix_ty) = (t_qname.clone(), ix.clone());
                        type_chips = type_chips.child(
                            chip_btn(
                                SharedString::from(format!("idx-ty-{idx}-{name}")),
                                name.into(),
                                active,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    let mut next = ix_ty.clone();
                                    next.index_type = opt_ty;
                                    let tbl = tbl_ty.clone();
                                    this.edit_text(false, window, cx, |text| {
                                        dbml_edit::update_index(text, &tbl, idx, &next)
                                    });
                                },
                            )),
                        );
                    }
                    let type_row = div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(chip_row_lbl(crate::i18n::t("Tipo")))
                        .child(type_chips);

                    let (tbl_uq, ix_uq) = (t_qname.clone(), ix.clone());
                    let uq_chip = chip_btn(
                        SharedString::from(format!("idx-uq-{idx}")),
                        crate::i18n::t("único").into(),
                        ix.unique,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let mut next = ix_uq.clone();
                        next.unique = !next.unique;
                        let tbl = tbl_uq.clone();
                        this.edit_text(false, window, cx, |text| {
                            dbml_edit::update_index(text, &tbl, idx, &next)
                        });
                    }));
                    let (tbl_pk, ix_pk) = (t_qname.clone(), ix.clone());
                    let pk_chip = chip_btn(
                        SharedString::from(format!("idx-pk-{idx}")),
                        crate::i18n::t("primário").into(),
                        ix.pk,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let mut next = ix_pk.clone();
                        next.pk = !next.pk;
                        let tbl = tbl_pk.clone();
                        this.edit_text(false, window, cx, |text| {
                            dbml_edit::update_index(text, &tbl, idx, &next)
                        });
                    }));
                    let mode_row = div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(chip_row_lbl(crate::i18n::t("Modo")))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_wrap()
                                .gap(px(4.))
                                .child(uq_chip)
                                .child(pk_chip),
                        );

                    indexes_el = indexes_el.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .p(px(10.))
                            .rounded(px(8.))
                            .border_1()
                            .border_color(g.edge_soft)
                            .bg(g.hover)
                            .child(top_row)
                            .child(cols_row)
                            .child(type_row)
                            .child(mode_row),
                    );
                }
            }
            let indexes_sec = div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(8.))
                .pt(px(4.))
                .pb(px(14.))
                .px(px(14.))
                .border_t_1()
                .border_color(g.edge_soft)
                .child(sec_header(
                    crate::i18n::t("ÍNDICES"),
                    Some(n_idx),
                    Some(add_idx_btn),
                ))
                .child(indexes_el);

            // ── RELAÇÕES ───────────────────────────────────────────────
            let rels_el = if refs.is_empty() {
                div()
                    .text_size(px(11.5))
                    .line_height(px(17.))
                    .text_color(g.text_faint)
                    .child(crate::i18n::t(
                        "Sem relações · arraste de uma coluna para outra tabela",
                    ))
            } else {
                let mut rels = div().flex().flex_col().gap(px(4.));
                for (ri, r) in refs.iter().enumerate() {
                    let is_out = r.from.qualified_table() == t_qname;
                    let card = match r.kind {
                        schema::RefKind::ManyToOne => "n:1",
                        schema::RefKind::OneToMany => "1:n",
                        schema::RefKind::OneToOne => "1:1",
                        schema::RefKind::ManyToMany => "n:n",
                    };
                    let (ft, fc) = (r.from.qualified_table(), r.from.column.clone());
                    let (tt, tc) = (r.to.qualified_table(), r.to.column.clone());
                    let left_txt = if is_out {
                        format!(".{}", r.from.column)
                    } else {
                        format!(".{}", r.to.column)
                    };
                    let right_txt = if is_out {
                        format!("{tt}.{}", r.to.column)
                    } else {
                        format!("{ft}.{}", r.from.column)
                    };
                    rels = rels.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .h(px(30.))
                            .px(px(8.))
                            .rounded(px(6.))
                            .border_1()
                            .border_color(g.edge_soft)
                            .font_family(FONT_MONO)
                            .text_size(px(11.5))
                            .child(
                                Icon::new(if is_out {
                                    IconName::ArrowUpRight
                                } else {
                                    IconName::ArrowDownLeft
                                })
                                .size(px(12.))
                                .text_color(g.accent),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.))
                                    .overflow_hidden()
                                    .child(div().text_color(g.text).child(left_txt))
                                    .child(
                                        div()
                                            .text_size(px(10.5))
                                            .text_color(g.text_faint)
                                            .child(card),
                                    )
                                    .child(
                                        div().truncate().text_color(g.text_muted).child(right_txt),
                                    ),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("del-rel-{ri}")))
                                    .role(Role::Button)
                                    .aria_label(crate::i18n::t("Excluir relação e coluna FK"))
                                    .p(px(3.))
                                    .rounded(px(4.))
                                    .cursor_pointer()
                                    .text_color(g.text_faint)
                                    .hover({
                                        let d = g.danger;
                                        move |s| s.text_color(d)
                                    })
                                    .tooltip(|window, cx| {
                                        Tooltip::new(crate::i18n::t("Excluir relação e coluna FK"))
                                            .build(window, cx)
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        let (f, t) =
                                            ((ft.clone(), fc.clone()), (tt.clone(), tc.clone()));
                                        this.edit_text(false, window, cx, |text| {
                                            Some(dbml_edit::delete_ref(
                                                text,
                                                (&f.0, &f.1),
                                                (&t.0, &t.1),
                                            ))
                                        });
                                    }))
                                    .text_size(px(10.5))
                                    .child(crate::i18n::t("Excluir FK")),
                            ),
                    );
                }
                rels
            };
            let rels_sec = div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(8.))
                .pt(px(4.))
                .pb(px(14.))
                .px(px(14.))
                .border_t_1()
                .border_color(g.edge_soft)
                .child(sec_header(
                    crate::i18n::t("RELAÇÕES"),
                    Some(refs.len()),
                    None,
                ))
                .child(rels_el);

            (
                div()
                    .flex()
                    .flex_col()
                    .child(ident)
                    .child(general_sec)
                    .child(cols_sec)
                    .child(indexes_sec)
                    .child(rels_sec),
                Some(footer_danger(
                    "insp-del-table",
                    crate::i18n::t("Excluir tabela").to_string(),
                    crate::i18n::t("Excluir tabela"),
                    true,
                )),
            )
        } else if let Some(r) = selected_ref {
            let block = |label: &'static str, count: Option<usize>| {
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .pt(px(4.))
                    .pb(px(14.))
                    .px(px(14.))
                    .border_t_1()
                    .border_color(g.edge_soft)
                    .child(sec_header(label, count, None))
            };
            let mut card_row = div().flex().flex_wrap().gap(px(6.));
            for &kind in schema::RefKind::ALL {
                let active = r.kind == kind;
                let label = match kind {
                    schema::RefKind::ManyToOne => "N:1 (>)",
                    schema::RefKind::OneToMany => "1:N (<)",
                    schema::RefKind::OneToOne => "1:1 (-)",
                    schema::RefKind::ManyToMany => "N:N (<>)",
                };
                let rf = r.clone();
                let btn = Button::new(SharedString::from(format!("ref-kind-{label}")))
                    .xsmall()
                    .label(label);
                let btn = if active { btn.primary() } else { btn.ghost() };
                card_row = card_row.child(btn.on_click(cx.listener(move |this, _, window, cx| {
                    let ft = rf.from.qualified_table();
                    let fc = rf.from.column.clone();
                    let tt = rf.to.qualified_table();
                    let tc = rf.to.column.clone();
                    let update = dbml_edit::RefUpdate {
                        kind,
                        on_delete: rf.on_delete,
                        on_update: rf.on_update,
                    };
                    this.edit_text(false, window, cx, |t| {
                        Some(dbml_edit::update_ref(t, (&ft, &fc), (&tt, &tc), update))
                    });
                })));
            }

            let build_action_row =
                |is_delete: bool, current: Option<schema::RefAction>, cx: &mut Context<Self>| {
                    let mut row = div().flex().flex_wrap().gap(px(6.));
                    let choices: [Option<schema::RefAction>; 6] = [
                        None,
                        Some(schema::RefAction::NoAction),
                        Some(schema::RefAction::Restrict),
                        Some(schema::RefAction::Cascade),
                        Some(schema::RefAction::SetNull),
                        Some(schema::RefAction::SetDefault),
                    ];
                    for action in choices {
                        let active = current == action;
                        let label = action.map_or("None", schema::RefAction::as_dbml_str);
                        let id = SharedString::from(format!(
                            "ref-{}-{label}",
                            if is_delete { "del" } else { "upd" }
                        ));
                        let rf = r.clone();
                        let btn = Button::new(id).xsmall().label(label);
                        let btn = if active { btn.primary() } else { btn.ghost() };
                        row = row.child(btn.on_click(cx.listener(move |this, _, window, cx| {
                            let ft = rf.from.qualified_table();
                            let fc = rf.from.column.clone();
                            let tt = rf.to.qualified_table();
                            let tc = rf.to.column.clone();
                            let update = dbml_edit::RefUpdate {
                                kind: rf.kind,
                                on_delete: if is_delete { action } else { rf.on_delete },
                                on_update: if is_delete { rf.on_update } else { action },
                            };
                            this.edit_text(false, window, cx, |t| {
                                Some(dbml_edit::update_ref(t, (&ft, &fc), (&tt, &tc), update))
                            });
                        })));
                    }
                    row
                };

            let del_row = build_action_row(true, r.on_delete, cx);
            let upd_row = build_action_row(false, r.on_update, cx);

            (
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .px(px(14.))
                            .py(px(12.))
                            .border_t_1()
                            .border_color(g.edge_soft)
                            .child(
                                div()
                                    .size(px(30.))
                                    .rounded(px(8.))
                                    .bg(g.accent.opacity(0.16))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        Icon::new(IconName::Link2)
                                            .size(px(15.))
                                            .text_color(g.accent),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .font_family(FONT_MONO)
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(g.text)
                                            .truncate()
                                            .child(format!(
                                                "{}.{}",
                                                r.from.qualified_table(),
                                                r.from.column
                                            )),
                                    )
                                    .child(
                                        div()
                                            .font_family(FONT_MONO)
                                            .text_size(px(11.5))
                                            .text_color(g.text_faint)
                                            .truncate()
                                            .child(format!(
                                                "→ {}.{}",
                                                r.to.qualified_table(),
                                                r.to.column
                                            )),
                                    ),
                            ),
                    )
                    .child(block(crate::i18n::t("CARDINALIDADE"), None).child(card_row))
                    .child(block(crate::i18n::t("AO EXCLUIR"), None).child(del_row))
                    .child(block(crate::i18n::t("AO ATUALIZAR"), None).child(upd_row)),
                Some(footer_danger(
                    "insp-del-ref",
                    crate::i18n::t("Excluir relação e coluna FK").to_string(),
                    crate::i18n::t("Excluir relação e coluna FK"),
                    false,
                )),
            )
        } else if let Some((en, refs_by)) = selected_enum {
            let block = |label: &'static str, count: Option<usize>| {
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .pt(px(4.))
                    .pb(px(14.))
                    .px(px(14.))
                    .border_t_1()
                    .border_color(g.edge_soft)
                    .child(sec_header(label, count, None))
            };
            let editing = self
                .editing
                .as_ref()
                .map(|(t, st, _)| (t.clone(), st.clone()));
            let enum_field =
                |target: EditTarget, text: String, cx: &mut Context<Self>| -> AnyElement {
                    match &editing {
                        Some((t, st)) if *t == target => edit_box(st),
                        _ => {
                            let id = SharedString::from(format!("enum-edit-{target:?}"));
                            let value = text.clone();
                            div()
                                .id(id)
                                .role(Role::Button)
                                .aria_label(crate::i18n::t("Editar {}").replace("{}", &text))
                                .w_full()
                                .px(px(4.))
                                .rounded(px(4.))
                                .truncate()
                                .text_color(g.text)
                                .cursor_text()
                                .hover({
                                    let h = g.hover;
                                    move |st| st.bg(h)
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.start_edit(target.clone(), value.clone(), window, cx);
                                }))
                                .child(text)
                                .into_any_element()
                        }
                    }
                };
            let title_el = enum_field(EditTarget::Enum(en.name.clone()), en.name.clone(), cx);
            let mut values_list = div().flex().flex_col().gap(px(2.));
            for (idx, val) in en.values.iter().enumerate() {
                let en_up = en.name.clone();
                let v_up = val.name.clone();
                let en_dn = en.name.clone();
                let v_dn = val.name.clone();
                let en_del = en.name.clone();
                let v_del = val.name.clone();
                let val_el = enum_field(
                    EditTarget::EnumVal(en.name.clone(), val.name.clone()),
                    val.name.clone(),
                    cx,
                );
                values_list = values_list.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .h(px(26.))
                        .child(div().flex_1().min_w_0().child(val_el))
                        .when(idx > 0, |d| {
                            d.child(
                                Button::new(SharedString::from(format!("enum-up-{idx}")))
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::ArrowUp)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.edit_text(false, window, cx, |t| {
                                            dbml_edit::reorder_enum_value(t, &en_up, &v_up, true)
                                        });
                                    })),
                            )
                        })
                        .when(idx + 1 < en.values.len(), |d| {
                            d.child(
                                Button::new(SharedString::from(format!("enum-dn-{idx}")))
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::ArrowDown)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.edit_text(false, window, cx, |t| {
                                            dbml_edit::reorder_enum_value(t, &en_dn, &v_dn, false)
                                        });
                                    })),
                            )
                        })
                        .child(
                            trash(
                                SharedString::from(format!("enum-del-{idx}")),
                                crate::i18n::t("Excluir valor"),
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.edit_text(false, window, cx, |t| {
                                        dbml_edit::delete_enum_value(t, &en_del, &v_del)
                                    });
                                },
                            )),
                        ),
                );
            }
            if let Some((EditTarget::NewEnumVal(target_en), st)) = &editing
                && target_en == &en.name
            {
                values_list = values_list.child(div().py(px(2.)).child(edit_box(st)));
            }
            let en_add = en.name.clone();
            values_list = values_list.child(
                div().pt(px(4.)).child(
                    Button::new("insp-add-enum-val")
                        .ghost()
                        .xsmall()
                        .icon(IconName::Plus)
                        .label(crate::i18n::t("Adicionar valor"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.start_edit(
                                EditTarget::NewEnumVal(en_add.clone()),
                                String::new(),
                                window,
                                cx,
                            );
                        })),
                ),
            );
            let mut refs_list = div().flex().flex_col().gap(px(2.));
            if refs_by.is_empty() {
                refs_list = refs_list.child(
                    div()
                        .px(px(6.))
                        .text_size(TEXT_XS)
                        .text_color(g.text_faint)
                        .child(crate::i18n::t("Nenhuma coluna referencia este enum")),
                );
            } else {
                for (i, (tbl, col)) in refs_by.iter().enumerate() {
                    let tbl_click = tbl.clone();
                    refs_list = refs_list.child(
                        Self::row(g, SharedString::from(format!("enum-ref-{i}")), false)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.with_diagram(cx, |d| d.focus_table(&tbl_click));
                            }))
                            .child(
                                div()
                                    .truncate()
                                    .font_family(FONT_MONO)
                                    .text_size(px(12.))
                                    .text_color(g.text_muted)
                                    .child(format!("{tbl}.{col}")),
                            ),
                    );
                }
            }
            let n_vals = en.values.len();
            let sub = if n_vals == 1 {
                crate::i18n::t("Enum · 1 valor").to_string()
            } else {
                crate::i18n::t("Enum · {} valores").replace("{}", &n_vals.to_string())
            };
            (
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .px(px(14.))
                            .py(px(12.))
                            .border_t_1()
                            .border_color(g.edge_soft)
                            .child(
                                div()
                                    .size(px(30.))
                                    .rounded(px(8.))
                                    .bg(g.link.opacity(0.16))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        Icon::new(IconName::Tag).size(px(15.)).text_color(g.link),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(title_el)
                                    .child(
                                        div()
                                            .text_size(px(11.5))
                                            .text_color(g.text_faint)
                                            .child(sub),
                                    ),
                            ),
                    )
                    .child(block(crate::i18n::t("VALORES"), Some(n_vals)).child(values_list))
                    .child(
                        block(crate::i18n::t("REFERENCIADO POR"), Some(refs_by.len()))
                            .child(refs_list),
                    ),
                Some(footer_danger(
                    "insp-del-enum",
                    crate::i18n::t("Excluir enum").to_string(),
                    crate::i18n::t("Excluir enum"),
                    false,
                )),
            )
        } else {
            let tip = |action: &dyn Action, ctx: &str, text: &'static str| {
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .h(px(26.))
                    .text_size(TEXT_SM)
                    .text_color(g.text_muted)
                    .child(text)
                    .children(Kbd::binding_for_action(action, Some(ctx), window))
            };
            (
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .px(px(14.))
                    .py(SP_LG)
                    .child(
                        div()
                            .text_size(TEXT_BASE)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(g.text)
                            .child(crate::i18n::t("Nada selecionado")),
                    )
                    .child(
                        div()
                            .text_size(TEXT_SM)
                            .text_color(g.text_muted)
                            .child(crate::i18n::t("Clique em uma tabela para ver colunas, cor, grupo e relações. Shift+clique seleciona várias.")),
                    )
                    .child(div().h(px(1.)).my(SP_SM).bg(g.edge_soft))
                    .child(tip(&ToolSelect, actions::CANVAS, crate::i18n::t("Selecionar")))
                    .child(tip(&ToolHand, actions::CANVAS, crate::i18n::t("Mover a vista")))
                    .child(tip(&FitView, actions::CANVAS, crate::i18n::t("Ajustar à tela")))
                    .child(tip(&GroupSelection, actions::CTX, crate::i18n::t("Agrupar seleção")))
                    .child(tip(&ToggleEditor, actions::CTX, "Editor DBML"))
                    .child(tip(&OpenPalette, actions::CTX, crate::i18n::t("Todos os comandos"))),
                None,
            )
        };
        div()
            .id("inspector")
            .role(Role::Region)
            .aria_label(crate::i18n::t("Inspetor"))
            .size_full()
            .flex()
            .flex_col()
            .bg(g.sidebar)
            .border_l_1()
            .border_color(g.edge_soft)
            .text_size(TEXT_SM)
            .child(
                div()
                    .h(PANEL_HEADER_H)
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .pl(px(14.))
                    .pr(px(8.))
                    .border_b_1()
                    .border_color(g.edge_soft)
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(crate::i18n::t("Propriedades"))
                    .child(div().flex_1())
                    .when(is_single_table, |d| {
                        d.child(
                            Button::new("inspector-more")
                                .ghost()
                                .small()
                                .w(px(24.))
                                .h(px(24.))
                                .rounded(px(5.))
                                .icon(
                                    Icon::new(IconName::Ellipsis)
                                        .size(px(14.))
                                        .text_color(g.text_faint),
                                )
                                .tooltip(crate::i18n::t("Mais opções"))
                                .map(|button| {
                                    motion::dropdown(
                                        "inspector-more-motion",
                                        button,
                                        Anchor::TopRight,
                                        self.focus.clone(),
                                        |menu, _, _| {
                                            menu.menu_with_icon(
                                                crate::i18n::t("Duplicar tabela"),
                                                Icon::new(IconName::Copy),
                                                Box::new(DuplicateTable),
                                            )
                                            .menu_with_icon(
                                                crate::i18n::t("Renomear"),
                                                Icon::new(IconName::SquarePen),
                                                Box::new(RenameSelected),
                                            )
                                            .menu_with_icon(
                                                crate::i18n::t("Revelar no DBML"),
                                                Icon::new(IconName::Code),
                                                Box::new(RevealInDbml),
                                            )
                                            .separator()
                                            .menu_with_icon(
                                                crate::i18n::t("Excluir tabela"),
                                                Icon::new(IconName::Trash),
                                                Box::new(DeleteSelection),
                                            )
                                        },
                                    )
                                }),
                        )
                    })
                    .child(Self::pane_icon_btn(
                        g,
                        "inspector-close",
                        IconName::ChevronsRight,
                        crate::i18n::t("Fechar propriedades"),
                        Box::new(ToggleInspector),
                    )),
            )
            .child(
                div()
                    .id("inspector-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .child(body)
                    .child(div().flex_1().border_t_1().border_color(g.edge_soft)),
            )
            .children(footer)
    }

    pub(super) fn editor_panel(&self, g: &Glass, cx: &App) -> Div {
        let Some(doc) = self.doc() else { return div() };
        let selected_table = doc
            .diagram
            .read(cx)
            .selected()
            .map(schema::Table::qualified_name)
            .filter(|t| crate::ide::table_block(&doc.last_text, t).is_some());
        let errors = doc.counts().0;
        let (state_icon, state_color, state) = if errors > 0 {
            (
                IconName::CircleX,
                g.danger,
                if errors == 1 {
                    crate::i18n::t("1 erro").to_string()
                } else {
                    crate::i18n::t("{} erros").replace("{}", &errors.to_string())
                },
            )
        } else {
            (
                IconName::RefreshCw,
                g.ok,
                crate::i18n::t("sincronizado").to_string(),
            )
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(g.editor)
            .border_r_1()
            .border_color(g.edge_soft)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .h(PANEL_HEADER_H)
                    .flex_none()
                    .pl(px(12.))
                    .pr(px(8.))
                    .border_b_1()
                    .border_color(g.edge_soft)
                    .font_family(FONT_MONO)
                    .text_size(px(12.))
                    .text_color(g.text_faint)
                    .child(
                        Icon::new(IconName::FileCode)
                            .size(px(14.))
                            .text_color(g.accent),
                    )
                    .child(div().min_w_0().truncate().child(doc.title.clone()))
                    .children(selected_table.map(|name| {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(Icon::new(IconName::ChevronRight).size(px(12.)))
                            .child(
                                div()
                                    .text_color(g.text)
                                    .child(crate::i18n::t("Tabela {}").replace("{}", &name)),
                            )
                    }))
                    .child(div().flex_1())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .font_family(FONT)
                            .text_size(px(12.))
                            .text_color(state_color)
                            .child(Icon::new(state_icon).size(px(12.)))
                            .child(state),
                    )
                    .child(Self::pane_icon_btn(
                        g,
                        "editor-format",
                        IconName::WandSparkles,
                        crate::i18n::t("Formatar documento"),
                        Box::new(FormatDocument),
                    ))
                    .child(Self::pane_icon_btn(
                        g,
                        "editor-close",
                        IconName::X,
                        crate::i18n::t("Fechar editor"),
                        Box::new(ToggleEditor),
                    )),
            )
            .child(
                div().flex_1().min_h_0().py(px(10.)).child(
                    Editor::new(&doc.editor)
                        .appearance(false)
                        .bordered(false)
                        .text_size(px(12.5))
                        .line_height(px(22.))
                        .h_full(),
                ),
            )
    }

    /// Tela inicial (sem arquivo aberto): hero `dbmodeling` + colunas `INICIAR` e `RECENTES`.
    pub(super) fn empty_state(&self, g: &Glass, cx: &mut Context<Self>) -> Div {
        let item = |id: &'static str,
                    icon: IconName,
                    label: &'static str,
                    keys: &'static str,
                    primary: bool,
                    action: Box<dyn Action>| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(label)
                .px(px(10.))
                .py(px(9.))
                .rounded(px(8.))
                .flex()
                .items_center()
                .gap(px(10.))
                .text_size(px(13.5))
                .cursor_pointer()
                .when(primary, |d| d.bg(g.hover))
                .when(!primary, |d| {
                    let h = g.hover;
                    d.hover(move |s| s.bg(h))
                })
                .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
                .child(Icon::new(icon).size(px(16.)).text_color(g.accent))
                .child(label)
                .child(div().flex_1())
                .when(!keys.is_empty(), |d| {
                    d.child(
                        div()
                            .font_family(FONT_MONO)
                            .text_size(px(11.))
                            .text_color(g.text_faint)
                            .child(keys),
                    )
                })
        };
        let start = div()
            .w(px(260.))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(Self::empty_label(g, crate::i18n::t("INICIAR")).px(px(10.)))
            .child(item(
                "empty-folder",
                IconName::FolderOpen,
                crate::i18n::t("Abrir pasta"),
                "Ctrl Shift O",
                true,
                Box::new(OpenFolder),
            ))
            .child(item(
                "empty-new",
                IconName::FilePlus,
                crate::i18n::t("Novo arquivo"),
                "Ctrl N",
                false,
                Box::new(NewFile),
            ))
            .child(item(
                "empty-open",
                IconName::FileCode,
                crate::i18n::t("Abrir arquivo .dbml"),
                "Ctrl O",
                false,
                Box::new(OpenFile),
            ))
            .child(item(
                "empty-import",
                IconName::FileInput,
                crate::i18n::t("Importar SQL DDL"),
                "",
                false,
                Box::new(ImportSqlDdl),
            ))
            .child(item(
                "empty-connect",
                IconName::Database,
                crate::i18n::t("Conectar ao banco"),
                "",
                false,
                Box::new(ConnectDatabase),
            ));
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from);
        let now = crate::session::now_secs();
        let mut recent = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(Self::empty_label(g, crate::i18n::t("RECENTES")).px(px(12.)));
        let recent_hues = [0usize, 2, 5, 7, 4];
        for (i, r) in self.recent.iter().take(5).enumerate() {
            let name = file_title(&r.path);
            let dir = r
                .path
                .parent()
                .map(|d| crate::session::short_dir(d, home.as_deref()))
                .unwrap_or_default();
            let path = r.path.clone();
            let icon_color = TABLE_COLORS
                .get(recent_hues[i % recent_hues.len()])
                .and_then(|(_, h)| hex(h))
                .unwrap_or(g.accent);
            recent = recent.child(
                div()
                    .id(("recent", i))
                    .role(Role::Button)
                    .aria_label(crate::i18n::t("Abrir {}").replace("{}", &name))
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .px(px(12.))
                    .py(px(10.))
                    .rounded(px(9.))
                    .border_b_1()
                    .border_color(g.edge_soft)
                    .text_size(px(13.5))
                    .text_color(g.text)
                    .cursor_pointer()
                    .hover({
                        let h = g.hover;
                        move |s| s.bg(h)
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_path(path.clone(), window, cx);
                    }))
                    .child(
                        Icon::new(IconName::FileCode)
                            .size(px(15.))
                            .flex_none()
                            .text_color(icon_color),
                    )
                    .child(div().w(px(160.)).flex_none().truncate().child(name))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_family(FONT_MONO)
                            .text_size(px(12.))
                            .text_color(g.text_faint)
                            .child(dir),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(12.))
                            .text_color(g.text_muted)
                            .child(if r.tables == 1 {
                                crate::i18n::t("1 tabela").to_string()
                            } else {
                                crate::i18n::t("{} tabelas").replace("{}", &r.tables.to_string())
                            }),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(70.))
                            .flex()
                            .justify_end()
                            .text_size(px(12.))
                            .text_color(g.text_faint)
                            .child(crate::session::ago(r.at, now)),
                    ),
            );
        }
        let hero_shadow = BoxShadow {
            color: g.accent.opacity(0.35),
            offset: point(px(0.), px(12.)),
            blur_radius: px(30.),
            spread_radius: px(0.),
            inset: false,
        };
        div()
            .size_full()
            .bg(g.surface)
            .flex()
            .flex_col()
            .gap(px(40.))
            .px(px(80.))
            .py(px(72.))
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(18.))
                    .child(
                        div()
                            .size(px(60.))
                            .rounded(px(17.))
                            .shadow(vec![hero_shadow])
                            .child(img("brand/icon-64.png").size(px(60.)).rounded(px(17.))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(30.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("dbmodeling"),
                            )
                            .child(
                                div().text_size(px(14.)).text_color(g.text_muted).child(
                                    crate::i18n::t("Nenhum arquivo aberto · abra uma pasta de projeto ou inicie um novo esquema"),
                                ),
                            ),
                    ),
            )
            .child(div().flex().gap(px(56.)).min_w_0().child(start).child(recent))
    }

    fn empty_label(g: &Glass, text: &'static str) -> Div {
        div()
            .pb(px(8.))
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(g.text_faint)
            .child(text)
    }

    pub(super) fn statusbar(&self, g: &Glass, compact: bool, cx: &App) -> Div {
        let mut bar = div()
            .h(STATUSBAR_H)
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(16.))
            .px(SP_MD)
            .font_family(FONT_MONO)
            .bg(g.rail)
            .border_t_1()
            .border_color(g.edge_soft)
            .text_size(px(11.5))
            .text_color(g.text_faint);
        if let Some(doc) = self.doc() {
            let d = doc.diagram.read(cx);
            let s = d.schema();
            let problems = doc.diags.len();
            bar = bar
                .child(
                    div()
                        .id("status-problems")
                        .role(Role::Button)
                        .aria_label(crate::i18n::t("Problemas"))
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .cursor_pointer()
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(ShowProblems), cx);
                        })
                        .child(
                            Icon::new(if problems == 0 {
                                IconName::CircleCheck
                            } else {
                                IconName::CircleX
                            })
                            .size(px(12.))
                            .text_color(if problems == 0 {
                                g.ok
                            } else {
                                g.danger
                            }),
                        )
                        .child(format!(
                            "{} {}",
                            problems,
                            crate::i18n::t(if problems == 1 {
                                "problema"
                            } else {
                                "problemas"
                            })
                        )),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .child(Icon::new(IconName::Table2).size(px(12.)))
                        .child(
                            crate::i18n::t("{} tabelas · {} relações")
                                .replacen("{}", &s.tables.len().to_string(), 1)
                                .replacen("{}", &s.refs.len().to_string(), 1),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .justify_center()
                        .font_family(FONT)
                        .truncate()
                        .when(!compact, |e| e.child(d.hint())),
                )
                .child(
                    div()
                        .font_family(FONT_MONO)
                        .child(format!("{}%", (d.zoom() * 100.).round() as i32)),
                );
        } else {
            bar = bar.child(div().flex_1());
        }
        if self.loading > 0 {
            bar = bar.child(
                div()
                    .id("status-loading")
                    .role(Role::Status)
                    .aria_label(crate::i18n::t("Abrindo…"))
                    .text_color(g.text_muted)
                    .child(crate::i18n::t("Abrindo…")),
            );
        }
        if let Some(branch) = &self.git_branch.1 {
            bar = bar.child(
                div()
                    .id("status-branch")
                    .role(Role::Status)
                    .aria_label(crate::i18n::t("Branch Git {}").replace("{}", branch))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .child(Icon::new(IconName::GitBranch).size(px(12.)))
                    .child(branch.clone()),
            );
        }
        let theme_icon = if self.mode == Mode::Dark {
            IconName::Moon
        } else {
            IconName::Sun
        };
        let theme_name = crate::i18n::t(g.theme_name());
        bar.child("DBML")
            .child(
                div()
                    .id("status-version")
                    .role(Role::Button)
                    .aria_label(crate::i18n::t("Ver novidades desta versão"))
                    .px(px(5.))
                    .rounded(px(4.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(g.hover).text_color(g.text))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(ShowWhatsNew), cx))
                    .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
            )
            .child(
                div()
                    .id("status-theme")
                    .role(Role::Button)
                    .aria_label(crate::i18n::t("Alternar tema"))
                    .text_color(g.text_muted)
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .cursor_pointer()
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleTheme), cx))
                    .child(Icon::new(theme_icon).size(px(12.)))
                    .child(theme_name),
            )
    }
}

/// Menu principal: um submenu por grupo do catálogo (280px) e, no fim, paleta e atalhos.
fn main_menu_items(menu: PopupMenu, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let groups = ["Arquivo", "Editar", "Exibir", "Canvas", "Tabelas"];
    let mut menu = menu.min_w(px(220.));
    for group in groups {
        menu = menu.submenu_with_icon(
            Some(Icon::new(group_icon(group))),
            crate::i18n::t(group),
            window,
            cx,
            move |m, _, _| {
                actions::catalog()
                    .into_iter()
                    .filter(|e| e.group == group)
                    .fold(m.min_w(px(280.)), |m, e| {
                        let ic = action_icon(e.action.as_ref(), group);
                        m.menu_with_icon(e.label, Icon::new(ic), e.action)
                    })
            },
        );
    }
    menu.separator()
        .menu_with_icon(
            crate::i18n::t("Paleta de comandos"),
            Icon::new(IconName::Command),
            Box::new(OpenPalette),
        )
        .menu_with_icon(
            crate::i18n::t("Atalhos de teclado"),
            Icon::new(IconName::Keyboard),
            Box::new(ShowShortcuts),
        )
        .menu_with_icon(
            crate::i18n::t("Tour guiado"),
            Icon::new(IconName::Info),
            Box::new(ShowTour),
        )
        .menu_with_icon(
            crate::i18n::t("O que há de novo"),
            Icon::new(IconName::WandSparkles),
            Box::new(ShowWhatsNew),
        )
}

/// Botão da titlebar que abre o popover de exportação (290px): título em caixa alta e uma linha
/// por formato, com selo da extensão (imagem: com descrição). As ações são as do catálogo.
fn export_dropdown(
    g: &Glass,
    focus: &FocusHandle,
    id: &'static str,
    icon: IconName,
    tip: &'static str,
    group: &'static str,
) -> Div {
    type ExportRow = (
        Box<dyn Action>,
        &'static str,
        &'static str,
        &'static str,
        IconName,
    );
    let code = group == actions::GROUP_EXPORT_CODE;
    let g = *g;
    // (ação, nome, descrição, extensão, ícone)
    let rows: Vec<ExportRow> = if code {
        let db = IconName::Database;
        vec![
            (Box::new(ExportSqlPostgres), "PostgreSQL", "", ".sql", db),
            (
                Box::new(ExportSqlMysql),
                "MySQL",
                "",
                ".sql",
                IconName::Database,
            ),
            (
                Box::new(ExportSqlSqlite),
                "SQLite",
                "",
                ".sql",
                IconName::Database,
            ),
            (
                Box::new(ExportSqlSqlServer),
                "SQL Server",
                "",
                ".sql",
                IconName::Database,
            ),
            (
                Box::new(ExportSqlOracle),
                "Oracle",
                "",
                ".sql",
                IconName::Database,
            ),
            (
                Box::new(ExportMermaid),
                "Mermaid erDiagram",
                "",
                ".mmd",
                IconName::GitFork,
            ),
        ]
    } else {
        vec![
            (
                Box::new(ExportPng),
                "PNG",
                crate::i18n::t("Raster · resolução 2×"),
                ".png",
                IconName::Image,
            ),
            (
                Box::new(ExportJpeg),
                "JPEG",
                crate::i18n::t("Raster · fundo sólido"),
                ".jpg",
                IconName::Image,
            ),
            (
                Box::new(ExportSvg),
                crate::i18n::t("SVG vetorial"),
                crate::i18n::t("Escalável, editável"),
                ".svg",
                IconName::PenTool,
            ),
        ]
    };
    let rows = std::rc::Rc::new(rows);
    div().flex_none().child(
        Button::new(id)
            .ghost()
            .small()
            .w(px(28.))
            .h(px(28.))
            .rounded(px(7.))
            .icon(Icon::new(icon).size(px(15.)).text_color(g.text_faint))
            .tooltip(tip)
            .map(|button| {
                motion::dropdown(
                    id,
                    button,
                    Anchor::TopRight,
                    focus.clone(),
                    move |menu, _, _| {
                        let mut menu = menu.min_w(px(290.)).label(if code {
                            crate::i18n::t("EXPORTAR CÓDIGO")
                        } else {
                            crate::i18n::t("EXPORTAR IMAGEM")
                        });
                        for (action, name, desc, ext, icon) in rows.iter() {
                            let (name, desc, ext, icon) = (*name, *desc, *ext, *icon);
                            menu = menu.menu_element(action.boxed_clone(), move |_, _| {
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .w_full()
                                    .min_h(if code { px(26.) } else { px(48.) })
                                    .py(if code { px(0.) } else { px(5.) })
                                    .child(
                                        div()
                                            .flex()
                                            .flex_none()
                                            .size(if code { px(16.) } else { px(28.) })
                                            .items_center()
                                            .justify_center()
                                            .when(!code, |d| d.rounded(px(7.)).bg(g.input))
                                            .child(
                                                Icon::new(icon)
                                                    .size(px(15.))
                                                    .text_color(g.text_muted),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .flex_1()
                                            .min_w_0()
                                            .gap(px(2.))
                                            .child(
                                                div()
                                                    .text_size(TEXT_SM)
                                                    .line_height(px(18.))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(crate::i18n::t(name)),
                                            )
                                            .when(!desc.is_empty(), |d| {
                                                d.child(
                                                    div()
                                                        .text_size(px(11.5))
                                                        .line_height(px(15.))
                                                        .text_color(g.text_faint)
                                                        .child(crate::i18n::t(desc)),
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_none()
                                            .items_center()
                                            .justify_center()
                                            .w(px(44.))
                                            .h(px(20.))
                                            .rounded(px(4.))
                                            .bg(g.input)
                                            .font_family(FONT_MONO)
                                            .text_size(px(10.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(g.text_muted)
                                            .child(ext),
                                    )
                            });
                        }
                        menu
                    },
                )
            }),
    )
}
