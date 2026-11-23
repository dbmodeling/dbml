#![cfg_attr(
    not(windows),
    allow(
        dead_code,
        reason = "compilado em cfg(test) no Linux para validar tipos e lógica sem depender do runner Windows"
    )
)]
//! Instalador do Windows (ticket 05; modelo do Zed). **Não verificado no Windows.**
//!
//! Fluxo: o instalador Inno verificado roda em silêncio com `/update=true` e instala em
//! `{app}\install`, sem tocar no app em execução (grava `updates\versions.txt` no fim). A troca
//! dos arquivos é do `tools\auto_update_helper.exe` (crate `auto-update-helper`):
//!
//! - **ao sair** do app: [`finalize_on_exit`] inicia
//!   `auto_update_helper.exe --launch false --wait-pid <pid>` em segundo plano e o app encerra;
//! - **"Reiniciar para atualizar"**: [`relaunch_via_helper`] inicia
//!   `auto_update_helper.exe --launch true --wait-pid <pid>` e o app sai; o helper espera o app,
//!   troca os arquivos e abre o app novo. Depois disso [`finalize_on_exit`] não roda o helper
//!   de novo.
//! - **na abertura**: [`Installer::pending_version`] reconcilia restos e, se a flag e `install\`
//!   existem (queda de energia, `kill`), o app já mostra "Reiniciar para atualizar".
//!
//! Instalação **por máquina** (app sob `%ProgramFiles%`) ou sem escrita no diretório do app: a
//! política (`updater::policy`) já desativa o updater e a UI só mostra a explicação; nunca se usa
//! `/ALLUSERS` nem UAC. [`WindowsInstaller::install`] confere a mesma política por segurança.
//!
//! Toda a lógica de decisão (jobs, rollback, recuperação, argumentos) está em
//! `auto_update_helper`, testada no Linux; aqui só há cola com processos e caminhos.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use auto_update_helper::args;
use auto_update_helper::constants::{INSTALLER_ARGS, INSTALLER_POLL_MS, INSTALLER_TIMEOUT_SECS};
use auto_update_helper::fsops::RealFs;
use auto_update_helper::jobs::RetryPolicy;
use auto_update_helper::layout::Layout;
use auto_update_helper::recovery::{self, Startup};
use auto_update_helper::wait::wait_until;
use updater::lock::UpdateLock;
use updater::platform::TargetOs;
use updater::policy::{SystemEnv, disabled_reason};
use updater_net::round::VerifiedUpdate;

use crate::relaunch::RelaunchError;
use crate::update_install::{InstallError, Installer};

/// O helper já foi iniciado por "Reiniciar para atualizar": o hook de saída não o roda de novo.
static HANDED_OFF_TO_HELPER: AtomicBool = AtomicBool::new(false);

/// Instalador do Windows.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsInstaller;

fn fail(message: impl Into<String>) -> InstallError {
    InstallError::Failed(message.into())
}

/// Diretório de instalação deduzido do executável em execução (`{app}\db-modeling.exe`).
fn current_layout() -> Option<Layout> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| Layout::from_app_exe(&exe))
}

fn cache_dir() -> Option<PathBuf> {
    crate::auto_update::cache_dir_with(TargetOs::Windows, &|name| std::env::var_os(name))
}

/// Roda o instalador Inno em silêncio e espera terminar (com prazo; passado disso, encerra-o).
fn run_installer(installer: &Path) -> Result<(), InstallError> {
    let mut child = Command::new(installer)
        .args(INSTALLER_ARGS)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| fail(format!("não foi possível iniciar o instalador: {error}")))?;
    let mut status = None;
    let finished = wait_until(
        Duration::from_secs(INSTALLER_TIMEOUT_SECS),
        Duration::from_millis(INSTALLER_POLL_MS),
        &mut |duration| std::thread::sleep(duration),
        &mut || match child.try_wait() {
            Ok(Some(exit)) => {
                status = Some(exit);
                true
            }
            Ok(None) => false,
            Err(_) => true,
        },
    );
    if !finished {
        let _ = child.kill();
        let _ = child.wait();
        return Err(fail("o instalador não terminou no prazo"));
    }
    match status {
        Some(exit) if exit.success() => Ok(()),
        Some(exit) => Err(fail(format!("o instalador saiu com {exit}"))),
        None => Err(fail("não foi possível ler o resultado do instalador")),
    }
}

impl Installer for WindowsInstaller {
    fn is_supported(&self) -> bool {
        true
    }

    fn install(&self, update: &VerifiedUpdate) -> Result<(), InstallError> {
        // por máquina (ProgramFiles) ou sem escrita: nada de /ALLUSERS nem UAC, só explicação
        if let Some(reason) = disabled_reason(&SystemEnv) {
            return Err(fail(format!(
                "atualização desativada neste ambiente: {reason:?}"
            )));
        }
        let layout = current_layout().ok_or_else(|| fail("diretório do app indeterminado"))?;
        let helper = layout.helper_exe();
        if !helper.is_file() {
            return Err(fail(format!("helper ausente: {}", helper.display())));
        }
        // restos de uma tentativa anterior (install\ sem flag etc.) saem antes de instalar
        recovery::recover(&layout, &RealFs, &RetryPolicy::default(), &mut |duration| {
            std::thread::sleep(duration);
        })
        .map_err(|error| fail(format!("restos de atualização anterior: {error}")))?;
        run_installer(update.path())?;
        match recovery::inspect(&layout, &RealFs) {
            Startup::Pending => Ok(()),
            Startup::Clean => Err(fail(
                "o instalador terminou sem deixar install\\ e updates\\versions.txt",
            )),
        }
    }

    fn pending_version(&self) -> Option<String> {
        let layout = current_layout()?;
        if !recovery::has_leftovers(&layout, &RealFs) {
            return None;
        }
        // outra instância instalando segura a trava: não mexer na pasta enquanto isso
        let cache = cache_dir()?;
        let Ok(Some(_lock)) = UpdateLock::try_acquire(&cache) else {
            return None;
        };
        let state = recovery::recover(&layout, &RealFs, &RetryPolicy::default(), &mut |duration| {
            std::thread::sleep(duration);
        });
        match state {
            Ok(Startup::Pending) => recovery::pending_version(&layout, &RealFs),
            Ok(Startup::Clean) => None,
            Err(error) => {
                tracing::warn!("não foi possível reconciliar a atualização pendente: {error}");
                None
            }
        }
    }
}

/// Hook de encerramento: com atualização pendente, inicia
/// `auto_update_helper.exe --launch false --wait-pid <pid>` (sem bloquear o processo do app, para
/// que `db-modeling.exe` termine imediatamente e o helper possa substituir e apagar `old\`).
/// Não roda se "Reiniciar para atualizar" já entregou a troca ao helper.
pub fn finalize_on_exit() {
    if HANDED_OFF_TO_HELPER.load(Ordering::Acquire) {
        return;
    }
    let Some(layout) = current_layout() else {
        return;
    };
    let helper = layout.helper_exe();
    if recovery::inspect(&layout, &RealFs) != Startup::Pending || !helper.is_file() {
        return;
    }
    if let Err(error) = Command::new(&helper)
        .args(args::build(false, Some(std::process::id())))
        .spawn()
    {
        tracing::warn!("não foi possível iniciar o auto_update_helper: {error}");
    }
}

/// "Reiniciar para atualizar": inicia `auto_update_helper.exe --launch true --wait-pid <pid>`
/// (o chamador sai em seguida).
///
/// # Errors
/// [`RelaunchError::Unsupported`] se não há helper (build de desenvolvimento ou instalação sem
/// `tools\`); [`RelaunchError::Io`] se o processo não inicia. Em ambos o app segue aberto.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn relaunch_via_helper() -> Result<(), RelaunchError> {
    let helper = current_layout()
        .map(|layout| layout.helper_exe())
        .filter(|helper| helper.is_file())
        .ok_or(RelaunchError::Unsupported)?;
    Command::new(helper)
        .args(args::build(true, Some(std::process::id())))
        .spawn()
        .map_err(RelaunchError::Io)?;
    HANDED_OFF_TO_HELPER.store(true, Ordering::Release);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_installer_declara_suporte_e_sem_layout_nao_tem_pendencia() {
        assert!(WindowsInstaller.is_supported());
        assert_eq!(WindowsInstaller.pending_version(), None);
    }

    #[test]
    fn relaunch_sem_helper_no_layout_retorna_unsupported() {
        // O binário de teste do Cargo não tem `tools/auto_update_helper.exe` ao lado.
        assert!(matches!(
            relaunch_via_helper(),
            Err(RelaunchError::Unsupported)
        ));
    }

    #[test]
    fn finalize_on_exit_respeita_handoff_e_ausencia_de_pendencia() {
        HANDED_OFF_TO_HELPER.store(true, Ordering::Release);
        finalize_on_exit();
        HANDED_OFF_TO_HELPER.store(false, Ordering::Release);
        finalize_on_exit();
    }
}
