//! Abstração de rede. O ticket 03 implementa [`Transport`] com `ureq`; o núcleo e os testes só
//! conhecem a trait.

use std::path::Path;

use thiserror::Error;

/// Falhas de transporte.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum TransportError {
    /// Sem conexão, DNS, TLS ou timeout.
    #[error("falha de rede: {0}")]
    Network(String),
    /// Resposta HTTP com status de erro.
    #[error("HTTP {0}")]
    Status(u16),
    /// Corpo maior que o limite permitido.
    #[error("resposta excede o limite de {0} bytes")]
    TooLarge(u64),
    /// Erro de disco ao gravar o download.
    #[error("erro de E/S: {0}")]
    Io(String),
}

/// Resposta a um GET condicional.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    /// `304 Not Modified`: o `ETag` enviado ainda vale.
    NotModified,
    /// Corpo e `ETag` (se houver) da resposta.
    Body {
        /// Bytes da resposta.
        bytes: Vec<u8>,
        /// Valor do cabeçalho `ETag`.
        etag: Option<String>,
    },
}

/// Cliente de rede injetável. Implementações são bloqueantes e rodam em thread própria.
pub trait Transport: Send + Sync {
    /// GET de um recurso pequeno (manifesto, assinatura), com `If-None-Match` opcional.
    ///
    /// # Errors
    /// [`TransportError`] em falha de rede, status de erro ou corpo acima de `max_bytes`.
    fn get(
        &self,
        url: &str,
        if_none_match: Option<&str>,
        max_bytes: u64,
    ) -> Result<Fetched, TransportError>;

    /// Baixa `url` para `dest`, recusando corpo acima de `max_bytes`.
    ///
    /// # Errors
    /// [`TransportError`] em falha de rede, status de erro, limite excedido ou de E/S.
    fn download(&self, url: &str, dest: &Path, max_bytes: u64) -> Result<(), TransportError>;
}

/// Fake em memória para testes (sem rede), com registro das requisições recebidas.
#[cfg(any(test, feature = "testing"))]
pub mod fake {
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::Mutex;

    use super::{Fetched, Transport, TransportError};

    /// Transporte falso: respostas pré-programadas por URL.
    #[derive(Debug, Default)]
    pub struct FakeTransport {
        responses: Mutex<HashMap<String, Result<Vec<u8>, TransportError>>>,
        etag: Option<String>,
        requests: Mutex<Vec<String>>,
    }

    impl FakeTransport {
        /// Fake vazio: toda URL sem resposta falha com `Network`.
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// Programa uma resposta de sucesso para `url`.
        #[must_use]
        pub fn with_body(self, url: &str, body: &[u8]) -> Self {
            self.lock_responses()
                .insert(url.to_owned(), Ok(body.to_vec()));
            self
        }

        /// Programa uma falha para `url`.
        #[must_use]
        pub fn with_error(self, url: &str, error: TransportError) -> Self {
            self.lock_responses().insert(url.to_owned(), Err(error));
            self
        }

        /// `ETag` devolvido pelo fake; um `If-None-Match` igual gera `NotModified`.
        #[must_use]
        pub fn with_etag(mut self, etag: &str) -> Self {
            self.etag = Some(etag.to_owned());
            self
        }

        /// URLs requisitadas até agora, em ordem.
        #[must_use]
        pub fn requests(&self) -> Vec<String> {
            self.requests
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default()
        }

        fn lock_responses(
            &self,
        ) -> std::sync::MutexGuard<'_, HashMap<String, Result<Vec<u8>, TransportError>>> {
            self.responses
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        }

        fn record(&self, url: &str) {
            self.requests
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(url.to_owned());
        }

        fn response(&self, url: &str) -> Result<Vec<u8>, TransportError> {
            self.record(url);
            self.lock_responses()
                .get(url)
                .cloned()
                .unwrap_or_else(|| Err(TransportError::Network(format!("sem resposta para {url}"))))
        }
    }

    impl Transport for FakeTransport {
        fn get(
            &self,
            url: &str,
            if_none_match: Option<&str>,
            max_bytes: u64,
        ) -> Result<Fetched, TransportError> {
            let body = self.response(url)?;
            if self.etag.is_some() && if_none_match == self.etag.as_deref() {
                return Ok(Fetched::NotModified);
            }
            if body.len() as u64 > max_bytes {
                return Err(TransportError::TooLarge(max_bytes));
            }
            Ok(Fetched::Body {
                bytes: body,
                etag: self.etag.clone(),
            })
        }

        fn download(&self, url: &str, dest: &Path, max_bytes: u64) -> Result<(), TransportError> {
            let body = self.response(url)?;
            if body.len() as u64 > max_bytes {
                return Err(TransportError::TooLarge(max_bytes));
            }
            std::fs::write(dest, body).map_err(|e| TransportError::Io(e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeTransport;
    use super::*;

    const URL: &str = "https://exemplo.test/latest.json";

    #[test]
    fn fake_devolve_corpo_e_registra_requisicao() {
        let t = FakeTransport::new()
            .with_body(URL, b"{}")
            .with_etag("\"v1\"");
        let got = t.get(URL, None, 1024).expect("get");
        assert_eq!(
            got,
            Fetched::Body {
                bytes: b"{}".to_vec(),
                etag: Some("\"v1\"".into())
            }
        );
        assert_eq!(t.requests(), vec![URL.to_owned()]);
    }

    #[test]
    fn fake_responde_not_modified_para_etag_igual() {
        let t = FakeTransport::new()
            .with_body(URL, b"{}")
            .with_etag("\"v1\"");
        assert_eq!(t.get(URL, Some("\"v1\""), 1024), Ok(Fetched::NotModified));
    }

    #[test]
    fn fake_respeita_limite_de_tamanho() {
        let t = FakeTransport::new().with_body(URL, &[0u8; 10]);
        assert_eq!(t.get(URL, None, 5), Err(TransportError::TooLarge(5)));
    }

    #[test]
    fn fake_sem_resposta_falha_como_rede() {
        let t = FakeTransport::new();
        assert!(matches!(
            t.get(URL, None, 10),
            Err(TransportError::Network(_))
        ));
    }

    #[test]
    fn fake_propaga_erro_programado_e_baixa_para_arquivo() {
        let t = FakeTransport::new()
            .with_error("https://x.test/a", TransportError::Status(503))
            .with_body("https://x.test/b", b"dados");
        assert_eq!(
            t.get("https://x.test/a", None, 10),
            Err(TransportError::Status(503))
        );

        let dir = tempfile::tempdir().expect("tempdir");
        let dest = dir.path().join("b.bin");
        t.download("https://x.test/b", &dest, 100)
            .expect("download");
        assert_eq!(std::fs::read(&dest).expect("ler"), b"dados");
    }
}
