//! Jobs de troca de arquivos, com `apply` e `rollback` (modelo do `updater.rs` do Zed).
//!
//! Ordem do plano ([`plan`]):
//! 1. criar `old\` e mover para lá os arquivos antigos que serão substituídos (não se apaga nada:
//!    "instalar arquivos novos pode falhar");
//! 2. mover os arquivos de `install\` para o lugar;
//! 3. apagar `updates` (a flag), `install` e `old` (sem rollback: falha só vira aviso).
//!
//! Cada job tem prazo ([`RetryPolicy`]): erro de E/S que não seja `NotFound` é repetido a cada
//! intervalo até estourar o prazo. Se um job falha, os já concluídos desfazem em **ordem inversa**
//! ([`Failure::rollback`]) e o chamador **não** relança o app.
//!
//! Renomear (mover no mesmo volume) é permitido para um `.exe` em execução, por isso o app e o
//! próprio helper entram na troca sem precisar terminar antes.

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crate::constants::{MANAGED_FILES, ManagedFile, job_timeout, retry_interval};
use crate::fsops::Fs;
use crate::layout::{Layout, display_name};

/// Prazo por job e intervalo entre tentativas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Tempo máximo tentando um job.
    pub timeout: Duration,
    /// Pausa entre tentativas.
    pub interval: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            timeout: job_timeout(),
            interval: retry_interval(),
        }
    }
}

/// Repete `op` enquanto falhar por E/S até o prazo. `NotFound` aborta de imediato (repetir não
/// faz o arquivo aparecer). O sono entra por parâmetro: o teste não espera de verdade.
///
/// # Errors
/// O último erro de `op`.
pub fn retry(
    policy: &RetryPolicy,
    sleep: &mut dyn FnMut(Duration),
    op: &mut dyn FnMut() -> io::Result<()>,
) -> io::Result<()> {
    let mut waited = Duration::ZERO;
    loop {
        match op() {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Err(error),
            Err(error) => {
                if waited >= policy.timeout {
                    return Err(error);
                }
                sleep(policy.interval);
                waited += policy.interval;
            }
        }
    }
}

/// Um passo da troca.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    /// Cria a pasta (e os pais). Rollback: remove se estiver vazia.
    MakeDir(PathBuf),
    /// Move `from` para `to`. Rollback: move de volta.
    Move {
        /// Origem.
        from: PathBuf,
        /// Destino.
        to: PathBuf,
        /// Arquivo gerenciado a que o movimento pertence (para a mensagem de erro).
        file: ManagedFile,
    },
    /// Apaga a pasta com tudo dentro, sem rollback; falha só gera aviso.
    RemoveDir(PathBuf),
}

impl Job {
    /// Passo que não bloqueia a troca se falhar e não tem rollback.
    #[must_use]
    pub fn is_best_effort(&self) -> bool {
        matches!(self, Self::RemoveDir(_))
    }

    /// Descrição para o log.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::MakeDir(path) => format!("criar {}", path.display()),
            Self::Move { from, to, .. } => format!("mover {} -> {}", from.display(), to.display()),
            Self::RemoveDir(path) => format!("apagar {}", path.display()),
        }
    }

    /// Arquivo gerenciado envolvido (só nos movimentos).
    #[must_use]
    pub fn file(&self) -> Option<ManagedFile> {
        match self {
            Self::Move { file, .. } => Some(file),
            _ => None,
        }
    }

    pub(crate) fn apply(&self, fs: &dyn Fs) -> io::Result<()> {
        match self {
            Self::MakeDir(path) => fs.create_dir_all(path),
            Self::Move { from, to, .. } => fs.rename(from, to),
            Self::RemoveDir(path) => fs.remove_all(path),
        }
    }

    fn rollback(&self, fs: &dyn Fs) -> io::Result<()> {
        match self {
            // a pasta só some se estiver vazia; sobra de outra origem não é nossa para apagar
            Self::MakeDir(path) => {
                let _ = fs.remove_dir(path);
                Ok(())
            }
            Self::Move { from, to, .. } => fs.rename(to, from),
            Self::RemoveDir(_) => Ok(()),
        }
    }
}

/// Monta o plano de troca olhando o que existe agora. Vazio se `install\` não tem nenhum arquivo
/// gerenciado (nada a trocar).
#[must_use]
pub fn plan(layout: &Layout, fs: &dyn Fs) -> Vec<Job> {
    let staged: Vec<ManagedFile> = MANAGED_FILES
        .iter()
        .copied()
        .filter(|file| fs.exists(&layout.staged(file)))
        .collect();
    if staged.is_empty() {
        return Vec::new();
    }
    let mut jobs = vec![Job::MakeDir(layout.old_dir())];
    let mut made = vec![layout.old_dir(), layout.app_dir().to_path_buf()];
    let mut ensure_parent = |jobs: &mut Vec<Job>, path: &PathBuf, exists: bool| {
        if let Some(parent) = path.parent()
            && !exists
            && !made.iter().any(|done| done == parent)
        {
            made.push(parent.to_path_buf());
            jobs.push(Job::MakeDir(parent.to_path_buf()));
        }
    };
    for file in &staged {
        let live = layout.live(file);
        if fs.exists(&live) {
            let backup = layout.backup(file);
            let backup_parent_exists = backup.parent().is_some_and(|parent| fs.exists(parent));
            ensure_parent(&mut jobs, &backup, backup_parent_exists);
            jobs.push(Job::Move {
                from: live,
                to: backup,
                file,
            });
        }
    }
    for file in &staged {
        let live = layout.live(file);
        let parent_exists = live.parent().is_some_and(|parent| fs.exists(parent));
        ensure_parent(&mut jobs, &live, parent_exists);
        jobs.push(Job::Move {
            from: layout.staged(file),
            to: live,
            file,
        });
    }
    jobs.push(Job::RemoveDir(layout.updates_dir()));
    jobs.push(Job::RemoveDir(layout.install_dir()));
    jobs.push(Job::RemoveDir(layout.old_dir()));
    jobs
}

/// Como terminou o desfazer dos jobs já concluídos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rollback {
    /// Tudo voltou: o app anterior segue utilizável.
    Complete,
    /// Algum passo não voltou (descrições): o app pode estar inconsistente.
    Incomplete(Vec<String>),
}

/// Falha de um job, já com o rollback feito.
#[derive(Debug)]
pub struct Failure {
    /// Posição do job que falhou no plano.
    pub index: usize,
    /// Descrição do job para o log.
    pub job: String,
    /// Arquivo gerenciado envolvido (se o job for um movimento).
    pub culprit: Option<ManagedFile>,
    /// Erro de E/S do job.
    pub error: io::Error,
    /// Resultado do desfazer.
    pub rollback: Rollback,
}

impl Failure {
    /// Nome legível do arquivo envolvido, se houver.
    #[must_use]
    pub fn culprit_name(&self) -> Option<String> {
        self.culprit.map(display_name)
    }
}

/// Resultado de uma troca concluída.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Done {
    /// Avisos de limpeza (pastas que não puderam ser apagadas agora).
    pub warnings: Vec<String>,
}

/// Executa o plano. Em falha desfaz os jobs concluídos em ordem inversa.
///
/// # Errors
/// [`Failure`] (boxed) com o job que falhou e o resultado do rollback.
pub fn execute(
    jobs: &[Job],
    fs: &dyn Fs,
    policy: &RetryPolicy,
    sleep: &mut dyn FnMut(Duration),
) -> Result<Done, Box<Failure>> {
    let mut done = Done::default();
    for (index, job) in jobs.iter().enumerate() {
        let result = retry(policy, sleep, &mut || job.apply(fs));
        match result {
            Ok(()) => {}
            Err(error) if job.is_best_effort() => {
                done.warnings.push(format!("{}: {error}", job.describe()));
            }
            Err(error) => {
                let rollback = rollback(&jobs[..index], fs, policy, sleep);
                return Err(Box::new(Failure {
                    index,
                    job: job.describe(),
                    culprit: job.file(),
                    error,
                    rollback,
                }));
            }
        }
    }
    Ok(done)
}

/// Desfaz `completed` em ordem inversa, tentando todos mesmo se um falhar.
pub fn rollback(
    completed: &[Job],
    fs: &dyn Fs,
    policy: &RetryPolicy,
    sleep: &mut dyn FnMut(Duration),
) -> Rollback {
    let failed: Vec<String> = completed
        .iter()
        .rev()
        .filter_map(|job| {
            retry(policy, sleep, &mut || job.rollback(fs))
                .err()
                .map(|error| format!("{}: {error}", job.describe()))
        })
        .collect();
    if failed.is_empty() {
        Rollback::Complete
    } else {
        Rollback::Incomplete(failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{CLI_FILE, MANAGED_FILES};
    use crate::fsops::RealFs;
    use crate::testkit::{Fault, FaultyFs, Fixture};

    fn no_sleep() -> impl FnMut(Duration) {
        |_| {}
    }

    #[test]
    fn retry_repete_ate_dar_certo() {
        let mut attempts = 0;
        let mut slept = Duration::ZERO;
        let result = retry(&RetryPolicy::default(), &mut |d| slept += d, &mut || {
            attempts += 1;
            if attempts < 4 {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            } else {
                Ok(())
            }
        });
        assert!(result.is_ok());
        assert_eq!(slept, Duration::from_millis(150));
    }

    #[test]
    fn retry_desiste_no_prazo_de_2s_com_passos_de_50ms() {
        let mut attempts = 0u32;
        let mut slept = Duration::ZERO;
        let result = retry(&RetryPolicy::default(), &mut |d| slept += d, &mut || {
            attempts += 1;
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(slept, Duration::from_secs(2));
        assert_eq!(attempts, 2000 / 50 + 1);
    }

    #[test]
    fn retry_aborta_em_not_found_sem_esperar() {
        let mut slept = Duration::ZERO;
        let result = retry(&RetryPolicy::default(), &mut |d| slept += d, &mut || {
            Err(io::Error::from(io::ErrorKind::NotFound))
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
        assert_eq!(slept, Duration::ZERO);
    }

    #[test]
    fn plano_segue_a_ordem_do_zed() {
        let fixture = Fixture::new();
        let layout = &fixture.layout;
        let jobs = plan(layout, &RealFs);
        // 1. old\, antigos para old\ (com old\tools\), 2. novos para o lugar, 3. apagar
        assert_eq!(jobs[0], Job::MakeDir(layout.old_dir()));
        let kinds: Vec<&str> = jobs
            .iter()
            .map(|job| match job {
                Job::MakeDir(_) => "mkdir",
                Job::Move { .. } => "move",
                Job::RemoveDir(_) => "rm",
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "mkdir", "move", "move", "move", "mkdir", "move", // old\ + 4 antigos
                "move", "move", "move", "move", // 4 novos
                "rm", "rm", "rm",
            ]
        );
        // os 4 arquivos, inclusive dbml.exe e o próprio helper
        let moved: Vec<ManagedFile> = jobs.iter().filter_map(Job::file).collect();
        for file in MANAGED_FILES {
            assert_eq!(moved.iter().filter(|f| **f == file).count(), 2, "{file:?}");
        }
        assert_eq!(jobs[10], Job::RemoveDir(layout.updates_dir()));
        assert_eq!(jobs[11], Job::RemoveDir(layout.install_dir()));
        assert_eq!(jobs[12], Job::RemoveDir(layout.old_dir()));
    }

    #[test]
    fn sem_arquivos_novos_nao_ha_plano() {
        let fixture = Fixture::new();
        std::fs::remove_dir_all(fixture.layout.install_dir()).unwrap();
        assert!(plan(&fixture.layout, &RealFs).is_empty());
    }

    #[test]
    fn troca_completa_deixa_so_os_arquivos_novos() {
        let fixture = Fixture::new();
        let jobs = plan(&fixture.layout, &RealFs);
        let done = execute(&jobs, &RealFs, &RetryPolicy::default(), &mut no_sleep()).unwrap();
        assert!(done.warnings.is_empty());
        assert_eq!(
            fixture.snapshot(),
            Fixture::expected_after_success(),
            "app com os arquivos novos, sem updates, install nem old"
        );
    }

    #[test]
    fn arquivo_que_o_app_antigo_nao_tinha_e_criado() {
        // o app 0.3.x instalado não tem `tools\`: o helper novo chega junto da atualização
        let fixture = Fixture::new();
        std::fs::remove_dir_all(fixture.layout.app_dir().join("tools")).unwrap();
        let jobs = plan(&fixture.layout, &RealFs);
        execute(&jobs, &RealFs, &RetryPolicy::default(), &mut no_sleep()).unwrap();
        let after = fixture.snapshot();
        assert_eq!(
            after["tools/auto_update_helper.exe"],
            "new:tools/auto_update_helper.exe"
        );
    }

    #[test]
    fn falha_em_qualquer_job_faz_rollback_completo() {
        let probe = Fixture::new();
        let jobs = plan(&probe.layout, &RealFs);
        let reversible = jobs.iter().filter(|job| !job.is_best_effort()).count();
        assert_eq!(reversible, 10);
        for failing in 0..reversible {
            let fixture = Fixture::new();
            let jobs = plan(&fixture.layout, &RealFs);
            let before = fixture.snapshot();
            let fs = FaultyFs::failing(Fault::of(&jobs[failing]));
            let failure = execute(&jobs, &fs, &RetryPolicy::default(), &mut no_sleep())
                .expect_err("o job deve falhar");
            assert_eq!(failure.index, failing);
            assert_eq!(failure.rollback, Rollback::Complete, "job {failing}");
            assert_eq!(
                fixture.snapshot(),
                before,
                "job {failing}: o app anterior volta intacto, com install\\ e a flag"
            );
            assert!(
                !fixture.layout.old_dir().exists(),
                "job {failing}: old\\ some"
            );
        }
    }

    #[test]
    fn dbml_em_uso_vira_falha_limpa_com_rollback_e_nome_do_arquivo() {
        let fixture = Fixture::new();
        let jobs = plan(&fixture.layout, &RealFs);
        let before = fixture.snapshot();
        let target = fixture.layout.live(CLI_FILE);
        let fs = FaultyFs::failing(Fault::RenameFrom(target));
        let failure = execute(&jobs, &fs, &RetryPolicy::default(), &mut no_sleep()).unwrap_err();
        assert_eq!(failure.culprit, Some(CLI_FILE));
        assert_eq!(failure.culprit_name().as_deref(), Some("dbml.exe"));
        assert_eq!(failure.rollback, Rollback::Complete);
        assert_eq!(fixture.snapshot(), before);
    }

    #[test]
    fn rollback_incompleto_e_reportado() {
        let fixture = Fixture::new();
        let jobs = plan(&fixture.layout, &RealFs);
        // falha o 3º arquivo novo E o desfazer do 1º antigo
        let failing = jobs
            .iter()
            .position(|job| matches!(job, Job::Move { from, .. } if from == &fixture.layout.staged(CLI_FILE)))
            .unwrap();
        let first_old = jobs
            .iter()
            .find(|job| matches!(job, Job::Move { .. }))
            .unwrap();
        let undo = match first_old {
            Job::Move { from, to, .. } => Fault::RenameFromTo(to.clone(), from.clone()),
            _ => unreachable!(),
        };
        let fs = FaultyFs::failing_all(vec![Fault::of(&jobs[failing]), undo]);
        let failure = execute(&jobs, &fs, &RetryPolicy::default(), &mut no_sleep()).unwrap_err();
        assert!(matches!(failure.rollback, Rollback::Incomplete(ref steps) if steps.len() == 1));
    }

    #[test]
    fn falha_na_limpeza_so_gera_aviso() {
        let fixture = Fixture::new();
        let jobs = plan(&fixture.layout, &RealFs);
        let fs = FaultyFs::failing(Fault::RemoveAll(fixture.layout.old_dir()));
        let done = execute(&jobs, &fs, &RetryPolicy::default(), &mut no_sleep()).unwrap();
        assert_eq!(done.warnings.len(), 1);
        // os arquivos novos já estão no lugar; só sobrou a pasta `old`
        assert_eq!(fixture.snapshot()["db-modeling.exe"], "new:db-modeling.exe");
    }
}
