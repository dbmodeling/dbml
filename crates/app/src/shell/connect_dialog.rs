//! Modal "Conectar ao banco…": banco em dropdown, connection string sincronizada com os campos
//! e senha oculta por padrão. A regra de sincronia mora em `connect_db::ConnectModel` (pura); aqui
//! ficam só os widgets e a ligação entre eles. A senha vive apenas no `InputState` do campo Senha.

use super::*;
use crate::connect_db::{
    ConnectModel, FormFields, Status, engine_label, engine_menu, introspect_blocking,
    parse_schemas, test_blocking,
};
use crate::constants as k;
use db_introspect::Dialect;
use gpui_kit::component::Disableable as _;
use gpui_kit::component::menu::PopupMenuItem;
use std::rc::Rc;
use std::time::Duration;

/// Widgets do modal; clonável para os callbacks.
#[derive(Clone)]
pub(super) struct ConnectWidgets {
    model: Entity<ConnectModel>,
    host: Entity<InputState>,
    port: Entity<InputState>,
    database: Entity<InputState>,
    user: Entity<InputState>,
    password: Entity<InputState>,
    path: Entity<InputState>,
    schemas: Entity<InputState>,
    conn: Entity<InputState>,
}

/// O que o `Shell` guarda do modal aberto (referências fracas: fechar o modal as solta), para a
/// paleta saber que ele está aberto e o atalho global alcançar a senha.
pub(super) struct ConnectHandles {
    model: WeakEntity<ConnectModel>,
    password: WeakEntity<InputState>,
    conn: WeakEntity<InputState>,
}

impl ConnectHandles {
    pub(super) fn is_open(&self) -> bool {
        self.model.upgrade().is_some()
    }
}

/// Alterna mostrar/ocultar: um só estado para o campo Senha e para a string montada.
fn toggle_reveal(
    model: &Entity<ConnectModel>,
    password: &Entity<InputState>,
    conn: &Entity<InputState>,
    window: &mut Window,
    cx: &mut App,
) {
    let pw = password.read(cx).value().to_string();
    let (revealed, shown) = model.update(cx, |m, cx| {
        m.toggle_reveal();
        cx.notify();
        (
            m.revealed,
            m.url_error.is_none().then(|| m.display_url(&pw)),
        )
    });
    password.update(cx, |s, cx| s.set_masked(!revealed, window, cx));
    // com erro pendente, o texto do campo é o do usuário: não reescrever
    if let Some(shown) = shown {
        conn.update(cx, |s, cx| s.set_value(shown, window, cx));
    }
}

impl ConnectWidgets {
    fn handles(&self) -> ConnectHandles {
        ConnectHandles {
            model: self.model.downgrade(),
            password: self.password.downgrade(),
            conn: self.conn.downgrade(),
        }
    }

    fn password_text(&self, cx: &App) -> String {
        self.password.read(cx).value().to_string()
    }

    fn read_fields(&self, cx: &App) -> FormFields {
        let v = |e: &Entity<InputState>| e.read(cx).value().to_string();
        FormFields {
            host: v(&self.host),
            port: v(&self.port),
            database: v(&self.database),
            user: v(&self.user),
            path: v(&self.path),
        }
    }

    fn show_string(&self, shown: String, window: &mut Window, cx: &mut App) {
        self.conn.update(cx, |s, cx| s.set_value(shown, window, cx));
    }

    fn sync_port_placeholder(&self, window: &mut Window, cx: &mut App) {
        let ph = self.model.read(cx).port_placeholder();
        self.port
            .update(cx, |s, cx| s.set_placeholder(ph, window, cx));
    }

    /// Um campo mudou: a string é reescrita a partir dos campos.
    fn field_edited(&self, window: &mut Window, cx: &mut App) {
        let (fields, pw) = (self.read_fields(cx), self.password_text(cx));
        let shown = self.model.update(cx, |m, cx| {
            cx.notify();
            m.edit_fields(fields, &pw)
        });
        self.show_string(shown, window, cx);
    }

    /// Banco, TLS ou certificado mudou: reescreve a string com os campos atuais.
    fn refresh(&self, window: &mut Window, cx: &mut App) {
        let pw = self.password_text(cx);
        let shown = self.model.update(cx, |m, cx| {
            cx.notify();
            m.refresh_url(&pw)
        });
        self.show_string(shown, window, cx);
        self.sync_port_placeholder(window, cx);
    }

    fn select_engine(&self, dialect: Dialect, window: &mut Window, cx: &mut App) {
        self.model.update(cx, |m, _| m.set_dialect(dialect));
        self.password
            .update(cx, |s, cx| s.set_masked(true, window, cx));
        self.refresh(window, cx);
    }

    /// Lê a string digitada e, se válida, preenche os campos (sem reescrever a string).
    fn apply_string(&self, window: &mut Window, cx: &mut App) {
        let text = self.conn.read(cx).value().to_string();
        let result = self.model.update(cx, |m, cx| {
            cx.notify();
            m.edit_string(&text)
        });
        let Ok(applied) = result else {
            return;
        };
        let f = self.model.read(cx).fields.clone();
        for (field, value) in [
            (&self.host, f.host),
            (&self.port, f.port),
            (&self.database, f.database),
            (&self.user, f.user),
            (&self.path, f.path),
        ] {
            if field.read(cx).value().as_ref() != value {
                field.update(cx, |s, cx| s.set_value(value, window, cx));
            }
        }
        if let Some(pw) = applied.password {
            self.password
                .update(cx, |s, cx| s.set_value(pw, window, cx));
        }
        self.sync_port_placeholder(window, cx);
    }

    /// Tecla na string: lê depois de uma pausa curta, só se nada mais foi digitado.
    fn string_edited(&self, window: &mut Window, cx: &mut App) {
        let seq = self.model.update(cx, |m, _| {
            m.string_edit_seq += 1;
            m.string_edit_seq
        });
        let this = self.clone();
        window
            .spawn(cx, async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(k::CONN_STRING_DEBOUNCE_MS))
                    .await;
                cx.update(|window, cx| {
                    if this.model.read(cx).string_edit_seq == seq {
                        this.apply_string(window, cx);
                    }
                })
                .ok();
            })
            .detach();
    }

    /// Antes de conectar, aplica uma edição da string ainda no debounce.
    fn flush_string(&self, window: &mut Window, cx: &mut App) {
        let pending = {
            let m = self.model.read(cx);
            m.origin == crate::connect_db::EditOrigin::String && m.url_error.is_none()
        };
        if pending {
            self.model.update(cx, |m, _| m.string_edit_seq += 1);
            self.apply_string(window, cx);
        }
    }
}

impl Shell {
    pub(super) fn connect_dialog_open(&self) -> bool {
        self.connect_dialog
            .as_ref()
            .is_some_and(ConnectHandles::is_open)
    }

    /// "Mostrar/ocultar senha" vindo da paleta ou do atalho quando o foco não está no modal.
    pub(super) fn toggle_password_visibility(
        &mut self,
        _: &TogglePasswordVisibility,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(h) = self.connect_dialog.as_ref() else {
            return;
        };
        if let (Some(model), Some(password), Some(conn)) =
            (h.model.upgrade(), h.password.upgrade(), h.conn.upgrade())
        {
            toggle_reveal(&model, &password, &conn, window, cx);
        }
    }

    /// "Connect to Database…": lê o catálogo de um banco (somente leitura) e gera o DBML.
    pub(super) fn connect_database(
        &mut self,
        _: &ConnectDatabase,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let model = cx.new(|_| ConnectModel::new(Dialect::Postgres));
        let input = |window: &mut Window, cx: &mut Context<Self>, ph: &str| {
            let ph = SharedString::from(ph.to_owned());
            cx.new(|cx| InputState::new(window, cx).placeholder(ph))
        };
        let default_port = model.read(cx).port_placeholder();
        let w = ConnectWidgets {
            model: model.clone(),
            host: input(window, cx, "localhost"),
            port: input(window, cx, &default_port),
            database: input(window, cx, crate::i18n::t("nome do banco")),
            user: input(window, cx, crate::i18n::t("usuário")),
            password: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(crate::i18n::t("senha"))
                    .masked(true)
            }),
            path: input(window, cx, "/path/to/database.sqlite"),
            schemas: input(
                window,
                cx,
                crate::i18n::t("todos os schemas (ou: public, sales)"),
            ),
            conn: input(
                window,
                cx,
                crate::i18n::t("postgres://usuário:senha@host:5432/banco"),
            ),
        };
        w.refresh(window, cx);
        self.connect_dialog = Some(w.handles());

        // assinaturas presas ao modal: somem (e soltam o model) quando ele fecha
        let mut subs = Vec::new();
        for field in [&w.host, &w.port, &w.database, &w.user, &w.password, &w.path] {
            let w = w.clone();
            subs.push(
                cx.subscribe_in(field, window, move |_, _, ev: &InputEvent, window, cx| {
                    if matches!(ev, InputEvent::Change) {
                        w.field_edited(window, cx);
                    }
                }),
            );
        }
        {
            let w = w.clone();
            subs.push(cx.subscribe_in(
                &w.conn.clone(),
                window,
                move |_, _, ev: &InputEvent, window, cx| {
                    if matches!(ev, InputEvent::Change) {
                        w.string_edited(window, cx);
                    }
                },
            ));
        }
        let subs = Rc::new(subs);
        let menu_focus = cx.focus_handle();

        let shell_entity = cx.entity();
        let has_existing = self
            .doc()
            .is_some_and(|d| !d.editor.read(cx).value().trim().is_empty());
        let g = Glass::of_scaled(self.mode, self.translucent, self.ui_scale);

        window.open_dialog(cx, move |dialog, _, _| {
            let _keep_alive = &subs;
            let w = w.clone();
            let shell_entity = shell_entity.clone();
            let menu_focus = menu_focus.clone();
            dialog_bare(dialog, &g, k::DB_DIALOG_W_PX).margin_top(px(k::DB_DIALOG_TOP_PX)).content(
                move |content, _, cx| {
                    let model = w.model.clone();
                    let st = model.read(cx).clone();
                    let is_file = st.dialect == Dialect::Sqlite;
                    let connecting = st.status == Status::Connecting;

                    let label = |text: &'static str, width: f32| {
                        div()
                            .w(px(width))
                            .flex_none()
                            .text_size(px(k::DB_FIELD_LABEL_FONT_PX))
                            .text_color(g.text_muted)
                            .child(text)
                    };
                    let field_box = || {
                        div()
                            .h(px(k::DB_FIELD_H_PX))
                            .px(px(k::DB_FIELD_PAD_X_PX))
                            .rounded(px(k::DB_FIELD_RADIUS_PX))
                            .bg(g.input_solid)
                            .border_1()
                            .border_color(g.edge_strong)
                            .flex()
                            .items_center()
                            .gap(px(k::DB_FORM_GAP_PX))
                    };
                    let input_box = |field: &Entity<InputState>| {
                        field_box()
                            .flex_1()
                            .child(Input::new(field).small().appearance(false))
                    };
                    let line = || {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(k::DB_FIELD_GAP_PX))
                    };
                    let row = |text: &'static str, field: &Entity<InputState>| {
                        line()
                            .child(label(text, k::DB_FIELD_LABEL_W_PX))
                            .child(input_box(field))
                    };

                    // Button factory: `test` only checks credentials and lists schemas.
                    let action_btn = |id: &'static str, test: bool| {
                        let w = w.clone();
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
                            w.flush_string(window, cx);
                            let params = w.model.read(cx).params(&w.password_text(cx));
                            let filter = parse_schemas(&w.schemas.read(cx).value());
                            w.model.update(cx, |m, cx| {
                                m.reset_result();
                                m.status = Status::Connecting;
                                cx.notify();
                            });
                            let w = w.clone();
                            window
                                .spawn(cx, async move |cx| {
                                    let exec = cx.background_executor().clone();
                                    if test {
                                        let result = exec
                                            .spawn(async move { test_blocking(&params, filter) })
                                            .await;
                                        cx.update(|_, cx| {
                                            w.model.update(cx, |m, cx| {
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
                                        cx.update(|window, cx| {
                                            w.model.update(cx, |m, cx| {
                                                m.finish(result);
                                                cx.notify();
                                            });
                                            // conectou: a senha volta a ficar oculta
                                            if !w.model.read(cx).revealed {
                                                w.password.update(cx, |s, cx| {
                                                    s.set_masked(true, window, cx);
                                                });
                                                w.refresh(window, cx);
                                            }
                                        })
                                        .ok();
                                    }
                                })
                                .detach();
                        })
                    };
                    let connect_btn = action_btn(k::ID_DB_CONNECT, false);
                    let test_btn = action_btn(k::ID_DB_TEST, true);

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
                                model.update(cx, |m, _| m.close());
                                window.close_dialog(cx);
                            })
                        };
                    let replace_label = if st.confirm_replace {
                        crate::i18n::t("Confirmar substituição?").to_string()
                    } else {
                        crate::i18n::t("Substituir").to_string()
                    };

                    let is_failed = matches!(st.status, Status::Failed(_));
                    let failed_raw = match &st.status {
                        Status::Failed(msg) => Some(msg.clone()),
                        _ => None,
                    };
                    let failed_hint = failed_raw
                        .as_deref()
                        .map(|msg| crate::i18n::t(connection_hint(msg, is_file)));
                    let show_details_toggle = failed_raw
                        .as_deref()
                        .zip(failed_hint)
                        .is_some_and(|(raw, hint)| !raw.trim().is_empty() && raw.trim() != hint);
                    let offer_tls_require = !is_file
                        && st.tls != db_introspect::TlsMode::Require
                        && failed_raw
                            .as_deref()
                            .is_some_and(is_tls_cert_error);

                    let status_line = match &st.status {
                        Status::Idle => {
                            div().child(crate::i18n::t("Somente leitura: apenas o catálogo é lido, nunca os seus dados."))
                        }
                        Status::Connecting => div().child(crate::i18n::t("Conectando e lendo o esquema…")),
                        Status::Failed(_) => div()
                            .text_color(rgb(crate::glass::DIAG_ERROR))
                            .child(failed_hint.unwrap_or_default()),
                        Status::Tested(msg) => div().child(msg.clone()),
                        Status::Ready => div().child(crate::i18n::t(if st.tables == 1 { "{} tabela encontrada. Substitua o documento ou insira-a." } else { "{} tabelas encontradas. Substitua o documento ou insira-as." }).replace("{}", &st.tables.to_string())),
                    };

                    // Banco: dropdown com o mesmo menu do kit da barra de título (teclado, Esc).
                    let engine = {
                        let trigger = Button::new(k::ID_DB_ENGINE)
                            .outline()
                            .small()
                            .w(px(k::DB_ENGINE_TRIGGER_W_PX))
                            .label(engine_label(st.dialect))
                            .dropdown_caret(true)
                            .disabled(connecting);
                        let menu_w = w.clone();
                        let open_w = w.clone();
                        let key_w = w.clone();
                        let popover = motion::dropdown(
                            k::ID_DB_ENGINE_MENU,
                            trigger,
                            Anchor::TopLeft,
                            menu_focus.clone(),
                            move |menu, _, cx| {
                                let current = menu_w.model.read(cx).dialect;
                                engine_menu(current).into_iter().fold(
                                    menu.min_w(px(k::DB_ENGINE_MENU_W_PX)),
                                    |m, item| {
                                        let w = menu_w.clone();
                                        m.item(
                                            PopupMenuItem::new(item.label)
                                                .checked(item.checked)
                                                .disabled(!item.enabled)
                                                .on_click(move |_, window, cx| {
                                                    w.select_engine(item.dialect, window, cx);
                                                }),
                                        )
                                    },
                                )
                            },
                        )
                        .open(st.engine_menu_open)
                        .on_open_change(move |open, _, cx| {
                            let open = *open;
                            open_w.model.update(cx, |m, cx| {
                                m.engine_menu_open = open;
                                cx.notify();
                            });
                        });
                        line()
                            .child(label(crate::i18n::t("Tipo de banco"), k::DB_FIELD_LABEL_W_PX))
                            .child(
                                div()
                                    // seta para baixo também abre a lista (Enter/Espaço vêm do botão)
                                    .on_key_down(move |ev, _, cx| {
                                        if ev.keystroke.key == k::KEY_ARROW_DOWN
                                            && !key_w.model.read(cx).engine_menu_open
                                        {
                                            key_w.model.update(cx, |m, cx| {
                                                m.engine_menu_open = true;
                                                cx.notify();
                                            });
                                            cx.stop_propagation();
                                        }
                                    })
                                    .child(popover),
                            )
                    };

                    // Connection string (mono) + Copiar; erro curto logo abaixo, associado ao campo.
                    let copy_btn = {
                        let w = w.clone();
                        Button::new(k::ID_DB_CONN_COPY)
                            .ghost()
                            .xsmall()
                            .icon(IconName::Copy)
                            .tooltip(crate::i18n::t("Copiar connection string"))
                            .accessibility_label(crate::i18n::t("Copiar connection string"))
                            .disabled(st.url.is_empty())
                            .on_click(move |_, _, cx| {
                                let text = w.model.read(cx).copy_text(&w.password_text(cx));
                                cx.write_to_clipboard(ClipboardItem::new_string(text));
                            })
                    };
                    let conn_row = line()
                        .child(label("Connection string", k::DB_FIELD_LABEL_W_PX))
                        .child(
                            field_box()
                                .flex_1()
                                .when(st.url_error.is_some(), |d| {
                                    d.border_color(rgb(crate::glass::DIAG_ERROR))
                                })
                                .child(
                                    div()
                                        .flex_1()
                                        .font_family(FONT_MONO)
                                        .text_size(px(k::DB_CONN_FONT_PX))
                                        .child(Input::new(&w.conn).small().appearance(false)),
                                )
                                .child(copy_btn),
                        );
                    let conn_error = st.url_error_text().map(|msg| {
                        div()
                            .pl(px(k::DB_FIELD_LABEL_W_PX + k::DB_FIELD_GAP_PX))
                            .text_size(px(k::DB_ERROR_FONT_PX))
                            .text_color(rgb(crate::glass::DIAG_ERROR))
                            .child(msg)
                    });

                    let mut form = div()
                        .flex()
                        .flex_col()
                        .gap(px(k::DB_FORM_GAP_PX))
                        .px(px(k::DB_FORM_PAD_X_PX))
                        .py(px(k::DB_FORM_PAD_Y_PX))
                        .child(engine)
                        .child(conn_row)
                        .children(conn_error);
                    if is_file {
                        let pick = {
                            let w = w.clone();
                            dialog_btn(&g, k::ID_DB_PICK_FILE, crate::i18n::t("Procurar…"), None, false, true, true)
                                .h(px(k::DB_FIELD_H_PX))
                                .text_color(g.text)
                                .on_click(move |_, window, cx| {
                                    let rx = cx.prompt_for_paths(PathPromptOptions {
                                        files: true,
                                        directories: false,
                                        multiple: false,
                                        prompt: Some(crate::i18n::t("Selecione o banco SQLite").into()),
                                    });
                                    let w = w.clone();
                                    window
                                        .spawn(cx, async move |cx| {
                                            let Ok(Ok(Some(paths))) = rx.await else {
                                                return;
                                            };
                                            let Some(p) = paths.into_iter().next() else {
                                                return;
                                            };
                                            cx.update(|window, cx| {
                                                w.path.update(cx, |e, cx| {
                                                    e.set_value(p.display().to_string(), window, cx);
                                                });
                                                w.field_edited(window, cx);
                                            })
                                            .ok();
                                        })
                                        .detach();
                                })
                        };
                        form = form.child(
                            line()
                                .child(div().flex_1().child(row(crate::i18n::t("Arquivo"), &w.path)))
                                .child(pick),
                        );
                    } else {
                        let eye = {
                            let w = w.clone();
                            let text = crate::i18n::t(if st.revealed { "Ocultar senha" } else { "Mostrar senha" });
                            Button::new(k::ID_DB_PASSWORD_REVEAL)
                                .ghost()
                                .xsmall()
                                .icon(if st.revealed { IconName::EyeOff } else { IconName::Eye })
                                .toggled(st.revealed)
                                .tooltip(text)
                                .accessibility_label(text)
                                .on_click(move |_, window, cx| {
                                    toggle_reveal(&w.model, &w.password, &w.conn, window, cx);
                                })
                        };
                        form = form
                            .child(
                                row("Host", &w.host)
                                    .child(label(crate::i18n::t("Porta"), k::DB_FIELD_LABEL2_W_PX))
                                    .child(input_box(&w.port).flex_none().w(px(k::DB_PORT_FIELD_W_PX))),
                            )
                            .child(row(crate::i18n::t("Banco"), &w.database))
                            .child(
                                row(crate::i18n::t("Usuário"), &w.user)
                                    .child(label(crate::i18n::t("Senha"), k::DB_FIELD_LABEL2_W_PX))
                                    .child(
                                        field_box()
                                            .flex_1()
                                            .child(div().flex_1().child(Input::new(&w.password).small().appearance(false)))
                                            .child(eye),
                                    ),
                            )
                            .child(row("Schemas", &w.schemas));
                        {
                            let ssl_on = st.tls != db_introspect::TlsMode::Disable;
                            let verify_cert = st.tls == db_introspect::TlsMode::Verify;
                            let w_toggle = w.clone();
                            let switch_track = div()
                                .w(px(k::TLS_SWITCH_W_PX))
                                .h(px(k::TLS_SWITCH_H_PX))
                                .p(px(k::TLS_SWITCH_PAD_PX))
                                .rounded(px(k::TLS_SWITCH_RADIUS_PX))
                                .bg(if ssl_on { g.accent } else { g.edge_strong })
                                .flex()
                                .items_center()
                                .when(ssl_on, Styled::justify_end)
                                .child(
                                    div()
                                        .w(px(k::TLS_SWITCH_THUMB_PX))
                                        .h(px(k::TLS_SWITCH_THUMB_PX))
                                        .rounded(px(k::TLS_SWITCH_RADIUS_PX))
                                        .bg(gpui::white()),
                                );
                            let ssl_toggle = div()
                                .id(k::ID_TLS_TOGGLE)
                                .flex()
                                .items_center()
                                .gap(px(k::DB_FORM_GAP_PX))
                                .when(!connecting, |d| {
                                    d.cursor_pointer().on_click(move |_, window, cx| {
                                        w_toggle.model.update(cx, |m, _| {
                                            m.tls = if ssl_on {
                                                db_introspect::TlsMode::Disable
                                            } else {
                                                db_introspect::TlsMode::Require
                                            };
                                            m.trust_cert = false;
                                        });
                                        w_toggle.refresh(window, cx);
                                    })
                                })
                                .child(switch_track)
                                .child(
                                    div()
                                        .text_size(px(k::TLS_LABEL_FONT_PX))
                                        .text_color(g.text)
                                        .child(crate::i18n::t("Usar SSL")),
                                );
                            let mut tls_row = div()
                                .flex()
                                .items_center()
                                .gap(px(k::TLS_CTRL_GAP_PX))
                                .child(ssl_toggle);
                            if ssl_on {
                                let w_verify = w.clone();
                                tls_row = tls_row.child(
                                    Checkbox::new(k::ID_TLS_VERIFY_CERT)
                                        .label(crate::i18n::t("Verificar certificado"))
                                        .checked(verify_cert)
                                        .disabled(connecting)
                                        .on_click(move |checked, window, cx| {
                                            let checked = *checked;
                                            w_verify.model.update(cx, |m, _| {
                                                m.tls = if checked {
                                                    db_introspect::TlsMode::Verify
                                                } else {
                                                    db_introspect::TlsMode::Require
                                                };
                                                m.trust_cert = false;
                                            });
                                            w_verify.refresh(window, cx);
                                        }),
                                );
                            }
                            form = form.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(k::TLS_ROW_GAP_PX))
                                    .child(
                                        div()
                                            .w(px(k::TLS_LABEL_WIDTH_PX))
                                            .flex_none()
                                            .text_size(px(k::TLS_LABEL_FONT_PX))
                                            .text_color(g.text_muted)
                                            .child(k::TLS_FIELD_LABEL),
                                    )
                                    .child(tls_row),
                            );
                        }
                    }

                    let toggle_w = w.clone();
                    content.child(
                        div()
                            .flex()
                            .flex_col()
                            .w_full()
                            .key_context(actions::CONNECT_DB)
                            .on_action(move |_: &TogglePasswordVisibility, window, cx| {
                                toggle_reveal(&toggle_w.model, &toggle_w.password, &toggle_w.conn, window, cx);
                            })
                            .child(dialog_header(
                                &g,
                                crate::i18n::t("Conectar ao banco"),
                                crate::i18n::t("Faça a engenharia reversa de um esquema existente para DBML."),
                            ))
                            .child(form)
                            .child(
                                div()
                                    .id(k::ID_DB_STATUS)
                                    .max_h(px(k::DB_STATUS_MAX_H_PX))
                                    .overflow_y_scroll()
                                    .mx(px(k::DB_FORM_PAD_X_PX))
                                    .mt(px(k::DB_STATUS_MARGIN_TOP_PX))
                                    .px(px(k::DB_STATUS_PAD_X_PX))
                                    .py(px(k::DB_STATUS_PAD_Y_PX))
                                    .rounded(px(k::DB_STATUS_RADIUS_PX))
                                    .bg(g.input)
                                    .flex()
                                    .items_start()
                                    .gap(px(k::DB_FORM_GAP_PX))
                                    .text_size(px(k::DB_FIELD_LABEL_FONT_PX))
                                    .text_color(g.text_muted)
                                    .child(
                                        Icon::new(if is_failed {
                                            IconName::TriangleAlert
                                        } else {
                                            IconName::ShieldCheck
                                        })
                                            .size(px(k::DB_STATUS_ICON_PX))
                                            .mt(px(2.0))
                                            .flex_none()
                                            .text_color(if is_failed {
                                                rgb(crate::glass::DIAG_ERROR).into()
                                            } else {
                                                g.ok
                                            }),
                                    )
                                    .child(div().flex_1().min_w_0().child(status_line)),
                            )
                            .when(show_details_toggle || offer_tls_require, |d| {
                                let w_tls = w.clone();
                                let w_det = w.clone();
                                let details_label = crate::i18n::t(if st.show_error_details {
                                    "Ocultar detalhes"
                                } else {
                                    "Detalhes"
                                });
                                d.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(k::DB_FORM_GAP_PX))
                                        .px(px(k::DB_FORM_PAD_X_PX))
                                        .pt(px(k::DB_FORM_GAP_PX))
                                        .when(offer_tls_require, |row| {
                                            row.child(
                                                dialog_btn(
                                                    &g,
                                                    k::ID_DB_USE_TLS_REQUIRE,
                                                    crate::i18n::t("Usar TLS: Exigir"),
                                                    None,
                                                    false,
                                                    true,
                                                    !connecting,
                                                )
                                                .h(px(k::TLS_BTN_HEIGHT_PX))
                                                .on_click(move |_, window, cx| {
                                                    w_tls.model.update(cx, |m, _| {
                                                        m.tls = db_introspect::TlsMode::Require;
                                                        m.reset_result();
                                                    });
                                                    w_tls.refresh(window, cx);
                                                }),
                                            )
                                        })
                                        .when(show_details_toggle, |row| {
                                            row.child(
                                                dialog_btn(
                                                    &g,
                                                    k::ID_DB_ERROR_DETAILS_TOGGLE,
                                                    details_label,
                                                    None,
                                                    false,
                                                    false,
                                                    true,
                                                )
                                                .h(px(k::TLS_BTN_HEIGHT_PX))
                                                .on_click(move |_, _, cx| {
                                                    w_det.model.update(cx, |m, cx| {
                                                        m.toggle_error_details();
                                                        cx.notify();
                                                    });
                                                }),
                                            )
                                        }),
                                )
                            })
                            .when(st.show_error_details, |d| {
                                if let Some(raw) = failed_raw.clone() {
                                    d.child(
                                        div()
                                            .id(k::ID_DB_ERROR_DETAILS)
                                            .max_h(px(k::DB_ERROR_DETAILS_MAX_H_PX))
                                            .overflow_y_scroll()
                                            .mx(px(k::DB_FORM_PAD_X_PX))
                                            .mt(px(k::DB_FORM_GAP_PX))
                                            .px(px(k::DB_ERROR_DETAILS_PAD_X_PX))
                                            .py(px(k::DB_ERROR_DETAILS_PAD_Y_PX))
                                            .rounded(px(k::DB_STATUS_RADIUS_PX))
                                            .bg(g.input)
                                            .border_1()
                                            .border_color(g.edge)
                                            .font_family(FONT_MONO)
                                            .text_size(px(k::DB_ERROR_DETAILS_FONT_PX))
                                            .text_color(g.text_muted)
                                            .child(raw),
                                    )
                                } else {
                                    d
                                }
                            })
                            .when(!conflicts.is_empty(), |d| {
                                d.child(div().px(px(k::DB_FORM_PAD_X_PX)).text_size(TEXT_XS)
                                    .text_color(rgb(crate::glass::DIAG_WARNING))
                                    .child(conflicts.clone()))
                            })
                            .when(!st.warnings.is_empty(), |d| {
                                let mut list = div()
                                    .id(k::ID_DB_WARNINGS)
                                    .flex()
                                    .flex_col()
                                    .max_h(px(k::DB_WARNINGS_MAX_H_PX))
                                    .overflow_y_scroll()
                                    .px(px(k::DB_FORM_PAD_X_PX))
                                    .pt(px(k::DB_FORM_GAP_PX))
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
                                    .px(px(k::DB_FORM_PAD_X_PX))
                                    .py(px(k::DB_FOOTER_PAD_Y_PX))
                                    .child(test_btn)
                                    .child(div().flex_1())
                                    .child({
                                        let model = model.clone();
                                        dialog_btn(&g, k::ID_DB_CANCEL, crate::i18n::t("Cancelar"), None, false, false, true)
                                            .on_click(move |_, window, cx| {
                                                model.update(cx, |m, _| m.close());
                                                window.close_dialog(cx);
                                            })
                                    })
                                    .child(connect_btn)
                                    .when(can_apply, |d| {
                                        d.child(apply_btn(k::ID_DB_REPLACE, replace_label, true, false))
                                            .child(apply_btn(k::ID_DB_INSERT, crate::i18n::t("Inserir").to_string(), false, true))
                                    }),
                            ),
                    )
                },
            )
        });
    }
}
