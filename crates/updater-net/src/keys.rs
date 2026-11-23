//! Conjunto de chaves públicas aceitas para a assinatura do manifesto.

use updater::verify::{VerifyError, verify_signature};

/// Chaves aceitas: em produção, as duas embutidas em `updater::constants::PUBLIC_KEYS`.
#[derive(Debug, Clone, Default)]
pub enum KeySet {
    /// As duas chaves embutidas (primária e reserva). É a única variante em release.
    #[default]
    Production,
    /// **Gancho de teste:** chaves base64 de teste no lugar das embutidas. Só em debug ou com a
    /// feature `harness`.
    #[cfg(any(debug_assertions, feature = "harness"))]
    Custom(Vec<String>),
}

impl KeySet {
    /// Produção; em debug ou com a feature `harness`, a chave de `DBM_UPDATE_TEST_PUBLIC_KEY`
    /// (se definida) no lugar das embutidas. **É este o ponto de entrada do app**: em release o
    /// gancho não existe.
    #[must_use]
    pub fn from_environment() -> Self {
        #[cfg(any(debug_assertions, feature = "harness"))]
        if let Some(key) = std::env::var(crate::constants::ENV_UPDATE_TEST_PUBLIC_KEY)
            .ok()
            .map(|key| key.trim().to_owned())
            .filter(|key| !key.is_empty())
        {
            return Self::Custom(vec![key]);
        }
        Self::Production
    }

    /// Verifica `data` contra o texto completo (4 linhas) da `.sig`.
    ///
    /// # Errors
    /// [`VerifyError`] se nenhuma chave aceita validar a assinatura.
    pub fn verify(&self, data: &[u8], signature: &str) -> Result<(), VerifyError> {
        match self {
            Self::Production => verify_signature(data, signature),
            #[cfg(any(debug_assertions, feature = "harness"))]
            Self::Custom(keys) => {
                let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
                updater::verify::verify_signature_with_keys(data, signature, &keys)
            }
        }
    }
}
