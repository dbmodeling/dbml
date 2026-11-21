//! Falhas fatais na inicialização (sem GPU utilizável, fontes inválidas, janela que não abre) e
//! panics: gravam log, mostram diálogo nativo com a causa e encerram com código diferente de zero.
//! No Windows (`windows_subsystem = "windows"`) não há console, então o diálogo é a única pista.

use std::path::PathBuf;
use std::process::Command;

/// Arquivo de log de falhas, na pasta de configuração do app.
pub fn log_path() -> Option<PathBuf> {
    crate::session::config_dir().map(|d| d.join("startup-error.log"))
}

/// Linha de log: instante Unix e mensagem em uma linha só.
pub fn log_line(at: u64, message: &str) -> String {
    format!("[{at}] {}\n", message.replace('\n', " | "))
}

fn append_log(message: &str) {
    use std::io::Write;
    let Some(path) = log_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(log_line(crate::session::now_secs(), message).as_bytes());
    }
}

/// Comandos candidatos (programa, argumentos) para mostrar um diálogo de erro nativo, em ordem
/// de preferência. `os` segue `std::env::consts::OS`.
pub fn dialog_candidates(os: &str, title: &str, message: &str) -> Vec<(&'static str, Vec<String>)> {
    match os {
        "macos" => {
            let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
            vec![(
                "osascript",
                vec![
                    "-e".into(),
                    format!("display alert \"{}\" message \"{}\" as critical", esc(title), esc(message)),
                ],
            )]
        }
        "windows" => {
            let esc = |s: &str| s.replace('\'', "''");
            vec![(
                "powershell",
                vec![
                    "-NoProfile".into(),
                    "-Command".into(),
                    format!(
                        "Add-Type -AssemblyName PresentationFramework; [void][System.Windows.MessageBox]::Show('{}','{}','OK','Error')",
                        esc(message),
                        esc(title)
                    ),
                ],
            )]
        }
        _ => vec![
            ("zenity", vec!["--error".into(), "--title".into(), title.into(), "--text".into(), message.into()]),
            ("kdialog", vec!["--error".into(), message.into(), "--title".into(), title.into()]),
            ("xmessage", vec!["-center".into(), format!("{title}: {message}")]),
        ],
    }
}

fn show_dialog(title: &str, message: &str) {
    for (program, args) in dialog_candidates(std::env::consts::OS, title, message) {
        let mut cmd = Command::new(program);
        cmd.args(&args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        if cmd.status().is_ok() {
            return;
        }
    }
}

/// Registra a falha (stderr + log), avisa o usuário com a causa e sai com código 1.
pub fn report(message: &str) -> ! {
    eprintln!("{message}");
    append_log(message);
    show_dialog(
        crate::i18n::t("DB Modeling não conseguiu iniciar"),
        &format!(
            "{message}\n\n{}",
            crate::i18n::t("Verifique o driver de vídeo (Vulkan/OpenGL no Linux, DirectX no Windows, Metal no macOS) e abra o app de novo.")
        ),
    );
    std::process::exit(1);
}

/// Panics também vão para o log; em release (`panic = "abort"`) o diálogo avisa antes do abort.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = format!("panic: {info}");
        append_log(&message);
        // abas com edições não salvas viram cópias de recuperação antes do abort
        crate::recovery::dump_live_for_panic();
        if !cfg!(debug_assertions) {
            show_dialog(crate::i18n::t("DB Modeling encerrou com um erro"), &message);
        }
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_line_is_single_line_with_timestamp() {
        assert_eq!(log_line(5, "a\nb"), "[5] a | b\n");
    }

    #[test]
    fn linux_dialog_tries_zenity_then_kdialog_then_xmessage() {
        let c = dialog_candidates("linux", "T", "M");
        let names: Vec<_> = c.iter().map(|(p, _)| *p).collect();
        assert_eq!(names, ["zenity", "kdialog", "xmessage"]);
        assert!(c[0].1.contains(&"M".to_string()));
    }

    #[test]
    fn macos_dialog_escapes_quotes() {
        let c = dialog_candidates("macos", "T", "say \"hi\"");
        assert_eq!(c[0].0, "osascript");
        assert!(c[0].1[1].contains("say \\\"hi\\\""));
    }

    #[test]
    fn windows_dialog_escapes_single_quotes() {
        let c = dialog_candidates("windows", "T", "it's");
        assert_eq!(c[0].0, "powershell");
        assert!(c[0].1[2].contains("'it''s'"));
    }
}
