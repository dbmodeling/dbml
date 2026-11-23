//! Controlador do auto-update do App Desktop.
//!
//! Dirige o `updater-net` numa **thread dedicada** (nunca na de UI nem no frame loop): primeira
//! checagem imediata e depois a cada 1 h (`updater::schedule`), checagem manual por comando,
//! instalação pela trait [`Installer`] e estado compartilhado que a UI só **lê** (e observa por
//! um canal `watch`, sem polling no lado da UI).
//!
//! A lógica de decisão ([`next_step`], [`detect_suspend`], [`settle_install`], [`cache_dir_with`])
//! é pura e testada; a thread e a rede são finas.
//!
//! Ponto de plug da instalação: [`crate::update_install`]. Interface com a UI: `Shell` guarda um
//! `Arc<Controller>` e chama [`Controller::view`], [`Controller::check_now`],
//! [`Controller::restart_ready`] e [`Controller::subscribe`].

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use tokio::sync::watch;
use updater::platform::{PlatformKey, TargetOs};
use updater::policy::{DisabledReason, SystemEnv, disabled_reason};
use updater::schedule::Schedule;
use updater::state::{Trigger, UpdateState};
use updater::version::parse_version;
use updater_net::etag::MemoryEtagStore;
use updater_net::fetch::SystemFreeSpace;
use updater_net::keys::KeySet;
use updater_net::round::{RoundError, RoundOutcome, RoundParams, Services, SkipReason, run_round};
use updater_net::ureq_transport::{TransportConfig, UreqTransport};

use crate::constants::{
    ENV_HOME, ENV_LOCALAPPDATA, ENV_XDG_CACHE_HOME, LINUX_CACHE_SUBDIR, MACOS_CACHES_SUBDIR,
    SUSPEND_GAP_SECS, UPDATE_TICK_MS,
};
use crate::update_install::{InstallError, Installer, platform_installer};
use crate::update_notice::UpdateNotice as PersistedNotice;
use crate::updater_ui::{
    DisabledText, KEY_UNSUPPORTED_PLATFORM, UpdateNotice, UpdateView, disabled_text, view_of,
};

/// Se o app pode consultar/instalar atualizações neste ambiente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    /// Pode atualizar.
    Enabled,
    /// Desativada por uma razão do `updater::policy` (explicação do empacotador etc.).
    Disabled(DisabledReason),
    /// O instalador não atua aqui (plataforma sem instalador, Linux fora do layout do tarball):
    /// nenhuma rodada, nenhum download. Carrega a chave `i18n` do motivo.
    InstallerUnsupported(&'static str),
}

impl Availability {
    /// Decide a disponibilidade: a razão de desativação tem precedência (o texto do pacote é mais
    /// útil que "sem instalador").
    #[must_use]
    pub fn decide(reason: Option<DisabledReason>, installer: &dyn Installer) -> Self {
        match reason {
            Some(reason) => Self::Disabled(reason),
            None if installer.is_supported() => Self::Enabled,
            None => Self::InstallerUnsupported(installer.unsupported_note()),
        }
    }

    fn note(&self) -> Option<DisabledText> {
        match self {
            Self::Enabled => None,
            Self::Disabled(reason) => Some(disabled_text(reason)),
            Self::InstallerUnsupported(key) => Some(DisabledText::Key(key)),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Lógica pura
// ---------------------------------------------------------------------------------------------

/// Entradas de [`next_step`].
#[derive(Debug, Clone, Copy)]
pub struct StepInput {
    /// O controlador está sendo encerrado.
    pub shutdown: bool,
    /// Há um pedido de checagem manual.
    pub manual_pending: bool,
    /// Checagens automáticas permitidas (`auto_update` ligado **e** ambiente habilitado).
    pub auto_enabled: bool,
    /// Tempo até a próxima checagem automática (`Schedule::time_until_due`).
    pub until_due: Duration,
}

/// O que a thread do updater faz a seguir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Encerrar a thread.
    Stop,
    /// Rodar uma rodada.
    Run(Trigger),
    /// Dormir até o evento (`None`: sem prazo, só um evento acorda; sem polling).
    Wait(Option<Duration>),
}

/// Decide o próximo passo. Manual tem prioridade; automático só quando vence o intervalo.
#[must_use]
pub fn next_step(input: StepInput) -> Step {
    if input.shutdown {
        Step::Stop
    } else if input.manual_pending {
        Step::Run(Trigger::Manual)
    } else if !input.auto_enabled {
        Step::Wait(None)
    } else if input.until_due.is_zero() {
        Step::Run(Trigger::Automatic)
    } else {
        Step::Wait(Some(input.until_due))
    }
}

/// O sistema foi suspenso? O relógio de parede avançou muito mais que o monotônico (que no Linux
/// e no macOS não conta o tempo suspenso).
#[must_use]
pub fn detect_suspend(wall: Duration, monotonic: Duration, threshold: Duration) -> bool {
    wall.saturating_sub(monotonic) >= threshold
}

/// Fecha a instalação na máquina de estados: `Ok` → `installed()`; `Err` → `fail()`. Devolve
/// `true` se instalou.
pub fn settle_install(state: &Mutex<UpdateState>, outcome: &Result<(), InstallError>) -> bool {
    let mut state = lock(state);
    if outcome.is_ok() {
        state.installed().is_ok()
    } else {
        let _ = state.fail();
        false
    }
}

/// Cache do usuário (`<cache>/db-modeling/updates/` fica dentro dele): `%LOCALAPPDATA%` no
/// Windows, `~/Library/Caches` no macOS, `$XDG_CACHE_HOME` ou `~/.cache` no Linux.
#[must_use]
pub fn cache_dir_with(os: TargetOs, var: &dyn Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let non_empty = |name: &str| {
        var(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    match os {
        TargetOs::Windows => non_empty(ENV_LOCALAPPDATA),
        TargetOs::MacOs => non_empty(ENV_HOME).map(|home| home.join(MACOS_CACHES_SUBDIR)),
        TargetOs::Linux => non_empty(ENV_XDG_CACHE_HOME)
            .or_else(|| non_empty(ENV_HOME).map(|home| home.join(LINUX_CACHE_SUBDIR))),
    }
}

/// Instalação pendente achada na abertura (Windows: `updates\versions.txt` e `install\`; o app foi
/// encerrado sem passar pelo helper): já abre em "Reiniciar para atualizar". Versão ilegível na
/// flag cai na versão em execução (só a existência da flag importa).
fn restore_pending_install(shared: &Shared, installer: &dyn Installer) {
    if !installer.is_supported() {
        return;
    }
    let Some(text) = installer.pending_version() else {
        return;
    };
    let version = parse_version(&text).or_else(|_| parse_version(env!("CARGO_PKG_VERSION")));
    if let Ok(version) = version {
        let _ = lock(&shared.state).restore_pending(version);
    }
}

fn skip_notice(reason: &SkipReason) -> Option<UpdateNotice> {
    match reason {
        SkipReason::Disabled(reason) => Some(UpdateNotice::Disabled(disabled_text(reason))),
        SkipReason::UnsupportedPlatform => Some(UpdateNotice::Disabled(DisabledText::Key(
            KEY_UNSUPPORTED_PLATFORM,
        ))),
        SkipReason::LockBusy => Some(UpdateNotice::AnotherInstance),
        SkipReason::AutoUpdateOff | SkipReason::Busy => None,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---------------------------------------------------------------------------------------------
// Estado compartilhado
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
struct Inner {
    auto_update: bool,
    manual_pending: bool,
    /// A próxima checagem automática deve rodar já (retomada de suspensão).
    force_due: bool,
    shutdown: bool,
    notice: Option<UpdateNotice>,
}

struct Shared {
    state: Mutex<UpdateState>,
    inner: Mutex<Inner>,
    wake: Condvar,
    changed: watch::Sender<u64>,
    availability: Availability,
}

impl Shared {
    fn new(availability: Availability, auto_update: bool) -> Self {
        Self {
            state: Mutex::new(UpdateState::new()),
            inner: Mutex::new(Inner {
                auto_update,
                manual_pending: false,
                force_due: false,
                shutdown: false,
                notice: None,
            }),
            wake: Condvar::new(),
            changed: watch::channel(0).0,
            availability,
        }
    }

    fn bump(&self) {
        self.changed.send_modify(|generation| {
            *generation = generation.wrapping_add(1);
        });
    }

    fn set_notice(&self, notice: Option<UpdateNotice>) {
        lock(&self.inner).notice = notice;
        self.bump();
    }

    fn view(&self) -> UpdateView {
        let notice = lock(&self.inner).notice.clone();
        view_of(lock(&self.state).status(), notice.as_ref())
    }

    fn check_now(&self) {
        if let Some(text) = self.availability.note() {
            self.set_notice(Some(UpdateNotice::Disabled(text)));
            return;
        }
        if lock(&self.state).is_busy() {
            return;
        }
        {
            let mut inner = lock(&self.inner);
            inner.manual_pending = true;
            inner.notice = None;
        }
        self.wake.notify_all();
        self.bump();
    }

    /// Retomada de suspensão: só as fases de rede reiniciam (`resume_after_suspend`). Manual em
    /// andamento refaz a checagem manual; automática antecipa a próxima.
    fn on_resume(&self) {
        let (restart, trigger) = {
            let mut state = lock(&self.state);
            (state.resume_after_suspend(), state.trigger())
        };
        if restart {
            self.request_retry(trigger);
        }
        self.bump();
    }

    fn request_retry(&self, trigger: Trigger) {
        {
            let mut inner = lock(&self.inner);
            match trigger {
                Trigger::Manual => inner.manual_pending = true,
                Trigger::Automatic => inner.force_due = true,
            }
        }
        self.wake.notify_all();
    }

    fn status_changed_since(
        &self,
        last: &updater::state::UpdateStatus,
    ) -> Option<updater::state::UpdateStatus> {
        let status = lock(&self.state).status().clone();
        (&status != last).then_some(status)
    }
}

/// Ponto de entrada de uma rodada (a rede real em produção, falsa nos testes).
pub type Runner = Box<dyn Fn(&Mutex<UpdateState>, Trigger, bool) -> RoundOutcome + Send + Sync>;

/// Controlador observado pela UI.
pub struct Controller {
    shared: Arc<Shared>,
}

impl Controller {
    /// Inicia o controlador de produção: política do sistema, instalador da plataforma e rede
    /// real. A thread nasce aqui, mas só toca na rede se a rodada for permitida.
    #[must_use]
    pub fn start(auto_update: bool) -> Self {
        let installer = platform_installer();
        let availability = Availability::decide(disabled_reason(&SystemEnv), installer.as_ref());
        Self::spawn(availability, auto_update, installer, net_runner)
    }

    fn spawn(
        availability: Availability,
        auto_update: bool,
        installer: Box<dyn Installer>,
        make_runner: impl FnOnce() -> Runner + Send + 'static,
    ) -> Self {
        let shared = Arc::new(Shared::new(availability, auto_update));
        restore_pending_install(&shared, installer.as_ref());
        let worker = Arc::clone(&shared);
        let spawned = thread::Builder::new()
            .name(crate::constants::UPDATER_THREAD_NAME.to_owned())
            .spawn(move || worker_main(&worker, &make_runner(), installer.as_ref()));
        if let Err(error) = spawned {
            tracing::warn!("thread do updater não iniciou: {error}");
        }
        Self { shared }
    }

    /// O que o controle da title bar mostra agora.
    #[must_use]
    pub fn view(&self) -> UpdateView {
        self.shared.view()
    }

    /// Texto de desativação (ou "sem instalador") para a tela de configurações; `None` = ativo.
    #[must_use]
    pub fn disabled_note(&self) -> Option<DisabledText> {
        self.shared.availability.note()
    }

    /// Versão já instalada que aguarda reinício, se houver.
    #[must_use]
    pub fn restart_ready(&self) -> Option<String> {
        let state = lock(&self.shared.state);
        matches!(state.status(), updater::state::UpdateStatus::Updated(_))
            .then(|| state.pending().map(ToString::to_string))
            .flatten()
    }

    /// Checagem manual ("Verificar atualizações"). Desativado: mostra a razão no lugar da ação.
    pub fn check_now(&self) {
        self.shared.check_now();
    }

    /// Liga/desliga a checagem automática (o chamador persiste em `settings.txt`).
    pub fn set_auto_update(&self, on: bool) {
        lock(&self.shared.inner).auto_update = on;
        self.shared.wake.notify_all();
        self.shared.bump();
    }

    /// Dispensa "Falha ao atualizar", "Atualizado" e os avisos.
    pub fn dismiss(&self) {
        let _ = lock(&self.shared.state).dismiss();
        self.shared.set_notice(None);
    }

    /// Fim dos 4 s de um aviso passageiro ("Atualizado" ou "outra janela já verifica"): dispensa
    /// só se ele ainda estiver na tela (uma checagem nova que já começou não pode ser apagada
    /// pelo temporizador antigo).
    pub fn dismiss_transient(&self) {
        let dismissed = {
            let mut state = lock(&self.shared.state);
            let mut inner = lock(&self.shared.inner);
            let up_to_date = matches!(state.status(), updater::state::UpdateStatus::UpToDate)
                && state.dismiss().is_ok();
            let another = inner.notice == Some(UpdateNotice::AnotherInstance);
            if another {
                inner.notice = None;
            }
            up_to_date || another
        };
        if dismissed {
            self.shared.bump();
        }
    }

    /// O relançamento não abriu; o app segue aberto e a versão nova continua instalada.
    pub fn report_restart_failed(&self) {
        self.shared.set_notice(Some(UpdateNotice::RestartFailed));
    }

    /// Canal que muda a cada alteração de estado: a UI espera em `changed()` (sem polling).
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.shared.changed.subscribe()
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        // Não faz `join`: a thread pode estar numa rodada de rede, e o app já está saindo.
        lock(&self.shared.inner).shutdown = true;
        self.shared.wake.notify_all();
    }
}

// ---------------------------------------------------------------------------------------------
// Thread do updater
// ---------------------------------------------------------------------------------------------

fn suspend_threshold() -> Duration {
    Duration::from_secs(SUSPEND_GAP_SECS)
}

/// Espera trabalho: devolve o gatilho da próxima rodada, ou `None` ao encerrar.
fn wait_for_work(shared: &Shared, schedule: &mut Schedule) -> Option<Trigger> {
    let mut inner = lock(&shared.inner);
    loop {
        if std::mem::take(&mut inner.force_due) {
            *schedule = Schedule::default();
        }
        let now = SystemTime::now();
        let step = next_step(StepInput {
            shutdown: inner.shutdown,
            manual_pending: inner.manual_pending,
            auto_enabled: inner.auto_update && shared.availability == Availability::Enabled,
            until_due: schedule.time_until_due(now),
        });
        match step {
            Step::Stop => return None,
            Step::Run(trigger) => {
                if trigger == Trigger::Manual {
                    inner.manual_pending = false;
                }
                return Some(trigger);
            }
            Step::Wait(None) => {
                inner = shared
                    .wake
                    .wait(inner)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            Step::Wait(Some(timeout)) => {
                inner = shared
                    .wake
                    .wait_timeout(inner, timeout)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0;
            }
        }
    }
}

/// Roda a rodada com um "ticker" que (1) avisa a UI quando o estado muda no meio dela
/// (`Checking → Downloading → Installing`; o `updater-net` muda o estado sem notificar) e (2)
/// percebe a suspensão do sistema pelo salto do relógio entre dois ticks: a rodada de rede em
/// andamento volta ao repouso (`resume_after_suspend`) e é refeita.
fn run_with_ticker(
    shared: &Shared,
    runner: &Runner,
    trigger: Trigger,
    auto_update: bool,
) -> RoundOutcome {
    let (stop, stopped) = mpsc::channel::<()>();
    thread::scope(|scope| {
        scope.spawn(move || {
            let mut last = lock(&shared.state).status().clone();
            let (mut wall, mut mono) = (SystemTime::now(), Instant::now());
            while let Err(RecvTimeoutError::Timeout) =
                stopped.recv_timeout(Duration::from_millis(UPDATE_TICK_MS))
            {
                let (wall_now, mono_now) = (SystemTime::now(), Instant::now());
                let wall_gap = wall_now.duration_since(wall).unwrap_or_default();
                if detect_suspend(wall_gap, mono_now - mono, suspend_threshold()) {
                    shared.on_resume();
                }
                (wall, mono) = (wall_now, mono_now);
                if let Some(status) = shared.status_changed_since(&last) {
                    last = status;
                    shared.bump();
                }
            }
        });
        let outcome = runner(&shared.state, trigger, auto_update);
        drop(stop);
        outcome
    })
}

fn worker_main(shared: &Shared, runner: &Runner, installer: &dyn Installer) {
    let mut schedule = Schedule::default();
    while let Some(trigger) = wait_for_work(shared, &mut schedule) {
        let auto_update = lock(&shared.inner).auto_update;
        schedule.record_attempt(SystemTime::now());
        shared.bump();
        match run_with_ticker(shared, runner, trigger, auto_update) {
            RoundOutcome::Skipped(reason) => {
                if trigger == Trigger::Manual {
                    shared.set_notice(skip_notice(&reason));
                }
            }
            RoundOutcome::UpToDate => {}
            RoundOutcome::Ready(update) => install(shared, installer, update),
            RoundOutcome::Failed(error) => {
                tracing::warn!("rodada de atualização falhou: {error}");
            }
        }
        shared.bump();
    }
}

/// Instala segurando o `VerifiedUpdate` (e, com ele, a trava) até o fim.
fn install(shared: &Shared, installer: &dyn Installer, update: updater_net::round::VerifiedUpdate) {
    install_in(
        shared,
        installer,
        update,
        crate::session::config_dir().as_deref(),
    );
}

fn install_in(
    shared: &Shared,
    installer: &dyn Installer,
    update: updater_net::round::VerifiedUpdate,
    config_dir: Option<&std::path::Path>,
) {
    let outcome = update
        .recheck()
        .map_err(InstallError::Recheck)
        .and_then(|()| installer.install(&update));
    if let Err(error) = &outcome {
        tracing::warn!("instalação da atualização falhou: {error}");
    }
    if settle_install(&shared.state, &outcome)
        && let Some(dir) = config_dir
    {
        remember_for_next_start(&update, dir);
    }
    drop(update);
}

/// Grava o aviso "Atualizado para X" para o primeiro start depois do reinício.
fn remember_for_next_start(update: &updater_net::round::VerifiedUpdate, dir: &std::path::Path) {
    let notice = PersistedNotice {
        version: update.version().to_string(),
        notes_url: update.notes_url().to_owned(),
    };
    if let Err(error) = crate::update_notice::save(dir, &notice) {
        tracing::warn!("não foi possível gravar o aviso de atualização: {error}");
    }
}

/// Runner de produção: rede real (`ureq` + `rustls`), `ETag` em memória, chaves embutidas.
/// Construído na própria thread do updater (carregar as raízes do sistema não toca a UI).
fn net_runner() -> Runner {
    let transport = UreqTransport::new(&TransportConfig::from_environment());
    let etags = MemoryEtagStore::new();
    let keys = KeySet::from_environment();
    let cache_dir =
        TargetOs::current().and_then(|os| cache_dir_with(os, &|name| std::env::var_os(name)));
    let current = parse_version(env!("CARGO_PKG_VERSION"));
    Box::new(move |state, trigger, auto_update| {
        let failure = |error: RoundError| {
            let mut state = lock(state);
            if state.begin_check(trigger).is_ok() {
                let _ = state.fail();
            }
            RoundOutcome::Failed(error)
        };
        let transport = match &transport {
            Ok(transport) => transport,
            Err(error) => return failure(RoundError::Transport(error.clone())),
        };
        let Some(cache_dir) = &cache_dir else {
            return failure(RoundError::Lock(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "pasta de cache do usuário indisponível",
            )));
        };
        let Ok(current) = &current else {
            return RoundOutcome::Skipped(SkipReason::UnsupportedPlatform);
        };
        let services = Services {
            transport,
            env: &SystemEnv,
            state,
            etags: &etags,
            free_space: &SystemFreeSpace,
            keys: &keys,
        };
        run_round(
            &services,
            &RoundParams {
                cache_dir,
                current_version: current,
                platform: PlatformKey::current(),
                auto_update,
                trigger,
                now: SystemTime::now(),
            },
        )
    })
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::Receiver;

    use super::*;
    use crate::update_install::UnsupportedInstaller;

    const WAIT: Duration = Duration::from_secs(5);
    const QUIET: Duration = Duration::from_millis(300);

    fn input() -> StepInput {
        StepInput {
            shutdown: false,
            manual_pending: false,
            auto_enabled: true,
            until_due: Duration::ZERO,
        }
    }

    #[test]
    fn primeira_checagem_automatica_e_imediata() {
        assert_eq!(next_step(input()), Step::Run(Trigger::Automatic));
    }

    #[test]
    fn automatica_espera_o_intervalo() {
        let hour = Duration::from_secs(3600);
        assert_eq!(
            next_step(StepInput {
                until_due: hour,
                ..input()
            }),
            Step::Wait(Some(hour))
        );
    }

    #[test]
    fn desligada_dorme_sem_prazo_e_sem_polling() {
        assert_eq!(
            next_step(StepInput {
                auto_enabled: false,
                ..input()
            }),
            Step::Wait(None)
        );
    }

    #[test]
    fn manual_tem_prioridade_mesmo_com_automatica_desligada() {
        assert_eq!(
            next_step(StepInput {
                manual_pending: true,
                auto_enabled: false,
                until_due: Duration::from_secs(10),
                ..input()
            }),
            Step::Run(Trigger::Manual)
        );
    }

    #[test]
    fn encerrar_tem_prioridade_sobre_tudo() {
        assert_eq!(
            next_step(StepInput {
                shutdown: true,
                manual_pending: true,
                ..input()
            }),
            Step::Stop
        );
    }

    #[test]
    fn suspensao_e_o_salto_do_relogio_de_parede() {
        let threshold = Duration::from_secs(30);
        assert!(detect_suspend(
            Duration::from_secs(3600),
            Duration::from_secs(10),
            threshold
        ));
        assert!(!detect_suspend(
            Duration::from_secs(12),
            Duration::from_secs(10),
            threshold
        ));
        // relógio de parede atrasado (ajuste manual) não é suspensão
        assert!(!detect_suspend(
            Duration::from_secs(1),
            Duration::from_secs(100),
            threshold
        ));
    }

    #[test]
    fn disponibilidade_prioriza_a_razao_de_desativacao() {
        let supported = PendingInstaller {
            supported: true,
            pending: None,
        };
        assert_eq!(
            Availability::decide(None, &supported),
            Availability::Enabled
        );
        assert_eq!(
            Availability::decide(None, &UnsupportedInstaller),
            Availability::InstallerUnsupported(KEY_UNSUPPORTED_PLATFORM)
        );
        assert_eq!(
            Availability::decide(Some(DisabledReason::ProgramFiles), &UnsupportedInstaller),
            Availability::Disabled(DisabledReason::ProgramFiles)
        );
    }

    #[test]
    fn motivo_do_instalador_aparece_no_lugar_da_acao() {
        struct Outside;
        impl Installer for Outside {
            fn is_supported(&self) -> bool {
                false
            }
            fn unsupported_note(&self) -> &'static str {
                crate::updater_ui::KEY_LINUX_NOT_TARBALL
            }
            fn install(
                &self,
                _update: &updater_net::round::VerifiedUpdate,
            ) -> Result<(), InstallError> {
                Err(InstallError::Unsupported)
            }
        }
        let availability = Availability::decide(None, &Outside);
        assert_eq!(
            availability.note(),
            Some(DisabledText::Key(crate::updater_ui::KEY_LINUX_NOT_TARBALL))
        );
    }

    #[test]
    fn cache_por_sistema() {
        let vars = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| OsString::from(value))
            }
        };
        assert_eq!(
            cache_dir_with(TargetOs::Linux, &vars(&[("HOME", "/home/u")])),
            Some(PathBuf::from("/home/u/.cache"))
        );
        assert_eq!(
            cache_dir_with(
                TargetOs::Linux,
                &vars(&[("HOME", "/home/u"), ("XDG_CACHE_HOME", "/x")])
            ),
            Some(PathBuf::from("/x"))
        );
        assert_eq!(
            cache_dir_with(TargetOs::Linux, &vars(&[("XDG_CACHE_HOME", "")])),
            None
        );
        assert_eq!(
            cache_dir_with(TargetOs::MacOs, &vars(&[("HOME", "/Users/u")])),
            Some(PathBuf::from("/Users/u/Library/Caches"))
        );
        assert_eq!(
            cache_dir_with(TargetOs::Windows, &vars(&[("LOCALAPPDATA", "C:\\L")])),
            Some(PathBuf::from("C:\\L"))
        );
        assert_eq!(cache_dir_with(TargetOs::Windows, &vars(&[])), None);
    }

    fn installing_state(trigger: Trigger) -> Mutex<UpdateState> {
        let mut state = UpdateState::new();
        state.begin_check(trigger).expect("begin");
        state
            .found_update(parse_version("0.4.0").expect("versão"))
            .expect("found");
        state.download_verified().expect("download");
        Mutex::new(state)
    }

    #[test]
    fn instalacao_ok_vira_reiniciar() {
        let state = installing_state(Trigger::Automatic);
        assert!(settle_install(&state, &Ok(())));
        assert_eq!(
            view_of(lock(&state).status(), None),
            UpdateView::Restart("0.4.0".to_owned())
        );
    }

    #[test]
    fn falha_na_instalacao_e_silenciosa_se_automatica_e_visivel_se_manual() {
        let automatic = installing_state(Trigger::Automatic);
        assert!(!settle_install(&automatic, &Err(InstallError::Unsupported)));
        assert_eq!(view_of(lock(&automatic).status(), None), UpdateView::Hidden);

        let manual = installing_state(Trigger::Manual);
        assert!(!settle_install(&manual, &Err(InstallError::Unsupported)));
        assert_eq!(view_of(lock(&manual).status(), None), UpdateView::Failed);
    }

    #[test]
    fn retomada_de_suspensao_refaz_a_rodada_manual_em_andamento() {
        let shared = Shared::new(Availability::Enabled, false);
        {
            let mut state = lock(&shared.state);
            state.begin_check(Trigger::Manual).expect("begin");
        }
        shared.on_resume();
        assert!(lock(&shared.inner).manual_pending);
        assert_eq!(shared.view(), UpdateView::Hidden);
    }

    #[test]
    fn retomada_de_suspensao_antecipa_a_automatica_e_ignora_instalacao() {
        let shared = Shared::new(Availability::Enabled, true);
        lock(&shared.state)
            .begin_check(Trigger::Automatic)
            .expect("begin");
        shared.on_resume();
        assert!(lock(&shared.inner).force_due);

        // instalando: a fase de disco não reinicia
        let shared = Shared::new(Availability::Enabled, true);
        *lock(&shared.state) = installing_state(Trigger::Automatic)
            .into_inner()
            .expect("state");
        shared.on_resume();
        assert!(!lock(&shared.inner).force_due);
        assert_eq!(shared.view(), UpdateView::Installing);
    }

    #[test]
    fn manual_durante_operacao_em_andamento_e_ignorado() {
        let shared = Shared::new(Availability::Enabled, true);
        lock(&shared.state)
            .begin_check(Trigger::Automatic)
            .expect("begin");
        shared.check_now();
        assert!(!lock(&shared.inner).manual_pending);
    }

    #[test]
    fn manual_desativado_mostra_a_razao_sem_pedir_rodada() {
        let shared = Shared::new(
            Availability::Disabled(DisabledReason::File("use o apt".to_owned())),
            true,
        );
        shared.check_now();
        assert!(!lock(&shared.inner).manual_pending);
        assert_eq!(
            shared.view(),
            UpdateView::Disabled(DisabledText::Verbatim("use o apt".to_owned()))
        );
    }

    #[test]
    fn aviso_passageiro_so_e_dispensado_enquanto_estiver_na_tela() {
        let controller = Controller {
            shared: Arc::new(Shared::new(Availability::Enabled, true)),
        };
        {
            let mut state = lock(&controller.shared.state);
            state.begin_check(Trigger::Manual).expect("begin");
            state.no_update().expect("no update");
        }
        assert_eq!(controller.view(), UpdateView::UpToDate);
        controller.dismiss_transient();
        assert_eq!(controller.view(), UpdateView::Hidden);

        // checagem nova em andamento não é apagada pelo temporizador antigo
        lock(&controller.shared.state)
            .begin_check(Trigger::Manual)
            .expect("begin");
        controller.dismiss_transient();
        assert_eq!(controller.view(), UpdateView::Checking);
    }

    #[test]
    fn aviso_de_outra_instancia_some_pelo_temporizador_e_falha_ao_reiniciar_nao() {
        let controller = Controller {
            shared: Arc::new(Shared::new(Availability::Enabled, true)),
        };
        controller
            .shared
            .set_notice(Some(UpdateNotice::AnotherInstance));
        assert_eq!(controller.view(), UpdateView::AnotherInstance);
        controller.dismiss_transient();
        assert_eq!(controller.view(), UpdateView::Hidden);

        controller.report_restart_failed();
        controller.dismiss_transient();
        assert_eq!(controller.view(), UpdateView::RestartFailed);
    }

    #[test]
    fn aviso_de_falha_ao_reiniciar_e_dispensavel() {
        let controller = Controller {
            shared: Arc::new(Shared::new(Availability::Enabled, true)),
        };
        controller.report_restart_failed();
        assert_eq!(controller.view(), UpdateView::RestartFailed);
        controller.dismiss();
        assert_eq!(controller.view(), UpdateView::Hidden);
    }

    // ---- thread real com rodada falsa ----

    fn fake_controller(
        availability: Availability,
        auto_update: bool,
    ) -> (Controller, Receiver<Trigger>) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let controller = Controller::spawn(
            availability,
            auto_update,
            Box::new(UnsupportedInstaller),
            move || {
                Box::new(move |_state, trigger, _auto| {
                    let _ = lock(&tx).send(trigger);
                    RoundOutcome::UpToDate
                })
            },
        );
        (controller, rx)
    }

    #[test]
    fn thread_roda_a_primeira_checagem_na_hora() {
        let (_controller, rx) = fake_controller(Availability::Enabled, true);
        assert_eq!(rx.recv_timeout(WAIT), Ok(Trigger::Automatic));
        // e só de novo daqui a 1 h: nada mais chega
        assert!(rx.recv_timeout(QUIET).is_err());
    }

    #[test]
    fn automatica_desligada_nao_toca_na_rede_mas_a_manual_roda() {
        let (controller, rx) = fake_controller(Availability::Enabled, false);
        assert!(rx.recv_timeout(QUIET).is_err());
        controller.check_now();
        assert_eq!(rx.recv_timeout(WAIT), Ok(Trigger::Manual));
    }

    #[test]
    fn ligar_a_automatica_dispara_a_checagem() {
        let (controller, rx) = fake_controller(Availability::Enabled, false);
        assert!(rx.recv_timeout(QUIET).is_err());
        controller.set_auto_update(true);
        assert_eq!(rx.recv_timeout(WAIT), Ok(Trigger::Automatic));
    }

    #[test]
    fn desativado_nunca_roda_nem_manual() {
        let (controller, rx) = fake_controller(
            Availability::InstallerUnsupported(KEY_UNSUPPORTED_PLATFORM),
            true,
        );
        controller.check_now();
        assert!(rx.recv_timeout(QUIET).is_err());
        assert!(matches!(controller.view(), UpdateView::Disabled(_)));
    }

    #[test]
    fn ui_e_avisada_por_canal_sem_polling() {
        let (controller, rx) = fake_controller(Availability::Enabled, false);
        let mut changes = controller.subscribe();
        controller.check_now();
        assert_eq!(rx.recv_timeout(WAIT), Ok(Trigger::Manual));
        assert!(changes.has_changed().expect("canal aberto"));
        changes.mark_changed();
    }

    /// Instalador falso que anuncia uma instalação pendente na abertura.
    struct PendingInstaller {
        supported: bool,
        pending: Option<&'static str>,
    }

    impl Installer for PendingInstaller {
        fn is_supported(&self) -> bool {
            self.supported
        }

        fn install(
            &self,
            _update: &updater_net::round::VerifiedUpdate,
        ) -> Result<(), InstallError> {
            Err(InstallError::Unsupported)
        }

        fn pending_version(&self) -> Option<String> {
            self.pending.map(str::to_owned)
        }
    }

    fn pending_controller(installer: PendingInstaller) -> Controller {
        Controller::spawn(Availability::Enabled, false, Box::new(installer), || {
            Box::new(|_state, _trigger, _auto| RoundOutcome::UpToDate)
        })
    }

    #[test]
    fn instalacao_pendente_na_abertura_vira_reiniciar_para_atualizar() {
        let controller = pending_controller(PendingInstaller {
            supported: true,
            pending: Some("0.9.0"),
        });
        assert_eq!(controller.restart_ready().as_deref(), Some("0.9.0"));
        assert!(matches!(controller.view(), UpdateView::Restart(_)));
    }

    #[test]
    fn flag_sem_versao_legivel_usa_a_versao_em_execucao() {
        let controller = pending_controller(PendingInstaller {
            supported: true,
            pending: Some(""),
        });
        assert_eq!(
            controller.restart_ready().as_deref(),
            Some(env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn sem_pendencia_ou_sem_instalador_abre_em_repouso() {
        for installer in [
            PendingInstaller {
                supported: true,
                pending: None,
            },
            PendingInstaller {
                supported: false,
                pending: Some("0.9.0"),
            },
        ] {
            let controller = pending_controller(installer);
            assert_eq!(controller.restart_ready(), None);
            assert!(matches!(controller.view(), UpdateView::Hidden));
        }
    }

    #[test]
    fn install_recalcula_hash_antes_de_chamar_instalador_e_grava_aviso() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use updater::constants::{MANIFEST_SIGNATURE_URL, MANIFEST_URL};
        use updater::policy::PolicyEnv;
        use updater::transport::{Fetched, Transport, TransportError};

        struct FixtureEnv;
        impl PolicyEnv for FixtureEnv {
            fn var(&self, _name: &str) -> Option<String> {
                None
            }
            fn read_file(&self, _path: &std::path::Path) -> Option<String> {
                None
            }
            fn target_os(&self) -> Option<TargetOs> {
                Some(TargetOs::Linux)
            }
            fn app_dir(&self) -> Option<PathBuf> {
                Some(PathBuf::from("/home/u/.local/db-modeling.app"))
            }
            fn canonicalize(&self, path: &std::path::Path) -> Option<PathBuf> {
                Some(path.to_path_buf())
            }
            fn is_writable(&self, _dir: &std::path::Path) -> bool {
                true
            }
        }

        struct FixtureTransport;
        impl Transport for FixtureTransport {
            fn get(
                &self,
                url: &str,
                _etag: Option<&str>,
                _max: u64,
            ) -> Result<Fetched, TransportError> {
                let bytes = match url {
                    MANIFEST_URL => {
                        include_bytes!("../../updater/tests/fixtures/latest.json").to_vec()
                    }
                    MANIFEST_SIGNATURE_URL => {
                        include_bytes!("../../updater/tests/fixtures/latest.json.sig").to_vec()
                    }
                    other => panic!("url inesperada: {other}"),
                };
                Ok(Fetched::Body { bytes, etag: None })
            }
            fn download(
                &self,
                _url: &str,
                dest: &std::path::Path,
                _max: u64,
            ) -> Result<(), TransportError> {
                // O SHA-256 de `"tarball fictício do linux\n"` é o da fixture `latest.json`.
                std::fs::write(dest, "tarball fictício do linux\n").unwrap();
                Ok(())
            }
        }

        struct UnlimitedSpace;
        impl updater_net::fetch::FreeSpace for UnlimitedSpace {
            fn available(&self, _dir: &std::path::Path) -> std::io::Result<u64> {
                Ok(u64::MAX)
            }
        }

        struct CountingInstaller(Arc<AtomicUsize>);
        impl Installer for CountingInstaller {
            fn is_supported(&self) -> bool {
                true
            }
            fn install(
                &self,
                _update: &updater_net::round::VerifiedUpdate,
            ) -> Result<(), InstallError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }

        let test_pub = include_str!("../../updater/tests/fixtures/test-public-key.txt").trim();
        let keys = KeySet::Custom(vec![test_pub.to_owned()]);
        let dir = tempfile::tempdir().expect("tempdir");

        let make_ready = |cache_sub: &str| {
            let shared = Shared::new(Availability::Enabled, true);
            let cache = dir.path().join(cache_sub);
            let etags = MemoryEtagStore::new();
            let services = Services {
                transport: &FixtureTransport,
                env: &FixtureEnv,
                state: &shared.state,
                etags: &etags,
                free_space: &UnlimitedSpace,
                keys: &keys,
            };
            let current = parse_version("0.3.0").unwrap();
            let outcome = run_round(
                &services,
                &RoundParams {
                    cache_dir: &cache,
                    current_version: &current,
                    platform: Some(PlatformKey::LinuxX86_64),
                    auto_update: true,
                    trigger: Trigger::Manual,
                    now: SystemTime::now(),
                },
            );
            let RoundOutcome::Ready(update) = outcome else {
                panic!("esperava Ready, veio {outcome:?}");
            };
            (shared, update)
        };

        // (a) Caminho feliz: recheck passa, instalador é chamado e update-notice.txt é gravado.
        let (shared_ok, update_ok) = make_ready("cache-ok");
        let calls = Arc::new(AtomicUsize::new(0));
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap();
        install_in(
            &shared_ok,
            &CountingInstaller(Arc::clone(&calls)),
            update_ok,
            Some(&config_dir),
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            view_of(lock(&shared_ok.state).status(), None),
            UpdateView::Restart("0.4.0".to_owned())
        );
        let saved = crate::update_notice::take(&config_dir, "0.4.0").expect("aviso gravado");
        assert_eq!(saved.version, "0.4.0");

        // (b) Artefato adulterado entre o download e a instalação: recheck barra antes do instalador.
        let (shared_bad, update_bad) = make_ready("cache-bad");
        std::fs::write(update_bad.path(), b"adulterado-depois-do-download").unwrap();
        let calls_bad = Arc::new(AtomicUsize::new(0));
        install_in(
            &shared_bad,
            &CountingInstaller(Arc::clone(&calls_bad)),
            update_bad,
            Some(&config_dir),
        );
        assert_eq!(calls_bad.load(Ordering::SeqCst), 0);
        assert_eq!(
            view_of(lock(&shared_bad.state).status(), None),
            UpdateView::Failed
        );
        assert!(crate::update_notice::take(&config_dir, "0.4.0").is_none());
    }
}
