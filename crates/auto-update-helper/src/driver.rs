//! Fluxo do helper: esperar o app sair, reconciliar restos, liberar handles, trocar e relançar.
//!
//! Tudo que toca o sistema operacional (sono, espera de PID, Restart Manager, abrir o app, log)
//! entra por [`Env`]; no Windows é `win::WinEnv`, nos testes é um fake. A decisão (o que fazer, em
//! que ordem, quando **não** relançar) é toda daqui e roda em qualquer SO.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::args::Options;
use crate::constants::{CLI_FILE, RELEASE_FILES};
use crate::fsops::Fs;
use crate::jobs::{Failure, RetryPolicy, Rollback, execute, plan, retry};
use crate::layout::{Layout, display_name};
use crate::messages::Problem;
use crate::recovery::{Startup, recover};

/// Ambiente do helper (tudo que não é decisão nem arquivo).
pub trait Env {
    /// Pausa entre tentativas.
    fn sleep(&mut self, duration: Duration);
    /// Espera o processo `pid` terminar (com prazo; estourar o prazo não é erro).
    fn wait_for_exit(&mut self, pid: u32);
    /// Pede ao Restart Manager que libere os arquivos (melhor esforço).
    fn release_handles(&mut self, files: &[PathBuf]);
    /// Abre o app já atualizado.
    ///
    /// # Errors
    /// Erro ao iniciar o processo.
    fn launch(&mut self, exe: &Path, args: &[OsString]) -> io::Result<()>;
    /// Anota uma linha no log.
    fn log(&mut self, line: &str);
}

/// Como a execução terminou.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nada pendente (abriu o app se pedido).
    NothingToDo,
    /// Troca concluída (abriu o app se pedido).
    Applied {
        /// Quantas limpezas não puderam ser feitas agora (ficam para a próxima abertura).
        warnings: usize,
    },
    /// Algo deu errado (já foi para o log); o chamador mostra a mensagem. Nos casos de falha da
    /// troca o app anterior foi restaurado e **nada é relançado**.
    Failed {
        /// Qual mensagem mostrar.
        problem: Problem,
        /// Detalhe técnico (arquivo, erro) para a caixa de erro.
        detail: String,
    },
}

fn failed(problem: Problem, detail: impl Into<String>) -> Outcome {
    Outcome::Failed {
        problem,
        detail: detail.into(),
    }
}

/// Falha de um job → problema a mostrar. `dbml.exe` em uso é uma falha limpa com mensagem própria.
fn classify(failure: &Failure) -> Outcome {
    let name = failure.culprit_name().unwrap_or_default();
    let detail = format!("{name}: {}", failure.error);
    match (&failure.rollback, failure.culprit) {
        (Rollback::Incomplete(_), _) => failed(Problem::Inconsistent, detail),
        (Rollback::Complete, Some(file)) if file == CLI_FILE => failed(Problem::CliInUse, detail),
        (Rollback::Complete, _) => failed(Problem::RolledBack, detail),
    }
}

/// `dbml.exe` (que vai ser trocado) está em uso por outro processo, mesmo após as tentativas?
fn cli_in_use(layout: &Layout, fs: &dyn Fs, policy: &RetryPolicy, env: &mut dyn Env) -> bool {
    let cli = layout.live(CLI_FILE);
    if !fs.exists(&cli) || !fs.exists(&layout.staged(CLI_FILE)) {
        return false;
    }
    retry(policy, &mut |d| env.sleep(d), &mut || {
        if fs.is_in_use(&cli) {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        } else {
            Ok(())
        }
    })
    .is_err()
}

fn launch_if_requested(
    layout: &Layout,
    options: &Options,
    env: &mut dyn Env,
    done: Outcome,
) -> Outcome {
    if !options.launch {
        return done;
    }
    env.log("abrindo o app");
    match env.launch(&layout.app_exe(), &options.launch_args) {
        Ok(()) => done,
        Err(error) => {
            env.log(&format!("não abriu o app: {error}"));
            failed(Problem::LaunchFailed, error.to_string())
        }
    }
}

/// Executa o fluxo completo.
pub fn run(
    layout: &Layout,
    options: &Options,
    policy: &RetryPolicy,
    fs: &dyn Fs,
    env: &mut dyn Env,
) -> Outcome {
    if let Some(pid) = options.wait_pid {
        env.log(&format!("aguardando o processo {pid} sair"));
        env.wait_for_exit(pid);
    }
    match recover(layout, fs, policy, &mut |d| env.sleep(d)) {
        Err(error) => {
            env.log(&format!("recuperação falhou: {error}"));
            return failed(Problem::RecoveryFailed, error.to_string());
        }
        Ok(Startup::Clean) => {
            env.log("nada pendente");
            return launch_if_requested(layout, options, env, Outcome::NothingToDo);
        }
        Ok(Startup::Pending) => {}
    }
    let jobs = plan(layout, fs);
    if jobs.is_empty() {
        // flag e install\ sem nenhum arquivo gerenciado: nada a trocar, limpa o que sobrou
        env.log("install\\ sem arquivos gerenciados; limpando");
        let _ = fs.remove_all(&layout.updates_dir());
        let _ = fs.remove_all(&layout.install_dir());
        return launch_if_requested(layout, options, env, Outcome::NothingToDo);
    }
    let release: Vec<PathBuf> = RELEASE_FILES.iter().map(|file| layout.live(file)).collect();
    env.release_handles(&release);
    if cli_in_use(layout, fs, policy, env) {
        let name = display_name(CLI_FILE);
        env.log(&format!("{name} em uso: nada foi trocado"));
        return failed(Problem::CliInUse, format!("{name}: em uso"));
    }
    env.log(&format!("trocando arquivos ({} jobs)", jobs.len()));
    match execute(&jobs, fs, policy, &mut |d| env.sleep(d)) {
        Ok(done) => {
            for warning in &done.warnings {
                env.log(&format!("aviso: {warning}"));
            }
            let outcome = Outcome::Applied {
                warnings: done.warnings.len(),
            };
            launch_if_requested(layout, options, env, outcome)
        }
        Err(failure) => {
            env.log(&format!(
                "falhou no job {} ({}): {}; rollback: {:?}",
                failure.index, failure.job, failure.error, failure.rollback
            ));
            classify(&failure)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::ManagedFile;
    use crate::fsops::RealFs;
    use crate::testkit::{Fault, FaultyFs, Fixture};

    #[derive(Default)]
    struct FakeEnv {
        events: Vec<String>,
        launched: Vec<(PathBuf, Vec<OsString>)>,
        launch_fails: bool,
        slept: Duration,
    }

    impl Env for FakeEnv {
        fn sleep(&mut self, duration: Duration) {
            self.slept += duration;
        }
        fn wait_for_exit(&mut self, pid: u32) {
            self.events.push(format!("wait {pid}"));
        }
        fn release_handles(&mut self, files: &[PathBuf]) {
            self.events.push(format!("release {}", files.len()));
        }
        fn launch(&mut self, exe: &Path, args: &[OsString]) -> io::Result<()> {
            self.events.push("launch".into());
            if self.launch_fails {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            self.launched.push((exe.to_path_buf(), args.to_vec()));
            Ok(())
        }
        fn log(&mut self, _: &str) {}
    }

    fn options(launch: bool) -> Options {
        Options {
            launch,
            wait_pid: Some(77),
            launch_args: vec![OsString::from("schema.dbml")],
        }
    }

    fn go(fixture: &Fixture, launch: bool, fs: &dyn Fs, env: &mut FakeEnv) -> Outcome {
        run(
            &fixture.layout,
            &options(launch),
            &RetryPolicy::default(),
            fs,
            env,
        )
    }

    #[test]
    fn sucesso_espera_o_pid_libera_troca_e_relanca_com_os_argumentos() {
        let fixture = Fixture::new();
        let mut env = FakeEnv::default();
        let outcome = go(&fixture, true, &RealFs, &mut env);
        assert_eq!(outcome, Outcome::Applied { warnings: 0 });
        assert_eq!(env.events, ["wait 77", "release 1", "launch"]);
        assert_eq!(
            env.launched,
            [(
                fixture.layout.app_exe(),
                vec![OsString::from("schema.dbml")]
            )]
        );
        assert_eq!(fixture.snapshot(), Fixture::expected_after_success());
    }

    #[test]
    fn launch_false_troca_sem_abrir_o_app() {
        let fixture = Fixture::new();
        let mut env = FakeEnv::default();
        assert_eq!(
            go(&fixture, false, &RealFs, &mut env),
            Outcome::Applied { warnings: 0 }
        );
        assert!(env.launched.is_empty());
        assert_eq!(fixture.snapshot(), Fixture::expected_after_success());
    }

    #[test]
    fn falha_de_job_faz_rollback_e_nao_relanca() {
        let cases: [ManagedFile; 3] = [
            CLI_FILE,
            &["icon.ico"],
            &["tools", "auto_update_helper.exe"],
        ];
        for fault_on in cases {
            let fixture = Fixture::new();
            let before = fixture.snapshot();
            let fs = FaultyFs::failing(Fault::RenameFrom(fixture.layout.live(fault_on)));
            let mut env = FakeEnv::default();
            let outcome = go(&fixture, true, &fs, &mut env);
            let Outcome::Failed { problem, .. } = outcome else {
                panic!("devia falhar: {fault_on:?}");
            };
            assert!(matches!(problem, Problem::CliInUse | Problem::RolledBack));
            assert!(env.launched.is_empty(), "falha nunca relança");
            assert!(!env.events.contains(&"launch".to_owned()));
            assert_eq!(
                fixture.snapshot(),
                before,
                "app anterior intacto: {fault_on:?}"
            );
        }
    }

    #[test]
    fn dbml_em_uso_nao_troca_nada_e_avisa() {
        let fixture = Fixture::new();
        let before = fixture.snapshot();
        let fs = FaultyFs::failing(Fault::InUse(fixture.layout.live(CLI_FILE)));
        let mut env = FakeEnv::default();
        let outcome = go(&fixture, true, &fs, &mut env);
        assert!(matches!(
            outcome,
            Outcome::Failed {
                problem: Problem::CliInUse,
                ..
            }
        ));
        assert_eq!(fixture.snapshot(), before);
        assert!(env.launched.is_empty());
        // esperou o prazo inteiro (2 s) antes de desistir
        assert_eq!(env.slept, Duration::from_secs(2));
    }

    #[test]
    fn rollback_incompleto_e_inconsistente_e_nao_relanca() {
        let fixture = Fixture::new();
        let layout = &fixture.layout;
        let fs = FaultyFs::failing_all(vec![
            Fault::RenameFrom(layout.staged(&["icon.ico"])),
            // o desfazer do primeiro antigo também falha
            Fault::RenameFromTo(
                layout.backup(&["db-modeling.exe"]),
                layout.live(&["db-modeling.exe"]),
            ),
        ]);
        let mut env = FakeEnv::default();
        let outcome = go(&fixture, true, &fs, &mut env);
        assert!(matches!(
            outcome,
            Outcome::Failed {
                problem: Problem::Inconsistent,
                ..
            }
        ));
        assert!(env.launched.is_empty());
    }

    #[test]
    fn nada_pendente_apenas_relanca_se_pedido() {
        let fixture = Fixture::new();
        std::fs::remove_dir_all(fixture.layout.updates_dir()).unwrap();
        std::fs::remove_dir_all(fixture.layout.install_dir()).unwrap();
        let before = fixture.snapshot();
        let mut env = FakeEnv::default();
        assert_eq!(go(&fixture, true, &RealFs, &mut env), Outcome::NothingToDo);
        assert_eq!(env.launched.len(), 1);
        assert_eq!(fixture.snapshot(), before);
        let mut env = FakeEnv::default();
        assert_eq!(go(&fixture, false, &RealFs, &mut env), Outcome::NothingToDo);
        assert!(env.launched.is_empty());
    }

    #[test]
    fn restos_de_instalacao_incompleta_sao_limpos_sem_perder_o_app() {
        let fixture = Fixture::new();
        std::fs::remove_dir_all(fixture.layout.updates_dir()).unwrap();
        let mut env = FakeEnv::default();
        assert_eq!(go(&fixture, false, &RealFs, &mut env), Outcome::NothingToDo);
        assert!(!fixture.layout.install_dir().exists());
        assert_eq!(fixture.snapshot()["db-modeling.exe"], "old:db-modeling.exe");
    }

    #[test]
    fn troca_feita_mas_app_novo_nao_abre_e_reportado() {
        let fixture = Fixture::new();
        let mut env = FakeEnv {
            launch_fails: true,
            ..FakeEnv::default()
        };
        let outcome = go(&fixture, true, &RealFs, &mut env);
        assert!(matches!(
            outcome,
            Outcome::Failed {
                problem: Problem::LaunchFailed,
                ..
            }
        ));
        assert_eq!(fixture.snapshot(), Fixture::expected_after_success());
    }

    #[test]
    fn troca_interrompida_e_revertida_e_depois_retomada() {
        // o helper morreu depois de mover o app novo; a próxima execução reverte e conclui
        let fixture = Fixture::new();
        let jobs = plan(&fixture.layout, &RealFs);
        for job in &jobs[..7] {
            job.apply(&RealFs).unwrap();
        }
        let mut env = FakeEnv::default();
        assert_eq!(
            go(&fixture, false, &RealFs, &mut env),
            Outcome::Applied { warnings: 0 }
        );
        assert_eq!(fixture.snapshot(), Fixture::expected_after_success());
    }
}
