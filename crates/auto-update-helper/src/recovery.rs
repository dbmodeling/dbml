//! Estado da instalação pendente e recuperação de restos (abertura do app e início do helper).
//!
//! A flag `updates\versions.txt` é o marco: o Inno só a grava **depois** de copiar tudo para
//! `install\`, e o plano de troca só a apaga **depois** de mover todos os arquivos. Daí:
//!
//! | flag | `install\` | `old\` | significado | ação |
//! |---|---|---|---|---|
//! | sim | sim | não | pendente (nada trocado) | "Reiniciar para atualizar" |
//! | sim | qualquer | sim | troca interrompida (queda, `kill`) | **reverter** de `old\` e voltar a pendente |
//! | sim | não | não | flag sobrando | apagar a flag |
//! | não | sim ou não | sim ou não | instalação do Inno incompleta, ou troca já concluída | apagar `install\` e `old\` |
//!
//! Reverter nunca deixa meia versão: cada arquivo novo volta para `install\` e o antigo volta ao
//! lugar. O app anterior segue utilizável em qualquer ponto de falha.

use std::io;
use std::time::Duration;

use crate::constants::MANAGED_FILES;
use crate::fsops::Fs;
use crate::jobs::{RetryPolicy, retry};
use crate::layout::Layout;

/// Situação da instalação em disco.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Startup {
    /// Nada pendente.
    Clean,
    /// Atualização instalada em `install\` aguardando a troca ("Reiniciar para atualizar").
    Pending,
}

/// Olha o disco sem alterar nada: pendente se a flag e `install\` existem.
#[must_use]
pub fn inspect(layout: &Layout, fs: &dyn Fs) -> Startup {
    if fs.exists(&layout.flag_file()) && fs.is_dir(&layout.install_dir()) {
        Startup::Pending
    } else {
        Startup::Clean
    }
}

/// Existe algo de uma atualização (flag, `install\` ou `old\`)? Barato; o app só toma a trava e
/// reconcilia o disco quando isto é verdadeiro (no caso comum não toca em nada).
#[must_use]
pub fn has_leftovers(layout: &Layout, fs: &dyn Fs) -> bool {
    fs.exists(&layout.flag_file())
        || fs.exists(&layout.install_dir())
        || fs.exists(&layout.old_dir())
}

/// Versão gravada na flag (texto sem espaços nas pontas), se houver instalação pendente.
#[must_use]
pub fn pending_version(layout: &Layout, fs: &dyn Fs) -> Option<String> {
    (inspect(layout, fs) == Startup::Pending).then(|| {
        fs.read_to_string(&layout.flag_file())
            .map(|text| text.trim().to_owned())
            .unwrap_or_default()
    })
}

/// Reconcilia o disco (ver a tabela do módulo) e devolve o estado resultante.
///
/// # Errors
/// Erro de E/S se a reversão de uma troca interrompida falhar (nada é apagado nesse caso).
pub fn recover(
    layout: &Layout,
    fs: &dyn Fs,
    policy: &RetryPolicy,
    sleep: &mut dyn FnMut(Duration),
) -> io::Result<Startup> {
    if fs.exists(&layout.flag_file()) {
        if fs.exists(&layout.old_dir()) {
            revert(layout, fs, policy, sleep)?;
        }
        if fs.is_dir(&layout.install_dir()) {
            return Ok(Startup::Pending);
        }
        // flag sem nada instalado: resto de uma limpeza interrompida
        let _ = fs.remove_all(&layout.updates_dir());
        return Ok(Startup::Clean);
    }
    // sem flag: instalação do Inno incompleta (install\) ou troca concluída sem limpar (old\)
    let _ = fs.remove_all(&layout.install_dir());
    let _ = fs.remove_all(&layout.old_dir());
    Ok(Startup::Clean)
}

/// Desfaz uma troca interrompida: o arquivo novo (que já estava no lugar) volta para `install\` e
/// o antigo volta de `old\`. Só apaga `old\` se tudo voltou.
fn revert(
    layout: &Layout,
    fs: &dyn Fs,
    policy: &RetryPolicy,
    sleep: &mut dyn FnMut(Duration),
) -> io::Result<()> {
    for file in MANAGED_FILES.iter().rev() {
        let backup = layout.backup(file);
        if !fs.exists(&backup) {
            continue;
        }
        let live = layout.live(file);
        let staged = layout.staged(file);
        if fs.exists(&live) {
            if fs.exists(&staged) {
                // os três lados existem (old\, live e install\): não dá para saber qual é qual;
                // aborta sem sobrescrever old\
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("estado ambíguo para {file:?}: old\\, live e install\\ coexistem"),
                ));
            }
            if let Some(parent) = staged.parent() {
                fs.create_dir_all(parent)?;
            }
            retry(policy, sleep, &mut || fs.rename(&live, &staged))?;
        }
        if let Some(parent) = live.parent() {
            fs.create_dir_all(parent)?;
        }
        retry(policy, sleep, &mut || fs.rename(&backup, &live))?;
    }
    let _ = fs.remove_all(&layout.old_dir());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsops::RealFs;
    use crate::jobs::{execute, plan};
    use crate::testkit::{FLAG_VERSION, Fixture};

    fn run_recover(fixture: &Fixture) -> Startup {
        recover(
            &fixture.layout,
            &RealFs,
            &RetryPolicy::default(),
            &mut |_| {},
        )
        .unwrap()
    }

    #[test]
    fn flag_e_install_significam_pendente_e_trazem_a_versao() {
        let fixture = Fixture::new();
        assert_eq!(inspect(&fixture.layout, &RealFs), Startup::Pending);
        assert_eq!(
            pending_version(&fixture.layout, &RealFs).as_deref(),
            Some(FLAG_VERSION)
        );
    }

    #[test]
    fn disco_limpo_nao_tem_restos() {
        let fixture = Fixture::new();
        assert!(has_leftovers(&fixture.layout, &RealFs));
        let clean = tempfile::tempdir().unwrap();
        assert!(!has_leftovers(&Layout::new(clean.path()), &RealFs));
        // depois de uma troca completa também não sobra nada
        let jobs = plan(&fixture.layout, &RealFs);
        execute(&jobs, &RealFs, &RetryPolicy::default(), &mut |_| {}).unwrap();
        assert!(!has_leftovers(&fixture.layout, &RealFs));
    }

    #[test]
    fn so_a_flag_ou_so_install_nao_e_pendente() {
        let only_flag = Fixture::new();
        std::fs::remove_dir_all(only_flag.layout.install_dir()).unwrap();
        assert_eq!(inspect(&only_flag.layout, &RealFs), Startup::Clean);
        let only_install = Fixture::new();
        std::fs::remove_dir_all(only_install.layout.updates_dir()).unwrap();
        assert_eq!(inspect(&only_install.layout, &RealFs), Startup::Clean);
        assert_eq!(pending_version(&only_install.layout, &RealFs), None);
    }

    #[test]
    fn pendente_sem_mexer_em_nada_quando_esta_tudo_certo() {
        let fixture = Fixture::new();
        let before = fixture.snapshot();
        assert_eq!(run_recover(&fixture), Startup::Pending);
        assert_eq!(fixture.snapshot(), before);
    }

    #[test]
    fn instalacao_do_inno_incompleta_e_limpa_sem_tocar_no_app() {
        // `kill` no meio do instalador: install\ parcial e nenhuma flag
        let fixture = Fixture::new();
        std::fs::remove_dir_all(fixture.layout.updates_dir()).unwrap();
        assert_eq!(run_recover(&fixture), Startup::Clean);
        assert!(!fixture.layout.install_dir().exists());
        let after = fixture.snapshot();
        assert_eq!(after["db-modeling.exe"], "old:db-modeling.exe");
        assert_eq!(after.len(), 4);
    }

    #[test]
    fn flag_sobrando_sem_install_e_apagada() {
        let fixture = Fixture::new();
        std::fs::remove_dir_all(fixture.layout.install_dir()).unwrap();
        assert_eq!(run_recover(&fixture), Startup::Clean);
        assert!(!fixture.layout.updates_dir().exists());
    }

    #[test]
    fn troca_interrompida_em_qualquer_ponto_reverte_para_o_app_anterior() {
        let probe = Fixture::new();
        let total = plan(&probe.layout, &RealFs).len();
        for applied in 1..total {
            let fixture = Fixture::new();
            let before = fixture.snapshot();
            let jobs = plan(&fixture.layout, &RealFs);
            for job in &jobs[..applied] {
                job.apply(&RealFs).unwrap();
            }
            let state = run_recover(&fixture);
            if fixture.layout.flag_file().exists() {
                // a flag ainda existia: volta a pendente com o app anterior inteiro
                assert_eq!(state, Startup::Pending, "{applied} jobs");
                assert_eq!(fixture.snapshot(), before, "{applied} jobs");
                // e a atualização pendente é retomada com sucesso
                let jobs = plan(&fixture.layout, &RealFs);
                execute(&jobs, &RealFs, &RetryPolicy::default(), &mut |_| {}).unwrap();
                assert_eq!(fixture.snapshot(), Fixture::expected_after_success());
            } else {
                // flag já apagada = todos os arquivos já tinham sido movidos: troca concluída
                assert_eq!(state, Startup::Clean, "{applied} jobs");
                assert_eq!(fixture.snapshot(), Fixture::expected_after_success());
            }
        }
    }

    #[test]
    fn reversao_que_falha_nao_apaga_nada() {
        let fixture = Fixture::new();
        let jobs = plan(&fixture.layout, &RealFs);
        for job in &jobs[..8] {
            job.apply(&RealFs).unwrap();
        }
        let fs = crate::testkit::FaultyFs::failing(crate::testkit::Fault::RenameFrom(
            fixture.layout.live(crate::constants::CLI_FILE),
        ));
        let result = recover(&fixture.layout, &fs, &RetryPolicy::default(), &mut |_| {});
        assert!(result.is_err());
        assert!(
            fixture.layout.old_dir().exists(),
            "old\\ continua para nova tentativa"
        );
    }

    #[test]
    fn coexistencia_de_old_live_e_install_aborta_reversao_sem_perder_old() {
        let fixture = Fixture::new();
        let backup = fixture.layout.backup(crate::constants::CLI_FILE);
        std::fs::create_dir_all(backup.parent().unwrap()).unwrap();
        std::fs::write(&backup, "backup:dbml.exe").unwrap();
        let result = recover(
            &fixture.layout,
            &RealFs,
            &RetryPolicy::default(),
            &mut |_| {},
        );
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "backup:dbml.exe");
    }
}
