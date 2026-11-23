//! Instalador Linux, no modelo do Zed, para as três formas de instalação do app:
//!
//! - **Tarball / `install.sh`** (ticket 07): `tar` do tarball verificado em um diretório privado e
//!   `rsync -av --delete db-modeling.app` para o prefixo do usuário (`~/.local`). Sem root: pasta
//!   sem permissão de escrita faz o `rsync` falhar e a falha vira "Falha ao atualizar" (via
//!   [`InstallError::Failed`]).
//! - **`.deb` e `.rpm`**: o pacote verificado (SHA-256 + minisign, pelo `updater-net`) é instalado
//!   por cima com `pkexec dpkg -i <arquivo>` / `pkexec dnf install -y <arquivo>`. **O `pkexec` é o
//!   único caminho privilegiado**: o polkit abre o diálogo de senha do próprio sistema e o app
//!   nunca vê a senha. Sem `pkexec` no `PATH` o instalador fica "indisponível" e a UI orienta
//!   baixar o pacote na página de Releases. Nada passa por shell: cada argumento é um item do
//!   [`Command`].
//!
//! - **Como detecta:** [`detect_install_kind`] olha o executável em uso. Se está em
//!   `<prefixo>/db-modeling.app/bin/db-modeling` (layout do tarball; `current_exe()` pode terminar
//!   em ` (deleted)` depois de uma atualização, tratado por [`clean_exe_path`]) é tarball. Senão,
//!   `dpkg -S <exe>` (pacote `db-modeling`) indica `.deb` e `rpm -qf <exe>` indica `.rpm`. Qualquer
//!   outro caminho (build de desenvolvimento) é "indisponível": não baixa nem copia nada, porque a
//!   atualização iria para um lugar que o processo em execução não usa.
//! - `.desktop`, ícones e o symlink `~/.local/bin/db-modeling` são do `install.sh`; o updater só
//!   troca a pasta `db-modeling.app` inteira (o `--delete` só age dentro dela).
//!
//! A lógica pura (detecção, comandos) é testada sem tocar o disco; a execução real do tarball é
//! testada em diretório temporário com `tar` e `rsync` reais (os testes pulam se faltarem).

use std::ffi::{OsStr, OsString};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use updater::platform::{PackageFormat, PlatformKey};
use updater_net::private_dir::ensure_private_dir;
use updater_net::round::VerifiedUpdate;

use crate::constants::{
    ENV_PATH, LINUX_APP_DIR_NAME, LINUX_BIN_DIR_NAME, LINUX_DNF_ASSUME_YES_FLAG, LINUX_DNF_BIN,
    LINUX_DNF_INSTALL_ARG, LINUX_DPKG_ARCH_SEPARATOR, LINUX_DPKG_BIN, LINUX_DPKG_INSTALL_FLAG,
    LINUX_DPKG_OWNER_FLAG, LINUX_DPKG_OWNER_SEPARATOR, LINUX_DPKG_OWNERS_SEPARATOR, LINUX_EXE_NAME,
    LINUX_EXTRACT_DIR_NAME, LINUX_MISSING_TOOL_HINT, LINUX_PACKAGE_NAME, LINUX_PKEXEC_BIN,
    LINUX_RPM_BIN, LINUX_RPM_NAME_FORMAT, LINUX_RPM_OWNER_FLAG, LINUX_RPM_QUERYFORMAT_FLAG,
    LINUX_RSYNC_ARCHIVE_FLAGS, LINUX_RSYNC_BIN, LINUX_RSYNC_DELETE_FLAG, LINUX_RSYNC_NO_LINKS_FLAG,
    LINUX_STDERR_MAX_CHARS, LINUX_SYSTEM_PREFIXES, LINUX_TAR_BIN, LINUX_TAR_DEST_FLAG,
    LINUX_TAR_EXTRACT_FLAGS, LINUX_TAR_NO_SAME_OWNER_FLAG, UNIX_EXEC_BITS,
};
use crate::relaunch::clean_exe_path;
use crate::update_install::{InstallError, Installer};
use crate::updater_ui::{
    KEY_LINUX_NO_PKEXEC, KEY_LINUX_NOT_TARBALL, KEY_LINUX_SYSTEM_PREFIX, KEY_UNSUPPORTED_PLATFORM,
};

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
    /// O artefato verificado não é o da forma de instalação em uso (tarball, `.deb` ou `.rpm`).
    WrongPlatform,
    /// Instalação por pacote sem `pkexec` no `PATH`: só dá para baixar e instalar à mão.
    PkexecMissing,
    /// O caminho do pacote baixado não é absoluto (o `dnf` o trataria como nome de pacote).
    RelativePackagePath(PathBuf),
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
            Self::WrongPlatform => {
                f.write_str("o artefato verificado não é o da forma de instalação em uso")
            }
            Self::PkexecMissing => f.write_str(
                "`pkexec` não encontrado: baixe o pacote na página de Releases e instale manualmente",
            ),
            Self::RelativePackagePath(path) => {
                write!(f, "caminho do pacote não é absoluto: {}", path.display())
            }
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

/// Consulta de dono do executável no `.deb`: `dpkg -S <exe>`.
#[must_use]
pub fn dpkg_query_command(exe: &Path) -> CommandSpec {
    CommandSpec {
        program: LINUX_DPKG_BIN,
        args: vec![LINUX_DPKG_OWNER_FLAG.into(), exe.into()],
    }
}

/// Consulta de dono do executável no `.rpm`: `rpm -qf --queryformat %{NAME} <exe>`.
#[must_use]
pub fn rpm_query_command(exe: &Path) -> CommandSpec {
    CommandSpec {
        program: LINUX_RPM_BIN,
        args: vec![
            LINUX_RPM_OWNER_FLAG.into(),
            LINUX_RPM_QUERYFORMAT_FLAG.into(),
            LINUX_RPM_NAME_FORMAT.into(),
            exe.into(),
        ],
    }
}

/// Instalação do pacote baixado com privilégio: `pkexec dpkg -i <deb>` ou
/// `pkexec dnf install -y <rpm>`. O arquivo vai como um único argumento (sem shell).
#[must_use]
pub fn package_install_command(format: PackageFormat, package: &Path) -> CommandSpec {
    let args = match format {
        PackageFormat::Deb => vec![
            LINUX_DPKG_BIN.into(),
            LINUX_DPKG_INSTALL_FLAG.into(),
            package.into(),
        ],
        PackageFormat::Rpm => vec![
            LINUX_DNF_BIN.into(),
            LINUX_DNF_INSTALL_ARG.into(),
            LINUX_DNF_ASSUME_YES_FLAG.into(),
            package.into(),
        ],
    };
    CommandSpec {
        program: LINUX_PKEXEC_BIN,
        args,
    }
}

/// A saída do `dpkg -S` (`pacote[:arq][, outro]: caminho`) lista o pacote do app como dono?
#[must_use]
pub fn dpkg_owner_is_app(stdout: &str) -> bool {
    stdout
        .lines()
        .filter_map(|line| line.split_once(LINUX_DPKG_OWNER_SEPARATOR))
        .flat_map(|(owners, _path)| owners.split(LINUX_DPKG_OWNERS_SEPARATOR))
        .filter_map(|owner| owner.split(LINUX_DPKG_ARCH_SEPARATOR).next())
        .any(|name| name == LINUX_PACKAGE_NAME)
}

/// A saída do `rpm -qf --queryformat %{NAME}` é o nome do pacote do app?
#[must_use]
pub fn rpm_owner_is_app(stdout: &str) -> bool {
    stdout.lines().any(|name| name.trim() == LINUX_PACKAGE_NAME)
}

/// Como o app em execução foi instalado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallKind {
    /// Layout do tarball / `install.sh`, com o prefixo do usuário.
    Tarball(PathBuf),
    /// Pacote do sistema (`.deb` ou `.rpm`).
    Package(PackageFormat),
}

impl InstallKind {
    /// Chave do artefato correspondente no manifesto.
    #[must_use]
    pub fn platform_key(&self) -> PlatformKey {
        match self {
            Self::Tarball(_) => PlatformKey::LinuxX86_64,
            Self::Package(format) => PlatformKey::LinuxX86_64.for_package(*format),
        }
    }
}

/// Descobre como o executável `exe` foi instalado. `query` roda um comando de consulta e devolve
/// o `stdout` se ele terminou com sucesso (injetável: os testes não executam nada).
///
/// # Errors
/// [`LinuxInstallError::SystemPrefix`] para o layout do tarball em prefixo de sistema;
/// [`LinuxInstallError::NotTarballLayout`] se não é tarball nem pertence a um pacote do app.
pub fn detect_install_kind(
    exe: &Path,
    query: &dyn Fn(&CommandSpec) -> Option<String>,
) -> Result<InstallKind, LinuxInstallError> {
    let clean = match resolve_prefix(exe) {
        Ok(prefix) => return Ok(InstallKind::Tarball(prefix)),
        Err(LinuxInstallError::NotTarballLayout(clean)) => clean,
        Err(other) => return Err(other),
    };
    if query(&dpkg_query_command(&clean)).is_some_and(|out| dpkg_owner_is_app(&out)) {
        return Ok(InstallKind::Package(PackageFormat::Deb));
    }
    if query(&rpm_query_command(&clean)).is_some_and(|out| rpm_owner_is_app(&out)) {
        return Ok(InstallKind::Package(PackageFormat::Rpm));
    }
    Err(LinuxInstallError::NotTarballLayout(clean))
}

/// Executa `spec` sem shell e devolve o `stdout` se saiu com sucesso (programa ausente, falha ou
/// saída que não é UTF-8 viram `None`).
fn query_stdout(spec: &CommandSpec) -> Option<String> {
    let output = Command::new(spec.program)
        .args(&spec.args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `program` é um arquivo executável em algum diretório de `path` (formato do `PATH`)?
fn program_in_path(program: &str, path: &OsStr) -> bool {
    std::env::split_paths(path).any(|dir| {
        std::fs::metadata(dir.join(program))
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & UNIX_EXEC_BITS != 0)
    })
}

/// O `pkexec` está disponível neste ambiente?
fn pkexec_available() -> bool {
    std::env::var_os(ENV_PATH).is_some_and(|path| program_in_path(LINUX_PKEXEC_BIN, &path))
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

/// Instala o pacote verificado por cima do instalado, com `pkexec` (o polkit pede a senha).
/// Não verifica o SHA-256 (o controlador já chamou `recheck`).
///
/// # Errors
/// [`LinuxInstallError::RelativePackagePath`] se o caminho não é absoluto;
/// [`LinuxInstallError::PkexecMissing`] sem `pkexec` no `PATH`; falhas de [`run`].
pub fn install_package(format: PackageFormat, package: &Path) -> Result<(), LinuxInstallError> {
    if !package.is_absolute() {
        return Err(LinuxInstallError::RelativePackagePath(
            package.to_path_buf(),
        ));
    }
    run(&package_install_command(format, package)).map_err(|error| match error {
        LinuxInstallError::MissingTool(LINUX_PKEXEC_BIN) => LinuxInstallError::PkexecMissing,
        other => other,
    })
}

/// Instalador Linux: tarball (`tar` + `rsync`) ou pacote (`pkexec`), conforme a instalação em uso.
/// A detecção roda uma vez (consulta `dpkg`/`rpm`) e fica guardada.
#[derive(Debug, Default)]
pub struct LinuxInstaller {
    kind: OnceLock<Result<InstallKind, LinuxInstallError>>,
}

impl LinuxInstaller {
    fn kind(&self) -> &Result<InstallKind, LinuxInstallError> {
        self.kind.get_or_init(|| {
            let exe = std::env::current_exe()
                .map_err(|error| LinuxInstallError::Io(error.to_string()))?;
            detect_install_kind(&exe, &query_stdout)
        })
    }
}

/// Chave `i18n` do motivo de não atualizar, para o erro de [`detect_install_kind`]: diz se é o
/// layout (build de desenvolvimento), um prefixo de sistema ou a falta do `pkexec`.
#[must_use]
pub fn unsupported_note_for(error: &LinuxInstallError) -> &'static str {
    match error {
        LinuxInstallError::NotTarballLayout(_) => KEY_LINUX_NOT_TARBALL,
        LinuxInstallError::SystemPrefix(_) => KEY_LINUX_SYSTEM_PREFIX,
        LinuxInstallError::PkexecMissing => KEY_LINUX_NO_PKEXEC,
        _ => KEY_UNSUPPORTED_PLATFORM,
    }
}

impl Installer for LinuxInstaller {
    fn is_supported(&self) -> bool {
        match self.kind() {
            Ok(InstallKind::Tarball(_)) => true,
            Ok(InstallKind::Package(_)) => pkexec_available(),
            Err(_) => false,
        }
    }

    fn unsupported_note(&self) -> &'static str {
        match self.kind() {
            Err(error) => unsupported_note_for(error),
            Ok(InstallKind::Package(_)) if !pkexec_available() => KEY_LINUX_NO_PKEXEC,
            Ok(_) => KEY_UNSUPPORTED_PLATFORM,
        }
    }

    fn platform(&self) -> Option<PlatformKey> {
        match self.kind() {
            Ok(kind) => Some(kind.platform_key()),
            Err(_) => PlatformKey::current(),
        }
    }

    fn install(&self, update: &VerifiedUpdate) -> Result<(), InstallError> {
        let failed = |error: LinuxInstallError| InstallError::Failed(error.to_string());
        let kind = self
            .kind()
            .as_ref()
            .map_err(|error| InstallError::Failed(error.to_string()))?;
        if update.platform() != kind.platform_key() {
            return Err(failed(LinuxInstallError::WrongPlatform));
        }
        match kind {
            InstallKind::Tarball(prefix) => {
                install_artifact(update.path(), update.updates_dir(), prefix)
            }
            InstallKind::Package(format) => install_package(*format, update.path()),
        }
        .map_err(failed)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

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

    /// Consulta falsa: devolve a saída de cada programa (ou `None` = falhou/ausente).
    fn fake_query(
        dpkg: Option<&str>,
        rpm: Option<&str>,
    ) -> impl Fn(&CommandSpec) -> Option<String> {
        let dpkg = dpkg.map(str::to_owned);
        let rpm = rpm.map(str::to_owned);
        move |spec| match spec.program {
            "dpkg" => dpkg.clone(),
            "rpm" => rpm.clone(),
            other => panic!("consulta inesperada: {other}"),
        }
    }

    #[test]
    fn dpkg_reconhece_o_pacote_dono_do_executavel() {
        for out in [
            "db-modeling: /usr/bin/db-modeling\n",
            "db-modeling:amd64: /usr/bin/db-modeling\n",
            "outro, db-modeling: /usr/bin/db-modeling\n",
        ] {
            assert!(dpkg_owner_is_app(out), "{out}");
        }
        for out in [
            "",
            "dpkg-query: no path found matching pattern /usr/bin/db-modeling\n",
            "db-modeling-extra: /usr/bin/db-modeling\n",
            "outro: /usr/bin/db-modeling\n",
            "outro: /usr/share/db-modeling: x\n",
        ] {
            assert!(!dpkg_owner_is_app(out), "{out}");
        }
    }

    #[test]
    fn rpm_reconhece_o_pacote_dono_do_executavel() {
        assert!(rpm_owner_is_app("db-modeling"));
        assert!(rpm_owner_is_app("db-modeling\n"));
        assert!(!rpm_owner_is_app(""));
        assert!(!rpm_owner_is_app("db-modeling-extra"));
        assert!(!rpm_owner_is_app("file /x is not owned by any package"));
    }

    #[test]
    fn deteccao_prefere_o_layout_do_tarball() {
        let query = fake_query(Some("db-modeling: x"), None);
        assert_eq!(
            detect_install_kind(Path::new(HOME_EXE), &query).unwrap(),
            InstallKind::Tarball(PathBuf::from("/home/u/.local"))
        );
    }

    #[test]
    fn deteccao_acha_deb_e_rpm_pelo_dono_do_executavel() {
        let exe = Path::new("/usr/bin/db-modeling");
        assert_eq!(
            detect_install_kind(
                exe,
                &fake_query(Some("db-modeling: /usr/bin/db-modeling"), None)
            )
            .unwrap(),
            InstallKind::Package(PackageFormat::Deb)
        );
        assert_eq!(
            detect_install_kind(exe, &fake_query(None, Some("db-modeling"))).unwrap(),
            InstallKind::Package(PackageFormat::Rpm)
        );
        // `(deleted)` depois da troca do arquivo não atrapalha
        let deleted = format!(
            "/usr/bin/db-modeling{}",
            crate::constants::DELETED_EXE_SUFFIX
        );
        assert_eq!(
            detect_install_kind(Path::new(&deleted), &fake_query(None, Some("db-modeling")))
                .unwrap(),
            InstallKind::Package(PackageFormat::Rpm)
        );
    }

    #[test]
    fn deteccao_consulta_o_caminho_limpo() {
        let seen = std::cell::RefCell::new(Vec::new());
        let query = |spec: &CommandSpec| {
            seen.borrow_mut().push(spec.args.clone());
            None
        };
        let deleted = format!(
            "/usr/bin/db-modeling{}",
            crate::constants::DELETED_EXE_SUFFIX
        );
        let _ = detect_install_kind(Path::new(&deleted), &query);
        let seen = seen.into_inner();
        assert_eq!(seen.len(), 2, "dpkg e rpm");
        assert!(
            seen.iter()
                .all(|args| args.last().unwrap() == "/usr/bin/db-modeling")
        );
    }

    #[test]
    fn deteccao_sem_dono_nem_layout_e_indisponivel() {
        let query = fake_query(None, None);
        for exe in ["/usr/bin/db-modeling", "/home/u/proj/target/release/app"] {
            assert!(
                matches!(
                    detect_install_kind(Path::new(exe), &query),
                    Err(LinuxInstallError::NotTarballLayout(_))
                ),
                "{exe}"
            );
        }
        // saída que não é do app (outro pacote dono)
        assert!(
            detect_install_kind(
                Path::new("/usr/bin/db-modeling"),
                &fake_query(Some("outro: /usr/bin/db-modeling"), Some("outro"))
            )
            .is_err()
        );
    }

    #[test]
    fn prefixo_de_sistema_nao_consulta_pacotes() {
        let query = |_: &CommandSpec| -> Option<String> { panic!("não deve consultar") };
        assert!(matches!(
            detect_install_kind(Path::new("/usr/db-modeling.app/bin/db-modeling"), &query),
            Err(LinuxInstallError::SystemPrefix(_))
        ));
    }

    #[test]
    fn consultas_de_dono_sao_dpkg_s_e_rpm_qf_sem_shell() {
        let exe = Path::new("/usr/bin/db-modeling");
        let dpkg = dpkg_query_command(exe);
        assert_eq!(dpkg.program, "dpkg");
        assert_eq!(dpkg.args, ["-S", "/usr/bin/db-modeling"]);
        let rpm = rpm_query_command(exe);
        assert_eq!(rpm.program, "rpm");
        assert_eq!(
            rpm.args,
            ["-qf", "--queryformat", "%{NAME}", "/usr/bin/db-modeling"]
        );
    }

    #[test]
    fn instalacao_de_pacote_usa_pkexec_com_argumentos_separados() {
        let deb = package_install_command(
            PackageFormat::Deb,
            Path::new("/home/u/.cache/db-modeling/updates/a b; rm -rf x.deb"),
        );
        assert_eq!(deb.program, "pkexec");
        assert_eq!(
            deb.args,
            [
                "dpkg",
                "-i",
                "/home/u/.cache/db-modeling/updates/a b; rm -rf x.deb"
            ]
        );
        let rpm = package_install_command(
            PackageFormat::Rpm,
            Path::new("/home/u/.cache/db-modeling/updates/db-modeling.rpm"),
        );
        assert_eq!(rpm.program, "pkexec");
        assert_eq!(
            rpm.args,
            [
                "dnf",
                "install",
                "-y",
                "/home/u/.cache/db-modeling/updates/db-modeling.rpm"
            ]
        );
    }

    #[test]
    fn plataforma_do_manifesto_segue_a_forma_de_instalacao() {
        assert_eq!(
            InstallKind::Tarball(PathBuf::from("/x")).platform_key(),
            PlatformKey::LinuxX86_64
        );
        assert_eq!(
            InstallKind::Package(PackageFormat::Deb).platform_key(),
            PlatformKey::LinuxX86_64Deb
        );
        assert_eq!(
            InstallKind::Package(PackageFormat::Rpm).platform_key(),
            PlatformKey::LinuxX86_64Rpm
        );
    }

    #[test]
    fn programa_no_path_exige_arquivo_executavel() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let tool = dir.path().join("pkexec");
        fs::write(&tool, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
        let path = std::env::join_paths([other.path(), dir.path()]).unwrap();
        assert!(program_in_path("pkexec", &path));
        assert!(!program_in_path("outro", &path));
        // sem bit de execução não conta
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!program_in_path("pkexec", &path));
        // pasta com o nome do programa não conta
        fs::create_dir(other.path().join("dnf")).unwrap();
        assert!(!program_in_path("dnf", &path));
        assert!(!program_in_path("pkexec", OsStr::new("")));
    }

    #[test]
    fn pacote_com_caminho_relativo_e_recusado() {
        let error = install_package(PackageFormat::Deb, Path::new("rel/a.deb")).unwrap_err();
        assert!(matches!(error, LinuxInstallError::RelativePackagePath(_)));
    }

    #[test]
    fn mensagens_de_pacote_orientam_a_pagina_de_releases() {
        assert!(
            LinuxInstallError::PkexecMissing
                .to_string()
                .contains("Releases")
        );
        assert_eq!(
            unsupported_note_for(&LinuxInstallError::PkexecMissing),
            KEY_LINUX_NO_PKEXEC
        );
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
