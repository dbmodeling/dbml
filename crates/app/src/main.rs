#![cfg_attr(windows, windows_subsystem = "windows")]

//! DB Modeling: visual database modeling with DBML ↔ canvas.

mod actions;
mod canvas;
mod canvas_geom;
mod connect_db;
mod disk_sync;
mod fatal;
mod glass;
mod harness;
mod history;
mod ide;
mod i18n;
mod import_sql;
mod motion;
mod platform;
mod session;
mod settings;
mod shell;

use glass::Mode;
use gpui_kit::*;
use std::borrow::Cow;
use std::path::PathBuf;

/// Identificador da app no Linux (Wayland `app_id` / X11 `WM_CLASS`). Deve ser igual ao
/// nome do `.desktop` instalado pelos pacotes (`scripts/build-deb.sh`, `scripts/build-fedora.sh`).
const APP_ID: &str = "db-modeling";

pub(crate) const FONTS: [&[u8]; 5] = [
    include_bytes!("../assets/fonts/Geist-Regular.ttf"),
    include_bytes!("../assets/fonts/Geist-Medium.ttf"),
    include_bytes!("../assets/fonts/Geist-SemiBold.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Regular.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Medium.ttf"),
];

/// Assets do app: arquivos da marca + catálogo de ícones do GPUI Kit.
struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let brand: Option<&'static [u8]> = match path {
            "brand/icon-64.png" => Some(include_bytes!("../assets/brand/icon-64.png")),
            "brand/mark.svg" => Some(include_bytes!("../assets/brand/mark.svg")),
            _ => None,
        };
        match brand {
            Some(b) => Ok(Some(Cow::Borrowed(b))),
            None => gpui_kit::assets::AllAssets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        gpui_kit::assets::AllAssets.list(path)
    }
}

fn main() {
    fatal::install_panic_hook();
    i18n::set(std::env::var("DBM_LANG").ok().and_then(|v| i18n::Lang::parse(&v)).or_else(|| settings::load().lang).unwrap_or_else(i18n::system));
    if std::env::args().any(|a| a == "--version" || a == "-V" || a == "-v") {
        println!("db-modeling {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if std::env::args().any(|a| a == "--help" || a == "-h") {
        println!("dbmodeling {}", env!("CARGO_PKG_VERSION"));
        println!("{}", i18n::t("Modelagem visual de esquemas de banco de dados no canvas ou em texto DBML\n"));
        println!("{}", i18n::t("Uso: db-modeling [ARQUIVO.dbml | PASTA]\n"));
        println!("{}", i18n::t("Opções:"));
        println!("{}", i18n::t("  -v, -V, --version   Mostra a versão do aplicativo"));
        println!("{}", i18n::t("  -h, --help          Mostra esta mensagem de ajuda"));
        return;
    }
    gpui_kit::application().with_assets(AppAssets).run(|cx| {
        gpui_kit::init(cx);
        if motion::disabled() {
            cx.set_reduce_motion(true);
        }
        cx.text_system()
            .add_fonts(FONTS.iter().map(|f| Cow::Borrowed(*f)).collect())
            .unwrap_or_else(|e| fatal::report(&format!("{}: {e}", i18n::t("Não foi possível carregar as fontes embutidas"))));
        cx.bind_keys(actions::catalog().into_iter().filter_map(|e| e.binding));
        let prefs = settings::load();
        i18n::set(std::env::var("DBM_LANG").ok().and_then(|v| i18n::Lang::parse(&v)).or(prefs.lang).unwrap_or_else(i18n::system));
        let env_mode = match crate::harness::var("DBM_MODE").as_deref() {
            Some("light" | "light-translucent") => Some(Mode::Light),
            Some("dark" | "dark-translucent") => Some(Mode::Dark),
            _ => None,
        };
        // transparência da janela desativada (decisão de produto, 2026-11-16): sempre opaca
        let translucent = false;
        let background = platform::window_background(translucent);
        // DBM_SIZE=LxA abre a janela nesse tamanho (harness de screenshot do responsivo)
        let bounds = crate::harness::var("DBM_SIZE").and_then(|v| {
            let (w, h) = v.split_once('x')?;
            let sz = size(px(w.parse().ok()?), px(h.parse().ok()?));
            Some(WindowBounds::Windowed(Bounds::centered(None, sz, cx)))
        });
        let opts = WindowOptions {
            window_bounds: bounds,
            window_background: background,
            window_min_size: Some(size(px(640.), px(420.))),
            // Barra de título própria só no Windows (legenda desenhada pelo app).
            titlebar: Some(TitlebarOptions {
                title: Some("dbmodeling".into()),
                appears_transparent: cfg!(windows),
                ..Default::default()
            }),
            // Wayland: app_id precisa casar com `db-modeling.desktop` (e com
            // `StartupWMClass`), senão o GNOME/KDE mostra "Unknown" e ícone genérico.
            app_id: Some(APP_ID.into()),
            ..Default::default()
        };
        let (win, shell) = gpui_kit::open_window(opts, cx, |window, cx| {
            // tema: DBM_MODE (harness de screenshot) > preferência salva > sistema
            let mode = env_mode.or(prefs.mode).unwrap_or(match window.appearance() {
                WindowAppearance::Light | WindowAppearance::VibrantLight => Mode::Light,
                _ => Mode::Dark,
            });
            shell::apply_theme_scaled(mode, translucent, prefs.ui_scale, cx);
            // `app arquivo.dbml` abre o arquivo; `app pasta/` abre o projeto; sem argumento, o exemplo
            let start = std::env::args().nth(1).map(PathBuf::from);
            let shell = cx.new(|cx| shell::Shell::new(start, mode, translucent, window, cx));
            let focus = shell.focus_handle(cx);
            window.focus(&focus, cx);
            // X11: WM_CLASS só é definido via set_app_id (WindowOptions.app_id é ignorado lá)
            window.set_app_id(APP_ID);
            shell
        })
        .unwrap_or_else(|e| fatal::report(&format!("{}: {e}", i18n::t("Não foi possível abrir a janela (sem backend de GPU utilizável?)"))));
        shell::register_global_actions(win, &shell, cx);
    });
}
