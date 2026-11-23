//! Critério de aceite 3: com `auto_update = false` ou qualquer razão de desativação ativa, uma
//! rodada automática não faz nenhuma requisição; a manual só é bloqueada por razão de
//! desativação. Liga política + máquina de estados + `FakeTransport`, sem rede.
//!
//! `run_round` é o contrato que o ticket 03 implementa de verdade: perguntar à política antes de
//! tocar no transporte e só então mexer no estado.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};

use updater::constants::{ENV_UPDATE_EXPLANATION, MANIFEST_URL, UPDATE_EXPLANATION_FILE};
use updater::platform::TargetOs;
use updater::policy::{
    DisabledReason, PolicyEnv, disabled_reason, may_check_automatically, may_check_manually,
};
use updater::state::{Trigger, UpdateState, UpdateStatus};
use updater::transport::fake::FakeTransport;
use updater::transport::{Fetched, Transport};

const MAX_BYTES: u64 = 1024;

/// Executa uma rodada de checagem; devolve se ela foi tentada.
fn run_round(
    trigger: Trigger,
    auto_update: bool,
    reason: Option<&DisabledReason>,
    state: &mut UpdateState,
    transport: &dyn Transport,
) -> bool {
    let allowed = match trigger {
        Trigger::Automatic => may_check_automatically(auto_update, reason),
        Trigger::Manual => may_check_manually(reason),
    };
    if !allowed {
        return false;
    }
    state.begin_check(trigger).expect("estado em repouso");
    match transport.get(MANIFEST_URL, None, MAX_BYTES) {
        Ok(Fetched::Body { .. } | Fetched::NotModified) => state.no_update().expect("checking"),
        Err(_) => state.fail().expect("checking"),
    }
    true
}

fn fake_with_manifest() -> FakeTransport {
    FakeTransport::new().with_body(MANIFEST_URL, b"{}")
}

/// Uma razão de cada tipo que a política pode produzir.
fn all_reasons() -> Vec<DisabledReason> {
    vec![
        DisabledReason::Env("texto".into()),
        DisabledReason::File("texto".into()),
        DisabledReason::UnsupportedPlatform,
        DisabledReason::UnknownAppLocation,
        DisabledReason::ProgramFiles,
        DisabledReason::AppTranslocation,
        DisabledReason::NotWritable,
    ]
}

#[test]
fn auto_update_desligado_nao_faz_requisicao_automatica() {
    let transport = fake_with_manifest();
    let mut state = UpdateState::new();
    let ran = run_round(Trigger::Automatic, false, None, &mut state, &transport);
    assert!(!ran);
    assert_eq!(transport.requests(), Vec::<String>::new());
    assert_eq!(state.status(), &UpdateStatus::Idle);
}

#[test]
fn qualquer_razao_ativa_bloqueia_automatica_e_manual_sem_requisicao() {
    for reason in all_reasons() {
        for auto_update in [true, false] {
            let transport = fake_with_manifest();
            let mut state = UpdateState::new();
            for trigger in [Trigger::Automatic, Trigger::Manual] {
                let ran = run_round(trigger, auto_update, Some(&reason), &mut state, &transport);
                assert!(!ran, "{reason:?} {trigger:?} auto_update={auto_update}");
            }
            assert_eq!(transport.requests(), Vec::<String>::new(), "{reason:?}");
            assert_eq!(state.status(), &UpdateStatus::Idle);
        }
    }
}

#[test]
fn auto_update_desligado_ainda_permite_checagem_manual() {
    let transport = fake_with_manifest();
    let mut state = UpdateState::new();
    assert!(run_round(
        Trigger::Manual,
        false,
        None,
        &mut state,
        &transport
    ));
    assert_eq!(transport.requests(), vec![MANIFEST_URL.to_owned()]);
    assert_eq!(state.status(), &UpdateStatus::UpToDate);
}

#[test]
fn ligado_e_sem_razao_a_rodada_automatica_requisita_o_manifesto_uma_vez() {
    let transport = fake_with_manifest();
    let mut state = UpdateState::new();
    assert!(run_round(
        Trigger::Automatic,
        true,
        None,
        &mut state,
        &transport
    ));
    assert_eq!(transport.requests(), vec![MANIFEST_URL.to_owned()]);
    assert_eq!(state.status(), &UpdateStatus::Idle, "silencioso");
}

#[test]
fn falha_de_rede_automatica_e_silenciosa_e_manual_vira_errored() {
    let broken = FakeTransport::new();
    let mut state = UpdateState::new();
    run_round(Trigger::Automatic, true, None, &mut state, &broken);
    assert_eq!(state.status(), &UpdateStatus::Idle);
    run_round(Trigger::Manual, true, None, &mut state, &broken);
    assert_eq!(state.status(), &UpdateStatus::Errored);
}

/// Ambiente mínimo: só a variável de explicação, em Linux.
struct EnvWithExplanation(Option<String>);

impl PolicyEnv for EnvWithExplanation {
    fn var(&self, name: &str) -> Option<String> {
        (name == ENV_UPDATE_EXPLANATION)
            .then(|| self.0.clone())
            .flatten()
    }
    fn read_file(&self, path: &Path) -> Option<String> {
        assert_eq!(path, Path::new(UPDATE_EXPLANATION_FILE));
        None
    }
    fn target_os(&self) -> Option<TargetOs> {
        Some(TargetOs::Linux)
    }
    fn app_dir(&self) -> Option<PathBuf> {
        None
    }
    fn canonicalize(&self, _path: &Path) -> Option<PathBuf> {
        None
    }
    fn is_writable(&self, _dir: &Path) -> bool {
        true
    }
}

#[test]
fn razao_vinda_do_ambiente_bloqueia_a_rodada_de_ponta_a_ponta() {
    let transport = fake_with_manifest();
    let mut state = UpdateState::new();

    let env = EnvWithExplanation(Some("gerenciado pelo pacote".into()));
    let reason = disabled_reason(&env);
    assert!(reason.is_some());
    assert!(!run_round(
        Trigger::Automatic,
        true,
        reason.as_ref(),
        &mut state,
        &transport
    ));
    assert!(!run_round(
        Trigger::Manual,
        true,
        reason.as_ref(),
        &mut state,
        &transport
    ));
    assert_eq!(transport.requests(), Vec::<String>::new());

    let free = disabled_reason(&EnvWithExplanation(None));
    assert!(run_round(
        Trigger::Automatic,
        true,
        free.as_ref(),
        &mut state,
        &transport
    ));
    assert_eq!(transport.requests().len(), 1);
}
