//! Uma rodada completa de atualização, da política ao arquivo verificado.
//!
//! Ordem: política (`auto_update` + razão de desativação) → plataforma → trava → limpeza de
//! órfãos → manifesto (`ETag`) → `.sig` → assinatura → interpretação → regra de versão →
//! artefato da plataforma → download → SHA-256 (ao baixar e recalculado). A **instalação não
//! entra aqui** (tickets 05–07): a rodada termina entregando [`VerifiedUpdate`] com a máquina de
//! estados em `Installing(versão)`; quem instala chama `installed()` ou `fail()`.
//!
//! Bloqueante: o chamador roda isto em thread dedicada.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use semver::Version;
use thiserror::Error;
use updater::constants::{MANIFEST_SIGNATURE_URL, MANIFEST_URL};
use updater::lock::UpdateLock;
use updater::manifest::{Manifest, ManifestError};
use updater::platform::PlatformKey;
use updater::policy::{
    DisabledReason, PolicyEnv, disabled_reason, may_check_automatically, may_check_manually,
};
use updater::schedule::sweep_orphans;
use updater::state::{TransitionError, Trigger, UpdateState};
use updater::transport::{Fetched, Transport, TransportError};
use updater::verify::VerifyError;
use updater::version::{VersionParseError, parse_version, should_update};

use crate::constants::{MAX_MANIFEST_BYTES, MAX_SIGNATURE_BYTES};
use crate::etag::{EtagRecord, EtagStore};
use crate::fetch::{FetchError, FreeSpace, download_verified, recheck};
use crate::keys::KeySet;
use crate::private_dir::ensure_private_dir;

/// Serviços injetados (rede, ambiente, estado, `ETag`, disco e chaves).
pub struct Services<'a> {
    /// Cliente de rede (`UreqTransport` em produção, fake nos testes).
    pub transport: &'a dyn Transport,
    /// Ambiente consultado pela política de desativação.
    pub env: &'a dyn PolicyEnv,
    /// Máquina de estados compartilhada com a UI (travada só durante cada transição).
    pub state: &'a Mutex<UpdateState>,
    /// Cache do `ETag` do manifesto.
    pub etags: &'a dyn EtagStore,
    /// Espaço livre em disco.
    pub free_space: &'a dyn FreeSpace,
    /// Chaves públicas aceitas.
    pub keys: &'a KeySet,
}

/// Parâmetros de uma rodada.
#[derive(Debug, Clone, Copy)]
pub struct RoundParams<'a> {
    /// Cache do usuário (`<cache>/db-modeling/updates/` fica dentro dele).
    pub cache_dir: &'a Path,
    /// Versão em execução.
    pub current_version: &'a Version,
    /// Plataforma desta compilação (`PlatformKey::current()`); `None` = sem artefato publicado.
    pub platform: Option<PlatformKey>,
    /// Configuração `auto_update` do usuário.
    pub auto_update: bool,
    /// Quem disparou a rodada.
    pub trigger: Trigger,
    /// Instante atual (para a limpeza de órfãos).
    pub now: SystemTime,
}

/// Por que a rodada nem começou (nenhuma requisição, estado intacto).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// `auto_update` desligado e a rodada é automática.
    AutoUpdateOff,
    /// Há uma razão de desativação (explicação do empacotador, `ProgramFiles`, ...).
    Disabled(DisabledReason),
    /// Esta combinação de SO/arquitetura não tem artefato publicado.
    UnsupportedPlatform,
    /// Outra instância detém a trava de atualização.
    LockBusy,
    /// Já há uma checagem, download ou instalação em andamento neste processo.
    Busy,
}

/// Falhas de uma rodada (a máquina de estados já recebeu `fail()`).
///
/// A assinatura divergente do manifesto (`latest.json` e `.sig` são dois GETs; uma release
/// publicada entre eles gera a divergência) é tratada como falha **transitória**: silenciosa na
/// rodada automática e `Errored` na manual, como qualquer outra falha; o `ETag` não é gravado e a
/// próxima rodada tenta de novo.
#[derive(Debug, Error)]
pub enum RoundError {
    /// Não foi possível obter a trava ou preparar o diretório de updates.
    #[error("trava ou diretório de updates: {0}")]
    Lock(#[source] io::Error),
    /// Falha de rede ao buscar manifesto ou assinatura.
    #[error("rede: {0}")]
    Transport(#[from] TransportError),
    /// O servidor respondeu `304` sem que houvesse `ETag` enviado.
    #[error("resposta 304 inesperada")]
    UnexpectedNotModified,
    /// A `.sig` não é UTF-8.
    #[error("assinatura não é texto UTF-8")]
    SignatureNotUtf8,
    /// A assinatura não valida o manifesto recebido.
    #[error("assinatura do manifesto recusada: {0}")]
    Signature(#[from] VerifyError),
    /// Manifesto malformado, sem a plataforma, com URL ou hash inválidos.
    #[error("manifesto: {0}")]
    Manifest(#[from] ManifestError),
    /// A versão do manifesto não é semver.
    #[error("versão do manifesto: {0}")]
    Version(#[from] VersionParseError),
    /// Falha ao baixar ou verificar o artefato.
    #[error("artefato: {0}")]
    Fetch(#[from] FetchError),
    /// Transição inválida da máquina de estados (bug do chamador).
    #[error("estado: {0}")]
    State(#[from] TransitionError),
}

/// Atualização baixada e com SHA-256 conferido, pronta para o instalador da plataforma.
///
/// Mantém a trava de atualização enquanto existir: o instalador deve segurá-la até terminar
/// (descartar libera). **Antes de montar, extrair ou executar**, chame [`VerifiedUpdate::recheck`].
#[derive(Debug)]
pub struct VerifiedUpdate {
    version: Version,
    path: PathBuf,
    sha256: String,
    notes_url: String,
    team_id: Option<String>,
    platform: PlatformKey,
    lock: UpdateLock,
}

impl VerifiedUpdate {
    /// Versão a instalar (já maior que a em execução).
    #[must_use]
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// Caminho do artefato verificado, dentro do diretório privado de updates.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// SHA-256 esperado (hexadecimal minúsculo), igual ao do manifesto assinado.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// Link das notas da versão (validado: pertence ao repositório fixado).
    #[must_use]
    pub fn notes_url(&self) -> &str {
        &self.notes_url
    }

    /// Team ID da Apple declarado no manifesto (só existe se o build do macOS foi assinado).
    #[must_use]
    pub fn team_id(&self) -> Option<&str> {
        self.team_id.as_deref()
    }

    /// Plataforma do artefato.
    #[must_use]
    pub fn platform(&self) -> PlatformKey {
        self.platform
    }

    /// Diretório de updates protegido pela trava (única pasta que a limpeza de órfãos toca).
    #[must_use]
    pub fn updates_dir(&self) -> &Path {
        self.lock.updates_dir()
    }

    /// Recalcula o SHA-256 do arquivo agora e o compara ao do manifesto assinado.
    ///
    /// # Errors
    /// [`FetchError::RecheckMismatch`] se o arquivo mudou; [`FetchError::Io`] se não puder ler.
    pub fn recheck(&self) -> Result<(), FetchError> {
        recheck(&self.path, &self.sha256)
    }
}

/// Resultado de uma rodada.
#[derive(Debug)]
pub enum RoundOutcome {
    /// Nada foi feito (política, plataforma, trava ou rodada em andamento); estado intacto.
    Skipped(SkipReason),
    /// Sem versão nova (estado: repouso, ou `UpToDate` se manual).
    UpToDate,
    /// Atualização baixada e verificada (estado: `Installing(versão)`).
    Ready(VerifiedUpdate),
    /// Falhou (estado: repouso em silêncio se automática, `Errored` se manual).
    Failed(RoundError),
}

fn lock_state(state: &Mutex<UpdateState>) -> MutexGuard<'_, UpdateState> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Aplica a política: `Some(motivo)` se a rodada não deve começar.
fn skip_reason(services: &Services<'_>, params: &RoundParams<'_>) -> Option<SkipReason> {
    let reason = disabled_reason(services.env);
    let allowed = match params.trigger {
        Trigger::Automatic => may_check_automatically(params.auto_update, reason.as_ref()),
        Trigger::Manual => may_check_manually(reason.as_ref()),
    };
    if allowed {
        return None;
    }
    Some(reason.map_or(SkipReason::AutoUpdateOff, SkipReason::Disabled))
}

/// Executa uma rodada de atualização.
///
/// Não faz nenhuma requisição se a política proíbe, se a plataforma não tem artefato, se outra
/// instância detém a trava ou se já há uma rodada em andamento.
#[must_use]
pub fn run_round(services: &Services<'_>, params: &RoundParams<'_>) -> RoundOutcome {
    if let Some(reason) = skip_reason(services, params) {
        return RoundOutcome::Skipped(reason);
    }
    let Some(platform) = params.platform else {
        return RoundOutcome::Skipped(SkipReason::UnsupportedPlatform);
    };
    // A trava vem ANTES de qualquer limpeza ou escrita no diretório de updates.
    let lock = match UpdateLock::try_acquire(params.cache_dir) {
        Ok(Some(lock)) => Ok(lock),
        Ok(None) => return RoundOutcome::Skipped(SkipReason::LockBusy),
        Err(error) => Err(error),
    };
    if lock_state(services.state)
        .begin_check(params.trigger)
        .is_err()
    {
        return RoundOutcome::Skipped(SkipReason::Busy);
    }
    let result = match lock {
        Ok(lock) => attempt(services, params, platform, lock),
        Err(error) => Err(RoundError::Lock(error)),
    };
    let mut state = lock_state(services.state);
    match result {
        Ok(Attempt::UpToDate) => match state.no_update() {
            Ok(()) => RoundOutcome::UpToDate,
            Err(error) => RoundOutcome::Failed(RoundError::State(error)),
        },
        Ok(Attempt::Ready(update)) => match state.download_verified() {
            Ok(()) => RoundOutcome::Ready(update),
            Err(error) => {
                let _ = state.fail();
                RoundOutcome::Failed(RoundError::State(error))
            }
        },
        Err(error) => {
            let _ = state.fail();
            RoundOutcome::Failed(error)
        }
    }
}

enum Attempt {
    UpToDate,
    Ready(VerifiedUpdate),
}

/// Corpo da rodada: tudo que pode falhar, sem mexer no resultado final da máquina de estados.
fn attempt(
    services: &Services<'_>,
    params: &RoundParams<'_>,
    platform: PlatformKey,
    lock: UpdateLock,
) -> Result<Attempt, RoundError> {
    // Limpeza de órfãos só com a trava em mãos; falha de limpeza não impede a rodada.
    let _ = sweep_orphans(&lock, params.now);
    ensure_private_dir(lock.updates_dir()).map_err(RoundError::Lock)?;

    let pending = lock_state(services.state).pending().cloned();
    let sent_etag = services
        .etags
        .load()
        .filter(|record| record.applies_to(params.current_version, pending.as_ref()))
        .map(|record| record.etag);

    let (manifest_bytes, etag) =
        match services
            .transport
            .get(MANIFEST_URL, sent_etag.as_deref(), MAX_MANIFEST_BYTES)?
        {
            Fetched::NotModified if sent_etag.is_some() => return Ok(Attempt::UpToDate),
            Fetched::NotModified => return Err(RoundError::UnexpectedNotModified),
            Fetched::Body { bytes, etag } => (bytes, etag),
        };

    // Segundo GET: se uma release foi publicada entre os dois, a assinatura não bate (falha
    // transitória; ver `RoundError`).
    let Fetched::Body { bytes: sig, .. } =
        services
            .transport
            .get(MANIFEST_SIGNATURE_URL, None, MAX_SIGNATURE_BYTES)?
    else {
        return Err(RoundError::UnexpectedNotModified);
    };
    let signature = String::from_utf8(sig).map_err(|_| RoundError::SignatureNotUtf8)?;
    services.keys.verify(&manifest_bytes, &signature)?;

    let remember = |services: &Services<'_>| {
        if let Some(etag) = etag.clone() {
            services.etags.save(EtagRecord {
                etag,
                current: params.current_version.clone(),
                pending: pending.clone(),
            });
        }
    };

    let Some(manifest) = Manifest::parse(&manifest_bytes)? else {
        // Esquema desconhecido: ignorado como "sem atualização".
        remember(services);
        return Ok(Attempt::UpToDate);
    };
    let remote = parse_version(&manifest.version)?;
    if should_update(params.current_version, &remote, pending.as_ref()).is_err() {
        remember(services);
        return Ok(Attempt::UpToDate);
    }

    let asset = manifest.asset_for(platform)?;
    lock_state(services.state).found_update(remote.clone())?;
    // Há versão nova: nenhum `ETag` antigo pode transformar uma falha futura em "sem novidade".
    services.etags.clear();

    let downloaded = download_verified(
        services.transport,
        services.free_space,
        lock.updates_dir(),
        &remote,
        asset,
    )?;
    Ok(Attempt::Ready(VerifiedUpdate {
        version: remote,
        path: downloaded.path,
        sha256: downloaded.sha256,
        notes_url: manifest.notes_url.clone(),
        team_id: asset.team_id.clone(),
        platform,
        lock,
    }))
}
