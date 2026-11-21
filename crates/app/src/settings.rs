//! Preferências do usuário (tema, larguras dos painéis), em texto
//! simples `chave=valor` no diretório de configuração do sistema.

use crate::glass::{EDITOR_W, INSPECTOR_W, Mode, SIDEBAR_W};
use std::path::PathBuf;

#[derive(Clone, Copy)]
pub struct Settings {
    pub mode: Option<Mode>,
    pub lang: Option<crate::i18n::Lang>,
    pub translucent: Option<bool>,
    pub sidebar_w: f32,
    pub editor_w: f32,
    pub inspector_w: f32,
    pub col_type_w: f32,
    pub ui_scale: f32,
    pub inspector_auto_hide: bool,
    pub sidebar_open: bool,
    pub inspector_open: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: None,
            lang: None,
            translucent: None,
            sidebar_w: f32::from(SIDEBAR_W),
            editor_w: f32::from(EDITOR_W),
            inspector_w: f32::from(INSPECTOR_W),
            col_type_w: 110.0,
            ui_scale: 1.0,
            inspector_auto_hide: true,
            sidebar_open: false,
            inspector_open: true,
        }
    }
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("db-modeling").join("settings.txt"))
}

/// Larguras fora de faixa (arquivo editado à mão/corrompido) voltam ao padrão.
fn width(v: &str, default: f32) -> f32 {
    v.trim()
        .parse::<f32>()
        .ok()
        .filter(|w| (120.0..=1600.0).contains(w))
        .unwrap_or(default)
}

pub fn load() -> Settings {
    let Some(text) = path().and_then(|p| std::fs::read_to_string(p).ok()) else {
        return Settings::default();
    };
    parse(&text)
}

fn parse(text: &str) -> Settings {
    let mut s = Settings::default();
    let d = Settings::default();
    for line in text.lines() {
        match line.split_once('=') {
            Some(("lang", v)) => s.lang = crate::i18n::Lang::parse(v),
            Some(("mode", "dark")) => s.mode = Some(Mode::Dark),
            Some(("mode", "light")) => s.mode = Some(Mode::Light),
            Some(("translucent", "true")) => s.translucent = Some(true),
            Some(("translucent", "false")) => s.translucent = Some(false),
            Some(("sidebar_w", v)) => s.sidebar_w = width(v, d.sidebar_w),
            Some(("editor_w", v)) => s.editor_w = width(v, d.editor_w),
            Some(("inspector_w", v)) => s.inspector_w = width(v, d.inspector_w),
            Some(("col_type_w", v)) => {
                s.col_type_w = v
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|w| (60.0..=400.0).contains(w))
                    .unwrap_or(d.col_type_w);
            }
            Some(("ui_scale", v)) => {
                s.ui_scale = v
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|scale| (0.75..=2.0).contains(scale))
                    .unwrap_or(1.0);
            }
            Some(("inspector_auto_hide", v)) => {
                s.inspector_auto_hide = v.trim() != "false";
            }
            Some(("sidebar_open", v)) => {
                s.sidebar_open = v.trim() == "true";
            }
            Some(("inspector_open", v)) => {
                s.inspector_open = v.trim() == "true";
            }
            _ => {}
        }
    }
    s
}

fn serialize(s: &Settings) -> String {
    let mode = match s.mode {
        Some(Mode::Light) => "light",
        _ => "dark",
    };
    let translucent = s
        .translucent
        .unwrap_or_else(crate::platform::supports_blur);
    let lang = s.lang.map_or_else(String::new, |lang| format!("lang={}\n", lang.as_str()));
    format!(
        "{lang}mode={mode}\ntranslucent={translucent}\nsidebar_w={}\neditor_w={}\ninspector_w={}\ncol_type_w={}\nui_scale={:.2}\ninspector_auto_hide={}\nsidebar_open={}\ninspector_open={}\n",
        s.sidebar_w.round(),
        s.editor_w.round(),
        s.inspector_w.round(),
        s.col_type_w.round(),
        s.ui_scale,
        s.inspector_auto_hide,
        s.sidebar_open,
        s.inspector_open,
    )
}

/// Falha ao salvar só perde a preferência; não interrompe o app.
pub fn save(s: &Settings) {
    let Some(p) = path() else { return };
    let body = serialize(s);
    let _ = crate::session::atomic_write_sync(&p, &body);
}

#[cfg(test)]
mod tests {
    use super::{Mode, Settings, parse, serialize};

    #[test]
    fn language_is_optional_and_roundtrips() {
        use crate::i18n::Lang;
        assert_eq!(parse("mode=light\n").lang, None);
        assert!(!serialize(&Settings::default()).contains("lang="));
        assert_eq!(parse(&serialize(&Settings { lang: Some(Lang::PtBr), ..Settings::default() })).lang, Some(Lang::PtBr));
        assert_eq!(parse("lang=invalid\n").lang, None);
    }

    #[test]
    fn old_settings_without_translucent_keep_none_and_roundtrip_saves_it() {
        let old = "mode=light\nsidebar_w=260\ntranslucent_cards=true\n";
        let parsed = parse(old);
        assert!(parsed.mode == Some(Mode::Light));
        assert_eq!(parsed.translucent, None);
        assert_eq!(parsed.sidebar_w, 260.0);

        let saved = serialize(&Settings {
            mode: Some(Mode::Dark),
            translucent: Some(true),
            ..Settings::default()
        });
        let again = parse(&saved);
        assert!(again.mode == Some(Mode::Dark));
        assert_eq!(again.translucent, Some(true));
        assert!(!saved.contains("translucent_cards"));
    }
}
