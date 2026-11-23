//! Política de atualização: `auto_update` e razões de desativação.
//!
//! O ambiente (variáveis, arquivos, permissões) entra por [`PolicyEnv`], de modo que os testes
//! não tocam no sistema. A política **falha fechada**: se não der para saber onde o app está
//! instalado, a atualização fica desativada com uma razão explícita.

use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::constants::{
    APP_TRANSLOCATION_MARKER, ENV_PROGRAM_FILES, ENV_UPDATE_EXPLANATION, MACOS_BUNDLE_EXE_DEPTH,
    MACOS_BUNDLE_EXTENSION, NORMALIZED_PATH_SEPARATOR, UPDATE_EXPLANATION_FILE,
    VERBATIM_PATH_PREFIX, WINDOWS_PATH_SEPARATOR, WRITE_PROBE_PREFIX,
};
use crate::platform::TargetOs;

/// Por que o auto-update está desativado, na ordem de precedência de [`disabled_reason`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisabledReason {
    /// Texto vindo de `DBM_UPDATE_EXPLANATION` (override do usuário/empacotador).
    Env(String),
    /// Texto do arquivo `/usr/lib/db-modeling/update-explanation` (`.deb`/`.rpm`).
    File(String),
    /// SO ou arquitetura sem política de atualização conhecida.
    UnsupportedPlatform,
    /// Não foi possível determinar o executável/diretório do app (Windows) ou o bundle `.app`
    /// (macOS: binário avulso, de desenvolvimento, fora de um bundle).
    UnknownAppLocation,
    /// Windows: diretório do app sob `%ProgramFiles%` (instalação por máquina).
    ProgramFiles,
    /// macOS: bundle sob App Translocation.
    AppTranslocation,
    /// Sem permissão de escrita onde o app seria atualizado (aproximação, ver [`PolicyEnv`]).
    NotWritable,
}

/// Ambiente consultado pela política; implementado por [`SystemEnv`] e por fakes de teste.
pub trait PolicyEnv {
    /// Valor de uma variável de ambiente.
    fn var(&self, name: &str) -> Option<String>;
    /// Conteúdo de um arquivo de texto, se existir e for legível.
    fn read_file(&self, path: &Path) -> Option<String>;
    /// Sistema operacional em execução.
    fn target_os(&self) -> Option<TargetOs>;
    /// Diretório do app (Windows: pasta do executável) ou bundle `.app` (macOS). `None` quando
    /// não dá para determinar: a política então **bloqueia** a atualização.
    fn app_dir(&self) -> Option<PathBuf>;
    /// Forma canônica do caminho (expande nomes 8.3, links e prefixo `\\?\`), se possível.
    fn canonicalize(&self, path: &Path) -> Option<PathBuf>;
    /// Se o processo consegue criar arquivos em `dir`. É uma **aproximação** do direito de
    /// escrita (ACLs, volumes somente leitura e cotas podem divergir do resultado real).
    fn is_writable(&self, dir: &Path) -> bool;
}

/// Ambiente real do sistema.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemEnv;

/// Bundle `.app` que contém o executável (`<bundle>/Contents/MacOS/<bin>`), ou `None` se o
/// executável estiver fora de um bundle (binário avulso ou de desenvolvimento).
#[must_use]
pub fn macos_bundle_from_exe(exe: &Path) -> Option<PathBuf> {
    let bundle = exe.ancestors().nth(MACOS_BUNDLE_EXE_DEPTH)?;
    bundle
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case(MACOS_BUNDLE_EXTENSION))
        .then(|| bundle.to_path_buf())
}

/// Diretório sondado para estimar o direito de atualizar. No macOS é o **pai** do bundle: criar
/// um arquivo dentro do `.app` (raiz ou `Contents/`) quebraria o selo do codesign. No Windows é
/// a própria pasta do app, onde o instalador grava `install\` e `updates\`.
#[must_use]
pub fn write_probe_dir(os: TargetOs, app_dir: &Path) -> Option<PathBuf> {
    match os {
        TargetOs::Windows => Some(app_dir.to_path_buf()),
        TargetOs::MacOs => app_dir.parent().map(Path::to_path_buf),
        TargetOs::Linux => None,
    }
}

/// Arquivo de sonda removido ao sair de escopo (RAII), mesmo em caso de pânico.
struct ProbeFile(PathBuf);

impl Drop for ProbeFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Contador que garante nome único por sonda dentro do processo.
static PROBE_COUNTER: AtomicU64 = AtomicU64::new(0);

impl PolicyEnv for SystemEnv {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn read_file(&self, path: &Path) -> Option<String> {
        fs::read_to_string(path).ok()
    }

    fn target_os(&self) -> Option<TargetOs> {
        TargetOs::current()
    }

    fn app_dir(&self) -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        match self.target_os()? {
            TargetOs::Windows => exe.parent().map(Path::to_path_buf),
            TargetOs::MacOs => macos_bundle_from_exe(&exe),
            TargetOs::Linux => None,
        }
    }

    fn canonicalize(&self, path: &Path) -> Option<PathBuf> {
        fs::canonicalize(path).ok()
    }

    fn is_writable(&self, dir: &Path) -> bool {
        let name = format!(
            "{WRITE_PROBE_PREFIX}{}-{}",
            std::process::id(),
            PROBE_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let path = dir.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                let _guard = ProbeFile(path);
                drop(file);
                true
            }
            Err(_) => false,
        }
    }
}

/// Texto não vazio (após `trim`), senão `None`.
fn non_blank(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// Forma comparável de um caminho: sem `\\?\`, minúsculas, `/` como separador e sem `/` final.
fn normalize(path: &str) -> String {
    let bare = path.strip_prefix(VERBATIM_PATH_PREFIX).unwrap_or(path);
    bare.replace(
        WINDOWS_PATH_SEPARATOR,
        &NORMALIZED_PATH_SEPARATOR.to_string(),
    )
    .trim_end_matches(NORMALIZED_PATH_SEPARATOR)
    .to_lowercase()
}

/// Caminho canonicalizado pelo ambiente (se possível; senão o próprio texto), já normalizado.
fn comparable(env: &dyn PolicyEnv, path: &Path) -> String {
    let resolved = env.canonicalize(path).unwrap_or_else(|| path.to_path_buf());
    normalize(&resolved.to_string_lossy())
}

/// `dir` é `root` ou está dentro dele, por componentes e sem diferenciar caixa.
fn is_under(env: &dyn PolicyEnv, dir: &Path, root: &Path) -> bool {
    let root = comparable(env, root);
    if root.is_empty() {
        return false;
    }
    let dir = comparable(env, dir);
    dir == root || dir.starts_with(&format!("{root}{NORMALIZED_PATH_SEPARATOR}"))
}

fn under_program_files(env: &dyn PolicyEnv, dir: &Path) -> bool {
    ENV_PROGRAM_FILES
        .iter()
        .filter_map(|name| env.var(name))
        .any(|root| is_under(env, dir, Path::new(&root)))
}

/// Razão de desativação, na ordem de precedência:
/// 1. variável `DBM_UPDATE_EXPLANATION`;
/// 2. arquivo `/usr/lib/db-modeling/update-explanation`;
/// 3. SO desconhecido, ou local do app indeterminado (Windows/macOS): falha fechada;
/// 4. Windows com o app sob `%ProgramFiles%` ou sem escrita;
/// 5. macOS com o bundle em App Translocation ou sem escrita no diretório pai do bundle.
///
/// `None` significa que o ambiente permite atualizar.
#[must_use]
pub fn disabled_reason(env: &dyn PolicyEnv) -> Option<DisabledReason> {
    if let Some(text) = env.var(ENV_UPDATE_EXPLANATION).and_then(|t| non_blank(&t)) {
        return Some(DisabledReason::Env(text));
    }
    if let Some(text) = env
        .read_file(Path::new(UPDATE_EXPLANATION_FILE))
        .and_then(|t| non_blank(&t))
    {
        return Some(DisabledReason::File(text));
    }
    let Some(os) = env.target_os() else {
        return Some(DisabledReason::UnsupportedPlatform);
    };
    if os == TargetOs::Linux {
        return None;
    }
    let Some(app_dir) = env.app_dir() else {
        return Some(DisabledReason::UnknownAppLocation);
    };
    if os == TargetOs::Windows && under_program_files(env, &app_dir) {
        return Some(DisabledReason::ProgramFiles);
    }
    if os == TargetOs::MacOs && app_dir.to_string_lossy().contains(APP_TRANSLOCATION_MARKER) {
        return Some(DisabledReason::AppTranslocation);
    }
    match write_probe_dir(os, &app_dir) {
        Some(probe) if env.is_writable(&probe) => None,
        Some(_) => Some(DisabledReason::NotWritable),
        None => Some(DisabledReason::UnknownAppLocation),
    }
}

/// Pode haver checagem **automática** (polling)? Exige `auto_update` ligado e nenhuma razão.
#[must_use]
pub fn may_check_automatically(auto_update: bool, reason: Option<&DisabledReason>) -> bool {
    auto_update && reason.is_none()
}

/// Pode haver checagem **manual**? Só uma razão de desativação a impede (`auto_update = false`
/// não impede).
#[must_use]
pub fn may_check_manually(reason: Option<&DisabledReason>) -> bool {
    reason.is_none()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    /// Ambiente falso: nada toca o sistema real.
    struct FakeEnv {
        vars: HashMap<String, String>,
        files: HashMap<PathBuf, String>,
        canonical: HashMap<PathBuf, PathBuf>,
        os: Option<TargetOs>,
        app_dir: Option<PathBuf>,
        writable: bool,
        probed: RefCell<Vec<PathBuf>>,
    }

    impl FakeEnv {
        fn new(os: TargetOs) -> Self {
            Self {
                vars: HashMap::new(),
                files: HashMap::new(),
                canonical: HashMap::new(),
                os: Some(os),
                app_dir: None,
                writable: true,
                probed: RefCell::new(Vec::new()),
            }
        }

        fn var(mut self, k: &str, v: &str) -> Self {
            self.vars.insert(k.into(), v.into());
            self
        }

        fn file(mut self, text: &str) -> Self {
            self.files
                .insert(PathBuf::from(UPDATE_EXPLANATION_FILE), text.into());
            self
        }

        fn app(mut self, dir: &str) -> Self {
            self.app_dir = Some(PathBuf::from(dir));
            self
        }

        fn canonical(mut self, from: &str, to: &str) -> Self {
            self.canonical.insert(from.into(), to.into());
            self
        }

        fn read_only(mut self) -> Self {
            self.writable = false;
            self
        }
    }

    impl PolicyEnv for FakeEnv {
        fn var(&self, name: &str) -> Option<String> {
            self.vars.get(name).cloned()
        }
        fn read_file(&self, path: &Path) -> Option<String> {
            self.files.get(path).cloned()
        }
        fn target_os(&self) -> Option<TargetOs> {
            self.os
        }
        fn app_dir(&self) -> Option<PathBuf> {
            self.app_dir.clone()
        }
        fn canonicalize(&self, path: &Path) -> Option<PathBuf> {
            self.canonical.get(path).cloned()
        }
        fn is_writable(&self, dir: &Path) -> bool {
            self.probed.borrow_mut().push(dir.to_path_buf());
            self.writable
        }
    }

    #[test]
    fn linux_limpo_permite_atualizar() {
        assert_eq!(disabled_reason(&FakeEnv::new(TargetOs::Linux)), None);
    }

    #[test]
    fn variavel_de_ambiente_desativa() {
        let env =
            FakeEnv::new(TargetOs::Linux).var("DBM_UPDATE_EXPLANATION", "  Use o gerenciador  ");
        assert_eq!(
            disabled_reason(&env),
            Some(DisabledReason::Env("Use o gerenciador".into()))
        );
    }

    #[test]
    fn variavel_vazia_e_ignorada() {
        let env = FakeEnv::new(TargetOs::Linux).var("DBM_UPDATE_EXPLANATION", "   ");
        assert_eq!(disabled_reason(&env), None);
    }

    #[test]
    fn arquivo_de_explicacao_desativa() {
        let env = FakeEnv::new(TargetOs::Linux).file("Atualize via apt\n");
        assert_eq!(
            disabled_reason(&env),
            Some(DisabledReason::File("Atualize via apt".into()))
        );
    }

    #[test]
    fn arquivo_vazio_e_ignorado() {
        assert_eq!(
            disabled_reason(&FakeEnv::new(TargetOs::Linux).file("\n")),
            None
        );
    }

    #[test]
    fn variavel_tem_precedencia_sobre_arquivo() {
        let env = FakeEnv::new(TargetOs::Linux)
            .file("do arquivo")
            .var("DBM_UPDATE_EXPLANATION", "da variável");
        assert_eq!(
            disabled_reason(&env),
            Some(DisabledReason::Env("da variável".into()))
        );
    }

    #[test]
    fn arquivo_tem_precedencia_sobre_windows_program_files() {
        let env = FakeEnv::new(TargetOs::Windows)
            .file("do arquivo")
            .var("ProgramFiles", r"C:\Program Files")
            .app(r"C:\Program Files\DB Modeling");
        assert_eq!(
            disabled_reason(&env),
            Some(DisabledReason::File("do arquivo".into()))
        );
    }

    #[test]
    fn windows_sob_program_files_desativa() {
        let env = FakeEnv::new(TargetOs::Windows)
            .var("ProgramFiles", r"C:\Program Files")
            .app(r"C:\Program Files\DB Modeling");
        assert_eq!(disabled_reason(&env), Some(DisabledReason::ProgramFiles));
    }

    #[test]
    fn windows_program_files_ignora_caixa_e_x86() {
        let env = FakeEnv::new(TargetOs::Windows)
            .var("ProgramFiles(x86)", r"C:\Program Files (x86)")
            .app(r"c:\program files (x86)\db modeling");
        assert_eq!(disabled_reason(&env), Some(DisabledReason::ProgramFiles));
    }

    #[test]
    fn windows_prefixo_verbatim_nao_escapa_da_checagem() {
        let env = FakeEnv::new(TargetOs::Windows)
            .var("ProgramFiles", r"C:\Program Files")
            .app(r"\\?\C:\Program Files\App");
        assert_eq!(disabled_reason(&env), Some(DisabledReason::ProgramFiles));
    }

    #[test]
    fn windows_nome_8_3_e_expandido_pela_canonicalizacao_injetada() {
        let env = FakeEnv::new(TargetOs::Windows)
            .var("ProgramFiles", r"C:\Program Files")
            .app(r"C:\PROGRA~1\App")
            .canonical(r"C:\PROGRA~1\App", r"\\?\C:\Program Files\App");
        assert_eq!(disabled_reason(&env), Some(DisabledReason::ProgramFiles));
    }

    #[test]
    fn windows_nome_8_3_sem_canonicalizacao_cai_no_texto_e_nao_casa() {
        // Sem ajuda do sistema só resta o texto: documenta o limite do fallback.
        let env = FakeEnv::new(TargetOs::Windows)
            .var("ProgramFiles", r"C:\Program Files")
            .app(r"C:\PROGRA~1\App");
        assert_eq!(disabled_reason(&env), None);
    }

    #[test]
    fn windows_prefixo_parecido_nao_conta_como_program_files() {
        let env = FakeEnv::new(TargetOs::Windows)
            .var("ProgramFiles", r"C:\Program Files")
            .app(r"C:\Program Files Custom\DB Modeling");
        assert_eq!(disabled_reason(&env), None);
    }

    #[test]
    fn windows_sem_escrita_desativa_e_sonda_a_pasta_do_app() {
        let env = FakeEnv::new(TargetOs::Windows)
            .app(r"D:\Apps\DB Modeling")
            .read_only();
        assert_eq!(disabled_reason(&env), Some(DisabledReason::NotWritable));
        assert_eq!(
            *env.probed.borrow(),
            vec![PathBuf::from(r"D:\Apps\DB Modeling")]
        );
    }

    #[test]
    fn windows_por_usuario_com_escrita_permite() {
        let env = FakeEnv::new(TargetOs::Windows)
            .var("ProgramFiles", r"C:\Program Files")
            .app(r"C:\Users\ana\AppData\Local\Programs\DB Modeling");
        assert_eq!(disabled_reason(&env), None);
    }

    #[test]
    fn windows_sem_diretorio_do_app_bloqueia() {
        let env = FakeEnv::new(TargetOs::Windows);
        assert_eq!(
            disabled_reason(&env),
            Some(DisabledReason::UnknownAppLocation)
        );
    }

    #[test]
    fn macos_sem_bundle_bloqueia() {
        let env = FakeEnv::new(TargetOs::MacOs);
        assert_eq!(
            disabled_reason(&env),
            Some(DisabledReason::UnknownAppLocation)
        );
    }

    #[test]
    fn so_desconhecido_bloqueia() {
        let mut env = FakeEnv::new(TargetOs::Linux);
        env.os = None;
        assert_eq!(
            disabled_reason(&env),
            Some(DisabledReason::UnsupportedPlatform)
        );
    }

    #[test]
    fn macos_em_app_translocation_desativa() {
        let env = FakeEnv::new(TargetOs::MacOs)
            .app("/private/var/folders/ab/xyz/T/AppTranslocation/8A1B/d/DB Modeling.app");
        assert_eq!(
            disabled_reason(&env),
            Some(DisabledReason::AppTranslocation)
        );
    }

    #[test]
    fn macos_sem_escrita_desativa_e_sonda_o_pai_do_bundle() {
        let env = FakeEnv::new(TargetOs::MacOs)
            .app("/Applications/DB Modeling.app")
            .read_only();
        assert_eq!(disabled_reason(&env), Some(DisabledReason::NotWritable));
        assert_eq!(
            *env.probed.borrow(),
            vec![PathBuf::from("/Applications")],
            "nunca sonda dentro do .app (selo do codesign)"
        );
    }

    #[test]
    fn macos_em_applications_com_escrita_permite() {
        let env = FakeEnv::new(TargetOs::MacOs).app("/Applications/DB Modeling.app");
        assert_eq!(disabled_reason(&env), None);
    }

    #[test]
    fn auto_update_desligado_recusa_checagem_automatica_mas_nao_a_manual() {
        assert!(!may_check_automatically(false, None));
        assert!(may_check_manually(None));
    }

    #[test]
    fn razao_ativa_recusa_automatica_e_manual() {
        let reason = DisabledReason::ProgramFiles;
        assert!(!may_check_automatically(true, Some(&reason)));
        assert!(!may_check_manually(Some(&reason)));
    }

    #[test]
    fn auto_update_ligado_sem_razao_permite_ambas() {
        assert!(may_check_automatically(true, None));
        assert!(may_check_manually(None));
    }

    #[test]
    fn bundle_e_o_ancestral_app_na_profundidade_esperada() {
        let exe = Path::new("/Applications/DB Modeling.app/Contents/MacOS/db-modeling");
        assert_eq!(
            macos_bundle_from_exe(exe),
            Some(PathBuf::from("/Applications/DB Modeling.app"))
        );
        let upper = Path::new("/Applications/X.APP/Contents/MacOS/x");
        assert_eq!(
            macos_bundle_from_exe(upper),
            Some(PathBuf::from("/Applications/X.APP"))
        );
    }

    #[test]
    fn binario_fora_de_bundle_nao_tem_bundle() {
        for exe in [
            "/usr/local/bin/db-modeling",
            "/Users/ana/dev/target/debug/app",
            "/Applications/Foo.app/Contents/db",
            "/Foo.app/db",
            "db-modeling",
        ] {
            assert_eq!(macos_bundle_from_exe(Path::new(exe)), None, "{exe}");
        }
    }

    #[test]
    fn sonda_fica_no_pai_no_macos_e_na_pasta_do_app_no_windows() {
        let bundle = Path::new("/Applications/DB Modeling.app");
        assert_eq!(
            write_probe_dir(TargetOs::MacOs, bundle),
            Some(PathBuf::from("/Applications"))
        );
        let app = Path::new(r"C:\Users\ana\App");
        assert_eq!(
            write_probe_dir(TargetOs::Windows, app),
            Some(app.to_path_buf())
        );
        assert_eq!(write_probe_dir(TargetOs::Linux, app), None);
        assert_eq!(write_probe_dir(TargetOs::MacOs, Path::new("/")), None);
    }

    #[test]
    fn sistema_le_variavel_e_arquivo() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("explicacao");
        fs::write(&file, "texto").expect("escrever");
        assert_eq!(SystemEnv.read_file(&file).as_deref(), Some("texto"));
        assert_eq!(SystemEnv.read_file(&dir.path().join("nao-existe")), None);
        assert_eq!(SystemEnv.var("DBM_VARIAVEL_QUE_NAO_EXISTE_NUNCA"), None);
    }

    #[test]
    fn sistema_diretorio_gravavel_e_sem_sobras() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(SystemEnv.is_writable(dir.path()));
        assert!(SystemEnv.is_writable(dir.path()), "nome único a cada sonda");
        assert_eq!(fs::read_dir(dir.path()).expect("ls").count(), 0);
    }

    #[test]
    fn sistema_diretorio_inexistente_nao_e_gravavel() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(!SystemEnv.is_writable(&dir.path().join("nao-existe")));
    }

    #[cfg(unix)]
    #[test]
    fn sistema_diretorio_somente_leitura_nao_e_gravavel() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).expect("chmod");
        // Como root a escrita passa mesmo assim: não há o que afirmar.
        let root_bypasses = fs::File::create(dir.path().join("x")).is_ok();
        let writable = SystemEnv.is_writable(dir.path());
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).expect("chmod");
        if !root_bypasses {
            assert!(!writable);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn sistema_linux_nao_tem_diretorio_de_app_e_nao_e_bloqueado_por_ele() {
        assert_eq!(SystemEnv.app_dir(), None);
        assert_eq!(SystemEnv.target_os(), Some(TargetOs::Linux));
    }
}
