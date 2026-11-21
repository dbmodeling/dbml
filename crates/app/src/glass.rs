//! Camada de tema: única origem de cores cruas do app, mais as escalas de raio,
//! espaçamento e tamanhos de chrome. Telas só consomem `Glass`, nunca `rgba` direto.
//!
//! Direção visual (2026-11-14, protótipo "DB Modeling v4"): neutros quentes, escuro
//! profundo, linhas de 7–13%, acento "brasa" (oklch .66 .17 42) para seleção, foco e
//! ação primária; Geist + Geist Mono.

use gpui_kit::{BoxShadow, Hsla, Pixels, point, px, rgba};

/// Escala de espaçamento (sm, md, lg).
pub const SP_SM: Pixels = px(8.);
pub const SP_MD: Pixels = px(12.);
pub const SP_LG: Pixels = px(16.);

/// Escala tipográfica (Geist): legenda, secundário, corpo, título de painel.
pub const TEXT_XS: Pixels = px(12.);
pub const TEXT_SM: Pixels = px(13.);
pub const TEXT_BASE: Pixels = px(14.);

/// Cabeçalho de painel lateral e linha de lista (protótipo v3).
pub const PANEL_HEADER_H: Pixels = px(38.);
pub const LIST_ROW_H: Pixels = px(28.);

pub const TITLEBAR_H: Pixels = px(42.);
pub const STATUSBAR_H: Pixels = px(26.);
pub const RAIL_W: Pixels = px(48.);
pub const SIDEBAR_W: Pixels = px(248.);
pub const INSPECTOR_W: Pixels = px(288.);
pub const EDITOR_W: Pixels = px(440.);

/// Raios de cartão, ilha e grupo do protótipo v5 (consumidos pelo canvas).
#[allow(dead_code)]
pub const RADIUS_CARD: Pixels = px(12.);
#[allow(dead_code)]
pub const RADIUS_ISLAND: Pixels = px(10.);
#[allow(dead_code)]
pub const RADIUS_GROUP: Pixels = px(20.);

/// Sublinhado de diagnósticos no editor (tons médios, legíveis no claro e no escuro).
pub const DIAG_ERROR: u32 = 0xed4a49ff;
pub const DIAG_WARNING: u32 = 0xeba941ff;
pub const DIAG_INFO: u32 = 0x589aedff;

/// Botão Fechar da janela no Windows (vermelho do sistema).
#[cfg_attr(not(windows), allow(dead_code))]
pub const CAPTION_CLOSE: Hsla = Hsla {
    h: 356. / 360.,
    s: 0.86,
    l: 0.49,
    a: 1.,
};

/// Fontes da interface e de código, embarcadas em `assets/fonts` (Geist, licença OFL).
pub const FONT: &str = "Geist";
pub const FONT_MONO: &str = "Geist Mono";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
}

impl Mode {
    pub fn toggled(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }
}

/// Paleta para cor de tabelas e grupos (DBML `headercolor` / `color`).
pub const TABLE_COLORS: [(&str, &str); 8] = [
    ("Blue", "#589aed"),
    ("Cyan", "#00b1ba"),
    ("Green", "#4eb068"),
    ("Lime", "#8ea432"),
    ("Amber", "#c09000"),
    ("Orange", "#df784e"),
    ("Pink", "#d771a4"),
    ("Violet", "#9f85e5"),
];

/// Anel de seleção/foco: 3px de acento a 18% (cartão selecionado, campo focado).
pub fn ring(g: &Glass) -> BoxShadow {
    BoxShadow {
        color: g.accent.opacity(0.18),
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(3.),
        inset: false,
    }
}

/// Sombra suave e curta dos cartões de tabela (sempre presente).
pub fn card_shadow(mode: Mode) -> BoxShadow {
    BoxShadow {
        color: c(0x000000ff).opacity(if mode == Mode::Dark { 0.25 } else { 0.06 }),
        offset: point(px(0.), px(2.)),
        blur_radius: px(8.),
        spread_radius: px(0.),
        inset: false,
    }
}

/// Sombra de item selecionado (`t.selShadow`: realce interno no escuro, sombra curta no claro).
pub fn sel_shadow(mode: Mode) -> Vec<BoxShadow> {
    match mode {
        Mode::Dark => vec![BoxShadow {
            color: c(0xffffff0f),
            offset: point(px(0.), px(1.)),
            blur_radius: px(0.),
            spread_radius: px(0.),
            inset: true,
        }],
        Mode::Light => vec![
            BoxShadow {
                color: c(0x0000000a),
                offset: point(px(0.), px(1.)),
                blur_radius: px(0.),
                spread_radius: px(0.),
                inset: false,
            },
            BoxShadow {
                color: c(0x0000001a),
                offset: point(px(0.), px(1.)),
                blur_radius: px(3.),
                spread_radius: px(0.),
                inset: false,
            },
        ],
    }
}

/// Elevação de camadas flutuantes (ilhas do canvas, popovers): sombra ampla e suave
/// mais um contato curto, como nos modelos de elevação do Zed.
pub fn elevation(mode: Mode) -> Vec<BoxShadow> {
    let k = if mode == Mode::Dark { 0.42 } else { 0.12 };
    vec![
        BoxShadow {
            color: c(0x000000ff).opacity(k),
            offset: point(px(0.), px(6.)),
            blur_radius: px(18.),
            spread_radius: px(-4.),
            inset: false,
        },
        BoxShadow {
            color: c(0x000000ff).opacity(k * 0.6),
            offset: point(px(0.), px(1.)),
            blur_radius: px(2.),
            spread_radius: px(0.),
            inset: false,
        },
    ]
}

/// `#rrggbb` → cor (para `headercolor` vindo do DBML).
pub fn hex(s: &str) -> Option<Hsla> {
    let h = s.trim().trim_start_matches('#');
    (h.len() == 6)
        .then(|| u32::from_str_radix(h, 16).ok())
        .flatten()
        .map(|v| c((v << 8) | 0xff))
}

#[derive(Clone, Copy)]
pub struct Glass {
    pub mode: Mode,
    /// Janela e painéis translúcidos (`glass = true` no protótipo v5).
    pub translucent: bool,
    pub ui_scale: f32,
    /// Indica ao canvas se o editor DBML está aberto (para destacar o botão `braces` na ilha).
    pub editor_open: bool,
    /// Barra de título, barra de atividades e barra de status.
    pub chrome: Hsla,
    /// Trilhos (esquerdo e direito) e barra de status: um degrau abaixo do chrome.
    pub rail: Hsla,
    /// Barra lateral e inspector.
    pub sidebar: Hsla,
    /// Área de trabalho (canvas).
    pub surface: Hsla,
    /// Painel do editor DBML (`t.editor`).
    pub editor: Hsla,
    /// Pontos da grade do canvas.
    pub grid: Hsla,
    pub card: Hsla,
    /// Ilhas flutuantes (vidro): fundo translúcido + realce na borda superior.
    pub island: Hsla,
    /// Bordas de 1px (contorno de controles e cartões).
    pub edge: Hsla,
    /// Divisórias internas, mais discretas que `edge`.
    pub edge_soft: Hsla,
    /// Contorno mais forte: ilhas, popovers, botões secundários (`line2` do protótipo).
    pub edge_strong: Hsla,
    /// Fundo de diálogos e menus; `scrim` escurece o app atrás de diálogos.
    pub modal: Hsla,
    pub scrim: Hsla,
    /// Fundo "ghost" no hover; `active` é o pressionado (mais forte).
    pub hover: Hsla,
    pub active: Hsla,
    pub selected: Hsla,
    /// Seleção neutra de listas, abas e segmentos (o acento fica para o canvas e o foco).
    pub sel: Hsla,
    /// Fundo de campos, pílulas de contagem e trilho de segmentados.
    pub input: Hsla,
    /// Fundo de campo focado / entrada sólida (`t.inputSolid`).
    pub input_solid: Hsla,
    /// Cor interna do anel duplo das amostras de cor (`#121316` escuro / `#f6f6f4` claro).
    pub swatch_ring: Hsla,
    /// Chave estrangeira / ligação (azul do grupo padrão).
    pub link: Hsla,
    /// Selo `IDX` no canvas (ciano `HU(200)`).
    #[allow(dead_code)]
    pub idx: Hsla,
    /// Traço de relação inativa no canvas (`rels.stroke`) e amostra na legenda (`t.relLine`).
    #[allow(dead_code)]
    pub rel_stroke: Hsla,
    #[allow(dead_code)]
    pub rel_line: Hsla,
    /// Realce de sintaxe DBML: palavra-chave, nome, tipo.
    pub syn_kw: Hsla,
    pub syn_name: Hsla,
    pub syn_type: Hsla,
    pub syn_attr: Hsla,
    pub syn_num: Hsla,
    pub syn_comment: Hsla,
    pub syn_punct: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_faint: Hsla,
    pub accent: Hsla,
    pub danger: Hsla,
    pub warning: Hsla,
    pub ok: Hsla,
}

/// Tamanho real do "100%" da escala de UI: o antigo 130% virou o normal (2026-11-16).
pub const UI_BASE: f32 = 1.3;

fn c(v: u32) -> Hsla {
    rgba(v).into()
}

impl Glass {
    /// Tokens do protótipo v5 para os quatro temas (`mode` × `translucent`).
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Design-token scale helpers retained for UI components"
        )
    )]
    pub fn of(mode: Mode, translucent: bool) -> Self {
        Self::of_scaled(mode, translucent, 1.0)
    }

    pub fn of_scaled(mode: Mode, translucent: bool, ui_scale: f32) -> Self {
        let mut g = Self::base(mode, translucent);
        // Translúcido em uma camada só: `chrome` (raiz da janela) carrega o alfa; os painéis somam só um
        // tom fino. O DirectX do gpui SOMA os alfas das camadas, então duas camadas grossas viram opaco.
        // Sem desfoque do sistema (X11/GNOME) o fundo aparece nítido: fica quase opaco para não brigar.
        if translucent && !crate::platform::supports_blur() {
            g.chrome.a = 0.92;
        }
        g.ui_scale = ui_scale * UI_BASE;
        g.selected = g
            .accent
            .opacity(if mode == Mode::Dark { 0.18 } else { 0.12 });
        g
    }

    /// Nome legível do tema ativo (exibido na barra de status e em Aparência).
    pub const fn theme_name(&self) -> &'static str {
        match self.mode {
            Mode::Light => "Claro",
            Mode::Dark => "Escuro",
        }
    }

    #[expect(
        dead_code,
        reason = "Design-token scale helpers retained for UI components"
    )]
    pub fn font_xs(&self) -> Pixels {
        px((12. * self.ui_scale).round())
    }

    #[expect(
        dead_code,
        reason = "Design-token scale helpers retained for UI components"
    )]
    pub fn font_sm(&self) -> Pixels {
        px((13.5 * self.ui_scale).round())
    }

    #[expect(
        dead_code,
        reason = "Design-token scale helpers retained for UI components"
    )]
    pub fn font_base(&self) -> Pixels {
        px((14.5 * self.ui_scale).round())
    }

    #[expect(
        dead_code,
        reason = "Design-token scale helpers retained for UI components"
    )]
    pub fn font_lg(&self) -> Pixels {
        px((16.5 * self.ui_scale).round())
    }

    #[expect(
        dead_code,
        reason = "Design-token scale helpers retained for UI components"
    )]
    pub fn icon_sm(&self) -> Pixels {
        px((14. * self.ui_scale).round())
    }

    #[expect(
        dead_code,
        reason = "Design-token scale helpers retained for UI components"
    )]
    pub fn icon_md(&self) -> Pixels {
        px((17. * self.ui_scale).round())
    }

    #[expect(
        dead_code,
        reason = "Design-token scale helpers retained for UI components"
    )]
    pub fn icon_lg(&self) -> Pixels {
        px((20. * self.ui_scale).round())
    }

    fn base(mode: Mode, translucent: bool) -> Self {
        match mode {
            // hierarquia (protótipo v5, linhas 584–609): `glass = true` vs `glass = false`
            Mode::Dark => Self {
                mode,
                translucent,
                ui_scale: 1.0,
                editor_open: false,
                chrome: if translucent {
                    c(0x0e0f11cc)
                } else {
                    c(0x0e0f11ff)
                },
                rail: if translucent {
                    c(0x00000014)
                } else {
                    c(0x0b0c0eff)
                },
                sidebar: if translucent {
                    c(0xffffff05)
                } else {
                    c(0x121316ff)
                },
                surface: if translucent {
                    c(0x00000026)
                } else {
                    c(0x0a0b0cff)
                },
                editor: if translucent {
                    c(0x00000040)
                } else {
                    c(0x0a0b0cff)
                },
                grid: c(0xffffff16),
                card: if translucent {
                    c(0x18191ccc)
                } else {
                    c(0x17181bff)
                },
                island: c(0x1c1d20db),
                edge: c(0xffffff1f),
                edge_soft: c(0xffffff12),
                hover: c(0xffffff0f),
                active: c(0xffffff17),
                selected: c(0xe567312e),
                sel: c(0xffffff17),
                input: c(0xffffff0d),
                input_solid: c(0x0000004d),
                swatch_ring: c(0x121316ff),
                link: c(0x589aedff),
                idx: c(0x00b1baff),
                rel_stroke: c(0xdce1eb52),
                rel_line: c(0xdce1eb6b),
                syn_kw: c(0x7cb4fcff),
                syn_name: c(0xececeeff),
                syn_type: c(0x64d1d7ff),
                edge_strong: c(0xffffff1f),
                modal: c(0x18191cf0),
                scrim: c(0x00000066),
                syn_attr: c(0xe39bdcff),
                syn_num: c(0xf6b669ff),
                syn_comment: c(0x5d6067ff),
                syn_punct: c(0x8b8e95ff),
                text: c(0xececeeff),
                text_muted: c(0xa3a5abff),
                text_faint: c(0x858890ff),
                accent: c(0xe56731ff),
                danger: c(0xed4a49ff),
                warning: c(0xeba941ff),
                ok: c(0x3eab5eff),
            },
            Mode::Light => Self {
                mode,
                translucent,
                ui_scale: 1.0,
                editor_open: false,
                chrome: if translucent {
                    c(0xf6f6f4cc)
                } else {
                    c(0xf3f3f1ff)
                },
                rail: if translucent {
                    c(0x0000000a)
                } else {
                    c(0xececeaff)
                },
                sidebar: if translucent {
                    c(0xffffff14)
                } else {
                    c(0xf7f7f5ff)
                },
                surface: if translucent {
                    c(0xffffff40)
                } else {
                    c(0xfbfbfaff)
                },
                editor: if translucent {
                    c(0xffffff66)
                } else {
                    c(0xffffffff)
                },
                grid: c(0x0000001f),
                card: if translucent { c(0xffffffcc) } else { c(0xffffffff) },
                island: c(0xffffffdb),
                edge: c(0x14141e1f),
                edge_soft: c(0x14141e14),
                hover: c(0x0000000d),
                active: c(0x00000017),
                selected: c(0xe567311f),
                sel: c(0xffffffff),
                input: if translucent {
                    c(0x0000000b)
                } else {
                    c(0xebebe8ff)
                },
                input_solid: c(0xffffffff),
                swatch_ring: c(0xf6f6f4ff),
                link: c(0x589aedff),
                idx: c(0x00b1baff),
                rel_stroke: c(0x1e1e284d),
                rel_line: c(0x1e1e286b),
                syn_kw: c(0x2663c9ff),
                syn_name: c(0x17181bff),
                syn_type: c(0x007d86ff),
                edge_strong: c(0x14141e21),
                modal: c(0xfcfcfbf0),
                scrim: c(0x1e1e2824),
                syn_attr: c(0xa04c9aff),
                syn_num: c(0xab5900ff),
                syn_comment: c(0x9a9ca2ff),
                syn_punct: c(0x6b6e75ff),
                text: c(0x17181bff),
                text_muted: c(0x55585fff),
                text_faint: c(0x6b6e75ff),
                accent: c(0xe56731ff),
                danger: c(0xed4a49ff),
                warning: c(0xb97a12ff),
                ok: c(0x3eab5eff),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Glass, Mode};

    #[test]
    fn four_themes_expose_distinct_tokens_and_names() {
        let light = Glass::of(Mode::Light, false);
        let light_tr = Glass::of(Mode::Light, true);
        let dark = Glass::of(Mode::Dark, false);
        let dark_tr = Glass::of(Mode::Dark, true);

        assert_eq!(light.theme_name(), "Claro");
        assert_eq!(light_tr.theme_name(), "Claro");
        assert_eq!(dark.theme_name(), "Escuro");
        assert_eq!(dark_tr.theme_name(), "Escuro");

        assert_eq!(light.chrome.a, 1.0);
        assert!(light_tr.chrome.a < 1.0);
        assert_ne!(light.input, light_tr.input);
        assert_ne!(light.surface, light_tr.surface);
        assert_ne!(light.editor, light_tr.editor);

        assert_eq!(dark.chrome.a, 1.0);
        assert!(dark_tr.chrome.a < 1.0);
        assert_ne!(dark.card, dark_tr.card);
        assert_ne!(dark.surface, dark_tr.surface);
        assert_ne!(dark.editor, dark_tr.editor);
    }
}
