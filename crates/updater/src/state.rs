//! Máquina de estados do updater, no modelo do `AutoUpdateStatus` do Zed.
//!
//! `Idle → Checking → Downloading → Installing → Updated(versão)` e `Errored`. Não existe o
//! estado "baixado mas não instalado": depois de baixar e verificar, instala na hora, e
//! `Updated` significa "reinicie para usar a versão nova".
//!
//! Decisões onde o spec é omisso:
//! - `UpToDate` é o resultado transitório de uma checagem **manual** sem versão nova ("Atualizado",
//!   some em [`crate::constants::UP_TO_DATE_VISIBLE_FOR`]); na checagem automática o resultado
//!   é silencioso (volta a repouso).
//! - "Repouso" é `Idle`, ou `Updated(pendente)` se este app já instalou uma versão que aguarda
//!   reinício; assim a ação "Reiniciar para atualizar" nunca some por causa de uma falha posterior.

use semver::Version;
use thiserror::Error;

use crate::constants::{
    ACTION_BEGIN_CHECK, ACTION_DISMISS, ACTION_DOWNLOAD_VERIFIED, ACTION_FAIL, ACTION_FOUND_UPDATE,
    ACTION_INSTALLED, ACTION_NO_UPDATE, ACTION_RESTORE_PENDING,
};

/// Quem disparou a checagem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// Polling (abertura e a cada 1 h): falhas são silenciosas.
    Automatic,
    /// Comando do usuário: falhas viram [`UpdateStatus::Errored`].
    Manual,
}

/// Estado visível do updater.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    /// Nada em andamento.
    Idle,
    /// Consultando o manifesto.
    Checking,
    /// Baixando (e verificando) a versão indicada.
    Downloading(Version),
    /// Instalando os arquivos da versão indicada.
    Installing(Version),
    /// Instalada; falta reiniciar ("Reiniciar para atualizar").
    Updated(Version),
    /// Checagem manual concluída sem versão nova ("Atualizado").
    UpToDate,
    /// Falha em operação manual ("Falha ao atualizar").
    Errored,
}

/// Transição inválida para o estado atual.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("transição `{action}` inválida a partir de {from:?}")]
pub struct TransitionError {
    /// Estado em que a transição foi tentada.
    pub from: UpdateStatus,
    /// Nome da transição tentada.
    pub action: &'static str,
}

/// Máquina de estados. Não faz E/S: quem a dirige chama as transições conforme os eventos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateState {
    status: UpdateStatus,
    trigger: Trigger,
    pending: Option<Version>,
}

impl Default for UpdateState {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdateState {
    /// Começa em `Idle`, sem instalação pendente.
    #[must_use]
    pub fn new() -> Self {
        Self {
            status: UpdateStatus::Idle,
            trigger: Trigger::Automatic,
            pending: None,
        }
    }

    /// Estado atual.
    #[must_use]
    pub fn status(&self) -> &UpdateStatus {
        &self.status
    }

    /// Quem disparou a checagem atual (ou a última). Permanece após `resume_after_suspend`:
    /// uma checagem **manual** interrompida por suspensão volta ao repouso (sem `Errored`) e é
    /// o chamador quem decide, lendo isto, se refaz a rodada.
    #[must_use]
    pub fn trigger(&self) -> Trigger {
        self.trigger
    }

    /// Versão já instalada que aguarda reinício, se houver.
    #[must_use]
    pub fn pending(&self) -> Option<&Version> {
        self.pending.as_ref()
    }

    /// Há operação em andamento (checando, baixando ou instalando)?
    #[must_use]
    pub fn is_busy(&self) -> bool {
        matches!(
            self.status,
            UpdateStatus::Checking | UpdateStatus::Downloading(_) | UpdateStatus::Installing(_)
        )
    }

    fn rest(&self) -> UpdateStatus {
        self.pending
            .clone()
            .map_or(UpdateStatus::Idle, UpdateStatus::Updated)
    }

    fn invalid(&self, action: &'static str) -> TransitionError {
        TransitionError {
            from: self.status.clone(),
            action,
        }
    }

    /// Restaura uma instalação pendente encontrada na abertura (Windows: `versions.txt` e
    /// `install\` presentes). Só vale em repouso.
    ///
    /// # Errors
    /// [`TransitionError`] se houver operação em andamento.
    pub fn restore_pending(&mut self, version: Version) -> Result<(), TransitionError> {
        if self.is_busy() {
            return Err(self.invalid(ACTION_RESTORE_PENDING));
        }
        self.pending = Some(version.clone());
        self.status = UpdateStatus::Updated(version);
        Ok(())
    }

    /// `Idle`/`Errored`/`UpToDate`/`Updated` → `Checking`.
    ///
    /// # Errors
    /// [`TransitionError`] se já houver operação em andamento.
    pub fn begin_check(&mut self, trigger: Trigger) -> Result<(), TransitionError> {
        if self.is_busy() {
            return Err(self.invalid(ACTION_BEGIN_CHECK));
        }
        self.trigger = trigger;
        self.status = UpdateStatus::Checking;
        Ok(())
    }

    /// `Checking` → `Downloading(versão)`: o manifesto tem versão válida e maior.
    ///
    /// # Errors
    /// [`TransitionError`] fora de `Checking`.
    pub fn found_update(&mut self, version: Version) -> Result<(), TransitionError> {
        if self.status != UpdateStatus::Checking {
            return Err(self.invalid(ACTION_FOUND_UPDATE));
        }
        self.status = UpdateStatus::Downloading(version);
        Ok(())
    }

    /// `Checking` → repouso (automática, em silêncio) ou `UpToDate` (manual).
    ///
    /// # Errors
    /// [`TransitionError`] fora de `Checking`.
    pub fn no_update(&mut self) -> Result<(), TransitionError> {
        if self.status != UpdateStatus::Checking {
            return Err(self.invalid(ACTION_NO_UPDATE));
        }
        self.status = match self.trigger {
            Trigger::Automatic => self.rest(),
            Trigger::Manual => UpdateStatus::UpToDate,
        };
        Ok(())
    }

    /// `Downloading(v)` → `Installing(v)`: baixado e verificado, instala na hora.
    ///
    /// # Errors
    /// [`TransitionError`] fora de `Downloading`.
    pub fn download_verified(&mut self) -> Result<(), TransitionError> {
        match &self.status {
            UpdateStatus::Downloading(version) => {
                self.status = UpdateStatus::Installing(version.clone());
                Ok(())
            }
            _ => Err(self.invalid(ACTION_DOWNLOAD_VERIFIED)),
        }
    }

    /// `Installing(v)` → `Updated(v)`; `v` passa a ser a versão pendente de reinício.
    ///
    /// # Errors
    /// [`TransitionError`] fora de `Installing`.
    pub fn installed(&mut self) -> Result<(), TransitionError> {
        match &self.status {
            UpdateStatus::Installing(version) => {
                let version = version.clone();
                self.pending = Some(version.clone());
                self.status = UpdateStatus::Updated(version);
                Ok(())
            }
            _ => Err(self.invalid(ACTION_INSTALLED)),
        }
    }

    /// Falha em `Checking`/`Downloading`/`Installing`: automática volta ao repouso em silêncio;
    /// manual vira `Errored`.
    ///
    /// Falha em `Installing` **invalida** a pendência antiga: os arquivos no disco já são uma
    /// mistura de versões e reiniciar seria inseguro, então a UI não pode oferecer "Reiniciar".
    /// Falha em `Checking`/`Downloading` não toca em disco e mantém a pendência.
    ///
    /// # Errors
    /// [`TransitionError`] se não houver operação em andamento.
    pub fn fail(&mut self) -> Result<(), TransitionError> {
        if !self.is_busy() {
            return Err(self.invalid(ACTION_FAIL));
        }
        if matches!(self.status, UpdateStatus::Installing(_)) {
            self.pending = None;
        }
        self.status = match self.trigger {
            Trigger::Automatic => self.rest(),
            Trigger::Manual => UpdateStatus::Errored,
        };
        Ok(())
    }

    /// Fim do tempo de exibição de `UpToDate` ou dispensa de `Errored` → repouso.
    ///
    /// # Errors
    /// [`TransitionError`] em outros estados.
    pub fn dismiss(&mut self) -> Result<(), TransitionError> {
        match self.status {
            UpdateStatus::UpToDate | UpdateStatus::Errored => {
                self.status = self.rest();
                Ok(())
            }
            _ => Err(self.invalid(ACTION_DISMISS)),
        }
    }

    /// Após suspensão do sistema só as fases de **rede** reiniciam: `Checking` e `Downloading`
    /// voltam ao repouso (retorna `true` para o chamador refazer a rodada); `Installing` segue.
    pub fn resume_after_suspend(&mut self) -> bool {
        if matches!(
            self.status,
            UpdateStatus::Checking | UpdateStatus::Downloading(_)
        ) {
            self.status = self.rest();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(major: u64, minor: u64) -> Version {
        Version::new(major, minor, 0)
    }

    fn checking(trigger: Trigger) -> UpdateState {
        let mut s = UpdateState::new();
        s.begin_check(trigger).expect("begin");
        s
    }

    fn installing(trigger: Trigger) -> UpdateState {
        let mut s = checking(trigger);
        s.found_update(v(0, 4)).expect("found");
        s.download_verified().expect("download");
        s
    }

    #[test]
    fn caminho_feliz_idle_ate_updated() {
        let mut s = UpdateState::new();
        assert_eq!(s.status(), &UpdateStatus::Idle);
        s.begin_check(Trigger::Automatic).expect("begin");
        assert_eq!(s.status(), &UpdateStatus::Checking);
        s.found_update(v(0, 4)).expect("found");
        assert_eq!(s.status(), &UpdateStatus::Downloading(v(0, 4)));
        s.download_verified().expect("download");
        assert_eq!(s.status(), &UpdateStatus::Installing(v(0, 4)));
        s.installed().expect("installed");
        assert_eq!(s.status(), &UpdateStatus::Updated(v(0, 4)));
        assert_eq!(s.pending(), Some(&v(0, 4)));
    }

    #[test]
    fn nao_existe_estado_baixado_mas_nao_instalado() {
        // Do `Downloading` só se vai a `Installing`; não há passo intermediário "Downloaded".
        let mut s = checking(Trigger::Automatic);
        s.found_update(v(0, 4)).expect("found");
        s.download_verified().expect("download");
        assert!(matches!(s.status(), UpdateStatus::Installing(_)));
    }

    #[test]
    fn automatica_sem_versao_nova_volta_a_idle_em_silencio() {
        let mut s = checking(Trigger::Automatic);
        s.no_update().expect("no_update");
        assert_eq!(s.status(), &UpdateStatus::Idle);
    }

    #[test]
    fn manual_sem_versao_nova_mostra_atualizado_e_depois_idle() {
        let mut s = checking(Trigger::Manual);
        s.no_update().expect("no_update");
        assert_eq!(s.status(), &UpdateStatus::UpToDate);
        s.dismiss().expect("dismiss");
        assert_eq!(s.status(), &UpdateStatus::Idle);
    }

    #[test]
    fn falha_automatica_em_qualquer_fase_volta_a_idle_em_silencio() {
        let mut s = checking(Trigger::Automatic);
        s.fail().expect("checking");
        assert_eq!(s.status(), &UpdateStatus::Idle);

        let mut s = checking(Trigger::Automatic);
        s.found_update(v(0, 4)).expect("found");
        s.fail().expect("downloading");
        assert_eq!(s.status(), &UpdateStatus::Idle);

        let mut s = installing(Trigger::Automatic);
        s.fail().expect("installing");
        assert_eq!(s.status(), &UpdateStatus::Idle);
    }

    #[test]
    fn falha_manual_em_qualquer_fase_vira_errored() {
        let mut s = checking(Trigger::Manual);
        s.fail().expect("checking");
        assert_eq!(s.status(), &UpdateStatus::Errored);

        let mut s = checking(Trigger::Manual);
        s.found_update(v(0, 4)).expect("found");
        s.fail().expect("downloading");
        assert_eq!(s.status(), &UpdateStatus::Errored);

        let mut s = installing(Trigger::Manual);
        s.fail().expect("installing");
        assert_eq!(s.status(), &UpdateStatus::Errored);
    }

    #[test]
    fn errored_pode_ser_dispensado_ou_substituido_por_nova_checagem() {
        let mut s = checking(Trigger::Manual);
        s.fail().expect("fail");
        s.dismiss().expect("dismiss");
        assert_eq!(s.status(), &UpdateStatus::Idle);

        let mut s = checking(Trigger::Manual);
        s.fail().expect("fail");
        s.begin_check(Trigger::Manual).expect("nova checagem");
        assert_eq!(s.status(), &UpdateStatus::Checking);
    }

    #[test]
    fn nao_inicia_checagem_em_operacao_em_andamento() {
        let mut s = checking(Trigger::Automatic);
        assert!(s.begin_check(Trigger::Manual).is_err());
        s.found_update(v(0, 4)).expect("found");
        assert!(s.begin_check(Trigger::Manual).is_err());
        s.download_verified().expect("download");
        assert!(s.begin_check(Trigger::Manual).is_err());
    }

    #[test]
    fn transicoes_fora_de_ordem_sao_recusadas() {
        let mut s = UpdateState::new();
        assert!(s.found_update(v(0, 4)).is_err());
        assert!(s.no_update().is_err());
        assert!(s.download_verified().is_err());
        assert!(s.installed().is_err());
        assert!(s.fail().is_err());
        assert!(s.dismiss().is_err());
        assert_eq!(
            s.status(),
            &UpdateStatus::Idle,
            "estado intacto após recusas"
        );

        let mut s = checking(Trigger::Automatic);
        assert!(s.download_verified().is_err());
        assert!(s.installed().is_err());
        assert!(s.dismiss().is_err());
    }

    #[test]
    fn checagem_continua_apos_updated_e_falha_preserva_o_reinicio() {
        let mut s = installing(Trigger::Automatic);
        s.installed().expect("installed");
        s.begin_check(Trigger::Automatic).expect("nova rodada");
        s.fail().expect("falha automática");
        assert_eq!(
            s.status(),
            &UpdateStatus::Updated(v(0, 4)),
            "o botão de reiniciar não some"
        );

        s.begin_check(Trigger::Manual).expect("manual");
        s.no_update().expect("sem novidade");
        assert_eq!(s.status(), &UpdateStatus::UpToDate);
        s.dismiss().expect("dismiss");
        assert_eq!(s.status(), &UpdateStatus::Updated(v(0, 4)));
    }

    /// Estado com `0.4.0` instalada e aguardando reinício, já com nova rodada em `Checking`.
    fn with_pending(trigger: Trigger) -> UpdateState {
        let mut s = installing(Trigger::Automatic);
        s.installed().expect("installed");
        s.begin_check(trigger).expect("nova rodada");
        s
    }

    #[test]
    fn falha_ao_baixar_ou_checar_mantem_a_pendencia_antiga() {
        let mut s = with_pending(Trigger::Manual);
        s.fail().expect("checking");
        assert_eq!(s.status(), &UpdateStatus::Errored);
        assert_eq!(s.pending(), Some(&v(0, 4)));

        let mut s = with_pending(Trigger::Automatic);
        s.found_update(v(0, 5)).expect("found");
        s.fail().expect("downloading");
        assert_eq!(s.status(), &UpdateStatus::Updated(v(0, 4)));
        assert_eq!(s.pending(), Some(&v(0, 4)));
    }

    #[test]
    fn falha_ao_instalar_versao_nova_invalida_a_pendencia_antiga() {
        // Os arquivos viraram uma mistura 0.4.0/0.5.0: reiniciar seria inseguro.
        let mut s = with_pending(Trigger::Automatic);
        s.found_update(v(0, 5)).expect("found");
        s.download_verified().expect("download");
        s.fail().expect("installing");
        assert_eq!(s.status(), &UpdateStatus::Idle, "sem botão de reiniciar");
        assert_eq!(s.pending(), None);

        let mut s = with_pending(Trigger::Manual);
        s.found_update(v(0, 5)).expect("found");
        s.download_verified().expect("download");
        s.fail().expect("installing");
        assert_eq!(s.status(), &UpdateStatus::Errored);
        assert_eq!(s.pending(), None);
        s.dismiss().expect("dismiss");
        assert_eq!(
            s.status(),
            &UpdateStatus::Idle,
            "dispensar não ressuscita a pendência"
        );
    }

    #[test]
    fn instalacao_pendente_na_abertura_vai_direto_a_updated() {
        let mut s = UpdateState::new();
        s.restore_pending(v(0, 4)).expect("restore");
        assert_eq!(s.status(), &UpdateStatus::Updated(v(0, 4)));
        assert_eq!(s.pending(), Some(&v(0, 4)));
    }

    #[test]
    fn restaurar_pendencia_durante_operacao_e_recusado() {
        let mut s = checking(Trigger::Automatic);
        assert!(s.restore_pending(v(0, 4)).is_err());
    }

    #[test]
    fn suspensao_de_checagem_manual_vai_ao_repouso_e_preserva_o_gatilho() {
        let mut s = checking(Trigger::Manual);
        assert!(s.resume_after_suspend());
        assert_eq!(s.status(), &UpdateStatus::Idle, "sem Errored");
        assert_eq!(s.trigger(), Trigger::Manual, "chamador decide se refaz");
    }

    #[test]
    fn suspensao_reinicia_so_as_fases_de_rede() {
        let mut s = checking(Trigger::Automatic);
        assert!(s.resume_after_suspend());
        assert_eq!(s.status(), &UpdateStatus::Idle);

        let mut s = checking(Trigger::Automatic);
        s.found_update(v(0, 4)).expect("found");
        assert!(s.resume_after_suspend());
        assert_eq!(s.status(), &UpdateStatus::Idle);

        let mut s = installing(Trigger::Automatic);
        assert!(!s.resume_after_suspend());
        assert_eq!(s.status(), &UpdateStatus::Installing(v(0, 4)));
    }
}
