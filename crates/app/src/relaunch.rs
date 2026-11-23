//! Relançamento do app depois de uma atualização (macOS e Linux; o Windows é do helper).
//!
//! Hoje o app só tem `Quit`. "Reiniciar para atualizar" grava a sessão e chama
//! [`relaunch_current`], que inicia **um processo curto do próprio executável** com
//! `--relaunch-after <pid>` (separado do app, sem terminal) e só então o app sai. O processo curto
//! (tratado em `main.rs` antes de abrir a UI, por [`run_helper`]) espera o PID antigo terminar e
//! abre o executável já atualizado. Assim o processo novo só lê a sessão depois que o antigo
//! terminou de gravá-la e de sair.
//!
//! # Falha ao relançar
//! - Se o processo curto nem chega a iniciar, [`relaunch_current`] devolve erro **antes** de o app
//!   sair: o app segue aberto e a UI mostra "Falha ao reiniciar".
//! - Se o app antigo já saiu e o processo curto não consegue abrir o novo (ou o app antigo não
//!   sai em [`crate::constants::RELAUNCH_WAIT_TIMEOUT_SECS`]), nada mais é aberto: a versão nova
//!   **já está instalada** e o usuário abre o app manualmente
//!   ([`crate::constants::RELAUNCH_MANUAL_HINT`], escrito em `stderr` e no log).
//!
//! **Não verificado no Windows/macOS** (ambiente de desenvolvimento é Linux).

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::constants::{ARG_RELAUNCH_AFTER, DELETED_EXE_SUFFIX};
#[cfg(unix)]
use crate::constants::{RELAUNCH_POLL_INTERVAL_MS, RELAUNCH_WAIT_TIMEOUT_SECS};

/// Resultado de interpretar os argumentos do processo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Args {
    /// Início normal do app.
    Normal,
    /// Processo curto de relançamento: espere este PID e abra o app.
    RelaunchAfter(u32),
    /// `--relaunch-after` sem PID válido.
    Invalid,
}

/// Interpreta os argumentos **sem** o nome do programa. `--relaunch-after` só vale como primeiro
/// argumento (o app trata o primeiro argumento comum como caminho de arquivo).
#[must_use]
pub fn parse_args(args: &[String]) -> Args {
    let Some(first) = args.first() else {
        return Args::Normal;
    };
    if first != ARG_RELAUNCH_AFTER {
        return Args::Normal;
    }
    match args.get(1).and_then(|pid| pid.parse::<u32>().ok()) {
        Some(pid) if pid != 0 && args.len() == 2 => Args::RelaunchAfter(pid),
        _ => Args::Invalid,
    }
}

/// No Linux, `current_exe()` devolve `<caminho> (deleted)` se o binário foi substituído por um
/// `rsync` enquanto o app rodava (justamente o caso da atualização): o caminho real é o anterior
/// ao sufixo.
#[must_use]
pub fn clean_exe_path(path: &Path) -> PathBuf {
    path.to_str()
        .and_then(|text| text.strip_suffix(DELETED_EXE_SUFFIX))
        .map_or_else(|| path.to_path_buf(), PathBuf::from)
}

/// Por que o relançamento não aconteceu.
#[derive(Debug)]
pub enum RelaunchError {
    /// Plataforma sem relançamento por este caminho (ou helper ausente em build de dev no Windows).
    #[cfg_attr(
        not(any(windows, test)),
        expect(
            dead_code,
            reason = "só o Windows e os testes de install_windows devolvem Unsupported"
        )
    )]
    Unsupported,
    /// Não foi possível localizar o executável ou iniciar o processo.
    Io(std::io::Error),
}

impl std::fmt::Display for RelaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => f.write_str("relançamento não suportado nesta plataforma"),
            Self::Io(error) => write!(f, "relançamento: {error}"),
        }
    }
}

/// Como a espera pelo PID terminou.
#[cfg(any(unix, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waited {
    /// O processo já saiu (ou nunca existiu).
    Exited,
    /// O tempo acabou com o processo ainda vivo.
    TimedOut,
}

#[cfg(any(unix, test))]
/// Espera `is_alive(pid)` virar falso, conferindo a cada `interval` até `timeout`. O relógio e o
/// sono entram por parâmetro para o teste não esperar de verdade.
pub fn wait_for_exit(
    pid: u32,
    interval: Duration,
    timeout: Duration,
    is_alive: &mut dyn FnMut(u32) -> bool,
    sleep: &mut dyn FnMut(Duration),
) -> Waited {
    let mut waited = Duration::ZERO;
    while is_alive(pid) {
        if waited >= timeout {
            return Waited::TimedOut;
        }
        sleep(interval);
        waited += interval;
    }
    Waited::Exited
}

/// No texto de `/proc/<pid>/stat` o estado vem depois do último `)` (o nome pode ter `)`); `Z` é
/// zumbi: terminou, só falta o pai colher. Para a espera, zumbi conta como "saiu".
#[cfg(any(target_os = "linux", test))]
#[must_use]
pub fn stat_is_zombie(stat: &str) -> bool {
    stat.rsplit_once(')')
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .is_some_and(|state| state == crate::constants::PROC_STATE_ZOMBIE)
}

/// O processo `pid` ainda existe? (Linux: `/proc/<pid>`, zumbi conta como morto.)
#[cfg(target_os = "linux")]
#[must_use]
pub fn pid_alive(pid: u32) -> bool {
    let dir = std::path::Path::new(crate::constants::PROC_DIR).join(pid.to_string());
    if !dir.exists() {
        return false;
    }
    !std::fs::read_to_string(dir.join(crate::constants::PROC_STAT_FILE))
        .is_ok_and(|stat| stat_is_zombie(&stat))
}

/// O processo `pid` ainda existe? (macOS: `kill -0 <pid>`, sem `/proc`.) **Não verificado.**
#[cfg(target_os = "macos")]
#[must_use]
pub fn pid_alive(pid: u32) -> bool {
    std::process::Command::new(crate::constants::KILL_BIN)
        .arg(crate::constants::KILL_PROBE_SIGNAL)
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Outras plataformas não usam este caminho (o Windows tem o helper do ticket 05).
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[must_use]
pub fn pid_alive(_pid: u32) -> bool {
    false
}

/// Lado do app que sai: inicia `<exe> --relaunch-after <pid>`, destacado do app (sem stdio e em
/// outro grupo de processos, para não morrer junto). Chamar **depois** de gravar a sessão e
/// **antes** de `cx.quit()`.
///
/// # Errors
/// [`RelaunchError::Io`] se o executável não for localizado ou o processo não iniciar.
#[cfg(unix)]
pub fn relaunch_current() -> Result<(), RelaunchError> {
    use std::os::unix::process::CommandExt as _;
    use std::process::{Command, Stdio};

    let exe = clean_exe_path(&std::env::current_exe().map_err(RelaunchError::Io)?);
    Command::new(exe)
        .arg(ARG_RELAUNCH_AFTER)
        .arg(std::process::id().to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map(drop)
        .map_err(RelaunchError::Io)
}

/// Windows: o relançamento é do `auto_update_helper` (ticket 05): inicia
/// `tools\auto_update_helper.exe --launch true --wait-pid <pid>` e o app sai em seguida; o helper
/// espera o app terminar, troca os arquivos e abre o app novo. **Não verificado no Windows.**
///
/// # Errors
/// [`RelaunchError::Unsupported`] se o helper não existe (build de desenvolvimento ou instalação
/// sem `tools\`); [`RelaunchError::Io`] se o processo não inicia. Em ambos o app **não** sai.
#[cfg(windows)]
pub fn relaunch_current() -> Result<(), RelaunchError> {
    crate::install_windows::relaunch_via_helper()
}

/// Lado do processo curto: espera o app antigo sair e abre o executável atualizado. Devolve
/// `true` se abriu o app. Roda em `main.rs` antes de qualquer UI.
#[cfg(unix)]
#[must_use]
pub fn run_helper(pid: u32) -> bool {
    use std::os::unix::process::CommandExt as _;
    use std::process::{Command, Stdio};

    let mut sleep = std::thread::sleep;
    let waited = wait_for_exit(
        pid,
        Duration::from_millis(RELAUNCH_POLL_INTERVAL_MS),
        Duration::from_secs(RELAUNCH_WAIT_TIMEOUT_SECS),
        &mut pid_alive,
        &mut sleep,
    );
    if waited == Waited::TimedOut {
        // o app antigo não saiu: não abre uma segunda cópia por cima dele
        return false;
    }
    let started = std::env::current_exe()
        .map(|exe| clean_exe_path(&exe))
        .and_then(|exe| {
            Command::new(exe)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .process_group(0)
                .spawn()
        });
    started.is_ok()
}

/// Windows: sem processo curto (o helper do ticket 05 faz isso).
#[cfg(windows)]
#[must_use]
pub fn run_helper(_pid: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn sem_argumentos_ou_com_arquivo_e_inicio_normal() {
        assert_eq!(parse_args(&[]), Args::Normal);
        assert_eq!(parse_args(&args(&["schema.dbml"])), Args::Normal);
        // só vale como primeiro argumento
        assert_eq!(
            parse_args(&args(&["schema.dbml", "--relaunch-after", "10"])),
            Args::Normal
        );
    }

    #[test]
    fn relaunch_after_com_pid_valido() {
        assert_eq!(
            parse_args(&args(&["--relaunch-after", "4242"])),
            Args::RelaunchAfter(4242)
        );
    }

    #[test]
    fn relaunch_after_invalido_nunca_abre_a_ui() {
        for bad in [
            args(&["--relaunch-after"]),
            args(&["--relaunch-after", "abc"]),
            args(&["--relaunch-after", "-1"]),
            args(&["--relaunch-after", "0"]),
            args(&["--relaunch-after", "10", "extra"]),
        ] {
            assert_eq!(parse_args(&bad), Args::Invalid, "{bad:?}");
        }
    }

    #[test]
    fn caminho_deletado_do_linux_e_limpo() {
        assert_eq!(
            clean_exe_path(&PathBuf::from(
                "/home/u/.local/db-modeling.app/bin/db-modeling (deleted)"
            )),
            PathBuf::from("/home/u/.local/db-modeling.app/bin/db-modeling")
        );
        let normal = PathBuf::from("/opt/db-modeling");
        assert_eq!(clean_exe_path(&normal), normal);
    }

    #[test]
    fn espera_termina_quando_o_pid_some() {
        let mut calls = 0;
        let mut slept = Duration::ZERO;
        let waited = wait_for_exit(
            7,
            Duration::from_millis(100),
            Duration::from_secs(30),
            &mut |_| {
                calls += 1;
                calls <= 3
            },
            &mut |d| slept += d,
        );
        assert_eq!(waited, Waited::Exited);
        assert_eq!(slept, Duration::from_millis(300));
    }

    #[test]
    fn pid_inexistente_nao_espera() {
        let mut slept = Duration::ZERO;
        let waited = wait_for_exit(
            7,
            Duration::from_millis(100),
            Duration::from_secs(30),
            &mut |_| false,
            &mut |d| slept += d,
        );
        assert_eq!((waited, slept), (Waited::Exited, Duration::ZERO));
    }

    #[test]
    fn espera_desiste_no_tempo_limite() {
        let mut slept = Duration::ZERO;
        let waited = wait_for_exit(
            7,
            Duration::from_millis(100),
            Duration::from_secs(1),
            &mut |_| true,
            &mut |d| slept += d,
        );
        assert_eq!(waited, Waited::TimedOut);
        assert_eq!(slept, Duration::from_secs(1));
    }

    #[test]
    fn zumbi_conta_como_encerrado() {
        assert!(stat_is_zombie("123 (db-modeling) Z 1 123 123 0 -1"));
        assert!(!stat_is_zombie("123 (db-modeling) S 1 123 123 0 -1"));
        // o nome pode conter parênteses e espaços
        assert!(stat_is_zombie("123 (a) b) Z 1"));
        assert!(!stat_is_zombie("lixo"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn processo_atual_esta_vivo_e_pid_enorme_nao() {
        assert!(pid_alive(std::process::id()));
        assert!(!pid_alive(u32::MAX));
    }
}
