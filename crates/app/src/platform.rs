//! Fundo da janela por plataforma (vidro só onde o sistema desfoca de verdade).

use gpui_kit::WindowBackgroundAppearance;

/// Número de build do Windows (ex.: 22631), via `RtlGetVersion` (não mente por compatibilidade).
#[cfg(windows)]
fn windows_build() -> u32 {
    #[repr(C)]
    struct OsVersionInfoW {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform: u32,
        csd: [u16; 128],
    }
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn RtlGetVersion(info: *mut OsVersionInfoW) -> i32;
    }
    let mut info = OsVersionInfoW {
        size: size_of::<OsVersionInfoW>() as u32,
        major: 0,
        minor: 0,
        build: 0,
        platform: 0,
        csd: [0; 128],
    };
    // SAFETY: estrutura com o tamanho declarado em `size`, como a API exige.
    if unsafe { RtlGetVersion(&mut info) } == 0 {
        info.build
    } else {
        0
    }
}

/// Indica se a plataforma/compositor suporta desfoque de fundo da janela (padrão de translucidez).
pub fn supports_blur() -> bool {
    #[cfg(any(windows, target_os = "macos"))]
    {
        true
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        std::env::var_os("WAYLAND_DISPLAY").is_some()
            && std::env::var("XDG_CURRENT_DESKTOP")
                .is_ok_and(|d| d.to_ascii_uppercase().contains("KDE"))
    }
}

/// Aparência de fundo da janela conforme a escolha de tema translúcido:
/// - `translucent = false`: sempre `Opaque`.
/// - `translucent = true`:
///   - Windows / macOS / KDE Wayland: `Blurred`.
///   - Demais sistemas (Linux X11 / GNOME): `Transparent` (sem desfoque).
pub fn window_background(translucent: bool) -> WindowBackgroundAppearance {
    if !translucent {
        return WindowBackgroundAppearance::Opaque;
    }
    // acrílico (desfoca o que está atrás da janela); Mica só tinge com o papel de parede
    #[cfg(windows)]
    {
        WindowBackgroundAppearance::Blurred
    }
    #[cfg(target_os = "macos")]
    {
        WindowBackgroundAppearance::Blurred
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        if supports_blur() {
            WindowBackgroundAppearance::Blurred
        } else {
            WindowBackgroundAppearance::Transparent
        }
    }
}

/// Fonte dos glifos dos botões de janela no Windows (Fluent no 11, MDL2 no 10).
#[cfg(windows)]
pub fn caption_font() -> &'static str {
    if windows_build() >= 22000 {
        "Segoe Fluent Icons"
    } else {
        "Segoe MDL2 Assets"
    }
}
