//! [`UreqTransport`]: implementação de [`updater::transport::Transport`] com `ureq` + `rustls`.
//!
//! - Raízes de certificado do **sistema** (`rustls-native-certs`) e `HTTPS_PROXY`/`https_proxy`.
//! - Redirecionamentos seguidos **à mão**, um a um, com a política de [`crate::url_policy`]
//!   (só HTTPS, só hosts do GitHub, sem travessia de caminho, no máximo [`MAX_REDIRECTS`]).
//! - `User-Agent: db-modeling-updater/<versão>` e nenhum outro dado do usuário sai ao servidor.
//! - Download em arquivo `.part` com retomada por `Range`; o chamador só o usa depois de
//!   verificar o SHA-256 (ver [`crate::fetch`]).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::Duration;

use updater::transport::{Fetched, Transport, TransportError};
use ureq::http::Response;
use ureq::tls::{Certificate, RootCerts, TlsConfig};
use ureq::{Agent, Body, Proxy};
use url::Url;

use crate::constants::{
    CONNECT_TIMEOUT, CONTENT_RANGE_UNIT_PREFIX, DOWNLOAD_BUFFER_BYTES, ENV_PROXY_VARS,
    HEADER_CONTENT_LENGTH, HEADER_CONTENT_RANGE, HEADER_ETAG, HEADER_IF_NONE_MATCH,
    HEADER_LOCATION, HEADER_RANGE, MAX_REDIRECTS, RANGE_DASH, RANGE_UNIT_PREFIX, READ_TIMEOUT,
    REDIRECT_STATUSES, RESOLVE_TIMEOUT, SMALL_REQUEST_TIMEOUT, STATUS_NOT_MODIFIED, STATUS_OK,
    STATUS_PARTIAL_CONTENT, UPDATER_VERSION, USER_AGENT_PREFIX, WRITE_TIMEOUT,
};
use crate::url_policy::{UrlRejection, check_url, resolve_redirect};

/// Cabeçalho extra de uma requisição (nome, valor).
type Header<'a> = (&'a str, String);

/// Configuração do transporte.
#[derive(Debug, Clone, Default)]
pub struct TransportConfig {
    proxy: Option<String>,
    #[cfg(any(debug_assertions, feature = "harness"))]
    test_origin: Option<Url>,
}

impl TransportConfig {
    /// Configuração de produção: proxy de `HTTPS_PROXY`/`https_proxy`, se definido.
    #[must_use]
    pub fn production() -> Self {
        Self::default().with_proxy(proxy_from_lookup(|name| std::env::var(name).ok()))
    }

    /// Define (ou remove) o proxy explicitamente (ex.: `http://proxy.local:3128`).
    #[must_use]
    pub fn with_proxy(mut self, proxy: Option<String>) -> Self {
        self.proxy = proxy;
        self
    }

    /// **Gancho de teste.** Envia toda requisição à `origin` (esquema, host e porta), mantendo
    /// caminho e query; a política de URL continua valendo sobre a URL *lógica* do GitHub.
    /// Só existe em debug ou com a feature `harness`.
    #[cfg(any(debug_assertions, feature = "harness"))]
    #[must_use]
    pub fn with_test_origin(mut self, origin: Url) -> Self {
        self.test_origin = Some(origin);
        self
    }

    /// Produção; em debug ou com a feature `harness`, também o gancho `DBM_UPDATE_URL` (origem
    /// de teste), se definido e válido. **É este o ponto de entrada do app**: em release o gancho
    /// não existe (o teste `release_has_no_hook` confere o binário).
    #[must_use]
    pub fn from_environment() -> Self {
        let config = Self::production();
        #[cfg(any(debug_assertions, feature = "harness"))]
        if let Some(origin) = std::env::var(crate::constants::ENV_UPDATE_URL)
            .ok()
            .and_then(|raw| Url::parse(&raw).ok())
        {
            return config.with_test_origin(origin);
        }
        config
    }

    #[cfg(any(debug_assertions, feature = "harness"))]
    fn test_origin(&self) -> Option<&Url> {
        self.test_origin.as_ref()
    }

    #[cfg(not(any(debug_assertions, feature = "harness")))]
    #[allow(clippy::unused_self)]
    fn test_origin(&self) -> Option<&Url> {
        None
    }
}

/// Valor de proxy a partir de `HTTPS_PROXY`/`https_proxy` (a primeira não vazia).
/// `lookup` é injetável para testar sem mexer no ambiente do processo.
#[must_use]
pub fn proxy_from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Option<String> {
    ENV_PROXY_VARS
        .iter()
        .filter_map(|name| lookup(name))
        .map(|value| value.trim().to_owned())
        .find(|value| !value.is_empty())
}

/// Raízes de certificado do sistema, no formato do `ureq`.
///
/// # Errors
/// [`TransportError::Network`] se o sistema não expuser nenhuma raiz (falha fechada: nunca
/// recai em raízes embutidas).
pub fn system_roots() -> Result<RootCerts, TransportError> {
    let loaded = rustls_native_certs::load_native_certs();
    let certs: Vec<Certificate<'static>> = loaded
        .certs
        .iter()
        .map(|der| Certificate::from_der(der.as_ref()).to_owned())
        .collect();
    if certs.is_empty() {
        return Err(TransportError::Network(
            "nenhuma raiz de certificado do sistema disponível".to_owned(),
        ));
    }
    Ok(RootCerts::new_with_certs(&certs))
}

/// Cliente de rede do updater.
#[derive(Debug, Clone)]
pub struct UreqTransport {
    agent: Agent,
    test_origin: Option<Url>,
}

fn network(error: impl std::fmt::Display) -> TransportError {
    TransportError::Network(error.to_string())
}

fn rejected(rejection: &UrlRejection) -> TransportError {
    TransportError::Network(format!("URL recusada pela política: {rejection}"))
}

fn header_text<'a>(response: &'a Response<Body>, name: &str) -> Option<&'a str> {
    response.headers().get(name)?.to_str().ok()
}

fn content_length(response: &Response<Body>) -> Option<u64> {
    header_text(response, HEADER_CONTENT_LENGTH)?
        .trim()
        .parse()
        .ok()
}

/// Início do intervalo de um `Content-Range: bytes <início>-<fim>/<total>`.
fn content_range_start(value: &str) -> Option<u64> {
    let range = value.trim().strip_prefix(CONTENT_RANGE_UNIT_PREFIX)?;
    range.split_once(RANGE_DASH)?.0.trim().parse().ok()
}

impl UreqTransport {
    /// Constrói o cliente (carrega as raízes do sistema e valida o proxy).
    ///
    /// # Errors
    /// [`TransportError::Network`] sem raízes de certificado ou com proxy inválido.
    pub fn new(config: &TransportConfig) -> Result<Self, TransportError> {
        let test_origin = config.test_origin().cloned();
        let roots = if test_origin.is_some() {
            // Origem de teste é HTTP local: não há TLS, então não depende das raízes da máquina.
            RootCerts::new_with_certs(&[])
        } else {
            system_roots()?
        };
        let proxy = config
            .proxy
            .as_deref()
            .map(Proxy::new)
            .transpose()
            .map_err(network)?;
        let agent = Agent::config_builder()
            .https_only(test_origin.is_none())
            .max_redirects(0)
            .http_status_as_error(false)
            .user_agent(format!("{USER_AGENT_PREFIX}/{UPDATER_VERSION}"))
            .proxy(proxy)
            .tls_config(TlsConfig::builder().root_certs(roots).build())
            .timeout_resolve(Some(RESOLVE_TIMEOUT))
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_per_read(Some(READ_TIMEOUT))
            .timeout_per_write(Some(WRITE_TIMEOUT))
            .build()
            .new_agent();
        Ok(Self { agent, test_origin })
    }

    /// URL realmente requisitada: a própria, ou a origem de teste com o mesmo caminho e query.
    fn actual_url(&self, logical: &Url) -> String {
        match &self.test_origin {
            Some(origin) => {
                let mut rewritten = origin.clone();
                rewritten.set_path(logical.path());
                rewritten.set_query(logical.query());
                rewritten.to_string()
            }
            None => logical.to_string(),
        }
    }

    /// GET seguindo redirecionamentos à mão, aplicando a política de URL em **cada** salto.
    fn fetch(
        &self,
        start: &str,
        headers: &[Header<'_>],
        global_timeout: Option<Duration>,
    ) -> Result<Response<Body>, TransportError> {
        let mut url = check_url(start).map_err(|e| rejected(&e))?;
        for followed in 0..=MAX_REDIRECTS {
            let mut request = self.agent.get(self.actual_url(&url));
            for (name, value) in headers {
                request = request.header(*name, value);
            }
            let response = request
                .config()
                .timeout_global(global_timeout)
                .build()
                .call()
                .map_err(network)?;
            let status = response.status().as_u16();
            if !REDIRECT_STATUSES.contains(&status) {
                return Ok(response);
            }
            if followed == MAX_REDIRECTS {
                return Err(network("redirecionamentos demais"));
            }
            let location = header_text(&response, HEADER_LOCATION)
                .ok_or_else(|| network("redirecionamento sem Location"))?;
            url = resolve_redirect(&url, location).map_err(|e| rejected(&e))?;
        }
        Err(network("redirecionamentos demais"))
    }
}

impl Transport for UreqTransport {
    fn get(
        &self,
        url: &str,
        if_none_match: Option<&str>,
        max_bytes: u64,
    ) -> Result<Fetched, TransportError> {
        let headers: Vec<Header<'_>> = if_none_match
            .map(|tag| (HEADER_IF_NONE_MATCH, tag.to_owned()))
            .into_iter()
            .collect();
        let response = self.fetch(url, &headers, Some(SMALL_REQUEST_TIMEOUT))?;
        let status = response.status().as_u16();
        if status == STATUS_NOT_MODIFIED && if_none_match.is_some() {
            return Ok(Fetched::NotModified);
        }
        if status != STATUS_OK {
            return Err(TransportError::Status(status));
        }
        if content_length(&response).is_some_and(|len| len > max_bytes) {
            return Err(TransportError::TooLarge(max_bytes));
        }
        let etag = header_text(&response, HEADER_ETAG).map(str::to_owned);
        let mut body = response.into_body();
        let mut bytes = Vec::new();
        // `take(max + 1)` detecta o excesso sem ler o corpo inteiro.
        body.as_reader()
            .take(max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(network)?;
        if bytes.len() as u64 > max_bytes {
            return Err(TransportError::TooLarge(max_bytes));
        }
        Ok(Fetched::Body { bytes, etag })
    }

    fn download(&self, url: &str, dest: &Path, max_bytes: u64) -> Result<(), TransportError> {
        let existing = resumable_len(dest, max_bytes)?;
        let mut headers: Vec<Header<'_>> = Vec::new();
        if existing > 0 {
            headers.push((
                HEADER_RANGE,
                format!("{RANGE_UNIT_PREFIX}{existing}{RANGE_DASH}"),
            ));
        }
        let response = self.fetch(url, &headers, None)?;
        let start = match response.status().as_u16() {
            STATUS_OK => 0,
            STATUS_PARTIAL_CONTENT if existing > 0 => {
                let ok = header_text(&response, HEADER_CONTENT_RANGE).and_then(content_range_start)
                    == Some(existing);
                if !ok {
                    // Intervalo inesperado: descarta o parcial; a próxima rodada recomeça.
                    let _ = fs::remove_file(dest);
                    return Err(network("Content-Range inesperado na retomada"));
                }
                existing
            }
            other => {
                // `416` (ou outro): o parcial não serve mais.
                let _ = fs::remove_file(dest);
                return Err(TransportError::Status(other));
            }
        };
        let expected_len = content_length(&response);
        if expected_len.is_some_and(|len| start.saturating_add(len) > max_bytes) {
            let _ = fs::remove_file(dest);
            return Err(TransportError::TooLarge(max_bytes));
        }
        let mut file = open_part(dest, start > 0).map_err(io_error)?;
        let total = copy_limited(
            response.into_body().into_reader(),
            &mut file,
            start,
            max_bytes,
        )
        .inspect_err(|error| {
            // Corpo grande demais invalida o arquivo; falha de rede mantém o parcial.
            if matches!(error, TransportError::TooLarge(_)) {
                let _ = fs::remove_file(dest);
            }
        })?;
        file.flush().map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        if let Some(len) = expected_len {
            let expected_total = start.saturating_add(len);
            if total != expected_total {
                if total > expected_total {
                    // Mais bytes do que o Content-Length anunciado: o .part está corrompido.
                    let _ = fs::remove_file(dest);
                }
                return Err(network("download truncado"));
            }
        }
        Ok(())
    }
}

/// Usado como `map_err(io_error)`; por isso recebe o erro por valor.
#[allow(clippy::needless_pass_by_value)]
fn io_error(error: io::Error) -> TransportError {
    TransportError::Io(error.to_string())
}

/// Tamanho do parcial existente (0 se não existe); parcial já acima do limite é descartado.
fn resumable_len(dest: &Path, max_bytes: u64) -> Result<u64, TransportError> {
    match fs::metadata(dest) {
        Ok(meta) if meta.is_file() && meta.len() <= max_bytes => Ok(meta.len()),
        Ok(_) => {
            fs::remove_file(dest).map_err(io_error)?;
            Ok(0)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(io_error(error)),
    }
}

/// Abre o `.part`: anexa se retomando, senão cria/trunca. Modo `0600` em Unix.
fn open_part(dest: &Path, append: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    if append {
        options.append(true);
    } else {
        options.write(true).create(true).truncate(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(PART_FILE_UNIX_MODE);
    }
    options.open(dest)
}

#[cfg(unix)]
use crate::constants::PART_FILE_UNIX_MODE;

/// Copia `reader` para `file` contando bytes desde `already`; passa de `max_bytes` → `TooLarge`.
fn copy_limited(
    mut reader: impl Read,
    file: &mut File,
    already: u64,
    max_bytes: u64,
) -> Result<u64, TransportError> {
    let mut buffer = vec![0u8; DOWNLOAD_BUFFER_BYTES];
    let mut total = already;
    loop {
        let read = reader.read(&mut buffer).map_err(network)?;
        if read == 0 {
            return Ok(total);
        }
        total += read as u64;
        if total > max_bytes {
            return Err(TransportError::TooLarge(max_bytes));
        }
        file.write_all(&buffer[..read]).map_err(io_error)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_vem_de_https_proxy_maiusculo_ou_minusculo() {
        let upper = |name: &str| (name == "HTTPS_PROXY").then(|| "http://p.test:3128".to_owned());
        let lower = |name: &str| (name == "https_proxy").then(|| " http://q.test:8080 ".to_owned());
        let both = |name: &str| Some(format!("http://{name}.test"));
        assert_eq!(
            proxy_from_lookup(upper).as_deref(),
            Some("http://p.test:3128")
        );
        assert_eq!(
            proxy_from_lookup(lower).as_deref(),
            Some("http://q.test:8080")
        );
        assert_eq!(
            proxy_from_lookup(both).as_deref(),
            Some("http://HTTPS_PROXY.test"),
            "a maiúscula tem precedência"
        );
        assert_eq!(proxy_from_lookup(|_| None), None);
        assert_eq!(proxy_from_lookup(|_| Some("  ".to_owned())), None);
    }

    #[test]
    fn raizes_do_sistema_carregam() {
        assert!(
            system_roots().is_ok(),
            "esta máquina precisa expor raízes de certificado (ca-certificates)"
        );
    }

    #[test]
    fn content_range_le_o_inicio_do_intervalo() {
        assert_eq!(content_range_start("bytes 100-999/1000"), Some(100));
        assert_eq!(content_range_start("bytes 0-9/10"), Some(0));
        assert_eq!(content_range_start("items 1-2/3"), None);
        assert_eq!(content_range_start("bytes */1000"), None);
    }
}
