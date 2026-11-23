//! Modelo de visão do controle de atualização da title bar (puro, sem GPUI).
//!
//! Traduz o estado do updater (`UpdateStatus`) e os avisos do controlador (`UpdateNotice`) no
//! que o controle único da title bar mostra, e como ele se comporta (ação ao ativar, rótulo
//! acessível, tempo de exibição). A ligação GPUI (`shell/panels.rs`) só desenha o que este
//! módulo decide.
//!
//! Textos ficam em pt-BR como chave de `i18n::t` (o inglês está em `i18n.rs`).

use std::time::Duration;

use updater::constants::UP_TO_DATE_VISIBLE_FOR;
use updater::policy::DisabledReason;
use updater::state::UpdateStatus;

/// Aviso do controlador que não é um estado da máquina de estados do updater.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateNotice {
    /// Atualização desativada: o texto aparece no lugar da ação, com link para a Release.
    Disabled(DisabledText),
    /// Outra janela/instância detém a trava e já está atualizando.
    AnotherInstance,
    /// O app não conseguiu abrir o processo de relançamento; a versão nova segue instalada.
    RestartFailed,
}

/// Texto de desativação: ou do empacotador/usuário (literal) ou uma chave traduzível.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisabledText {
    /// Texto vindo de `DBM_UPDATE_EXPLANATION` ou do arquivo dos pacotes: exibido como está.
    Verbatim(String),
    /// Chave pt-BR de `i18n::t`.
    Key(&'static str),
}

impl DisabledText {
    /// Texto no idioma atual.
    #[must_use]
    pub fn resolve(&self) -> String {
        match self {
            Self::Verbatim(text) => text.clone(),
            Self::Key(key) => crate::i18n::t(key).to_owned(),
        }
    }
}

/// Texto de desativação para cada razão do `updater::policy`.
#[must_use]
pub fn disabled_text(reason: &DisabledReason) -> DisabledText {
    match reason {
        DisabledReason::Env(text) | DisabledReason::File(text) => {
            DisabledText::Verbatim(text.clone())
        }
        DisabledReason::UnsupportedPlatform => DisabledText::Key(KEY_UNSUPPORTED_PLATFORM),
        DisabledReason::UnknownAppLocation => DisabledText::Key(KEY_UNKNOWN_LOCATION),
        DisabledReason::ProgramFiles => DisabledText::Key(KEY_PROGRAM_FILES),
        DisabledReason::AppTranslocation => DisabledText::Key(KEY_TRANSLOCATION),
        DisabledReason::NotWritable => DisabledText::Key(KEY_NOT_WRITABLE),
    }
}

/// Chaves (pt-BR) dos textos do controle; as traduções estão em `i18n.rs`.
pub const KEY_CHECKING: &str = "Verificando atualizações…";
pub const KEY_DOWNLOADING: &str = "Baixando atualização…";
pub const KEY_INSTALLING: &str = "Instalando atualização…";
pub const KEY_UP_TO_DATE: &str = "Atualizado";
pub const KEY_RESTART: &str = "Reiniciar para atualizar";
pub const KEY_RESTART_ARIA: &str = "Reiniciar para atualizar para a versão {}";
pub const KEY_FAILED: &str = "Falha ao atualizar";
pub const KEY_FAILED_ARIA: &str = "Falha ao atualizar. Ativar para tentar de novo";
pub const KEY_RESTART_FAILED: &str = "Falha ao reiniciar. Abra o app novamente";
pub const KEY_ANOTHER_INSTANCE: &str = "Outra janela já está verificando atualizações";
pub const KEY_DISABLED_ARIA: &str = "Atualização desativada: {}. Ativar para ver as Releases";
pub const KEY_UNSUPPORTED_PLATFORM: &str = "Atualização automática indisponível nesta plataforma";
pub const KEY_UNKNOWN_LOCATION: &str =
    "Não foi possível localizar a instalação do app; atualize manualmente";
pub const KEY_PROGRAM_FILES: &str =
    "Instalado para todos os usuários; atualize baixando o instalador novo";
pub const KEY_TRANSLOCATION: &str = "Mova o app para a pasta Aplicativos para atualizar";
pub const KEY_NOT_WRITABLE: &str = "Sem permissão de escrita na pasta do app; atualize manualmente";
/// Linux: o executável não está no layout do tarball (`<prefixo>/db-modeling.app/bin/db-modeling`).
#[cfg(any(target_os = "linux", test))]
pub use crate::constants::KEY_LINUX_NOT_TARBALL;
/// Linux: o tarball está num prefixo de sistema (`/usr`, `/var`...), que só os pacotes mexem.
#[cfg(target_os = "linux")]
pub use crate::constants::KEY_LINUX_SYSTEM_PREFIX;

/// O que o controle da title bar mostra agora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateView {
    /// Nada (o estado normal: o controle não aparece).
    Hidden,
    /// "Verificando atualizações…".
    Checking,
    /// "Baixando atualização…".
    Downloading,
    /// "Instalando atualização…".
    Installing,
    /// "Atualizado" (some em 4 s, só em checagem manual).
    UpToDate,
    /// "Reiniciar para atualizar" (carrega a versão instalada).
    Restart(String),
    /// "Falha ao atualizar" (checagem manual).
    Failed,
    /// "Falha ao reiniciar": o relançamento não abriu.
    RestartFailed,
    /// Razão de desativação no lugar da ação, com link para a Release.
    Disabled(DisabledText),
    /// Outra instância está atualizando.
    AnotherInstance,
}

/// O que ativar o controle (clique, Enter ou Espaço) faz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewAction {
    /// Informativo: não é botão.
    None,
    /// Reiniciar para concluir a atualização.
    Restart,
    /// Verificar de novo (após falha).
    Retry,
    /// Abrir a página das Releases (e dispensar o aviso).
    OpenReleases,
    /// Dispensar o aviso (falha ao reiniciar).
    Dismiss,
}

/// Decide a visão. Operações em andamento têm precedência (a UI nunca esconde o progresso), depois
/// os avisos, depois "reiniciar" (que sobrevive a um `Idle` porque a máquina de estados o mantém
/// em `Updated`).
#[must_use]
pub fn view_of(status: &UpdateStatus, notice: Option<&UpdateNotice>) -> UpdateView {
    match status {
        UpdateStatus::Checking => return UpdateView::Checking,
        UpdateStatus::Downloading(_) => return UpdateView::Downloading,
        UpdateStatus::Installing(_) => return UpdateView::Installing,
        _ => {}
    }
    match notice {
        Some(UpdateNotice::Disabled(text)) => return UpdateView::Disabled(text.clone()),
        Some(UpdateNotice::AnotherInstance) => return UpdateView::AnotherInstance,
        Some(UpdateNotice::RestartFailed) => return UpdateView::RestartFailed,
        None => {}
    }
    match status {
        UpdateStatus::Updated(version) => UpdateView::Restart(version.to_string()),
        UpdateStatus::UpToDate => UpdateView::UpToDate,
        UpdateStatus::Errored => UpdateView::Failed,
        _ => UpdateView::Hidden,
    }
}

impl UpdateView {
    /// Texto visível (já no idioma atual); `None` quando oculto.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        let key = match self {
            Self::Hidden => return None,
            Self::Checking => KEY_CHECKING,
            Self::Downloading => KEY_DOWNLOADING,
            Self::Installing => KEY_INSTALLING,
            Self::UpToDate => KEY_UP_TO_DATE,
            Self::Restart(_) => KEY_RESTART,
            Self::Failed => KEY_FAILED,
            Self::RestartFailed => KEY_RESTART_FAILED,
            Self::AnotherInstance => KEY_ANOTHER_INSTANCE,
            Self::Disabled(text) => return Some(text.resolve()),
        };
        Some(crate::i18n::t(key).to_owned())
    }

    /// Rótulo acessível: o texto visível, com a versão (reiniciar) ou a ação (falha, link).
    #[must_use]
    pub fn aria_label(&self) -> Option<String> {
        match self {
            Self::Restart(version) => Some(crate::i18n::t(KEY_RESTART_ARIA).replace("{}", version)),
            Self::Failed => Some(crate::i18n::t(KEY_FAILED_ARIA).to_owned()),
            Self::Disabled(text) => {
                Some(crate::i18n::t(KEY_DISABLED_ARIA).replace("{}", &text.resolve()))
            }
            other => other.label(),
        }
    }

    /// Ação ao ativar o controle.
    #[must_use]
    pub fn action(&self) -> ViewAction {
        match self {
            Self::Restart(_) => ViewAction::Restart,
            Self::Failed => ViewAction::Retry,
            Self::Disabled(_) => ViewAction::OpenReleases,
            Self::RestartFailed => ViewAction::Dismiss,
            _ => ViewAction::None,
        }
    }

    /// Tempo até o controle sumir sozinho; `None` = fica até o usuário agir ou o estado mudar.
    /// "Atualizado" some em 4 s; "outra janela já verifica" também (é só um aviso passageiro).
    /// A razão de desativação e as falhas ficam até o usuário agir.
    #[must_use]
    pub fn visible_for(&self) -> Option<Duration> {
        matches!(self, Self::UpToDate | Self::AnotherInstance).then_some(UP_TO_DATE_VISIBLE_FOR)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use updater::version::parse_version;

    macro_rules! v {
        ($text:expr) => {
            parse_version($text).expect("versão de teste")
        };
    }

    #[test]
    fn repouso_sem_aviso_esconde_o_controle() {
        assert_eq!(view_of(&UpdateStatus::Idle, None), UpdateView::Hidden);
        assert_eq!(UpdateView::Hidden.label(), None);
    }

    #[test]
    fn cada_estado_tem_o_texto_do_spec() {
        let label = |status: UpdateStatus| view_of(&status, None).label();
        assert_eq!(
            label(UpdateStatus::Checking).as_deref(),
            Some("Verificando atualizações…")
        );
        assert_eq!(
            label(UpdateStatus::Downloading(v!("0.4.0"))).as_deref(),
            Some("Baixando atualização…")
        );
        assert_eq!(
            label(UpdateStatus::Installing(v!("0.4.0"))).as_deref(),
            Some("Instalando atualização…")
        );
        assert_eq!(label(UpdateStatus::UpToDate).as_deref(), Some("Atualizado"));
        assert_eq!(
            label(UpdateStatus::Updated(v!("0.4.0"))).as_deref(),
            Some("Reiniciar para atualizar")
        );
        assert_eq!(
            label(UpdateStatus::Errored).as_deref(),
            Some("Falha ao atualizar")
        );
    }

    #[test]
    fn so_avisos_passageiros_somem_sozinhos_em_4_segundos() {
        assert_eq!(
            UpdateView::UpToDate.visible_for(),
            Some(Duration::from_secs(4))
        );
        assert_eq!(
            UpdateView::AnotherInstance.visible_for(),
            Some(Duration::from_secs(4))
        );
        for persistent in [
            UpdateView::Checking,
            UpdateView::Restart("0.4.0".into()),
            UpdateView::Failed,
            UpdateView::RestartFailed,
            UpdateView::Disabled(DisabledText::Key(KEY_PROGRAM_FILES)),
        ] {
            assert_eq!(persistent.visible_for(), None, "{persistent:?}");
        }
    }

    #[test]
    fn progresso_tem_precedencia_sobre_aviso() {
        let notice = UpdateNotice::RestartFailed;
        assert_eq!(
            view_of(&UpdateStatus::Checking, Some(&notice)),
            UpdateView::Checking
        );
        assert_eq!(
            view_of(&UpdateStatus::Updated(v!("0.4.0")), Some(&notice)),
            UpdateView::RestartFailed
        );
    }

    #[test]
    fn reiniciar_sobrevive_ao_repouso_e_oferece_a_acao() {
        let view = view_of(&UpdateStatus::Updated(v!("0.4.0")), None);
        assert_eq!(view, UpdateView::Restart("0.4.0".into()));
        assert_eq!(view.action(), ViewAction::Restart);
    }

    #[test]
    fn acoes_de_cada_visao() {
        assert_eq!(UpdateView::Failed.action(), ViewAction::Retry);
        assert_eq!(
            UpdateView::Disabled(DisabledText::Key(KEY_PROGRAM_FILES)).action(),
            ViewAction::OpenReleases
        );
        for passive in [
            UpdateView::Hidden,
            UpdateView::Checking,
            UpdateView::Downloading,
            UpdateView::Installing,
            UpdateView::UpToDate,
            UpdateView::AnotherInstance,
        ] {
            assert_eq!(passive.action(), ViewAction::None, "{passive:?}");
        }
        assert_eq!(UpdateView::RestartFailed.action(), ViewAction::Dismiss);
    }

    #[test]
    fn rotulo_acessivel_inclui_a_versao_e_a_acao() {
        let restart = UpdateView::Restart("0.4.0".into());
        assert_eq!(
            restart.aria_label().as_deref(),
            Some("Reiniciar para atualizar para a versão 0.4.0")
        );
        assert!(
            UpdateView::Failed
                .aria_label()
                .is_some_and(|label| label.contains("tentar de novo"))
        );
        // estados informativos anunciam o próprio texto
        assert_eq!(
            UpdateView::Checking.aria_label(),
            UpdateView::Checking.label()
        );
    }

    #[test]
    fn razao_de_desativacao_do_pacote_e_exibida_literalmente() {
        let text = disabled_text(&DisabledReason::File(
            "Atualize com `apt upgrade`".to_owned(),
        ));
        assert_eq!(text.resolve(), "Atualize com `apt upgrade`");
        let view = UpdateView::Disabled(text);
        assert_eq!(view.label().as_deref(), Some("Atualize com `apt upgrade`"));
        assert!(
            view.aria_label()
                .is_some_and(|aria| aria.contains("apt upgrade"))
        );
    }

    #[test]
    fn toda_razao_tem_texto_nao_vazio() {
        for reason in [
            DisabledReason::Env("x".into()),
            DisabledReason::File("y".into()),
            DisabledReason::UnsupportedPlatform,
            DisabledReason::UnknownAppLocation,
            DisabledReason::ProgramFiles,
            DisabledReason::AppTranslocation,
            DisabledReason::NotWritable,
        ] {
            assert_ne!(disabled_text(&reason).resolve().trim(), "");
        }
    }

    #[test]
    fn aviso_de_desativacao_aparece_no_repouso() {
        let notice = UpdateNotice::Disabled(DisabledText::Key(KEY_NOT_WRITABLE));
        assert!(matches!(
            view_of(&UpdateStatus::Idle, Some(&notice)),
            UpdateView::Disabled(_)
        ));
    }
}
