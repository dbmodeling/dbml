use super::*;

impl Shell {
    // ---- partes da tela ----

    pub(super) fn titlebar(
        &self,
        g: &Glass,
        maximized: bool,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut tabs = div()
            .h_full()
            .flex()
            .items_end()
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
                    .h(px(32.))
                    .max_w(px(220.))
                    .min_w(px(70.))
                    .flex_shrink(1.0)
                    .pl(px(10.))
                    .pr(px(4.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded_t(px(7.))
                    .text_size(TEXT_SM)
                    .text_color(if on { g.text } else { g.text_muted })
                    .when(on, |d| {
                        d.bg(g.surface).border_1().border_b_0().border_color(g.edge)
                    })
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
                        cx.listener(move |this, _, _, cx| this.close_tab_at(i, cx)),
                    )
                    .child(Icon::new(IconName::Database).xsmall().text_color(if on {
                        g.accent
                    } else {
                        g.text_faint
                    }))
                    .child(div().truncate().child(doc.title.clone()))
                    .child(
                        div()
                            .id(("tab-close", i))
                            .role(Role::Button)
                            .aria_label("Close tab")
                            .size(px(18.))
                            .rounded(px(4.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover({
                                let h = g.hover;
                                move |s| s.bg(h)
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.close_tab_at(i, cx);
                            }))
                            // alterado: ponto; senão o X (visível na aba ativa ou no hover)
                            .child(if doc.dirty {
                                div()
                                    .size(px(7.))
                                    .rounded_full()
                                    .bg(g.text_muted)
                                    .into_any_element()
                            } else {
                                div()
                                    .when(!on, |d| {
                                        d.invisible().group_hover("tab", gpui_kit::Styled::visible)
                                    })
                                    .child(Icon::new(IconName::X).xsmall().text_color(g.text_faint))
                                    .into_any_element()
                            }),
                    ),
            );
        }
        tabs = tabs.child(
            div().h(px(32.)).flex().items_center().child(
                Button::new("tab-new")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Plus)
                    .tooltip_with_action("New File", &NewFile, Some(actions::CTX))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(NewFile), cx)),
            ),
        );

        let main_menu = Button::new("main-menu")
            .ghost()
            .small()
            .icon(IconName::Menu)
            .dropdown_menu(|mut menu, _, _| {
                let mut last = "";
                for e in actions::catalog() {
                    if e.group != last {
                        if !last.is_empty() {
                            menu = menu.separator();
                        }
                        menu = menu.label(e.group);
                        last = e.group;
                    }
                    menu = menu.menu(e.label, e.action);
                }
                menu
            });
        let theme_icon = if self.mode == Mode::Dark {
            IconName::Sun
        } else {
            IconName::Moon
        };
        let mut bar = div()
            .h(TITLEBAR_H)
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(8.))
            .bg(g.chrome)
            .border_b_1()
            .border_color(g.edge)
            .child(main_menu)
            .child(
                img("brand/icon-64.png")
                    .size(px(20.))
                    .mx(px(2.))
                    .flex_none(),
            )
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
                    .aria_label("Search commands (Ctrl+K)")
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(SP_SM)
                    .h(px(26.))
                    .w(if compact { px(30.) } else { px(220.) })
                    .when(compact, gpui_kit::Styled::justify_center)
                    .px(SP_SM)
                    .rounded(px(6.))
                    .border_1()
                    .border_color(g.edge)
                    .bg(g.sidebar)
                    .text_size(TEXT_XS)
                    .text_color(g.text_faint)
                    .hover({
                        let h = g.hover;
                        move |s| s.bg(h)
                    })
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenPalette), cx))
                    .child(Icon::new(IconName::Search).xsmall())
                    .when(!compact, |d| {
                        d.child(div().flex_1().child("Search commands…"))
                            .child("Ctrl K")
                    }),
            )
            .child(
                div().flex_none().child(
                    Button::new("theme-toggle")
                        .ghost()
                        .small()
                        .icon(theme_icon)
                        .tooltip_with_action("Toggle Theme", &ToggleTheme, Some(actions::CTX))
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(ToggleTheme), cx);
                        }),
                ),
            );
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
                ("Restore", "\u{e923}")
            } else {
                ("Maximize", "\u{e922}")
            };
            bar = bar.child(
                div()
                    .h_full()
                    .flex_none()
                    .flex()
                    .child(caption(
                        "cap-min",
                        "Minimize",
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
                        "Close",
                        WindowControlArea::Close,
                        "\u{e8bb}",
                        true,
                    )),
            );
        }
        #[cfg(not(windows))]
        {
            let (max_label, max_icon) = if maximized {
                ("Restore", IconName::WindowRestore)
            } else {
                ("Maximize", IconName::WindowMaximize)
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
                            .aria_label("Minimize")
                            .occlude()
                            .w(px(38.))
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
                            .w(px(38.))
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
                            .aria_label("Close")
                            .occlude()
                            .w(px(38.))
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
                            .on_click(|_, _, cx| {
                                cx.stop_propagation();
                                cx.quit();
                            })
                            .child(Icon::new(IconName::WindowClose).small()),
                    ),
            );
        }
        bar
    }

    /// Barra de atividades (ícones à esquerda), como no VS Code / GitHub Desktop.
    pub(super) fn rail(&self, g: &Glass) -> Div {
        let errors = self.doc().map_or(0, |d| d.counts().0);
        let item = |id: &'static str,
                    icon: IconName,
                    tip: &'static str,
                    action: Box<dyn Action>,
                    is_active: bool,
                    badge: usize| {
            let on = is_active;
            let tip_action = action.boxed_clone();
            div()
                .id(id)
                .role(Role::Tab)
                .aria_label(tip)
                .relative()
                .size(px(34.))
                .rounded(px(7.))
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
                .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
                .child(Icon::new(icon))
                .when(badge > 0, |d| {
                    d.child(
                        div()
                            .absolute()
                            .top(px(3.))
                            .right(px(3.))
                            .min_w(px(14.))
                            .h(px(14.))
                            .px(px(3.))
                            .rounded_full()
                            // tintado (como no Zed), não vermelho sólido
                            .bg(g.danger.opacity(0.22))
                            .border_1()
                            .border_color(g.danger.opacity(0.55))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(9.))
                            .text_color(g.danger)
                            .child(badge.to_string()),
                    )
                })
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
            .bg(g.chrome)
            .border_r_1()
            .border_color(g.edge)
            .child(item(
                "rail-files",
                IconName::Folder,
                "Project Files",
                Box::new(ShowFiles),
                self.sidebar_open && !self.editor_open && self.pane == Pane::Files,
                0,
            ))
            .child(item(
                "rail-schema",
                IconName::Table2,
                "Schema",
                Box::new(ShowSchema),
                self.sidebar_open && !self.editor_open && self.pane == Pane::Schema,
                0,
            ))
            .child(item(
                "rail-problems",
                IconName::TriangleAlert,
                "Problems",
                Box::new(ShowProblems),
                self.sidebar_open && !self.editor_open && self.pane == Pane::Problems,
                errors,
            ))
            .child(item(
                "rail-editor",
                IconName::Braces,
                "DBML Editor",
                Box::new(ToggleEditor),
                self.editor_open,
                0,
            ))
            .child(div().flex_1())
            .child({
                let (mode, translucent, ui_scale, auto_hide) = (
                    self.mode,
                    self.translucent_cards,
                    self.ui_scale,
                    self.inspector_auto_hide,
                );
                Button::new("rail-appearance")
                    .ghost()
                    .icon(IconName::Palette)
                    .tooltip("Appearance, UI scale & panels")
                    .dropdown_menu(move |menu, _, _| {
                        menu.label("Theme")
                            .menu_with_check("Dark", mode == Mode::Dark, Box::new(ToggleTheme))
                            .menu_with_check("Light", mode == Mode::Light, Box::new(ToggleTheme))
                            .separator()
                            .label("UI Scale (Ctrl+Shift+= / -)")
                            .menu_with_check(
                                "Default (100%)",
                                (ui_scale - 1.0).abs() < 0.05,
                                Box::new(ResetUiZoom),
                            )
                            .menu_with_check(
                                "Medium (115%)",
                                (ui_scale - 1.15).abs() < 0.05,
                                Box::new(ZoomInUi),
                            )
                            .menu_with_check(
                                "Large (130%)",
                                (ui_scale - 1.30).abs() < 0.05,
                                Box::new(ZoomInUi),
                            )
                            .separator()
                            .label("Panels")
                            .menu_with_check(
                                "Auto-collapse inspector when nothing selected",
                                auto_hide,
                                Box::new(ToggleAutoCollapseInspector),
                            )
                            .menu_with_check(
                                "Translucent cards",
                                translucent,
                                Box::new(ToggleCardTransparency),
                            )
                    })
            })
            .child(item(
                "rail-keys",
                IconName::Keyboard,
                "Keyboard Shortcuts",
                Box::new(ShowShortcuts),
                false,
                0,
            ))
            .child(item(
                "rail-palette",
                IconName::Command,
                "Command Palette",
                Box::new(OpenPalette),
                false,
                0,
            ))
    }

    /// Barra de atividades da direita (Inspector e Legenda), simétrica à barra da esquerda.
    pub(super) fn right_rail(&self, g: &Glass, cx: &App) -> Div {
        let (has_selection, legend_open) = self.doc().map_or((false, false), |d| {
            let diag = d.diagram.read(cx);
            (
                diag.selected().is_some() || !diag.selection().is_empty(),
                diag.legend_open,
            )
        });

        let item = |id: &'static str,
                    icon: IconName,
                    tip: &'static str,
                    action: Box<dyn Action>,
                    is_active: bool,
                    badge: usize| {
            let on = is_active;
            let tip_action = action.boxed_clone();
            div()
                .id(id)
                .role(Role::Tab)
                .aria_label(tip)
                .relative()
                .size(px(34.))
                .rounded(px(7.))
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
                .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
                .child(Icon::new(icon))
                .when(badge > 0, |d| {
                    d.child(
                        div()
                            .absolute()
                            .top(px(3.))
                            .right(px(3.))
                            .min_w(px(14.))
                            .h(px(14.))
                            .px(px(3.))
                            .rounded_full()
                            .bg(g.accent.opacity(0.25))
                            .border_1()
                            .border_color(g.accent.opacity(0.6))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(9.))
                            .text_color(g.accent)
                            .child(badge.to_string()),
                    )
                })
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
            .bg(g.chrome)
            .border_l_1()
            .border_color(g.edge)
            .child(item(
                "rail-inspector",
                IconName::SlidersHorizontal,
                "Inspector / Details (Ctrl+I)",
                Box::new(ToggleInspector),
                self.inspector_open,
                usize::from(has_selection),
            ))
            .child(item(
                "rail-legend",
                IconName::Info,
                "Diagram Legend (Ctrl+Shift+L)",
                Box::new(ToggleLegend),
                legend_open,
                0,
            ))
            .child(div().flex_1())
    }

    fn section(g: &Glass, text: impl Into<SharedString>, count: Option<usize>) -> Div {
        div()
            .flex()
            .justify_between()
            .items_center()
            .px(SP_MD)
            .pt(SP_MD)
            .pb(px(4.))
            .text_size(TEXT_XS)
            .font_weight(FontWeight::MEDIUM)
            .text_color(g.text_faint)
            .child(text.into())
            .children(count.map(|n| div().child(n.to_string())))
    }

    fn row(g: &Glass, id: impl Into<ElementId>, on: bool) -> Stateful<Div> {
        div()
            .id(id)
            .role(Role::Button)
            .mx(px(6.))
            .h(px(28.))
            .px(px(8.))
            .rounded(px(6.))
            .flex()
            .items_center()
            .gap(SP_SM)
            .text_size(TEXT_SM)
            .text_color(if on { g.text } else { g.text_muted })
            .when(on, |d| d.bg(g.selected))
            .hover({
                let h = g.hover;
                move |s| s.bg(h)
            })
            .active({
                let a = g.active;
                move |s| s.bg(a)
            })
    }

    fn files_pane(&self, g: &Glass, cx: &mut Context<Self>) -> Div {
        let active_path = self.doc().and_then(|d| d.path.clone());
        let mut list = div().flex().flex_col().gap(px(2.)).px(px(6.));

        let (folder_title, count) = match &self.project {
            Some(p) => (file_title(p), self.files.len()),
            None => ("Open Files".to_string(), self.docs.len()),
        };

        let header = div()
            .flex()
            .justify_between()
            .items_center()
            .px(SP_MD)
            .pt(SP_MD)
            .pb(px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .min_w_0()
                    .flex_1()
                    .child(
                        Icon::new(if self.project.is_some() {
                            IconName::FolderOpen
                        } else {
                            IconName::Files
                        })
                        .xsmall()
                        .text_color(g.accent),
                    )
                    .child(
                        div()
                            .text_size(TEXT_SM)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(g.text)
                            .truncate()
                            .child(folder_title),
                    )
                    .child(
                        div()
                            .px(px(5.))
                            .py(px(1.))
                            .rounded_full()
                            .bg(g.hover)
                            .text_size(px(10.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(g.text_muted)
                            .child(count.to_string()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .child(
                        Button::new("pane-new-file")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Plus)
                            .tooltip_with_action("New File", &NewFile, Some(actions::CTX))
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(NewFile), cx);
                            }),
                    )
                    .child(
                        Button::new("pane-open-folder")
                            .ghost()
                            .xsmall()
                            .icon(IconName::FolderOpen)
                            .tooltip_with_action("Open Folder", &OpenFolder, Some(actions::CTX))
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(OpenFolder), cx);
                            }),
                    ),
            );

        if let Some(root) = self.project.clone() {
            for (i, f) in self.files.iter().enumerate() {
                let on = active_path.as_ref() == Some(f);
                let rel = f
                    .strip_prefix(&root)
                    .ok()
                    .and_then(|r| r.parent())
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                let path = f.clone();
                let fname = file_title(f);
                let is_dirty = self
                    .docs
                    .iter()
                    .find(|d| d.path.as_ref() == Some(f))
                    .is_some_and(|d| d.dirty);

                list = list.child(
                    div()
                        .id(("file", i))
                        .role(Role::Button)
                        .aria_label(fname.clone())
                        .h(px(32.))
                        .px(px(8.))
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .gap(px(7.))
                        .text_size(TEXT_SM)
                        .text_color(if on { g.text } else { g.text_muted })
                        .when(on, |d| {
                            d.bg(g.selected)
                                .font_weight(FontWeight::MEDIUM)
                                .border_l_2()
                                .border_color(g.accent)
                        })
                        .when(!on, |d| {
                            let h = g.hover;
                            d.hover(move |s| s.bg(h))
                        })
                        .active({
                            let a = g.active;
                            move |s| s.bg(a)
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_path(path.clone(), window, cx);
                        }))
                        .child(
                            Icon::new(IconName::Database)
                                .size(px(14.))
                                .text_color(if on { g.accent } else { g.text_faint }),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .justify_center()
                                .child(div().truncate().child(fname))
                                .when(!rel.is_empty(), |d| {
                                    d.child(
                                        div()
                                            .text_size(px(10.))
                                            .text_color(g.text_faint)
                                            .truncate()
                                            .child(rel),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .px(px(4.))
                                .py(px(1.))
                                .rounded(px(3.))
                                .bg(g.edge_soft)
                                .text_size(px(9.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(g.text_faint)
                                .child("DBML"),
                        )
                        .when(is_dirty, |r| {
                            r.child(div().size(px(6.)).rounded_full().bg(g.accent))
                        }),
                );
            }
        } else {
            for (i, d) in self.docs.iter().enumerate() {
                let on = i == self.active;
                let title = d.title.clone();
                list = list.child(
                    div()
                        .id(("open", i))
                        .role(Role::Button)
                        .aria_label(title.clone())
                        .h(px(32.))
                        .px(px(8.))
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .gap(px(7.))
                        .text_size(TEXT_SM)
                        .text_color(if on { g.text } else { g.text_muted })
                        .when(on, |st| {
                            st.bg(g.selected)
                                .font_weight(FontWeight::MEDIUM)
                                .border_l_2()
                                .border_color(g.accent)
                        })
                        .when(!on, |st| {
                            let h = g.hover;
                            st.hover(move |s| s.bg(h))
                        })
                        .active({
                            let a = g.active;
                            move |s| s.bg(a)
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.active = i;
                            cx.notify();
                        }))
                        .child(
                            Icon::new(IconName::Database)
                                .size(px(14.))
                                .text_color(if on { g.accent } else { g.text_faint }),
                        )
                        .child(div().flex_1().truncate().child(title))
                        .child(
                            div()
                                .px(px(4.))
                                .py(px(1.))
                                .rounded(px(3.))
                                .bg(g.edge_soft)
                                .text_size(px(9.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(g.text_faint)
                                .child("DBML"),
                        )
                        .when(d.dirty, |r| {
                            r.child(div().size(px(6.)).rounded_full().bg(g.accent))
                        }),
                );
            }
        }

        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(header)
            .child(list)
            .when(self.project.is_none(), |d| {
                d.child(
                    div()
                        .mx(px(8.))
                        .mt(SP_MD)
                        .p(SP_MD)
                        .rounded(px(8.))
                        .border_1()
                        .border_color(g.edge)
                        .bg(g.surface.opacity(0.4))
                        .flex()
                        .flex_col()
                        .items_center()
                        .text_center()
                        .gap(px(6.))
                        .child(
                            Icon::new(IconName::FolderOpen)
                                .size(px(24.))
                                .text_color(g.text_faint),
                        )
                        .child(
                            div()
                                .text_size(TEXT_SM)
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(g.text)
                                .child("Open Project Folder"),
                        )
                        .child(
                            div()
                                .text_size(TEXT_XS)
                                .text_color(g.text_muted)
                                .child("Browse all .dbml files in your project folder seamlessly."),
                        )
                        .child(
                            div().mt(px(4.)).child(
                                Button::new("banner-open-folder")
                                    .primary()
                                    .small()
                                    .icon(IconName::FolderOpen)
                                    .label("Open Folder…")
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(Box::new(OpenFolder), cx);
                                    }),
                            ),
                        ),
                )
            })
    }

    fn schema_pane(&self, g: &Glass, cx: &mut Context<Self>) -> Div {
        let Some(doc) = self.doc() else { return div() };
        let d = doc.diagram.read(cx);
        let sel = d.selection();
        let schema = d.schema().clone();
        let row = |t: &schema::Table, cx: &mut Context<Self>| {
            let on = sel.contains(&t.name);
            let name = t.name.clone();
            let tint = t.color.as_deref().and_then(hex);
            Self::row(g, SharedString::from(format!("side-{}", t.name)), on)
                .aria_label(format!("Table {}", t.name))
                .on_click(cx.listener(move |this, _, _, cx| {
                    let n = name.clone();
                    this.with_diagram(cx, |d| d.focus_table(&n));
                    this.inspector_open = true;
                    this.save_prefs();
                    cx.notify();
                }))
                .child(
                    Icon::new(IconName::Table2)
                        .xsmall()
                        .text_color(tint.unwrap_or(if on { g.accent } else { g.text_faint })),
                )
                .child(div().flex_1().truncate().child(t.name.clone()))
                .child(
                    div()
                        .text_size(TEXT_XS)
                        .text_color(g.text_faint)
                        .child(t.columns.len().to_string()),
                )
        };
        let mut body = div().flex().flex_col().gap(px(1.));
        for gr in &schema.groups {
            let c = gr
                .color
                .as_deref()
                .and_then(hex)
                .unwrap_or_else(|| crate::canvas::group_color(&schema, &gr.name));
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(SP_MD)
                    .pt(SP_SM)
                    .pb(px(2.))
                    .text_size(TEXT_XS)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(g.text_muted)
                    .child(div().size(px(7.)).rounded_full().bg(c))
                    .child(div().flex_1().child(gr.name.clone()))
                    .child(
                        div()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(g.text_faint)
                            .child(gr.tables.len().to_string()),
                    ),
            );
            for t in gr.tables.iter().filter_map(|n| schema.table(n)) {
                body = body.child(row(t, cx).pl(px(22.)));
            }
        }
        let loose: Vec<_> = schema
            .tables
            .iter()
            .filter(|t| schema.group_of(&t.name).is_none())
            .collect();
        if !schema.groups.is_empty() && !loose.is_empty() {
            body = body.child(
                div()
                    .px(SP_MD)
                    .pt(SP_SM)
                    .pb(px(2.))
                    .text_size(TEXT_XS)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(g.text_faint)
                    .child("No group"),
            );
        }
        for t in loose {
            body = body.child(row(t, cx));
        }
        let mut refs = div().flex().flex_col().gap(px(2.)).px(SP_MD);
        for r in &schema.refs {
            refs = refs.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .h(px(22.))
                    .text_size(TEXT_XS)
                    .text_color(g.text_muted)
                    .child(Icon::new(IconName::Link2).xsmall().text_color(g.text_faint))
                    .child(
                        div()
                            .truncate()
                            .child(format!("{} → {}", r.from.table, r.to.table)),
                    ),
            );
        }
        div()
            .flex()
            .flex_col()
            .child(Self::section(g, "TABLES", Some(schema.tables.len())))
            .child(body)
            .child(Self::section(g, "RELATIONS", Some(schema.refs.len())))
            .child(refs)
    }

    fn problems_pane(&self, g: &Glass, cx: &mut Context<Self>) -> Div {
        let Some(doc) = self.doc() else { return div() };
        let text = doc.editor.read(cx).value().to_string();
        let mut list = div().w_full().min_w_0().flex().flex_col().gap(px(1.));
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
            list = list.child(
                Self::row(g, ("problem", i), false)
                    .h_auto()
                    .min_w_0()
                    .py(px(5.))
                    .items_start()
                    .aria_label(d.message.clone())
                    .on_click(cx.listener(move |this, _, window, cx| this.reveal(at, window, cx)))
                    .child(
                        div()
                            .pt(px(2.))
                            .flex_shrink_0()
                            .child(Icon::new(icon).xsmall().text_color(color)),
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
                                    .w_full()
                                    .min_w_0()
                                    .text_color(g.text)
                                    .child(d.message.clone()),
                            )
                            .child(
                                div()
                                    .text_size(TEXT_XS)
                                    .text_color(g.text_faint)
                                    .child(format!("{} · line {line}", doc.title)),
                            ),
                    ),
            );
        }
        let body = if doc.diags.is_empty() {
            div()
                .p(SP_LG)
                .flex()
                .flex_col()
                .items_center()
                .gap(SP_SM)
                .text_size(TEXT_SM)
                .text_color(g.text_muted)
                .child(Icon::new(IconName::CircleCheck).text_color(g.ok))
                .child("No problems found.")
        } else {
            list
        };
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .child(Self::section(g, "PROBLEMS", Some(doc.diags.len())))
            .child(body)
    }

    pub(super) fn sidebar(&self, g: &Glass, cx: &mut Context<Self>) -> Stateful<Div> {
        let content = match self.pane {
            Pane::Files => self.files_pane(g, cx),
            Pane::Schema => self.schema_pane(g, cx),
            Pane::Problems => self.problems_pane(g, cx),
        };
        div()
            .id("sidebar")
            .role(Role::Region)
            .aria_label("Sidebar")
            .size_full()
            .bg(g.sidebar)
            .overflow_y_scroll()
            .child(content)
    }

    pub(super) fn inspector(
        &self,
        g: &Glass,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let tag = |text: &'static str| {
            div()
                .px(px(5.))
                .rounded(px(4.))
                .border_1()
                .border_color(g.edge)
                .text_size(TEXT_XS)
                .text_color(g.text_muted)
                .child(text)
        };
        let (selection, selected, group) = match self.doc() {
            Some(doc) => {
                let d = doc.diagram.read(cx);
                let sel = d.selection();
                let one = d.selected().map(|t| {
                    (
                        t.clone(),
                        d.refs_of(&t.name).into_iter().cloned().collect::<Vec<_>>(),
                    )
                });
                let group = one
                    .as_ref()
                    .and_then(|(t, _)| d.schema().group_of(&t.name).cloned());
                (sel, one, group)
            }
            None => (Vec::new(), None, None),
        };
        let header = |eyebrow: &'static str, title: String, sub: String| {
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .p(SP_MD)
                .border_b_1()
                .border_color(g.edge)
                .child(
                    div()
                        .text_size(TEXT_XS)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(g.text_faint)
                        .child(eyebrow),
                )
                .child(
                    div()
                        .text_size(TEXT_LG)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(g.text)
                        .child(title),
                )
                .child(div().text_size(TEXT_SM).text_color(g.text_muted).child(sub))
        };
        let body = if selection.len() > 1 {
            // várias tabelas: resumo + agrupar
            let mut list = div().flex().flex_col().gap(px(2.)).px(SP_MD);
            for n in &selection {
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(SP_SM)
                        .h(px(26.))
                        .text_size(TEXT_SM)
                        .child(
                            Icon::new(IconName::Table2)
                                .xsmall()
                                .text_color(g.text_faint),
                        )
                        .child(n.clone()),
                );
            }
            div()
                .flex()
                .flex_col()
                .child(header(
                    "SELECTION",
                    format!("{} tables", selection.len()),
                    "Shift+click adds or removes".into(),
                ))
                .child(Self::section(g, "TABLES", None))
                .child(list)
                .child(
                    div().p(SP_MD).flex().flex_col().gap(SP_SM).child(
                        Button::new("insp-group")
                            .primary()
                            .small()
                            .w_full()
                            .icon(IconName::LayoutGrid)
                            .label("Group selection")
                            .tooltip_with_action(
                                "Group selected tables",
                                &GroupSelection,
                                Some(actions::CTX),
                            )
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(GroupSelection), cx);
                            }),
                    ),
                )
        } else if let Some((t, refs)) = selected {
            // edição em linha: clique no texto abre o campo; Enter ou sair confirma
            let editing = self
                .editing
                .as_ref()
                .map(|(t, st, _)| (t.clone(), st.clone()));
            let field = |target: EditTarget,
                         text: String,
                         muted: bool,
                         cx: &mut Context<Self>|
             -> AnyElement {
                match &editing {
                    Some((t, st)) if *t == target => Input::new(st).small().into_any_element(),
                    _ => {
                        let id = SharedString::from(format!("edit-{target:?}"));
                        let value = text.clone();
                        div()
                            .id(id)
                            .role(Role::Button)
                            .aria_label(format!("Edit {text}"))
                            .w_full()
                            .px(px(4.))
                            .rounded(px(4.))
                            .truncate()
                            .text_color(if muted { g.text_faint } else { g.text })
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
            let col_type_w = self.col_type_w;
            let mut cols = div().flex().flex_col();
            let col_header = div()
                .flex()
                .items_center()
                .gap(px(6.))
                .px(SP_MD)
                .py(px(4.))
                .border_b_1()
                .border_color(g.edge_soft)
                .text_size(px(10.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(g.text_faint)
                .child(div().flex_1().min_w(px(60.)).child("NAME"))
                .child(
                    div()
                        .id("col-splitter-hdr")
                        .w(px(8.))
                        .h(px(14.))
                        .mx(px(-2.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor(CursorStyle::ResizeLeftRight)
                        .tooltip(|window, cx| {
                            Tooltip::new("Drag to resize columns (double-click to reset)")
                                .build(window, cx)
                        })
                        .hover(|s| s.bg(g.hover).rounded(px(2.)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                                if e.click_count >= 2 {
                                    this.col_type_w = 110.0;
                                    this.save_prefs();
                                    cx.notify();
                                } else {
                                    this.col_resize_start = Some((e.position.x, this.col_type_w));
                                }
                            }),
                        )
                        .child(div().w(px(1.5)).h(px(10.)).bg(g.edge)),
                )
                .child(div().flex_none().w(px(col_type_w)).child("TYPE"))
                .child(div().flex_none().w(px(62.)).child("FLAGS"))
                .child(div().w(px(16.)));
            cols = cols.child(col_header);
            for c in &t.columns {
                let chip = |id: String,
                            label: &'static str,
                            flag: &'static str,
                            on: bool,
                            cx: &mut Context<Self>| {
                    let (table, col) = (t.name.clone(), c.name.clone());
                    div()
                        .id(SharedString::from(id))
                        .role(Role::Switch)
                        .aria_label(format!("{label} on {}", c.name))
                        .tooltip(move |window, cx| {
                            Tooltip::new(match flag {
                                "pk" => "Primary key",
                                "fk" => "Foreign key (FK)",
                                "not null" => "Required (not null)",
                                _ => "Unique value",
                            })
                            .build(window, cx)
                        })
                        .px(px(4.))
                        .rounded(px(4.))
                        .border_1()
                        .text_size(px(10.))
                        .when(on, |d| {
                            d.bg(g.selected)
                                .border_color(g.accent.opacity(0.5))
                                .text_color(g.accent)
                        })
                        .when(!on, |d| {
                            d.border_color(g.edge_soft).text_color(g.text_faint)
                        })
                        .hover({
                            let e = g.edge;
                            move |st| st.border_color(e)
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.toggle_flag(&table, &col, flag, !on, window, cx);
                        }))
                        .child(label)
                };
                let name_el = field(
                    EditTarget::ColName(t.name.clone(), c.name.clone()),
                    c.name.clone(),
                    false,
                    cx,
                );
                let type_el = field(
                    EditTarget::ColType(t.name.clone(), c.name.clone()),
                    c.ty.clone(),
                    true,
                    cx,
                );
                let (table, col) = (t.name.clone(), c.name.clone());
                let is_editing_this_type = matches!(
                    &editing,
                    Some((EditTarget::ColType(tbl, col), _)) if tbl == &t.name && col == &c.name
                );
                cols = cols.child(
                    div()
                        .group("insp-col")
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .px(SP_MD)
                        .h(px(32.))
                        .border_b_1()
                        .border_color(g.edge_soft)
                        .text_size(TEXT_SM)
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(60.))
                                .overflow_hidden()
                                .child(name_el),
                        )
                        .child(
                            div()
                                .w(px(4.))
                                .h(px(14.))
                                .mx(px(-2.))
                                .flex_none()
                                .cursor(CursorStyle::ResizeLeftRight)
                                .hover(|s| s.bg(g.accent.opacity(0.4)).rounded(px(1.)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                                        if e.click_count >= 2 {
                                            this.col_type_w = 110.0;
                                            this.save_prefs();
                                            cx.notify();
                                        } else {
                                            this.col_resize_start =
                                                Some((e.position.x, this.col_type_w));
                                        }
                                    }),
                                ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(col_type_w))
                                .overflow_hidden()
                                .text_size(TEXT_XS)
                                .child(type_el),
                        )
                        .child(chip(format!("pk-{}", c.name), "PK", "pk", c.pk, cx))
                        .child(chip(format!("fk-{}", c.name), "FK", "fk", c.fk, cx))
                        .child(chip(
                            format!("nn-{}", c.name),
                            "NN",
                            "not null",
                            c.not_null,
                            cx,
                        ))
                        .child(chip(format!("uq-{}", c.name), "UQ", "unique", c.unique, cx))
                        .child(
                            div()
                                .invisible()
                                .group_hover("insp-col", gpui_kit::Styled::visible)
                                .child(
                                    Button::new(SharedString::from(format!("del-{}", c.name)))
                                        .ghost()
                                        .xsmall()
                                        .icon(IconName::Trash)
                                        .tooltip("Delete column")
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            let (t2, c2) = (table.clone(), col.clone());
                                            this.edit_text(false, window, cx, |text| {
                                                dbml_edit::delete_column(text, &t2, &c2)
                                            });
                                        })),
                                ),
                        ),
                );

                if is_editing_this_type {
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
                            suggestions.push((ty, desc));
                        }
                    }
                    for en in &enums {
                        if query.is_empty() || en.to_ascii_lowercase().contains(&query) {
                            suggestions.push((en.as_str(), "enumeration"));
                        }
                    }
                    suggestions.truncate(8);

                    if !suggestions.is_empty() {
                        let mut sugg_box = div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .p(px(6.))
                            .mx(SP_MD)
                            .my(px(2.))
                            .rounded(px(6.))
                            .bg(g.island)
                            .border_1()
                            .border_color(g.edge)
                            .shadow_md();

                        sugg_box = sugg_box.child(
                            div()
                                .flex()
                                .justify_between()
                                .px(px(4.))
                                .pb(px(2.))
                                .border_b_1()
                                .border_color(g.edge_soft)
                                .text_size(px(10.))
                                .text_color(g.text_faint)
                                .child("Type suggestions (click to apply)")
                                .child("Esc closes"),
                        );

                        for (s_type, s_desc) in suggestions {
                            let opt_type = s_type.to_string();
                            let (tbl, cl) = (t.name.clone(), c.name.clone());
                            sugg_box = sugg_box.child(
                                div()
                                    .id(SharedString::from(format!(
                                        "type-opt-{tbl}-{cl}-{opt_type}"
                                    )))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px(px(6.))
                                    .py(px(3.))
                                    .rounded(px(4.))
                                    .cursor_pointer()
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
                                            .text_size(TEXT_XS)
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(g.text)
                                            .child(s_type.to_string()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.))
                                            .text_color(g.text_faint)
                                            .child(s_desc.to_string()),
                                    ),
                            );
                        }
                        cols = cols.child(sugg_box);
                    }
                }
            }
            let add_table = t.name.clone();
            cols = cols
                .child(
                    div().px(SP_MD).py(SP_SM).child(
                        Button::new("insp-add-col")
                            .ghost()
                            .small()
                            .icon(IconName::Plus)
                            .label("Add column")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.add_column_to(add_table.clone(), window, cx);
                            })),
                    ),
                )
                .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                    if let Some((start_x, start_w)) = this.col_resize_start {
                        let delta = f32::from(e.position.x - start_x);
                        let new_w = (start_w - delta).clamp(60.0, 360.0);
                        if (new_w - this.col_type_w).abs() >= 0.5 {
                            this.col_type_w = new_w;
                            cx.notify();
                        }
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        if this.col_resize_start.take().is_some() {
                            this.save_prefs();
                            cx.notify();
                        }
                    }),
                );
            let title_el = field(EditTarget::Table(t.name.clone()), t.name.clone(), false, cx);
            let mut rels = div().flex().flex_col().gap(px(4.)).px(SP_MD);
            for (ri, r) in refs.iter().enumerate() {
                let card = match r.kind {
                    schema::RefKind::ManyToOne => "N:1",
                    schema::RefKind::OneToMany => "1:N",
                    schema::RefKind::OneToOne => "1:1",
                    schema::RefKind::ManyToMany => "N:N",
                };
                let (ft, fc) = (r.from.table.clone(), r.from.column.clone());
                let (tt, tc) = (r.to.table.clone(), r.to.column.clone());
                rels = rels.child(
                    div()
                        .group("insp-rel")
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(TEXT_SM)
                        .text_color(g.text_muted)
                        .child(Icon::new(IconName::Link2).xsmall().text_color(g.text_faint))
                        .child(div().flex_1().truncate().child(format!(
                            "{}.{} → {}.{}",
                            r.from.table, r.from.column, r.to.table, r.to.column
                        )))
                        .child(tag(card))
                        .child(
                            Button::new(SharedString::from(format!("del-rel-{ri}")))
                                .ghost()
                                .xsmall()
                                .icon(IconName::Trash)
                                .tooltip("Delete relation and FK column")
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
                                })),
                        ),
                );
            }
            // cor: amostras clicáveis (grava headercolor no DBML)
            let current = t.color.clone().unwrap_or_default().to_ascii_lowercase();
            let mut swatches = div().flex().flex_wrap().gap(px(6.)).px(SP_MD);
            for (i, (label, hexv)) in TABLE_COLORS.iter().enumerate() {
                let on = current == *hexv;
                let table = t.name.clone();
                let color = hex(hexv).unwrap_or_default();
                swatches = swatches.child(
                    div()
                        .id(("swatch", i))
                        .role(Role::Button)
                        .aria_label(format!("Color {label}"))
                        .tooltip(move |window, cx| Tooltip::new(*label).build(window, cx))
                        .size(px(22.))
                        .rounded_full()
                        .bg(color)
                        .border_2()
                        .border_color(if on {
                            g.text
                        } else {
                            gpui_kit::transparent_black()
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.set_color(&table, Some(hexv), window, cx);
                        })),
                );
            }
            let table = t.name.clone();
            swatches = swatches.child(
                div()
                    .id("swatch-none")
                    .role(Role::Button)
                    .aria_label("No color")
                    .tooltip(|window, cx| Tooltip::new("No color").build(window, cx))
                    .size(px(22.))
                    .rounded_full()
                    .border_1()
                    .border_color(g.edge)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Icon::new(IconName::X).xsmall().text_color(g.text_faint))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_color(&table, None, window, cx);
                    })),
            );
            let group_row = match group {
                Some(gr) => div()
                    .flex()
                    .items_center()
                    .gap(SP_SM)
                    .px(SP_MD)
                    .child(
                        Icon::new(IconName::LayoutGrid)
                            .xsmall()
                            .text_color(g.text_faint),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(TEXT_SM)
                            .text_color(g.text)
                            .child(gr.name.clone()),
                    )
                    .child(
                        Button::new("insp-ungroup")
                            .ghost()
                            .xsmall()
                            .label("Ungroup")
                            .tooltip_with_action(
                                "Remove table from group",
                                &UngroupSelection,
                                Some(actions::CTX),
                            )
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(UngroupSelection), cx);
                            }),
                    ),
                None => div()
                    .px(SP_MD)
                    .text_size(TEXT_SM)
                    .text_color(g.text_faint)
                    .child("No group · Shift+click other tables and press Ctrl+G to group"),
            };
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .p(SP_MD)
                        .border_b_1()
                        .border_color(g.edge)
                        .child(
                            div()
                                .text_size(TEXT_XS)
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(g.text_faint)
                                .child("TABLE"),
                        )
                        .child(
                            div()
                                .text_size(TEXT_LG)
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title_el),
                        )
                        .child(
                            div()
                                .text_size(TEXT_SM)
                                .text_color(g.text_muted)
                                .child(format!("{} columns · F2 to rename", t.columns.len())),
                        ),
                )
                .child(Self::section(g, "COLOR", None))
                .child(swatches)
                .child(Self::section(g, "GROUP", None))
                .child(group_row)
                .child(Self::section(g, "COLUMNS", None))
                .child(cols)
                .child(Self::section(g, "RELATIONS", None))
                .child(rels)
                .child(
                    div()
                        .p(SP_MD)
                        .mt(SP_SM)
                        .border_t_1()
                        .border_color(g.edge_soft)
                        .child(
                            Button::new("insp-del-table")
                                .ghost()
                                .small()
                                .icon(IconName::Trash)
                                .label("Delete table")
                                .tooltip_with_action(
                                    "Delete table",
                                    &DeleteSelection,
                                    Some(actions::CANVAS),
                                )
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(DeleteSelection), cx);
                                }),
                        ),
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
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .p(SP_LG)
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .text_size(TEXT_BASE)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(g.text)
                                .child("Nothing selected"),
                        )
                        .child(
                            Button::new("btn-hide-inspector")
                                .ghost()
                                .xsmall()
                                .icon(IconName::PanelRightClose)
                                .tooltip("Hide inspector (Ctrl+I)")
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(ToggleInspector), cx);
                                }),
                        ),
                )
                .child(
                    div()
                        .text_size(TEXT_SM)
                        .text_color(g.text_muted)
                        .child("Click a table to view columns, color, group, and relations. Shift+click selects multiple."),
                )
                .child(div().h(px(1.)).my(SP_SM).bg(g.edge))
                .child(tip(&ToolSelect, actions::CANVAS, "Select"))
                .child(tip(&ToolHand, actions::CANVAS, "Pan view"))
                .child(tip(&FitView, actions::CANVAS, "Fit to screen"))
                .child(tip(&GroupSelection, actions::CTX, "Group selection"))
                .child(tip(&ToggleEditor, actions::CTX, "DBML Editor"))
                .child(tip(&OpenPalette, actions::CTX, "All commands"))
        };
        div()
            .id("inspector")
            .role(Role::Region)
            .aria_label("Inspector")
            .size_full()
            .bg(g.sidebar)
            .border_l_1()
            .border_color(g.edge)
            .overflow_y_scroll()
            .child(body)
    }

    pub(super) fn editor_panel(&self, g: &Glass) -> Div {
        let Some(doc) = self.doc() else { return div() };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(g.surface)
            .border_r_1()
            .border_color(g.edge)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .h(px(32.))
                    .px(SP_MD)
                    .border_b_1()
                    .border_color(g.edge)
                    .text_size(TEXT_XS)
                    .text_color(g.text_muted)
                    .child(Icon::new(IconName::Braces).xsmall())
                    .child(div().flex_1().child(doc.title.clone()))
                    .child(
                        Button::new("editor-format")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Sparkles)
                            .tooltip_with_action(
                                "Format Document",
                                &FormatDocument,
                                Some(actions::CTX),
                            )
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(FormatDocument), cx);
                            }),
                    )
                    .child(
                        Button::new("editor-close")
                            .ghost()
                            .xsmall()
                            .icon(IconName::X)
                            .tooltip_with_action("Close Editor", &ToggleEditor, Some(actions::CTX))
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(ToggleEditor), cx);
                            }),
                    ),
            )
            .child(
                div().flex_1().min_h_0().child(
                    Editor::new(&doc.editor)
                        .appearance(false)
                        .bordered(false)
                        .h_full(),
                ),
            )
    }

    pub(super) fn empty_state(g: &Glass) -> Div {
        div()
            .size_full()
            .bg(g.surface)
            .flex()
            .items_center()
            .justify_center()
            .child(crate::widgets::empty_state(
                g,
                IconName::Database,
                "No file open",
                "Open a project folder or start a new schema.",
                crate::widgets::EmptyAction {
                    id: "empty-folder",
                    icon: IconName::FolderOpen,
                    label: "Open Folder",
                    keys: "Ctrl+Shift+O",
                    action: Box::new(OpenFolder),
                },
                crate::widgets::EmptyAction {
                    id: "empty-new",
                    icon: IconName::Plus,
                    label: "New File",
                    keys: "Ctrl+N",
                    action: Box::new(NewFile),
                },
            ))
    }

    pub(super) fn statusbar(&self, g: &Glass, compact: bool, cx: &App) -> Div {
        let mut bar = div()
            .h(STATUSBAR_H)
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .gap(SP_LG)
            .px(SP_MD)
            .bg(g.chrome)
            .border_t_1()
            .border_color(g.edge)
            .text_size(TEXT_XS)
            .text_color(g.text_muted);
        if let Some(doc) = self.doc() {
            let d = doc.diagram.read(cx);
            let s = d.schema();
            let (e, w) = doc.counts();
            bar = bar
                .child(
                    div()
                        .id("status-problems")
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(ShowProblems), cx);
                        })
                        .child(Icon::new(IconName::CircleX).xsmall().text_color(if e > 0 {
                            g.danger
                        } else {
                            g.text_faint
                        }))
                        .child(e.to_string())
                        .child(
                            Icon::new(IconName::TriangleAlert)
                                .xsmall()
                                .text_color(if w > 0 { g.warning } else { g.text_faint }),
                        )
                        .child(w.to_string()),
                )
                .child(format!(
                    "{} tables · {} relations",
                    s.tables.len(),
                    s.refs.len()
                ))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .justify_center()
                        .text_color(g.text_faint)
                        .truncate()
                        .when(!compact, |e| e.child(d.hint())),
                )
                .child(format!("Zoom {}%", (d.zoom() * 100.).round() as i32));
        } else {
            bar = bar.child(div().flex_1());
        }
        bar.child(format!("v{}", env!("CARGO_PKG_VERSION")))
            .child("DBML")
            .child(if self.mode == Mode::Dark {
                "Dark"
            } else {
                "Light"
            })
    }
}
