#![cfg_attr(windows, windows_subsystem = "windows")]

//! DB Modeling: visual database modeling with DBML ↔ canvas.

mod actions;
mod canvas;
mod canvas_geom;
mod glass;
mod ide;
mod platform;
mod session;
mod settings;
mod shell;
mod widgets;

use glass::Mode;
use gpui_kit::*;
use std::borrow::Cow;
use std::path::PathBuf;

const FONTS: [&[u8]; 4] = [
    include_bytes!("../assets/fonts/Inter-Light.ttf"),
    include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
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
    if std::env::args().any(|a| a == "--version" || a == "-V" || a == "-v") {
        println!("db-modeling {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if std::env::args().any(|a| a == "--help" || a == "-h") {
        println!("DB Modeling {}", env!("CARGO_PKG_VERSION"));
        println!("Visual database schema modeling on the canvas or in DBML text\n");
        println!("Usage: db-modeling [FILE.dbml | FOLDER]\n");
        println!("Options:");
        println!("  -v, -V, --version   Show application version");
        println!("  -h, --help          Show this help message");
        return;
    }
    gpui_kit::application().with_assets(AppAssets).run(|cx| {
        gpui_kit::init(cx);
        cx.text_system()
            .add_fonts(FONTS.iter().map(|f| Cow::Borrowed(*f)).collect())
            .expect("valid embedded fonts");
        cx.bind_keys(actions::catalog().into_iter().filter_map(|e| e.binding));
        let (background, translucent) = platform::window_background();
        // DBM_SIZE=LxA abre a janela nesse tamanho (harness de screenshot do responsivo)
        let bounds = std::env::var("DBM_SIZE").ok().and_then(|v| {
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
                title: Some("DB Modeling".into()),
                appears_transparent: cfg!(windows),
                ..Default::default()
            }),
            ..Default::default()
        };
        let (win, shell) = gpui_kit::open_window(opts, cx, |window, cx| {
            // tema: DBM_MODE (harness de screenshot) > preferência salva > sistema
            let prefs = settings::load();
            let mode = match std::env::var("DBM_MODE").as_deref() {
                Ok("light") => Mode::Light,
                Ok("dark") => Mode::Dark,
                _ => prefs.mode.unwrap_or(match window.appearance() {
                    WindowAppearance::Light | WindowAppearance::VibrantLight => Mode::Light,
                    _ => Mode::Dark,
                }),
            };
            shell::apply_theme_scaled(mode, prefs.ui_scale, cx);
            // `app arquivo.dbml` abre o arquivo; `app pasta/` abre o projeto; sem argumento, o exemplo
            let start = std::env::args().nth(1).map(PathBuf::from);
            let shell = cx.new(|cx| shell::Shell::new(start, mode, translucent, window, cx));
            let focus = shell.focus_handle(cx);
            window.focus(&focus, cx);
            shell
        })
        .expect("failed to open window");
        shell::register_global_actions(win, &shell, cx);
    });
}
