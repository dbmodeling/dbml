//! Ponto de plug dos instaladores por plataforma (Windows, macOS e Linux).
//!
//! A rodada do `updater-net` termina entregando um [`VerifiedUpdate`] (artefato baixado, SHA-256
//! conferido, trava de atualização retida) com a máquina de estados em `Installing(versão)`. Quem
//! instala implementa [`Installer`]; o controlador (`auto_update.rs`) faz o resto:
//!
//! 1. chama `update.recheck()` (recalcula o SHA-256 **antes** de montar/extrair/executar);
//! 2. chama [`Installer::install`] segurando o `VerifiedUpdate` (e, portanto, a trava);
//! 3. em `Ok` chama `state.installed()` (a UI passa a mostrar "Reiniciar para atualizar"); em `Err`
//!    chama `state.fail()` (silencioso se automática; "Falha ao atualizar" se manual).

use updater::platform::PlatformKey;
use updater_net::fetch::FetchError;
use updater_net::round::VerifiedUpdate;

/// Falha ao instalar (o motivo é só para o log: a UI mostra "Falha ao atualizar").
#[derive(Debug)]
pub enum InstallError {
    /// Esta plataforma não tem instalador ("não suportado nesta plataforma").
    #[cfg_attr(
        all(any(target_os = "linux", target_os = "macos", windows), not(test)),
        allow(
            dead_code,
            reason = "as 3 plataformas suportadas têm instalador real; só o provisório devolve isto"
        )
    )]
    Unsupported,
    /// O SHA-256 recalculado antes de instalar divergiu do manifesto, ou o arquivo sumiu.
    Recheck(FetchError),
    /// O instalador da plataforma falhou (mensagem para o log).
    #[cfg_attr(
        not(any(test, target_os = "linux", target_os = "macos", windows)),
        expect(
            dead_code,
            reason = "construído pelos instaladores reais de Linux, macOS e Windows"
        )
    )]
    Failed(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => f.write_str("instalação não suportada nesta plataforma"),
            Self::Recheck(error) => write!(f, "verificação final do artefato: {error}"),
            Self::Failed(message) => write!(f, "instalação: {message}"),
        }
    }
}

/// Instalador de uma plataforma.
pub trait Installer: Send + Sync {
    /// Esta plataforma sabe instalar? Se `false`, o controlador **não faz rodada nenhuma** (nem
    /// baixa o artefato à toa) e a UI mostra "indisponível nesta plataforma".
    fn is_supported(&self) -> bool;

    /// Chave de `i18n::t` com o motivo de `is_supported() == false`, mostrada no lugar da ação.
    /// Padrão: "indisponível nesta plataforma"; o Linux diz se é layout ou prefixo de sistema.
    fn unsupported_note(&self) -> &'static str {
        crate::updater_ui::KEY_UNSUPPORTED_PLATFORM
    }

    /// Chave do artefato no manifesto para esta instalação. Padrão: a da plataforma em execução;
    /// o Linux refina por tarball/`.deb`/`.rpm`.
    fn platform(&self) -> Option<PlatformKey> {
        PlatformKey::current()
    }

    /// Instala `update` (já verificado; `recheck` já foi chamado pelo controlador). Bloqueante:
    /// roda na thread do updater, nunca na de UI.
    ///
    /// # Errors
    /// [`InstallError`]; o controlador transforma em `state.fail()`.
    fn install(&self, update: &VerifiedUpdate) -> Result<(), InstallError>;

    /// Instalação já feita e aguardando reinício, achada **na abertura** (Windows: `updates\versions.txt`
    /// e `install\` existem; também reconcilia restos de falhas). Devolve o texto da versão (pode
    /// vir vazio). Padrão: nenhuma; o app abre em repouso.
    fn pending_version(&self) -> Option<String> {
        None
    }
}

/// Último passo do encerramento do app (depois de `Application::run` voltar). Windows: se há
/// instalação pendente, roda `auto_update_helper.exe --launch false` e espera; nas demais
/// plataformas não faz nada (os arquivos já foram trocados na instalação).
pub fn finalize_on_exit() {
    #[cfg(windows)]
    crate::install_windows::finalize_on_exit();
}

/// Instalador provisório: "não suportado nesta plataforma".
#[derive(Debug, Default, Clone, Copy)]
#[cfg_attr(
    all(any(target_os = "linux", target_os = "macos", windows), not(test)),
    allow(
        dead_code,
        reason = "as 3 plataformas suportadas têm instalador real; sobra só para outros alvos e testes"
    )
)]
pub struct UnsupportedInstaller;

impl Installer for UnsupportedInstaller {
    fn is_supported(&self) -> bool {
        false
    }

    fn install(&self, _update: &VerifiedUpdate) -> Result<(), InstallError> {
        Err(InstallError::Unsupported)
    }
}

/// Instalador da plataforma em execução. **Ponto de plug**: ver a documentação do módulo.
#[must_use]
pub fn platform_installer() -> Box<dyn Installer> {
    #[cfg(windows)]
    {
        windows_installer()
    }
    #[cfg(target_os = "macos")]
    {
        macos_installer()
    }
    #[cfg(target_os = "linux")]
    {
        linux_installer()
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        Box::new(UnsupportedInstaller)
    }
}

/// Windows (ticket 05): Inno com `/verysilent /update=true ...` em `{app}\install`; o
/// `auto_update_helper` aplica ao sair. **Não verificado no Windows.**
#[cfg(windows)]
fn windows_installer() -> Box<dyn Installer> {
    Box::new(crate::install_windows::WindowsInstaller)
}

/// macOS (ticket 06): `codesign --verify`, `hdiutil attach` e `rsync` sobre o bundle em execução.
/// **Não verificado no macOS.**
#[cfg(target_os = "macos")]
fn macos_installer() -> Box<dyn Installer> {
    Box::new(crate::install_macos::MacosInstaller)
}

/// Linux: `tar` + `rsync --delete` para `~/.local` (tarball) ou `pkexec dpkg -i`/`dnf install -y`
/// (`.deb`/`.rpm`).
#[cfg(target_os = "linux")]
fn linux_installer() -> Box<dyn Installer> {
    Box::new(crate::install_linux::LinuxInstaller::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instalador_provisorio_nao_e_suportado() {
        assert!(!UnsupportedInstaller.is_supported());
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        assert!(!platform_installer().is_supported());
    }

    #[cfg(windows)]
    #[test]
    fn windows_tem_instalador_real() {
        assert!(platform_installer().is_supported());
    }

    #[test]
    fn mensagens_de_erro_sao_legiveis() {
        assert!(
            InstallError::Unsupported
                .to_string()
                .contains("não suportada")
        );
        assert!(
            InstallError::Failed("rsync saiu com 23".into())
                .to_string()
                .contains("rsync saiu com 23")
        );
    }
}
