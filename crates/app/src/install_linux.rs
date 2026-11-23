//! Instalador Linux (ticket 07), no modelo do Zed: `tar` do tarball verificado em um diretório
//! privado e `rsync -av --delete db-modeling.app` para o prefixo do usuário (`~/.local`).
//!
//! - **Nunca usa root**, `sudo`, `pkexec`, `apt` ou `dnf`. Pasta sem permissão de escrita faz o
//!   `rsync` falhar e a falha vira "Falha ao atualizar" (via [`InstallError::Failed`]).
//! - **Onde instala:** o prefixo é derivado do executável em uso: só instala quando ele está em
//!   `<prefixo>/db-modeling.app/bin/db-modeling` (layout do tarball; `current_exe()` pode terminar
//!   em ` (deleted)` depois de uma atualização, tratado por [`clean_exe_path`]). Qualquer outro
//!   caminho (build de desenvolvimento, `/usr/bin` de pacote) é "indisponível": não baixa nem
//!   copia nada, porque o `rsync` iria para um lugar que o processo em execução não usa.
//! - `.desktop`, ícones e o symlink `~/.local/bin/db-modeling` são do `install.sh`; o updater só
//!   troca a pasta `db-modeling.app` inteira (o `--delete` só age dentro dela).
//!
//! A lógica pura (prefixo, comandos) é testada sem tocar o disco; a execução real é testada em
//! diretório temporário com `tar` e `rsync` reais (os testes pulam se faltarem).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use updater::platform::PlatformKey;
use updater_net::private_dir::ensure_private_dir;
use updater_net::round::VerifiedUpdate;

use crate::constants::{
    LINUX_APP_DIR_NAME, LINUX_BIN_DIR_NAME, LINUX_EXE_NAME, LINUX_EXTRACT_DIR_NAME,
    LINUX_MISSING_TOOL_HINT, LINUX_RSYNC_ARCHIVE_FLAGS, LINUX_RSYNC_BIN, LINUX_RSYNC_DELETE_FLAG,
    LINUX_RSYNC_NO_LINKS_FLAG, LINUX_STDERR_MAX_CHARS, LINUX_SYSTEM_PREFIXES, LINUX_TAR_BIN,
    LINUX_TAR_DEST_FLAG, LINUX_TAR_EXTRACT_FLAGS, LINUX_TAR_NO_SAME_OWNER_FLAG,
};
use crate::relaunch::clean_exe_path;
use crate::update_install::{InstallError, Installer};
use crate::updater_ui::{KEY_LINUX_NOT_TARBALL, KEY_LINUX_SYSTEM_PREFIX, KEY_UNSUPPORTED_PLATFORM};

/// Por que a instalação no prefixo do usuário não aconteceu.
#[derive(Debug, PartialEq, Eq)]
pub enum LinuxInstallError {
    /// O executável em uso não está em `<prefixo>/db-modeling.app/bin/db-modeling`.
    NotTarballLayout(PathBuf),
    /// O prefixo derivado é de sistema (`/usr`, ...): só o gerenciador de pacotes mexe ali.
    SystemPrefix(PathBuf),
    /// `tar` ou `rsync` não foi encontrado no `PATH`.
    MissingTool(&'static str),
    /// O programa não pôde ser iniciado.
    Spawn(String),
    /// O programa terminou com erro (`stderr` aparado).
    CommandFailed {
        /// Programa que falhou.
        program: &'static str,
        /// Código de saída (`None` se foi morto por sinal).
        code: Option<i32>,
        /// Fim do `stderr`.
        stderr: String,
    },
    /// O tarball não tem `db-modeling.app/bin/db-modeling` (ou não é pasta/arquivo comum).
    InvalidArtifact(String),
    /// Erro de E/S ao preparar o diretório de extração.
    Io(String),
    /// O artefato verificado não é o de Linux `x86_64`.
    WrongPlatform,
}

impl std::fmt::Display for LinuxInstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotTarballLayout(path) => write!(
                f,
                "executável fora do layout do tarball ({LINUX_APP_DIR_NAME}/{LINUX_BIN_DIR_NAME}/{LINUX_EXE_NAME}): {}",
                path.display()
            ),
            Self::SystemPrefix(path) => write!(
                f,
                "instalação de sistema ({}): atualize pelo gerenciador de pacotes",
                path.display()
            ),
            Self::MissingTool(program) => {
                write!(f, "`{program}` não encontrado: {LINUX_MISSING_TOOL_HINT}")
            }
            Self::Spawn(error) => write!(f, "não foi possível iniciar o programa: {error}"),
            Self::CommandFailed {
                program,
                code,
                stderr,
            } => match code {
                Some(code) => write!(f, "`{program}` saiu com {code}: {stderr}"),
                None => write!(f, "`{program}` foi interrompido: {stderr}"),
            },
            Self::InvalidArtifact(reason) => write!(f, "tarball inválido: {reason}"),
            Self::Io(error) => write!(f, "diretório de extração: {error}"),
            Self::WrongPlatform => f.write_str("o artefato verificado não é de Linux x86_64"),
        }
    }
}

/// Comando externo montado sem shell (argumentos nunca passam por interpretação).
#[derive(Debug, PartialEq, Eq)]
pub struct CommandSpec {
    /// Programa (resolvido pelo `PATH`).
    pub program: &'static str,
    /// Argumentos, um por item.
    pub args: Vec<OsString>,
}

/// `tar -xzf <tarball> -C <destino> --no-same-owner`.
#[must_use]
pub fn tar_command(tarball: &Path, dest: &Path) -> CommandSpec {
    CommandSpec {
        program: LINUX_TAR_BIN,
        args: vec![
            LINUX_TAR_EXTRACT_FLAGS.into(),
            tarball.into(),
            LINUX_TAR_DEST_FLAG.into(),
            dest.into(),
            LINUX_TAR_NO_SAME_OWNER_FLAG.into(),
        ],
    }
}

/// `rsync -av --no-links --delete <pasta-do-app> <prefixo>/` (sem barra final na origem: copia a
/// pasta `db-modeling.app` inteira para dentro do prefixo; a barra final no destino evita que um
/// prefixo inexistente seja criado com o nome errado).
#[must_use]
pub fn rsync_command(app_dir: &Path, prefix: &Path) -> CommandSpec {
    let mut dest = prefix.as_os_str().to_owned();
    dest.push("/");
    CommandSpec {
        program: LINUX_RSYNC_BIN,
        args: vec![
            LINUX_RSYNC_ARCHIVE_FLAGS.into(),
            LINUX_RSYNC_NO_LINKS_FLAG.into(),
            LINUX_RSYNC_DELETE_FLAG.into(),
            app_dir.into(),
            dest,
        ],
    }
}

/// Prefixo de instalação (a pasta que contém `db-modeling.app`) a partir do executável em uso.
///
/// # Errors
/// [`LinuxInstallError::NotTarballLayout`] se o executável não está no layout do tarball;
/// [`LinuxInstallError::SystemPrefix`] se o prefixo é de sistema.
pub fn resolve_prefix(exe: &Path) -> Result<PathBuf, LinuxInstallError> {
    let exe = clean_exe_path(exe);
    let bin_dir = exe.parent();
    let app_dir = bin_dir.and_then(Path::parent);
    let matches_layout = exe.is_absolute()
        && exe.file_name().is_some_and(|name| name == LINUX_EXE_NAME)
        && bin_dir
            .and_then(Path::file_name)
            .is_some_and(|name| name == LINUX_BIN_DIR_NAME)
        && app_dir
            .and_then(Path::file_name)
            .is_some_and(|name| name == LINUX_APP_DIR_NAME);
    let Some(prefix) = app_dir.and_then(Path::parent).filter(|_| matches_layout) else {
        return Err(LinuxInstallError::NotTarballLayout(exe));
    };
    if LINUX_SYSTEM_PREFIXES
        .iter()
        .any(|system| prefix.starts_with(system))
    {
        return Err(LinuxInstallError::SystemPrefix(prefix.to_path_buf()));
    }
    Ok(prefix.to_path_buf())
}

/// Confere que o tarball extraído trouxe `db-modeling.app/bin/db-modeling` (pasta e arquivo
/// comuns, sem nenhum symlink na árvore extraída) e devolve a pasta do app.
fn validate_extracted(extract: &Path) -> Result<PathBuf, LinuxInstallError> {
    reject_symlinks(extract)?;
    let app_dir = extract.join(LINUX_APP_DIR_NAME);
    let is_real = |path: &Path, want_dir: bool| {
        std::fs::symlink_metadata(path).is_ok_and(|meta| {
            !meta.file_type().is_symlink()
                && if want_dir {
                    meta.is_dir()
                } else {
                    meta.is_file()
                }
        })
    };
    if !is_real(&app_dir, true) {
        return Err(LinuxInstallError::InvalidArtifact(format!(
            "falta a pasta {LINUX_APP_DIR_NAME}"
        )));
    }
    let bin_dir = app_dir.join(LINUX_BIN_DIR_NAME);
    if !is_real(&bin_dir, true) {
        return Err(LinuxInstallError::InvalidArtifact(format!(
            "falta a pasta {LINUX_APP_DIR_NAME}/{LINUX_BIN_DIR_NAME}"
        )));
    }
    let exe = bin_dir.join(LINUX_EXE_NAME);
    if !is_real(&exe, false) {
        return Err(LinuxInstallError::InvalidArtifact(format!(
            "falta {LINUX_APP_DIR_NAME}/{LINUX_BIN_DIR_NAME}/{LINUX_EXE_NAME}"
        )));
    }
    Ok(app_dir)
}

/// Recusa qualquer symlink dentro da árvore extraída (o layout do tarball `db-modeling.app` só
/// contém diretórios e arquivos regulares).
fn reject_symlinks(dir: &Path) -> Result<(), LinuxInstallError> {
    let entries =
        std::fs::read_dir(dir).map_err(|error| LinuxInstallError::Io(error.to_string()))?;
    for entry in entries {
        let entry = entry.map_err(|error| LinuxInstallError::Io(error.to_string()))?;
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path)
            .map_err(|error| LinuxInstallError::Io(error.to_string()))?;
        if meta.file_type().is_symlink() {
            return Err(LinuxInstallError::InvalidArtifact(format!(
                "link simbólico proibido no tarball: {}",
                path.display()
            )));
        }
        if meta.is_dir() {
            reject_symlinks(&path)?;
        }
    }
    Ok(())
}

/// Fim do `stderr`, limitado, sem quebras de linha extras.
fn tail_of(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let text = text.trim();
    let count = text.chars().count();
    let skip = count.saturating_sub(LINUX_STDERR_MAX_CHARS);
    text.chars().skip(skip).collect()
}

/// Executa `spec` sem shell, sem stdin; falha se o programa faltar ou sair com erro.
fn run(spec: &CommandSpec) -> Result<(), LinuxInstallError> {
    let output = Command::new(spec.program)
        .args(&spec.args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => LinuxInstallError::MissingTool(spec.program),
            _ => LinuxInstallError::Spawn(error.to_string()),
        })?;
    if output.status.success() {
        return Ok(());
    }
    Err(LinuxInstallError::CommandFailed {
        program: spec.program,
        code: output.status.code(),
        stderr: tail_of(&output.stderr),
    })
}

/// Apaga o diretório de extração ao sair (inclusive em falha).
struct ExtractGuard(PathBuf);

impl Drop for ExtractGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Extrai `tarball` em `<work_root>/linux-extract` (privado ao usuário), valida e sincroniza para
/// `prefix`. Não verifica o SHA-256 (o controlador já chamou `recheck`).
///
/// # Errors
/// [`LinuxInstallError`] em qualquer etapa; o prefixo só é tocado depois de o tarball validar.
pub fn install_artifact(
    tarball: &Path,
    work_root: &Path,
    prefix: &Path,
) -> Result<(), LinuxInstallError> {
    let extract = work_root.join(LINUX_EXTRACT_DIR_NAME);
    match std::fs::remove_dir_all(&extract) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(LinuxInstallError::Io(error.to_string())),
    }
    ensure_private_dir(&extract).map_err(|error| LinuxInstallError::Io(error.to_string()))?;
    let _cleanup = ExtractGuard(extract.clone());

    run(&tar_command(tarball, &extract))?;
    let app_dir = validate_extracted(&extract)?;
    run(&rsync_command(&app_dir, prefix))
}

/// Instalador Linux por tarball. Deriva o prefixo do executável em uso.
#[derive(Debug, Default, Clone, Copy)]
pub struct LinuxInstaller;

impl LinuxInstaller {
    fn prefix() -> Result<PathBuf, LinuxInstallError> {
        let exe =
            std::env::current_exe().map_err(|error| LinuxInstallError::Io(error.to_string()))?;
        resolve_prefix(&exe)
    }
}

/// Chave `i18n` do motivo de não atualizar, para o erro de [`resolve_prefix`]: diz se é o layout
/// (build de desenvolvimento, `/usr/bin` sem arquivo de explicação) ou um prefixo de sistema.
#[must_use]
pub fn unsupported_note_for(error: &LinuxInstallError) -> &'static str {
    match error {
        LinuxInstallError::NotTarballLayout(_) => KEY_LINUX_NOT_TARBALL,
        LinuxInstallError::SystemPrefix(_) => KEY_LINUX_SYSTEM_PREFIX,
        _ => KEY_UNSUPPORTED_PLATFORM,
    }
}

impl Installer for LinuxInstaller {
    fn is_supported(&self) -> bool {
        Self::prefix().is_ok()
    }

    fn unsupported_note(&self) -> &'static str {
        Self::prefix().map_or_else(
            |error| unsupported_note_for(&error),
            |_| KEY_UNSUPPORTED_PLATFORM,
        )
    }

    fn install(&self, update: &VerifiedUpdate) -> Result<(), InstallError> {
        if update.platform() != PlatformKey::LinuxX86_64 {
            return Err(InstallError::Failed(
                LinuxInstallError::WrongPlatform.to_string(),
            ));
        }
        let prefix = Self::prefix().map_err(|error| InstallError::Failed(error.to_string()))?;
        install_artifact(update.path(), update.updates_dir(), &prefix)
            .map_err(|error| InstallError::Failed(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;

    const HOME_EXE: &str = "/home/u/.local/db-modeling.app/bin/db-modeling";
    const FORBIDDEN_PROGRAMS: [&str; 7] = ["sudo", "su", "pkexec", "apt", "apt-get", "dnf", "doas"];

    fn tool_available(program: &str) -> bool {
        Command::new(program)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
    }

    fn tools_available() -> bool {
        tool_available(LINUX_TAR_BIN) && tool_available(LINUX_RSYNC_BIN)
    }

    /// Escreve `<root>/db-modeling.app/<rel>` com `content` (cria as pastas).
    fn put(root: &Path, rel: &str, content: &str) {
        let path = root.join(LINUX_APP_DIR_NAME).join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    /// Cria um tarball `.tar.gz` com o conteúdo de `stage` (a pasta `db-modeling.app` fica na raiz).
    fn make_tarball(stage: &Path, out: &Path) {
        let status = Command::new("tar")
            .arg("-czf")
            .arg(out)
            .arg("-C")
            .arg(stage)
            .arg(".")
            .status()
            .unwrap();
        assert!(status.success());
    }

    /// Prefixo com a versão antiga instalada + um arquivo de fora que não pode sumir.
    fn old_install(prefix: &Path) {
        put(prefix, "bin/db-modeling", "velho");
        put(prefix, "share/velho-so-na-v1", "x");
        fs::create_dir_all(prefix.join("bin")).unwrap();
        fs::write(prefix.join("bin/atalho-do-usuario"), "meu").unwrap();
    }

    fn new_tarball(dir: &Path) -> PathBuf {
        let stage = dir.join("stage");
        put(&stage, "bin/db-modeling", "novo");
        put(&stage, "share/applications/db-modeling.desktop", "d");
        let out = dir.join("db-modeling-linux-x86_64.tar.gz");
        make_tarball(&stage, &out);
        out
    }

    #[test]
    fn prefixo_do_layout_do_tarball() {
        assert_eq!(
            resolve_prefix(Path::new(HOME_EXE)).unwrap(),
            PathBuf::from("/home/u/.local")
        );
        // outro prefixo de usuário é preservado
        assert_eq!(
            resolve_prefix(Path::new("/home/u/apps/db-modeling.app/bin/db-modeling")).unwrap(),
            PathBuf::from("/home/u/apps")
        );
    }

    #[test]
    fn prefixo_ignora_o_sufixo_deleted() {
        let deleted = format!("{HOME_EXE}{}", crate::constants::DELETED_EXE_SUFFIX);
        assert_eq!(
            resolve_prefix(Path::new(&deleted)).unwrap(),
            PathBuf::from("/home/u/.local")
        );
    }

    #[test]
    fn fora_do_layout_nao_instala() {
        for exe in [
            "/usr/bin/db-modeling",
            "/home/u/proj/target/release/app",
            "/home/u/.local/outro.app/bin/db-modeling",
            "/home/u/.local/db-modeling.app/libexec/db-modeling",
            "/home/u/.local/db-modeling.app/bin/outro",
            "db-modeling.app/bin/db-modeling",
        ] {
            assert!(
                matches!(
                    resolve_prefix(Path::new(exe)),
                    Err(LinuxInstallError::NotTarballLayout(_))
                ),
                "{exe}"
            );
        }
    }

    #[test]
    fn prefixo_de_sistema_e_recusado() {
        for exe in [
            "/usr/db-modeling.app/bin/db-modeling",
            "/usr/local/db-modeling.app/bin/db-modeling",
            "/var/lib/db-modeling.app/bin/db-modeling",
        ] {
            assert!(
                matches!(
                    resolve_prefix(Path::new(exe)),
                    Err(LinuxInstallError::SystemPrefix(_))
                ),
                "{exe}"
            );
        }
        // `/usrx` não é `/usr` (comparação por componente)
        assert!(resolve_prefix(Path::new("/usrx/db-modeling.app/bin/db-modeling")).is_ok());
    }

    #[test]
    fn motivo_de_nao_atualizar_diz_layout_ou_sistema() {
        let note = |exe: &str| {
            unsupported_note_for(&resolve_prefix(Path::new(exe)).expect_err("fora do suportado"))
        };
        assert_eq!(
            note("/home/u/proj/target/release/app"),
            KEY_LINUX_NOT_TARBALL
        );
        assert_eq!(note("/usr/bin/db-modeling"), KEY_LINUX_NOT_TARBALL);
        assert_eq!(
            note("/usr/db-modeling.app/bin/db-modeling"),
            KEY_LINUX_SYSTEM_PREFIX
        );
        assert_eq!(
            unsupported_note_for(&LinuxInstallError::MissingTool(LINUX_RSYNC_BIN)),
            KEY_UNSUPPORTED_PLATFORM
        );
    }

    #[test]
    fn comandos_sao_tar_e_rsync_sem_shell_nem_root() {
        let tar = tar_command(Path::new("/c/a b.tar.gz"), Path::new("/c/x"));
        assert_eq!(tar.program, "tar");
        assert_eq!(
            tar.args,
            ["-xzf", "/c/a b.tar.gz", "-C", "/c/x", "--no-same-owner"]
        );
        let rsync = rsync_command(
            Path::new("/c/x/db-modeling.app"),
            Path::new("/home/u/.local"),
        );
        assert_eq!(rsync.program, "rsync");
        assert_eq!(
            rsync.args,
            [
                "-av",
                "--no-links",
                "--delete",
                "/c/x/db-modeling.app",
                "/home/u/.local/"
            ]
        );
        for spec in [&tar, &rsync] {
            assert!(!FORBIDDEN_PROGRAMS.contains(&spec.program));
            assert!(spec.args.iter().all(|arg| {
                let arg = arg.to_string_lossy();
                !FORBIDDEN_PROGRAMS.iter().any(|bad| *bad == arg)
            }));
        }
    }

    #[test]
    fn stderr_e_aparado_no_fim() {
        let long = "x".repeat(LINUX_STDERR_MAX_CHARS * 3) + "FIM\n";
        let tail = tail_of(long.as_bytes());
        assert_eq!(tail.chars().count(), LINUX_STDERR_MAX_CHARS);
        assert!(tail.ends_with("FIM"));
    }

    #[test]
    fn programa_ausente_vira_dica_de_instalacao() {
        let spec = CommandSpec {
            program: "dbm-programa-que-nao-existe",
            args: vec![],
        };
        let error = run(&spec).unwrap_err();
        assert_eq!(
            error,
            LinuxInstallError::MissingTool("dbm-programa-que-nao-existe")
        );
        assert!(error.to_string().contains(LINUX_MISSING_TOOL_HINT));
    }

    #[test]
    fn atualiza_trocando_so_a_pasta_do_app() {
        if !tools_available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let prefix = dir.path().join("local");
        old_install(&prefix);
        let tarball = new_tarball(dir.path());
        let work = dir.path().join("updates");
        fs::create_dir(&work).unwrap();

        install_artifact(&tarball, &work, &prefix).unwrap();

        let app = prefix.join(LINUX_APP_DIR_NAME);
        assert_eq!(
            fs::read_to_string(app.join("bin/db-modeling")).unwrap(),
            "novo"
        );
        assert!(app.join("share/applications/db-modeling.desktop").exists());
        // `--delete` removeu o que a versão nova não tem...
        assert!(!app.join("share/velho-so-na-v1").exists());
        // ...mas não mexeu fora de db-modeling.app
        assert_eq!(
            fs::read_to_string(prefix.join("bin/atalho-do-usuario")).unwrap(),
            "meu"
        );
        // e a extração não sobra no diretório de updates
        assert!(!work.join(LINUX_EXTRACT_DIR_NAME).exists());
    }

    #[test]
    fn tarball_corrompido_falha_no_tar_e_limpa_a_extracao() {
        if !tools_available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let work = dir.path().join("updates");
        fs::create_dir(&work).unwrap();
        // tarball inválido: a extração falha, mas só depois de criar a pasta privada
        let bad = dir.path().join("ruim.tar.gz");
        fs::write(&bad, "não é tar").unwrap();
        let extract = work.join(LINUX_EXTRACT_DIR_NAME);
        // pasta pré-existente (resto de execução anterior) é substituída por uma privada
        fs::create_dir(&extract).unwrap();
        fs::write(extract.join("resto"), "x").unwrap();
        let prefix = dir.path().join("local");
        fs::create_dir(&prefix).unwrap();

        let error = install_artifact(&bad, &work, &prefix).unwrap_err();

        assert!(matches!(
            error,
            LinuxInstallError::CommandFailed { program: "tar", .. }
        ));
        assert!(!extract.exists(), "limpa mesmo em falha");
        assert_eq!(fs::read_dir(&prefix).unwrap().count(), 0);
    }

    #[test]
    fn tarball_sem_executavel_nao_toca_o_prefixo() {
        if !tools_available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let prefix = dir.path().join("local");
        old_install(&prefix);
        let stage = dir.path().join("stage");
        put(&stage, "share/so-isto", "x");
        let tarball = dir.path().join("t.tar.gz");
        make_tarball(&stage, &tarball);
        let work = dir.path().join("updates");
        fs::create_dir(&work).unwrap();

        let error = install_artifact(&tarball, &work, &prefix).unwrap_err();

        assert!(matches!(error, LinuxInstallError::InvalidArtifact(_)));
        assert_eq!(
            fs::read_to_string(prefix.join("db-modeling.app/bin/db-modeling")).unwrap(),
            "velho"
        );
    }

    #[test]
    fn tarball_com_symlink_e_recusado_sem_tocar_o_prefixo() {
        if !tools_available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let prefix = dir.path().join("local");
        old_install(&prefix);
        let stage = dir.path().join("stage");
        put(&stage, "bin/db-modeling", "novo");
        std::os::unix::fs::symlink(
            "/etc/passwd",
            stage.join(LINUX_APP_DIR_NAME).join("bin/escape"),
        )
        .unwrap();
        let tarball = dir.path().join("t.tar.gz");
        make_tarball(&stage, &tarball);
        let work = dir.path().join("updates");
        fs::create_dir(&work).unwrap();

        let error = install_artifact(&tarball, &work, &prefix).unwrap_err();

        assert!(matches!(error, LinuxInstallError::InvalidArtifact(_)));
        assert_eq!(
            fs::read_to_string(prefix.join("db-modeling.app/bin/db-modeling")).unwrap(),
            "velho"
        );
    }

    #[test]
    fn pasta_sem_escrita_falha_no_rsync() {
        if !tools_available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        // prefixo sem escrita e sem instalação anterior: o `rsync` não consegue criar o app.
        // (Com o app já existente e do próprio usuário o `rsync -a` corrige as permissões das
        // pastas e segue; o caso real de falha é prefixo de outro dono, ex. `/opt` do root.)
        let prefix = dir.path().join("local");
        fs::create_dir(&prefix).unwrap();
        let tarball = new_tarball(dir.path());
        let work = dir.path().join("updates");
        fs::create_dir(&work).unwrap();
        fs::set_permissions(&prefix, fs::Permissions::from_mode(0o555)).unwrap();
        let writable = fs::write(prefix.join("sonda"), "x").is_ok();

        let result = install_artifact(&tarball, &work, &prefix);

        // restaura para o tempdir poder ser apagado
        fs::set_permissions(&prefix, fs::Permissions::from_mode(0o755)).unwrap();
        if writable {
            return; // rodando como root: permissão não bloqueia
        }
        let error = result.unwrap_err();
        assert!(
            matches!(
                error,
                LinuxInstallError::CommandFailed {
                    program: "rsync",
                    ..
                }
            ),
            "{error}"
        );
        assert!(!prefix.join(LINUX_APP_DIR_NAME).exists());
    }

    #[test]
    fn executavel_em_uso_continua_rodando_apos_a_troca() {
        if !tools_available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let prefix = dir.path().join("local");
        old_install(&prefix);
        let exe = prefix.join(LINUX_APP_DIR_NAME).join("bin/db-modeling");
        fs::write(&exe, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
        let held = fs::File::open(&exe).unwrap(); // simula o binário em uso
        let tarball = new_tarball(dir.path());
        let work = dir.path().join("updates");
        fs::create_dir(&work).unwrap();

        install_artifact(&tarball, &work, &prefix).unwrap();

        drop(held);
        assert_eq!(fs::read_to_string(&exe).unwrap(), "novo");
    }
}
