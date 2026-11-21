//! Decisões puras sobre mudanças externas no arquivo aberto (`git pull`, outro editor...).
//!
//! O shell lê o arquivo em segundo plano e passa o resultado para estas funções; assim a regra
//! (recarregar se limpo, avisar sem modal se sujo, não sobrescrever calado ao salvar) é testável
//! sem GPUI.

/// O que fazer quando o disco difere do texto salvo pela última vez.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Watch {
    /// Nada a fazer.
    Idle,
    /// Documento sem edições (ou com o mesmo texto do disco): adota o texto do disco.
    Reload(String),
    /// Documento com edições não salvas: avisa uma vez, sem modal, e preserva o buffer.
    Conflict(String),
}

/// `disk`: conteúdo atual do arquivo (`None` se removido/ilegível); `saved`: texto da última
/// leitura/gravação; `current`: texto do editor; `warned`: conteúdo do disco já avisado.
pub fn watch_decision(
    saved: &str,
    current: &str,
    disk: Option<&str>,
    warned: Option<&str>,
) -> Watch {
    let Some(disk) = disk else { return Watch::Idle };
    if disk == saved {
        return Watch::Idle;
    }
    if current == saved || current == disk {
        return Watch::Reload(disk.to_owned());
    }
    if warned == Some(disk) {
        return Watch::Idle;
    }
    Watch::Conflict(disk.to_owned())
}

/// Salvar manualmente só passa direto se o disco ainda é o que o usuário abriu. Se mudou, o
/// primeiro Salvar avisa e "arma" aquele conteúdo; repetir o Salvar sobre o mesmo conteúdo confirma.
pub fn save_allowed(saved: &str, disk: Option<&str>, armed: Option<&str>) -> bool {
    match disk {
        None => true,
        Some(d) => d == saved || armed == Some(d),
    }
}

/// Lê o arquivo para comparação; `None` se não existe, não é legível ou não é UTF-8.
pub fn read_disk(path: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_or_missing_disk_is_idle() {
        assert_eq!(watch_decision("a", "a", Some("a"), None), Watch::Idle);
        assert_eq!(watch_decision("a", "edit", Some("a"), None), Watch::Idle);
        assert_eq!(watch_decision("a", "a", None, None), Watch::Idle);
    }

    #[test]
    fn clean_document_reloads_external_change() {
        assert_eq!(
            watch_decision("a", "a", Some("b"), None),
            Watch::Reload("b".into())
        );
    }

    #[test]
    fn dirty_document_raises_conflict_once() {
        assert_eq!(
            watch_decision("a", "edit", Some("b"), None),
            Watch::Conflict("b".into())
        );
        assert_eq!(
            watch_decision("a", "edit", Some("b"), Some("b")),
            Watch::Idle
        );
        // outra mudança externa depois do aviso: avisa de novo
        assert_eq!(
            watch_decision("a", "edit", Some("c"), Some("b")),
            Watch::Conflict("c".into())
        );
    }

    #[test]
    fn edit_equal_to_disk_adopts_disk() {
        assert_eq!(
            watch_decision("a", "b", Some("b"), None),
            Watch::Reload("b".into())
        );
    }

    #[test]
    fn save_needs_confirmation_when_disk_changed() {
        assert!(save_allowed("a", Some("a"), None));
        assert!(save_allowed("a", None, None));
        assert!(!save_allowed("a", Some("b"), None));
        assert!(!save_allowed("a", Some("c"), Some("b")));
        assert!(save_allowed("a", Some("b"), Some("b")));
    }

    #[test]
    fn read_disk_returns_none_for_missing_file() {
        assert_eq!(
            read_disk(std::path::Path::new("/nonexistent/dbm-nope.dbml")),
            None
        );
    }
}
