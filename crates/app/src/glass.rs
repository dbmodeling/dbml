//! Camada de tema: única origem de cores cruas do app, mais as escalas de raio,
//! espaçamento e tamanhos de chrome. Telas só consomem `Glass`, nunca `rgba` direto.
//!
//! Direção visual (2026-10-25, inspirada em ngDiagram e MonoCode): neutro, escuro
//! profundo, bordas discretas, cartões com raio maior e sombra suave, seleção com
//! anel azul; vidro só nas ilhas flutuantes do canvas.

use gpui_kit::{BoxShadow, Hsla, Pixels, point, px, rgba};

/// Raios: cartões e ilhas; controles usam o raio do tema do gpui-component.
pub const RADIUS_CARD: Pixels = px(12.);
pub const RADIUS_ISLAND: Pixels = px(10.);

/// Escala de espaçamento (sm, md, lg).
pub const SP_SM: Pixels = px(8.);
pub const SP_MD: Pixels = px(12.);
pub const SP_LG: Pixels = px(16.);

/// Escala tipográfica (Inter): legenda, secundário, corpo, título de painel.
pub const TEXT_XS: Pixels = px(12.);
pub const TEXT_SM: Pixels = px(13.5);
pub const TEXT_BASE: Pixels = px(14.5);
pub const TEXT_LG: Pixels = px(16.5);

pub const TITLEBAR_H: Pixels = px(38.);
pub const STATUSBAR_H: Pixels = px(24.);
pub const RAIL_W: Pixels = px(44.);
pub const SIDEBAR_W: Pixels = px(260.);
pub const INSPECTOR_W: Pixels = px(300.);
pub const EDITOR_W: Pixels = px(460.);

/// Sublinhado de diagnósticos no editor (tons médios, legíveis no claro e no escuro).
pub const DIAG_ERROR: u32 = 0xe5484dff;
pub const DIAG_WARNING: u32 = 0xd99a2bff;
pub const DIAG_INFO: u32 = 0x6b8fd6ff;

/// Botão Fechar da janela no Windows (vermelho do sistema).
#[cfg_attr(not(windows), allow(dead_code))]
pub const CAPTION_CLOSE: Hsla = Hsla {
    h: 356. / 360.,
    s: 0.86,
    l: 0.49,
    a: 1.,
};

/// Fonte da interface, embarcada em `assets/fonts` (Inter, licença OFL).
pub const FONT: &str = "Inter";

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
    ("Blue", "#3b82f6"),
    ("Cyan", "#06b6d4"),
    ("Green", "#22c55e"),
    ("Lime", "#84cc16"),
    ("Amber", "#f59e0b"),
    ("Orange", "#f97316"),
    ("Pink", "#ec4899"),
    ("Violet", "#8b5cf6"),
];

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
    pub ui_scale: f32,
    /// Barra de título, barra de atividades e barra de status.
    pub chrome: Hsla,
    /// Barra lateral e inspector.
    pub sidebar: Hsla,
    /// Área de trabalho (canvas, editor).
    pub surface: Hsla,
    /// Pontos da grade do canvas.
    pub grid: Hsla,
    pub card: Hsla,
    /// Ilhas flutuantes (vidro): fundo translúcido + realce na borda superior.
    pub island: Hsla,
    pub island_sheen: Hsla,
    /// Bordas de 1px (contorno de controles e cartões).
    pub edge: Hsla,
    /// Divisórias internas, mais discretas que `edge`.
    pub edge_soft: Hsla,
    /// Fundo "ghost" no hover; `active` é o pressionado (mais forte).
    pub hover: Hsla,
    pub active: Hsla,
    pub selected: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_faint: Hsla,
    pub accent: Hsla,
    pub danger: Hsla,
    pub warning: Hsla,
    pub ok: Hsla,
}

fn c(v: u32) -> Hsla {
    rgba(v).into()
}

impl Glass {
    /// `translucent`: chrome e laterais com alfa sobre o blur da janela. Só faz sentido
    /// onde o sistema desfoca o fundo (Windows); sem blur, tudo opaco para não "sujar".
    #[expect(
        dead_code,
        reason = "Design-token scale helpers retained for UI components"
    )]
    pub fn of(mode: Mode, translucent: bool) -> Self {
        Self::of_scaled(mode, translucent, 1.0)
    }

    pub fn of_scaled(mode: Mode, translucent: bool, ui_scale: f32) -> Self {
        let mut g = Self::base(mode);
        g.ui_scale = ui_scale;
        if translucent {
            let a = if mode == Mode::Dark { 0.62 } else { 0.76 };
            g.chrome = g.chrome.opacity(a);
            g.sidebar = g.sidebar.opacity(a);
        }
        g.selected = g
            .accent
            .opacity(if mode == Mode::Dark { 0.18 } else { 0.12 });
        g
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

    fn base(mode: Mode) -> Self {
        match mode {
            // hierarquia (como no Zed): chrome mais claro > laterais > área de trabalho;
            // a aba ativa tem a cor da área de trabalho e "funde" com o conteúdo
            Mode::Dark => Self {
                mode,
                ui_scale: 1.0,
                chrome: c(0x131314ff),
                sidebar: c(0x101011ff),
                surface: c(0x0a0a0bff),
                grid: c(0xffffff12),
                card: c(0x151516ff),
                island: c(0x161617e6),
                island_sheen: c(0xffffff24),
                edge: c(0xffffff1a),
                edge_soft: c(0xffffff0c),
                hover: c(0xffffff0f),
                active: c(0xffffff1a),
                selected: c(0xc6f2402e),
                text: c(0xececedff),
                text_muted: c(0xa3a6adff),
                text_faint: c(0x72757dff),
                accent: c(0xc6f240ff),
                danger: c(0xf2555aff),
                warning: c(0xe5b454ff),
                ok: c(0x4cc27aff),
            },
            Mode::Light => Self {
                mode,
                ui_scale: 1.0,
                chrome: c(0xe4e6eaff),
                sidebar: c(0xeef0f3ff),
                surface: c(0xf7f7f8ff),
                grid: c(0x1f232822),
                card: c(0xffffffff),
                island: c(0xfffffff2),
                island_sheen: c(0xffffffff),
                edge: c(0x1f232821),
                edge_soft: c(0x1f232814),
                hover: c(0x1f23280d),
                active: c(0x1f232817),
                selected: c(0x5f7d001f),
                text: c(0x1f2328ff),
                text_muted: c(0x57606aff),
                text_faint: c(0x848d97ff),
                accent: c(0x5f7d00ff),
                danger: c(0xcf222eff),
                warning: c(0x9a6700ff),
                ok: c(0x1a7f37ff),
            },
        }
    }
}
