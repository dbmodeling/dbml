//! Política de URL e de redirecionamento: só HTTPS, só hosts do GitHub, sem userinfo, sem porta
//! explícita e sem travessia de caminho.
//!
//! A URL é **parseada de verdade** (crate `url`), mas o parser da WHATWG *normaliza* `..` e
//! `%2e%2e` em vez de rejeitá-los; por isso o caminho **bruto** (antes do parser) também é
//! inspecionado, segmento a segmento.

use thiserror::Error;
use url::Url;

use crate::constants::{
    ALLOWED_HOST_SUFFIXES, ALLOWED_HOSTS, BACKSLASH, DOT_DOT_SEGMENT, DOT_SEGMENT, ENCODED_DOT,
    ENCODED_SEPARATORS, FRAGMENT_START, PATH_SEPARATOR, QUERY_START, SCHEME_HTTPS,
    SCHEME_SEPARATOR,
};

/// Motivo da recusa de uma URL.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UrlRejection {
    /// Não é uma URL válida (ou tem espaço/caractere de controle, que o parser removeria).
    #[error("URL malformada: {0}")]
    Malformed(String),
    /// Esquema diferente de HTTPS.
    #[error("esquema não permitido: {0}")]
    NotHttps(String),
    /// Host fora da lista do GitHub.
    #[error("host não permitido: {0}")]
    HostNotAllowed(String),
    /// Usuário/senha na URL.
    #[error("URL com credenciais embutidas")]
    UserInfo,
    /// Porta explícita diferente da padrão.
    #[error("porta explícita não permitida")]
    ExplicitPort,
    /// Segmento `.`/`..` (inclusive codificado) ou separador codificado no caminho.
    #[error("travessia ou separador codificado no caminho: {0}")]
    PathTraversal(String),
    /// A URL do artefato não pertence ao repositório fixado.
    #[error("URL fora do repositório fixado: {0}")]
    OutsideRepository(String),
}

/// `host` é um host do GitHub permitido (exato ou subdomínio de `githubusercontent.com`).
#[must_use]
pub fn is_allowed_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    ALLOWED_HOSTS.contains(&host.as_str())
        || ALLOWED_HOST_SUFFIXES
            .iter()
            .any(|suffix| host.len() > suffix.len() && host.ends_with(suffix))
}

/// Caminho **bruto** de uma URL absoluta (sem esquema, autoridade, query nem fragmento) ou de uma
/// referência relativa.
fn raw_path(raw: &str) -> &str {
    let delimiters = [PATH_SEPARATOR, QUERY_START, FRAGMENT_START];
    // Só é absoluta se o `://` vier antes de qualquer `/`, `?` ou `#`.
    let after_authority = match raw.find(SCHEME_SEPARATOR) {
        Some(index) if !raw[..index].contains(delimiters) => {
            let after_scheme = &raw[index + SCHEME_SEPARATOR.len()..];
            after_scheme
                .find(delimiters)
                .map_or("", |start| &after_scheme[start..])
        }
        _ => raw,
    };
    let end = after_authority
        .find([QUERY_START, FRAGMENT_START])
        .unwrap_or(after_authority.len());
    &after_authority[..end]
}

/// Rejeita `.`/`..` (inclusive `%2e`), `%2f`, `%5c` e `\` no caminho bruto.
fn check_raw_path(raw: &str) -> Result<(), UrlRejection> {
    let traversal = || UrlRejection::PathTraversal(raw.to_owned());
    let path = raw_path(raw);
    if path.contains(BACKSLASH) {
        return Err(traversal());
    }
    for segment in path.split(PATH_SEPARATOR) {
        let lowered = segment.to_ascii_lowercase();
        if ENCODED_SEPARATORS.iter().any(|enc| lowered.contains(enc)) {
            return Err(traversal());
        }
        let decoded = lowered.replace(ENCODED_DOT, DOT_SEGMENT);
        if decoded == DOT_SEGMENT || decoded == DOT_DOT_SEGMENT {
            return Err(traversal());
        }
    }
    Ok(())
}

fn check_parsed(url: Url) -> Result<Url, UrlRejection> {
    if url.scheme() != SCHEME_HTTPS {
        return Err(UrlRejection::NotHttps(url.scheme().to_owned()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(UrlRejection::UserInfo);
    }
    if url.port().is_some() {
        return Err(UrlRejection::ExplicitPort);
    }
    match url.host_str() {
        Some(host) if is_allowed_host(host) => Ok(url),
        other => Err(UrlRejection::HostNotAllowed(
            other.unwrap_or_default().to_owned(),
        )),
    }
}

fn check_no_control(raw: &str) -> Result<(), UrlRejection> {
    if raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(UrlRejection::Malformed(raw.to_owned()));
    }
    Ok(())
}

/// Valida uma URL absoluta de requisição (manifesto, assinatura, artefato).
///
/// # Errors
/// [`UrlRejection`] se não for HTTPS, não for de um host do GitHub, tiver credenciais, porta
/// explícita ou travessia de caminho.
pub fn check_url(raw: &str) -> Result<Url, UrlRejection> {
    check_no_control(raw)?;
    check_raw_path(raw)?;
    let url = Url::parse(raw).map_err(|_| UrlRejection::Malformed(raw.to_owned()))?;
    check_parsed(url)
}

/// Resolve o `Location` de um redirecionamento contra `base` (aceita relativo) e aplica a mesma
/// política de [`check_url`] ao destino.
///
/// # Errors
/// [`UrlRejection`] se o destino violar a política.
pub fn resolve_redirect(base: &Url, location: &str) -> Result<Url, UrlRejection> {
    check_no_control(location)?;
    check_raw_path(location)?;
    let url = base
        .join(location)
        .map_err(|_| UrlRejection::Malformed(location.to_owned()))?;
    check_parsed(url)
}

/// Valida a URL de um **artefato**: além de [`check_url`], precisa estar sob o prefixo fixado do
/// repositório (host e caminho), já parseado.
///
/// # Errors
/// [`UrlRejection::OutsideRepository`] (ou qualquer erro de [`check_url`]).
pub fn check_asset_url(raw: &str, repository_prefix: &str) -> Result<Url, UrlRejection> {
    let url = check_url(raw)?;
    let prefix = Url::parse(repository_prefix)
        .map_err(|_| UrlRejection::Malformed(repository_prefix.to_owned()))?;
    let same_host = url.host_str() == prefix.host_str();
    if same_host && url.path().starts_with(prefix.path()) {
        Ok(url)
    } else {
        Err(UrlRejection::OutsideRepository(raw.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use updater::constants::{MANIFEST_URL, RELEASES_URL_PREFIX};

    use super::*;

    const ASSET: &str = "https://github.com/dbmodeling/dbml/releases/download/v0.4.0/app.tar.gz";

    #[test]
    fn urls_fixas_do_projeto_passam() {
        assert!(check_url(MANIFEST_URL).is_ok());
        assert!(check_asset_url(ASSET, RELEASES_URL_PREFIX).is_ok());
    }

    #[test]
    fn exige_https() {
        assert!(matches!(
            check_url("http://github.com/dbmodeling/dbml/releases/x"),
            Err(UrlRejection::NotHttps(_))
        ));
        assert!(matches!(
            check_url("ftp://github.com/x"),
            Err(UrlRejection::NotHttps(_))
        ));
    }

    #[test]
    fn hosts_do_github_sao_os_unicos() {
        assert!(is_allowed_host("github.com"));
        assert!(is_allowed_host("objects.githubusercontent.com"));
        assert!(is_allowed_host("release-assets.githubusercontent.com"));
        assert!(!is_allowed_host("githubusercontent.com"));
        assert!(!is_allowed_host("evilgithubusercontent.com"));
        assert!(!is_allowed_host("github.com.evil.example"));
        assert!(!is_allowed_host("evil.example"));
        assert!(!is_allowed_host("api.github.com"));
    }

    #[test]
    fn host_fora_da_lista_e_recusado_ate_com_userinfo_enganoso() {
        assert!(matches!(
            check_url("https://evil.example/dbmodeling/dbml/releases/x"),
            Err(UrlRejection::HostNotAllowed(_))
        ));
        // `github.com` aqui é só o usuário; o host real é evil.example.
        assert!(check_url("https://github.com@evil.example/x").is_err());
        assert_eq!(
            check_url("https://user:pw@github.com/dbmodeling/dbml/releases/x"),
            Err(UrlRejection::UserInfo)
        );
    }

    #[test]
    fn porta_explicita_e_recusada() {
        assert_eq!(
            check_url("https://github.com:8443/dbmodeling/dbml/releases/x"),
            Err(UrlRejection::ExplicitPort)
        );
    }

    #[test]
    fn travessia_de_caminho_e_recusada_inclusive_codificada() {
        for bad in [
            "https://github.com/dbmodeling/dbml/releases/../../evil/x",
            "https://github.com/dbmodeling/dbml/releases/%2e%2e/%2e%2e/evil/x",
            "https://github.com/dbmodeling/dbml/releases/%2E%2e/evil/x",
            "https://github.com/dbmodeling/dbml/releases/.%2e/evil/x",
            "https://github.com/dbmodeling/dbml/releases/./x",
            "https://github.com/dbmodeling/dbml/releases/a%2fb",
            "https://github.com/dbmodeling/dbml/releases/a%5Cb",
            "https://github.com/dbmodeling/dbml/releases/a\\b",
        ] {
            assert!(
                matches!(check_url(bad), Err(UrlRejection::PathTraversal(_))),
                "deveria recusar: {bad}"
            );
        }
    }

    #[test]
    fn query_codificada_do_cdn_nao_e_confundida_com_travessia() {
        let signed = "https://objects.githubusercontent.com/github-production-release-asset/1/2?sp=r&sig=ab%2e%2e%2fcd&rscd=attachment%3B+filename%3Dx";
        assert!(check_url(signed).is_ok());
    }

    #[test]
    fn espaco_e_controle_sao_recusados() {
        assert!(matches!(
            check_url("https://github.com/dbmodeling/dbml/rel eases/x"),
            Err(UrlRejection::Malformed(_))
        ));
        assert!(matches!(
            check_url("https://github.com/dbmodeling/dbml/releases/\n../x"),
            Err(UrlRejection::Malformed(_))
        ));
    }

    #[test]
    fn redirecionamento_relativo_e_absoluto_passam_pela_mesma_politica() {
        let base = check_url(MANIFEST_URL).expect("base");
        let ok = resolve_redirect(&base, "/dbmodeling/dbml/releases/download/v1/latest.json")
            .expect("relativo");
        assert_eq!(ok.host_str(), Some("github.com"));
        let cdn =
            resolve_redirect(&base, "https://objects.githubusercontent.com/a/b?x=1").expect("cdn");
        assert_eq!(cdn.host_str(), Some("objects.githubusercontent.com"));
        assert!(matches!(
            resolve_redirect(&base, "https://evil.example/a"),
            Err(UrlRejection::HostNotAllowed(_))
        ));
        assert!(matches!(
            resolve_redirect(&base, "//evil.example/a"),
            Err(UrlRejection::HostNotAllowed(_))
        ));
        assert!(matches!(
            resolve_redirect(&base, "http://github.com/a"),
            Err(UrlRejection::NotHttps(_))
        ));
        assert!(matches!(
            resolve_redirect(&base, "/a/%2e%2e/%2e%2e/b"),
            Err(UrlRejection::PathTraversal(_))
        ));
        // `://` dentro da query de uma referência relativa não a transforma em absoluta.
        assert!(resolve_redirect(&base, "/ok/a?next=https://x.test/..").is_ok());
        assert!(matches!(
            resolve_redirect(&base, "../../b"),
            Err(UrlRejection::PathTraversal(_))
        ));
    }

    #[test]
    fn artefato_fora_do_repositorio_e_recusado() {
        for outside in [
            "https://github.com/outro/repo/releases/download/v1/app.tar.gz",
            "https://github.com/dbmodeling/dbml/archive/main.tar.gz",
            "https://github.com/dbmodeling/dbml-evil/releases/download/v1/app",
            "https://objects.githubusercontent.com/dbmodeling/dbml/releases/x",
        ] {
            assert!(
                matches!(
                    check_asset_url(outside, RELEASES_URL_PREFIX),
                    Err(UrlRejection::OutsideRepository(_))
                ),
                "deveria recusar: {outside}"
            );
        }
    }
}
