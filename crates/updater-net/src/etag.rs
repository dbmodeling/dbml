//! Cache do `ETag` do manifesto.
//!
//! O `ETag` só é gravado depois de um resultado **concluído** da rodada ("sem versão nova"), e
//! nunca por causa de uma atualização encontrada: se o download ou a instalação falharem, a
//! próxima rodada recebe `200` (e não `304 "sem novidade"`) e tenta de novo. O registro vale só
//! para a mesma versão em execução e a mesma versão pendente de reinício; se qualquer uma mudar
//! (reinstalação de versão antiga, instalação que falhou), ele é ignorado.

use std::sync::{Mutex, PoisonError};

use semver::Version;

/// `ETag` do manifesto e o contexto em que foi gravado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EtagRecord {
    /// Valor do cabeçalho `ETag`, como recebido.
    pub etag: String,
    /// Versão em execução quando foi gravado.
    pub current: Version,
    /// Versão já instalada aguardando reinício quando foi gravado.
    pub pending: Option<Version>,
}

impl EtagRecord {
    /// O registro ainda vale para este contexto?
    #[must_use]
    pub fn applies_to(&self, current: &Version, pending: Option<&Version>) -> bool {
        self.current == *current && self.pending.as_ref() == pending
    }
}

/// Armazém do `ETag` (a rodada só lê e grava; a persistência é do chamador).
pub trait EtagStore: Send + Sync {
    /// Último registro gravado.
    fn load(&self) -> Option<EtagRecord>;
    /// Grava o registro (substitui o anterior).
    fn save(&self, record: EtagRecord);
    /// Esquece o registro.
    fn clear(&self);
}

/// Armazém em memória: vale pela vida do processo (o app consulta de hora em hora).
#[derive(Debug, Default)]
pub struct MemoryEtagStore {
    record: Mutex<Option<EtagRecord>>,
}

impl MemoryEtagStore {
    /// Armazém vazio.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl EtagStore for MemoryEtagStore {
    fn load(&self) -> Option<EtagRecord> {
        self.record
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn save(&self, record: EtagRecord) {
        *self.record.lock().unwrap_or_else(PoisonError::into_inner) = Some(record);
    }

    fn clear(&self) {
        *self.record.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).expect("semver")
    }

    #[test]
    fn registro_so_vale_para_o_mesmo_contexto() {
        let record = EtagRecord {
            etag: "\"a\"".into(),
            current: v("0.3.4"),
            pending: None,
        };
        assert!(record.applies_to(&v("0.3.4"), None));
        assert!(!record.applies_to(&v("0.3.3"), None));
        assert!(!record.applies_to(&v("0.3.4"), Some(&v("0.4.0"))));
    }

    #[test]
    fn armazem_em_memoria_grava_le_e_limpa() {
        let store = MemoryEtagStore::new();
        assert_eq!(store.load(), None);
        let record = EtagRecord {
            etag: "\"a\"".into(),
            current: v("1.0.0"),
            pending: None,
        };
        store.save(record.clone());
        assert_eq!(store.load(), Some(record));
        store.clear();
        assert_eq!(store.load(), None);
    }
}
