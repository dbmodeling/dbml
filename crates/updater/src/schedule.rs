//! Agendamento puro: primeira checagem imediata, intervalo de 1 h, sem retentativa, e limpeza
//! de órfãos com mais de 24 h. O instante atual entra sempre como parâmetro (`SystemTime`), então
//! nada aqui lê o relógio, dorme nem cria threads.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::constants::{CHECK_INTERVAL, LOCK_FILE_NAME, ORPHAN_MAX_AGE};
use crate::lock::UpdateLock;

/// Cadência das checagens automáticas.
///
/// A primeira é imediata ([`Schedule::is_due`] é `true` enquanto nada foi tentado); depois, uma
/// a cada [`CHECK_INTERVAL`] contada **do início da última tentativa**, tenha ela dado certo ou
/// não: não há retentativa antecipada após falha.
#[derive(Debug, Clone)]
pub struct Schedule {
    interval: Duration,
    last_attempt: Option<SystemTime>,
}

impl Default for Schedule {
    fn default() -> Self {
        Self::new(CHECK_INTERVAL)
    }
}

impl Schedule {
    /// Agenda com o intervalo dado (use [`Schedule::default`] em produção).
    #[must_use]
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_attempt: None,
        }
    }

    /// Registra o início de uma tentativa (sucesso ou falha: o efeito é o mesmo).
    pub fn record_attempt(&mut self, now: SystemTime) {
        self.last_attempt = Some(now);
    }

    /// Tempo até a próxima checagem; zero se já venceu ou se nunca houve tentativa.
    ///
    /// Se o relógio andou para trás, considera vencida (evita ficar travado esperando).
    #[must_use]
    pub fn time_until_due(&self, now: SystemTime) -> Duration {
        match self.last_attempt {
            None => Duration::ZERO,
            Some(last) => match now.duration_since(last) {
                Ok(elapsed) => self.interval.saturating_sub(elapsed),
                Err(_) => Duration::ZERO,
            },
        }
    }

    /// Já está na hora de checar?
    #[must_use]
    pub fn is_due(&self, now: SystemTime) -> bool {
        self.time_until_due(now).is_zero()
    }
}

/// Filtra, entre `(caminho, última modificação)`, os órfãos: mais velhos que
/// [`ORPHAN_MAX_AGE`] (estritamente) e que não sejam o arquivo de trava.
#[must_use]
pub fn select_orphans<I>(entries: I, now: SystemTime) -> Vec<PathBuf>
where
    I: IntoIterator<Item = (PathBuf, SystemTime)>,
{
    entries
        .into_iter()
        .filter(|(path, _)| path.file_name().is_none_or(|name| name != LOCK_FILE_NAME))
        .filter(|(_, modified)| {
            now.duration_since(*modified)
                .is_ok_and(|age| age > ORPHAN_MAX_AGE)
        })
        .map(|(path, _)| path)
        .collect()
}

/// Apaga órfãos da pasta de updates **do cache** (`<cache>/db-modeling/updates/`): arquivos e
/// pastas com mais de 24 h, menos a trava. Exige [`UpdateLock`]: sem a trava não há como chamar,
/// e assim ninguém apaga o que outra instância está baixando. A pasta vem da própria trava, então
/// não dá para apontar para outro lugar (em especial, nunca para a `updates\` do Windows em
/// `{app}`, que guarda `versions.txt` e `install\` e é do `auto_update_helper`).
///
/// Retorna quantos itens foram removidos; os que não puderem ser removidos são ignorados.
///
/// # Errors
/// Erro ao listar o diretório (diretório inexistente conta como "nada a limpar").
pub fn sweep_orphans(lock: &UpdateLock, now: SystemTime) -> io::Result<usize> {
    let read = match fs::read_dir(lock.updates_dir()) {
        Ok(read) => read,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    let entries = read.filter_map(Result::ok).filter_map(|entry| {
        let modified = entry.metadata().ok()?.modified().ok()?;
        Some((entry.path(), modified))
    });
    let removed = select_orphans(entries, now)
        .into_iter()
        .filter(|path| {
            if path.is_dir() {
                fs::remove_dir_all(path).is_ok()
            } else {
                fs::remove_file(path).is_ok()
            }
        })
        .count();
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::SECONDS_PER_HOUR;
    use crate::lock::lock_path;

    fn base() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
    }

    fn hours(n: u64) -> Duration {
        Duration::from_secs(n * SECONDS_PER_HOUR)
    }

    #[test]
    fn primeira_checagem_e_imediata() {
        let schedule = Schedule::default();
        assert!(schedule.is_due(base()));
        assert_eq!(schedule.time_until_due(base()), Duration::ZERO);
    }

    #[test]
    fn proxima_checagem_so_apos_uma_hora() {
        let mut schedule = Schedule::default();
        schedule.record_attempt(base());
        assert!(!schedule.is_due(base()));

        let almost = base() + Duration::from_secs(SECONDS_PER_HOUR - 1);
        assert!(!schedule.is_due(almost));
        assert_eq!(schedule.time_until_due(almost), Duration::from_secs(1));

        assert!(schedule.is_due(base() + hours(1)));
    }

    #[test]
    fn falha_nao_antecipa_a_proxima_checagem_sem_retentativa() {
        // A falha não "reseta" o agendamento: continua valendo o início da última tentativa.
        let mut schedule = Schedule::default();
        schedule.record_attempt(base());
        let failed_at = base() + Duration::from_secs(5);
        assert!(!schedule.is_due(failed_at), "sem retentativa imediata");
        assert!(schedule.is_due(failed_at + hours(1)));
    }

    #[test]
    fn relogio_que_volta_no_tempo_nao_trava_o_agendamento() {
        let mut schedule = Schedule::default();
        schedule.record_attempt(base());
        assert!(schedule.is_due(base() - hours(5)));
    }

    #[test]
    fn orfaos_sao_so_os_estritamente_mais_velhos_que_24h() {
        let now = SystemTime::UNIX_EPOCH + hours(1000);
        let entries = vec![
            (PathBuf::from("novo"), now - hours(1)),
            (PathBuf::from("quase"), now - hours(24)),
            (
                PathBuf::from("velho"),
                now - hours(24) - Duration::from_secs(1),
            ),
            (PathBuf::from("antigo"), now - hours(240)),
            (PathBuf::from("futuro"), now + hours(1)),
        ];
        assert_eq!(
            select_orphans(entries, now),
            vec![PathBuf::from("velho"), PathBuf::from("antigo")]
        );
    }

    #[test]
    fn trava_nunca_e_orfa() {
        let now = SystemTime::UNIX_EPOCH + hours(1000);
        let entries = vec![(
            PathBuf::from("/c/db-modeling/updates/lock"),
            now - hours(500),
        )];
        assert_eq!(select_orphans(entries, now), Vec::<PathBuf>::new());
    }

    #[test]
    fn varredura_remove_arquivos_e_pastas_velhos_e_preserva_trava() {
        let cache = tempfile::tempdir().expect("tempdir");
        let lock = UpdateLock::try_acquire(cache.path())
            .expect("io")
            .expect("trava");
        let dir = lock.updates_dir().to_path_buf();
        let file = dir.join("download.tmp");
        let sub = dir.join("extraido");
        fs::write(&file, b"x").expect("arquivo");
        fs::create_dir(&sub).expect("pasta");
        fs::write(sub.join("a"), b"y").expect("filho");

        // Tudo foi modificado "agora"; 25 h depois, tudo é órfão exceto a trava.
        let later = SystemTime::now() + hours(25);
        assert_eq!(sweep_orphans(&lock, later).expect("varrer"), 2);
        assert!(!file.exists());
        assert!(!sub.exists());
        assert!(lock_path(cache.path()).exists());
    }

    #[test]
    fn varredura_preserva_itens_recentes() {
        let cache = tempfile::tempdir().expect("tempdir");
        let lock = UpdateLock::try_acquire(cache.path())
            .expect("io")
            .expect("trava");
        let file = lock.updates_dir().join("download.tmp");
        fs::write(&file, b"x").expect("arquivo");
        assert_eq!(sweep_orphans(&lock, SystemTime::now()).expect("varrer"), 0);
        assert!(file.exists());
    }

    #[test]
    fn quem_nao_tem_a_trava_nao_varre_e_o_item_em_uso_fica() {
        let cache = tempfile::tempdir().expect("tempdir");
        let holder = UpdateLock::try_acquire(cache.path())
            .expect("io")
            .expect("trava");
        let in_use = holder.updates_dir().join("download-em-andamento.part");
        fs::write(&in_use, b"x").expect("arquivo");

        // A segunda instância não obtém a trava, logo não tem como chamar `sweep_orphans`.
        let second = UpdateLock::try_acquire(cache.path()).expect("io");
        assert!(second.is_none());
        assert!(in_use.exists(), "item em uso permanece");
    }

    #[test]
    fn varredura_nunca_toca_a_pasta_updates_do_helper_fora_do_cache() {
        let cache = tempfile::tempdir().expect("tempdir");
        let app = tempfile::tempdir().expect("tempdir");
        let helper_updates = app.path().join("updates");
        fs::create_dir(&helper_updates).expect("pasta do helper");
        let versions = helper_updates.join("versions.txt");
        let install = app.path().join("install");
        fs::write(&versions, b"0.4.0").expect("versions");
        fs::create_dir(&install).expect("install");

        let lock = UpdateLock::try_acquire(cache.path())
            .expect("io")
            .expect("trava");
        let far_future = SystemTime::now() + hours(24 * 365);
        sweep_orphans(&lock, far_future).expect("varrer");
        assert!(versions.exists());
        assert!(install.exists());
    }
}
