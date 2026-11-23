//! Instalador do macOS (ticket 06): `hdiutil` + `rsync` sobre o bundle em execução.
//!
//! Fluxo (copiado do Zed, com a verificação de assinatura que o Zed não tem):
//!
//! 1. `hdiutil attach -nobrowse -readonly -plist -mountroot <tmp privado> <dmg>`;
//! 2. se o manifesto traz `team_id` (build assinado): `codesign --verify --deep --strict` no `.app`
//!    montado e comparação do Team ID (`codesign -d --verbose=4`) **antes de qualquer cópia**. O
//!    download por `ureq` não recebe quarentena, então o Gatekeeper não confere nada: esta é a
//!    única checagem de assinatura. Sem `team_id` (build sem assinatura) a verificação é pulada;
//! 3. `rsync -av --delete --exclude "Icon?" <montado>/ <bundle>/`;
//! 4. `hdiutil detach -force` **sempre** (sucesso, erro ou pânico), por guarda RAII ([`MountGuard`]).
//!
//! Nenhum caminho passa por shell: cada caminho (inclusive com espaço, como `DB Modeling.app`) é
//! um único argumento de `Command`. Bundle em App Translocation ou sem escrita **não chega aqui**:
//! `updater::policy::disabled_reason` já desativa a atualização e a UI só mostra a explicação;
//! [`ensure_updatable`] repete essa decisão (a mesma função, sem duplicar a regra) por defesa.
//!
//! # Risco conhecido
//! O `rsync` roda **sobre o app em execução**. Se for interrompido (queda de energia, `kill`, disco
//! cheio), o bundle pode ficar inconsistente (binário novo com recursos antigos, ou arquivos
//! faltando) e o app pode não abrir; a assinatura do bundle também pode ficar inválida. Não há
//! rollback: a recuperação é reinstalar pelo `.dmg`/`.pkg`. Depois de copiar, em build assinado, o
//! bundle instalado é verificado de novo com `codesign --verify`.
//!
//! A lógica (linhas de comando, plist, Team ID, fluxo e desmontagem) é independente de plataforma e
//! testada no Linux com um [`Host`] falso; só a execução de processos ([`SystemHost`],
//! [`MacosInstaller`]) existe sob `cfg(target_os = "macos")`. **NÃO VERIFICADO no macOS.**

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use updater::policy::DisabledReason;

use crate::constants::macos_update::{
    ARG_FORCE, ARG_MOUNTROOT, ARG_NOBROWSE, ARG_PLIST, ARG_READONLY, BUNDLE_NAME, CODESIGN_BIN,
    CODESIGN_IDENTITY_ARGS, CODESIGN_VERIFY_ARGS, DIR_TRAILING_SLASH, HDIUTIL_ATTACH, HDIUTIL_BIN,
    HDIUTIL_DETACH, MOUNT_ROOT_PREFIX, PLIST_KEY_DEV_ENTRY, PLIST_KEY_MOUNT_POINT,
    PLIST_STRING_CLOSE, PLIST_STRING_OPEN, RSYNC_ARGS, RSYNC_BIN, RSYNC_EXCLUDE_ICON,
    STDERR_MAX_CHARS, TEAM_ID_NOT_SET, TEAM_ID_PREFIX, VOLUME_NAME,
};

/// Por que a instalação no macOS falhou (só para o log; a UI mostra "Falha ao atualizar").
#[derive(Debug)]
pub enum MacosInstallError {
    /// App Translocation, sem escrita ou fora de um bundle: só a explicação (nada é montado).
    NotUpdatable(DisabledReason),
    /// Não foi possível criar a pasta privada de montagem.
    MountRoot(io::Error),
    /// Não foi possível executar um programa (`hdiutil`, `codesign` ou `rsync`).
    Spawn {
        program: &'static str,
        source: io::Error,
    },
    /// `hdiutil attach` falhou.
    Attach(String),
    /// A saída de `hdiutil attach -plist` não trouxe o ponto de montagem (ou aponta para fora de `mount_root`).
    NoMountPoint,
    /// O `.dmg` não contém `DB Modeling.app`.
    MissingApp(PathBuf),
    /// `codesign --verify --deep --strict` reprovou o `.app` (montado ou, depois da cópia, instalado).
    SignatureInvalid(String),
    /// O Team ID do `.app` não é o do manifesto (inclui `.app` sem Team ID): nada foi copiado.
    TeamMismatch {
        expected: String,
        found: Option<String>,
    },
    /// `rsync` saiu com erro: o bundle pode estar inconsistente (ver "Risco conhecido").
    Rsync(String),
}

impl std::fmt::Display for MacosInstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotUpdatable(reason) => write!(f, "bundle não atualizável: {reason:?}"),
            Self::MountRoot(error) => write!(f, "pasta de montagem: {error}"),
            Self::Spawn { program, source } => write!(f, "executar {program}: {source}"),
            Self::Attach(detail) => write!(f, "hdiutil attach: {detail}"),
            Self::NoMountPoint => f.write_str("hdiutil attach não informou o ponto de montagem"),
            Self::MissingApp(path) => write!(f, "{} não existe no volume", path.display()),
            Self::SignatureInvalid(detail) => write!(f, "codesign --verify: {detail}"),
            Self::TeamMismatch { expected, found } => write!(
                f,
                "Team ID do app ({}) diverge do manifesto ({expected}); nada foi copiado",
                found.as_deref().unwrap_or(TEAM_ID_NOT_SET)
            ),
            Self::Rsync(detail) => write!(
                f,
                "rsync falhou, o bundle pode estar inconsistente (reinstale pelo .dmg): {detail}"
            ),
        }
    }
}

/// Resultado de um processo (já decodificado).
#[derive(Debug, Clone, Default)]
pub struct CmdOutput {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Tudo que o fluxo pede ao sistema: processos e diretórios. Fake nos testes.
pub trait Host {
    /// Executa `program` com `args` (sem shell). `Err` só se o processo não pôde iniciar.
    fn run(&self, program: &str, args: &[OsString]) -> io::Result<CmdOutput>;
    /// `path` é um diretório existente?
    fn is_dir(&self, path: &Path) -> bool;
    /// Cria `path` (modo `0700`) se ainda não existir.
    fn create_private_dir(&self, path: &Path) -> io::Result<()>;
    /// Remove `path` se estiver vazio (best effort; um volume ainda montado não esvazia).
    fn remove_dir(&self, path: &Path);
}

// ---------------------------------------------------------------------------------------------
// Linhas de comando (puras)
// ---------------------------------------------------------------------------------------------

fn os(text: &str) -> OsString {
    OsString::from(text)
}

/// `hdiutil attach -nobrowse -readonly -plist -mountroot <mount_root> <dmg>`.
#[must_use]
pub fn attach_args(dmg: &Path, mount_root: &Path) -> Vec<OsString> {
    vec![
        os(HDIUTIL_ATTACH),
        os(ARG_NOBROWSE),
        os(ARG_READONLY),
        os(ARG_PLIST),
        os(ARG_MOUNTROOT),
        mount_root.as_os_str().to_owned(),
        dmg.as_os_str().to_owned(),
    ]
}

/// `hdiutil detach -force <alvo>` (ponto de montagem ou `/dev/diskN`).
#[must_use]
pub fn detach_args(target: &OsString) -> Vec<OsString> {
    vec![os(HDIUTIL_DETACH), os(ARG_FORCE), target.clone()]
}

/// `codesign --verify --deep --strict <app>`.
#[must_use]
pub fn verify_args(app: &Path) -> Vec<OsString> {
    CODESIGN_VERIFY_ARGS
        .iter()
        .map(|arg| os(arg))
        .chain([app.as_os_str().to_owned()])
        .collect()
}

/// `codesign -d --verbose=4 <app>` (a identidade sai em `stderr`).
#[must_use]
pub fn identity_args(app: &Path) -> Vec<OsString> {
    CODESIGN_IDENTITY_ARGS
        .iter()
        .map(|arg| os(arg))
        .chain([app.as_os_str().to_owned()])
        .collect()
}

/// `dir` com barra final (o `rsync` então copia o **conteúdo** do diretório).
fn with_trailing_slash(dir: &Path) -> OsString {
    let mut text = dir.as_os_str().to_owned();
    if !text.to_string_lossy().ends_with(DIR_TRAILING_SLASH) {
        text.push(DIR_TRAILING_SLASH);
    }
    text
}

/// `rsync -av --delete --exclude "Icon?" <app montado>/ <bundle>/`. Cada caminho é um argumento:
/// o espaço de `DB Modeling.app` nunca passa por shell, então não precisa de aspas.
#[must_use]
pub fn rsync_args(mounted_app: &Path, bundle: &Path) -> Vec<OsString> {
    RSYNC_ARGS
        .iter()
        .map(|arg| os(arg))
        .chain([
            os(RSYNC_EXCLUDE_ICON),
            with_trailing_slash(mounted_app),
            with_trailing_slash(bundle),
        ])
        .collect()
}

/// Pasta privada de montagem: `<base>/db-modeling-update-<pid>`.
#[must_use]
pub fn mount_root_in(base: &Path, pid: u32) -> PathBuf {
    base.join(format!("{MOUNT_ROOT_PREFIX}{pid}"))
}

// ---------------------------------------------------------------------------------------------
// Interpretação de saídas (pura)
// ---------------------------------------------------------------------------------------------

/// Volume montado, segundo o plist de `hdiutil attach -plist`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Attached {
    /// Ponto de montagem (só a entidade com sistema de arquivos tem).
    pub mount_point: Option<PathBuf>,
    /// Dispositivos (`/dev/diskN`, `/dev/diskNsM`): alvo alternativo do `detach`.
    pub dev_entries: Vec<String>,
}

impl Attached {
    /// O que passar a `hdiutil detach`: o ponto de montagem, senão o primeiro dispositivo.
    #[must_use]
    pub fn detach_target(&self) -> Option<OsString> {
        self.mount_point
            .as_ref()
            .map(|path| path.as_os_str().to_owned())
            .or_else(|| self.dev_entries.first().map(|dev| os(dev)))
    }
}

fn xml_unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Todos os `<string>` que seguem `<key>chave</key>` no plist.
fn plist_strings(xml: &str, key: &str) -> Vec<String> {
    let needle = format!("<key>{key}</key>");
    let mut found = Vec::new();
    let mut rest = xml;
    while let Some(position) = rest.find(&needle) {
        rest = &rest[position + needle.len()..];
        let Some(body) = rest.trim_start().strip_prefix(PLIST_STRING_OPEN) else {
            continue;
        };
        if let Some(end) = body.find(PLIST_STRING_CLOSE) {
            found.push(xml_unescape(&body[..end]));
        }
    }
    found
}

/// Lê o plist XML de `hdiutil attach -plist` (`system-entities` com `mount-point`/`dev-entry`).
#[must_use]
pub fn parse_attach_plist(xml: &str) -> Attached {
    Attached {
        mount_point: plist_strings(xml, PLIST_KEY_MOUNT_POINT)
            .into_iter()
            .find(|path| !path.is_empty())
            .map(PathBuf::from),
        dev_entries: plist_strings(xml, PLIST_KEY_DEV_ENTRY),
    }
}

/// Team ID em `codesign -d --verbose=4` (linha `TeamIdentifier=ABCDE12345`). `None` se a linha
/// não existe ou vale `not set` (build sem Team ID ou ad-hoc).
#[must_use]
pub fn parse_team_identifier(output: &str) -> Option<String> {
    output
        .lines()
        .find_map(|line| line.trim().strip_prefix(TEAM_ID_PREFIX))
        .map(str::trim)
        .filter(|id| !id.is_empty() && *id != TEAM_ID_NOT_SET)
        .map(str::to_owned)
}

/// Decisão sobre o Team ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeamCheck {
    /// Manifesto sem `team_id` (build sem assinatura): não se verifica nada.
    Skip,
    /// Mesmo Team ID: pode copiar.
    Match,
    /// Divergente ou ausente no `.app`: recusar.
    Mismatch { found: Option<String> },
}

/// Compara o Team ID do manifesto com o do `.app` montado (exato, diferencia caixa).
#[must_use]
pub fn check_team(expected: Option<&str>, found: Option<&str>) -> TeamCheck {
    match (expected, found) {
        (None, _) => TeamCheck::Skip,
        (Some(want), Some(have)) if want == have => TeamCheck::Match,
        (Some(_), other) => TeamCheck::Mismatch {
            found: other.map(str::to_owned),
        },
    }
}

/// A mesma decisão de `updater::policy::disabled_reason` (App Translocation, sem escrita, fora de
/// bundle, explicação do empacotador): qualquer razão impede a instalação.
///
/// # Errors
/// [`MacosInstallError::NotUpdatable`] com a razão.
pub fn ensure_updatable(reason: Option<DisabledReason>) -> Result<(), MacosInstallError> {
    reason.map_or(Ok(()), |reason| {
        Err(MacosInstallError::NotUpdatable(reason))
    })
}

/// Trecho curto de `stderr` para mensagens de erro.
fn short(text: &str) -> String {
    let trimmed = text.trim();
    let mut out: String = trimmed.chars().take(STDERR_MAX_CHARS).collect();
    if trimmed.chars().count() > STDERR_MAX_CHARS {
        out.push('…');
    }
    out
}

fn describe(output: &CmdOutput) -> String {
    let detail = short(&output.stderr);
    match output.code {
        Some(code) if detail.is_empty() => format!("saiu com {code}"),
        Some(code) => format!("saiu com {code}: {detail}"),
        None => format!("encerrado por sinal: {detail}"),
    }
}

// ---------------------------------------------------------------------------------------------
// Montagem com desmontagem garantida
// ---------------------------------------------------------------------------------------------

/// Volume montado. O `detach -force` roda em [`MountGuard::detach`] ou, no mais tardar, no `Drop`
/// (erro, `?` ou pânico): o volume nunca fica montado de propósito.
struct MountGuard<'a> {
    host: &'a dyn Host,
    target: OsString,
    detached: bool,
}

impl<'a> MountGuard<'a> {
    fn new(host: &'a dyn Host, target: OsString) -> Self {
        Self {
            host,
            target,
            detached: false,
        }
    }

    /// Desmonta (uma vez só; chamadas seguintes e o `Drop` não repetem).
    fn detach(&mut self) {
        if std::mem::replace(&mut self.detached, true) {
            return;
        }
        match self.host.run(HDIUTIL_BIN, &detach_args(&self.target)) {
            Ok(output) if output.success => {}
            Ok(output) => tracing::warn!("hdiutil detach: {}", describe(&output)),
            Err(error) => tracing::warn!("hdiutil detach: {error}"),
        }
    }
}

impl Drop for MountGuard<'_> {
    fn drop(&mut self) {
        self.detach();
    }
}

/// O que instalar e onde.
#[derive(Debug, Clone, Copy)]
pub struct InstallPlan<'a> {
    /// `.dmg` baixado e já conferido.
    pub dmg: &'a Path,
    /// Bundle em execução (`/Applications/DB Modeling.app`).
    pub bundle: &'a Path,
    /// `team_id` do manifesto; `None` = build sem assinatura (sem verificação).
    pub team_id: Option<&'a str>,
    /// Pasta privada onde o `hdiutil` monta o volume.
    pub mount_root: &'a Path,
}

fn run_program(
    host: &dyn Host,
    program: &'static str,
    args: &[OsString],
) -> Result<CmdOutput, MacosInstallError> {
    host.run(program, args)
        .map_err(|source| MacosInstallError::Spawn { program, source })
}

/// `codesign --verify --deep --strict` em `app`.
fn verify_signature(host: &dyn Host, app: &Path) -> Result<(), MacosInstallError> {
    let output = run_program(host, CODESIGN_BIN, &verify_args(app))?;
    if output.success {
        Ok(())
    } else {
        Err(MacosInstallError::SignatureInvalid(describe(&output)))
    }
}

/// Verifica assinatura e Team ID do `.app` montado. Sem `team_id` no plano, não faz nada.
fn verify_mounted_app(
    host: &dyn Host,
    app: &Path,
    team_id: Option<&str>,
) -> Result<(), MacosInstallError> {
    let Some(expected) = team_id else {
        return Ok(());
    };
    verify_signature(host, app)?;
    let output = run_program(host, CODESIGN_BIN, &identity_args(app))?;
    // `codesign -d` escreve a identidade em stderr; junta os dois para não depender disso
    let found = parse_team_identifier(&format!("{}\n{}", output.stderr, output.stdout));
    match check_team(Some(expected), found.as_deref()) {
        TeamCheck::Skip | TeamCheck::Match => Ok(()),
        TeamCheck::Mismatch { found } => Err(MacosInstallError::TeamMismatch {
            expected: expected.to_owned(),
            found,
        }),
    }
}

/// Com o volume montado: verifica, copia e confere o resultado.
fn install_from_volume(
    host: &dyn Host,
    plan: &InstallPlan<'_>,
    attached: &Attached,
) -> Result<(), MacosInstallError> {
    let mount_point = attached
        .mount_point
        .as_deref()
        .filter(|path| path.starts_with(plan.mount_root))
        .ok_or(MacosInstallError::NoMountPoint)?;
    let mounted_app = mount_point.join(BUNDLE_NAME);
    if !host.is_dir(&mounted_app) {
        return Err(MacosInstallError::MissingApp(mounted_app));
    }
    // antes de qualquer cópia: assinatura e Team ID do que está montado
    verify_mounted_app(host, &mounted_app, plan.team_id)?;

    let output = run_program(host, RSYNC_BIN, &rsync_args(&mounted_app, plan.bundle))?;
    if !output.success {
        return Err(MacosInstallError::Rsync(describe(&output)));
    }
    if plan.team_id.is_some() {
        // a cópia não pode ter quebrado o selo do bundle instalado
        verify_signature(host, plan.bundle)?;
    }
    Ok(())
}

/// Instala `plan.dmg` sobre `plan.bundle`. O volume é desmontado em **todos** os caminhos.
///
/// # Errors
/// [`MacosInstallError`]; depois de [`MacosInstallError::Rsync`] o bundle pode estar inconsistente.
pub fn run_install(host: &dyn Host, plan: &InstallPlan<'_>) -> Result<(), MacosInstallError> {
    host.create_private_dir(plan.mount_root)
        .map_err(MacosInstallError::MountRoot)?;
    let result = attach_and_install(host, plan);
    host.remove_dir(plan.mount_root);
    result
}

fn attach_and_install(host: &dyn Host, plan: &InstallPlan<'_>) -> Result<(), MacosInstallError> {
    let output = run_program(host, HDIUTIL_BIN, &attach_args(plan.dmg, plan.mount_root))?;
    if !output.success {
        // pode ter montado algo antes de falhar: tenta desmontar o ponto padrão do volume
        let _guard = MountGuard::new(host, plan.mount_root.join(VOLUME_NAME).into_os_string());
        return Err(MacosInstallError::Attach(describe(&output)));
    }
    let attached = parse_attach_plist(&output.stdout);
    let target = attached
        .detach_target()
        .unwrap_or_else(|| plan.mount_root.join(VOLUME_NAME).into_os_string());
    let mut guard = MountGuard::new(host, target);
    let result = install_from_volume(host, plan, &attached);
    guard.detach();
    result
}

// ---------------------------------------------------------------------------------------------
// Execução real (só no macOS)
// ---------------------------------------------------------------------------------------------

/// Processos e diretórios reais. **NÃO VERIFICADO no macOS.**
#[cfg(target_os = "macos")]
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemHost;

#[cfg(target_os = "macos")]
impl Host for SystemHost {
    fn run(&self, program: &str, args: &[OsString]) -> io::Result<CmdOutput> {
        let output = std::process::Command::new(program).args(args).output()?;
        Ok(CmdOutput {
            success: output.status.success(),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn create_private_dir(&self, path: &Path) -> io::Result<()> {
        updater_net::private_dir::ensure_private_dir(path)
    }

    fn remove_dir(&self, path: &Path) {
        let _ = std::fs::remove_dir(path);
    }
}

/// Instalador do macOS Apple Silicon (ticket 06). **NÃO VERIFICADO no macOS.**
#[cfg(target_os = "macos")]
#[derive(Debug, Default, Clone, Copy)]
pub struct MacosInstaller;

#[cfg(target_os = "macos")]
impl crate::update_install::Installer for MacosInstaller {
    fn is_supported(&self) -> bool {
        // Intel segue com download manual (fora do escopo).
        cfg!(target_arch = "aarch64")
    }

    fn install(
        &self,
        update: &updater_net::round::VerifiedUpdate,
    ) -> Result<(), crate::update_install::InstallError> {
        install_update(update)
            .map_err(|error| crate::update_install::InstallError::Failed(error.to_string()))
    }
}

#[cfg(target_os = "macos")]
fn install_update(update: &updater_net::round::VerifiedUpdate) -> Result<(), MacosInstallError> {
    ensure_updatable(updater::policy::disabled_reason(
        &updater::policy::SystemEnv,
    ))?;
    let bundle = std::env::current_exe()
        .ok()
        .and_then(|exe| updater::policy::macos_bundle_from_exe(&exe))
        .ok_or(MacosInstallError::NotUpdatable(
            DisabledReason::UnknownAppLocation,
        ))?;
    let mount_root = mount_root_in(update.updates_dir(), std::process::id());
    run_install(
        &SystemHost,
        &InstallPlan {
            dmg: update.path(),
            bundle: &bundle,
            team_id: update.team_id(),
            mount_root: &mount_root,
        },
    )
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    const MOUNT: &str = "/tmp/dbm/DB Modeling";
    const APP: &str = "/tmp/dbm/DB Modeling/DB Modeling.app";
    const BUNDLE: &str = "/Applications/DB Modeling.app";
    const TEAM: &str = "ABCDE12345";

    fn plist(mount: Option<&str>) -> String {
        let mount_entry = mount.map_or_else(String::new, |path| {
            format!("<key>mount-point</key>\n<string>{path}</string>")
        });
        format!(
            "<?xml version=\"1.0\"?>\n<plist version=\"1.0\"><dict><key>system-entities</key><array>\
             <dict><key>content-hint</key><string>GUID_partition_scheme</string>\
             <key>dev-entry</key><string>/dev/disk4</string></dict>\
             <dict><key>dev-entry</key><string>/dev/disk4s1</string>{mount_entry}</dict>\
             </array></dict></plist>"
        )
    }

    /// Host falso: grava as chamadas e responde por programa/argumento.
    struct Fake {
        calls: RefCell<Vec<String>>,
        app_exists: bool,
        attach: CmdOutput,
        verify_ok: bool,
        verify_installed_ok: bool,
        identity: String,
        rsync_ok: bool,
        detach_ok: bool,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
                app_exists: true,
                attach: CmdOutput {
                    success: true,
                    code: Some(0),
                    stdout: plist(Some(MOUNT)),
                    stderr: String::new(),
                },
                verify_ok: true,
                verify_installed_ok: true,
                identity: format!("Executable=x\nTeamIdentifier={TEAM}\n"),
                rsync_ok: true,
                detach_ok: true,
            }
        }

        fn log(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }

        fn count(&self, prefix: &str) -> usize {
            self.log().iter().filter(|c| c.starts_with(prefix)).count()
        }

        fn first(&self, prefix: &str) -> Option<usize> {
            self.log().iter().position(|c| c.starts_with(prefix))
        }
    }

    fn out(success: bool, stderr: &str) -> CmdOutput {
        CmdOutput {
            success,
            code: Some(i32::from(!success)),
            stdout: String::new(),
            stderr: stderr.to_owned(),
        }
    }

    impl Host for Fake {
        fn run(&self, program: &str, args: &[OsString]) -> io::Result<CmdOutput> {
            let text: Vec<String> = args
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            self.calls
                .borrow_mut()
                .push(format!("{program} {}", text.join("|")));
            Ok(match (program, text.first().map(String::as_str)) {
                ("hdiutil", Some("attach")) => self.attach.clone(),
                ("hdiutil", Some("detach")) => out(self.detach_ok, "busy"),
                ("codesign", Some("--verify")) => {
                    let installed = text.last().is_some_and(|p| p == BUNDLE);
                    out(
                        if installed {
                            self.verify_installed_ok
                        } else {
                            self.verify_ok
                        },
                        "invalid signature",
                    )
                }
                ("codesign", Some("-d")) => out(true, &self.identity),
                ("rsync", _) => out(self.rsync_ok, "rsync: write failed"),
                _ => out(false, "inesperado"),
            })
        }

        fn is_dir(&self, path: &Path) -> bool {
            self.app_exists && path == Path::new(APP)
        }

        fn create_private_dir(&self, path: &Path) -> io::Result<()> {
            self.calls
                .borrow_mut()
                .push(format!("mkdir {}", path.display()));
            Ok(())
        }

        fn remove_dir(&self, path: &Path) {
            self.calls
                .borrow_mut()
                .push(format!("rmdir {}", path.display()));
        }
    }

    fn plan(team_id: Option<&str>) -> InstallPlan<'_> {
        InstallPlan {
            dmg: Path::new("/cache/db modeling.dmg"),
            bundle: Path::new(BUNDLE),
            team_id,
            mount_root: Path::new("/tmp/dbm"),
        }
    }

    // ---- linhas de comando ----

    #[test]
    fn attach_monta_em_pasta_privada_sem_navegar() {
        let args = attach_args(Path::new("/c/a b.dmg"), Path::new("/tmp/dbm"));
        let text: Vec<_> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            text,
            [
                "attach",
                "-nobrowse",
                "-readonly",
                "-plist",
                "-mountroot",
                "/tmp/dbm",
                "/c/a b.dmg"
            ]
        );
    }

    #[test]
    fn detach_e_sempre_forcado() {
        let args = detach_args(&OsString::from("/dev/disk4"));
        assert_eq!(args, ["detach", "-force", "/dev/disk4"].map(OsString::from));
    }

    #[test]
    fn rsync_leva_barras_finais_exclude_e_caminhos_com_espaco_intactos() {
        let args = rsync_args(Path::new(APP), Path::new(BUNDLE));
        let expected: Vec<OsString> = [
            "-av",
            "--delete",
            "--exclude",
            "Icon?",
            "/tmp/dbm/DB Modeling/DB Modeling.app/",
            "/Applications/DB Modeling.app/",
        ]
        .map(OsString::from)
        .to_vec();
        assert_eq!(args, expected);
        // barra já existente não duplica
        assert_eq!(with_trailing_slash(Path::new("/a/")), OsString::from("/a/"));
    }

    #[test]
    fn codesign_verify_e_identity() {
        let verify = verify_args(Path::new(APP));
        assert_eq!(
            verify,
            ["--verify", "--deep", "--strict", APP].map(OsString::from)
        );
        let identity = identity_args(Path::new(APP));
        assert_eq!(identity, ["-d", "--verbose=4", APP].map(OsString::from));
    }

    #[test]
    fn pasta_de_montagem_leva_o_pid() {
        assert_eq!(
            mount_root_in(Path::new("/var/T"), 42),
            PathBuf::from("/var/T/db-modeling-update-42")
        );
    }

    // ---- plist ----

    #[test]
    fn plist_traz_ponto_de_montagem_e_dispositivos() {
        let attached = parse_attach_plist(&plist(Some(MOUNT)));
        assert_eq!(attached.mount_point, Some(PathBuf::from(MOUNT)));
        assert_eq!(attached.dev_entries, ["/dev/disk4", "/dev/disk4s1"]);
        assert_eq!(attached.detach_target(), Some(OsString::from(MOUNT)));
    }

    #[test]
    fn plist_sem_ponto_de_montagem_desmonta_pelo_dispositivo() {
        let attached = parse_attach_plist(&plist(None));
        assert_eq!(attached.mount_point, None);
        assert_eq!(attached.detach_target(), Some(OsString::from("/dev/disk4")));
    }

    #[test]
    fn plist_desescapa_xml_e_ignora_lixo() {
        let xml = "<key>mount-point</key>\n\t<string>/t/A &amp; B &lt;1&gt;</string>";
        assert_eq!(
            parse_attach_plist(xml).mount_point,
            Some(PathBuf::from("/t/A & B <1>"))
        );
        let empty = parse_attach_plist("lixo <key>mount-point</key><string></string>");
        assert_eq!(empty, Attached::default());
        assert_eq!(empty.detach_target(), None);
    }

    // ---- Team ID ----

    #[test]
    fn team_id_sai_da_linha_do_codesign() {
        let text =
            "Executable=/x/db-modeling\nIdentifier=com.dbmodeling.app\nTeamIdentifier=ABCDE12345\n";
        assert_eq!(parse_team_identifier(text).as_deref(), Some("ABCDE12345"));
        assert_eq!(parse_team_identifier("TeamIdentifier=not set"), None);
        assert_eq!(parse_team_identifier("TeamIdentifier="), None);
        assert_eq!(parse_team_identifier("sem nada"), None);
    }

    #[test]
    fn decisao_do_team_id() {
        assert_eq!(check_team(None, Some("X")), TeamCheck::Skip);
        assert_eq!(check_team(None, None), TeamCheck::Skip);
        assert_eq!(check_team(Some(TEAM), Some(TEAM)), TeamCheck::Match);
        assert_eq!(
            check_team(Some(TEAM), Some("ZZZZZ99999")),
            TeamCheck::Mismatch {
                found: Some("ZZZZZ99999".into())
            }
        );
        // caixa diferente e app sem Team ID também são recusados
        assert!(matches!(
            check_team(Some(TEAM), Some("abcde12345")),
            TeamCheck::Mismatch { .. }
        ));
        assert_eq!(
            check_team(Some(TEAM), None),
            TeamCheck::Mismatch { found: None }
        );
    }

    #[test]
    fn razao_de_desativacao_impede_instalar() {
        assert!(ensure_updatable(None).is_ok());
        for reason in [
            DisabledReason::AppTranslocation,
            DisabledReason::NotWritable,
        ] {
            assert!(matches!(
                ensure_updatable(Some(reason)),
                Err(MacosInstallError::NotUpdatable(_))
            ));
        }
    }

    // ---- fluxo ----

    #[test]
    fn build_assinado_verifica_antes_de_copiar_e_desmonta() {
        let host = Fake::new();
        run_install(&host, &plan(Some(TEAM))).expect("instala");
        let verify =
            host.first("codesign --verify|--deep|--strict|/tmp/dbm/DB Modeling/DB Modeling.app");
        let rsync = host.first("rsync");
        assert!(
            verify.is_some() && rsync.is_some() && verify < rsync,
            "{:?}",
            host.log()
        );
        // depois da cópia, o bundle instalado é conferido de novo
        assert_eq!(
            host.count("codesign --verify|--deep|--strict|/Applications/DB Modeling.app"),
            1
        );
        assert_eq!(host.count("hdiutil detach|-force|/tmp/dbm/DB Modeling"), 1);
        assert_eq!(
            host.log().last().map(String::as_str),
            Some("rmdir /tmp/dbm")
        );
    }

    #[test]
    fn build_sem_assinatura_pula_codesign() {
        let host = Fake::new();
        run_install(&host, &plan(None)).expect("instala");
        assert_eq!(host.count("codesign"), 0);
        assert_eq!(host.count("rsync"), 1);
        assert_eq!(host.count("hdiutil detach"), 1);
    }

    #[test]
    fn team_id_divergente_recusa_sem_copiar_e_desmonta() {
        let mut host = Fake::new();
        host.identity = "TeamIdentifier=ZZZZZ99999".into();
        let error = run_install(&host, &plan(Some(TEAM))).expect_err("recusa");
        assert!(
            matches!(error, MacosInstallError::TeamMismatch { .. }),
            "{error}"
        );
        assert!(error.to_string().contains("nada foi copiado"));
        assert_eq!(host.count("rsync"), 0);
        assert_eq!(host.count("hdiutil detach"), 1);
    }

    #[test]
    fn app_sem_team_id_em_build_assinado_e_recusado() {
        let mut host = Fake::new();
        host.identity = "TeamIdentifier=not set".into();
        assert!(run_install(&host, &plan(Some(TEAM))).is_err());
        assert_eq!(host.count("rsync"), 0);
        assert_eq!(host.count("hdiutil detach"), 1);
    }

    #[test]
    fn assinatura_invalida_recusa_sem_copiar_e_desmonta() {
        let mut host = Fake::new();
        host.verify_ok = false;
        let error = run_install(&host, &plan(Some(TEAM))).expect_err("recusa");
        assert!(
            matches!(error, MacosInstallError::SignatureInvalid(_)),
            "{error}"
        );
        assert_eq!(host.count("rsync"), 0);
        assert_eq!(host.count("codesign -d"), 0);
        assert_eq!(host.count("hdiutil detach"), 1);
    }

    #[test]
    fn rsync_com_erro_desmonta_e_avisa_do_bundle_inconsistente() {
        let mut host = Fake::new();
        host.rsync_ok = false;
        let error = run_install(&host, &plan(None)).expect_err("falha");
        assert!(matches!(error, MacosInstallError::Rsync(_)), "{error}");
        assert!(error.to_string().contains("inconsistente"));
        assert_eq!(host.count("hdiutil detach"), 1);
    }

    #[test]
    fn assinatura_quebrada_depois_da_copia_e_erro() {
        let mut host = Fake::new();
        host.verify_installed_ok = false;
        let error = run_install(&host, &plan(Some(TEAM))).expect_err("falha");
        assert!(
            matches!(error, MacosInstallError::SignatureInvalid(_)),
            "{error}"
        );
        assert_eq!(host.count("hdiutil detach"), 1);
    }

    #[test]
    fn dmg_sem_o_app_desmonta_e_nao_copia() {
        let mut host = Fake::new();
        host.app_exists = false;
        let error = run_install(&host, &plan(None)).expect_err("falha");
        assert!(matches!(error, MacosInstallError::MissingApp(_)), "{error}");
        assert_eq!(host.count("rsync"), 0);
        assert_eq!(host.count("hdiutil detach"), 1);
    }

    #[test]
    fn fluxo_sem_ponto_de_montagem_desmonta_pelo_dispositivo() {
        let mut host = Fake::new();
        host.attach.stdout = plist(None);
        let error = run_install(&host, &plan(None)).expect_err("falha");
        assert!(matches!(error, MacosInstallError::NoMountPoint), "{error}");
        assert_eq!(host.count("hdiutil detach|-force|/dev/disk4"), 1);
        assert_eq!(host.count("rsync"), 0);
    }

    #[test]
    fn ponto_de_montagem_fora_do_mount_root_e_recusado_e_desmonta() {
        let mut host = Fake::new();
        host.attach.stdout = plist(Some("/Volumes/OutroVolume"));
        let error = run_install(&host, &plan(None)).expect_err("fora do mount_root");
        assert!(matches!(error, MacosInstallError::NoMountPoint), "{error}");
        assert_eq!(host.count("hdiutil detach|-force|/Volumes/OutroVolume"), 1);
        assert_eq!(host.count("rsync"), 0);
    }

    #[test]
    fn plist_ilegivel_tenta_desmontar_o_ponto_padrao_do_volume() {
        let mut host = Fake::new();
        host.attach.stdout = "lixo".into();
        assert!(run_install(&host, &plan(None)).is_err());
        assert_eq!(host.count("hdiutil detach|-force|/tmp/dbm/DB Modeling"), 1);
    }

    #[test]
    fn attach_que_falha_ainda_tenta_desmontar() {
        let mut host = Fake::new();
        host.attach = out(false, "hdiutil: attach failed - corrupt");
        let error = run_install(&host, &plan(None)).expect_err("falha");
        assert!(matches!(error, MacosInstallError::Attach(_)), "{error}");
        assert_eq!(host.count("hdiutil detach"), 1);
        assert_eq!(host.count("rsync"), 0);
    }

    #[test]
    fn falha_no_detach_nao_derruba_instalacao_concluida() {
        let mut host = Fake::new();
        host.detach_ok = false;
        assert!(run_install(&host, &plan(None)).is_ok());
        assert_eq!(host.count("hdiutil detach"), 1);
    }

    #[test]
    fn guarda_desmonta_no_drop_uma_unica_vez() {
        let host = Fake::new();
        {
            let _guard = MountGuard::new(&host, OsString::from(MOUNT));
        }
        assert_eq!(host.count("hdiutil detach"), 1);
        let mut guard = MountGuard::new(&host, OsString::from(MOUNT));
        guard.detach();
        drop(guard);
        assert_eq!(host.count("hdiutil detach"), 2);
    }

    #[test]
    fn guarda_desmonta_mesmo_com_panico() {
        let host = Fake::new();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = MountGuard::new(&host, OsString::from(MOUNT));
            panic!("falha no meio da cópia");
        }));
        assert!(result.is_err());
        assert_eq!(host.count("hdiutil detach"), 1);
    }

    #[test]
    fn stderr_longo_e_truncado() {
        let long = "x".repeat(STDERR_MAX_CHARS + 50);
        let text = short(&long);
        assert_eq!(text.chars().count(), STDERR_MAX_CHARS + 1);
        assert!(text.ends_with('…'));
        assert_eq!(describe(&out(false, "")), "saiu com 1");
    }
}
