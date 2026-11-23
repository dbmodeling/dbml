//! Manifesto de atualização (`latest.json`, `schema: 1`).

use std::collections::HashMap;

use serde::Deserialize;
use thiserror::Error;

use crate::constants::{
    FORBIDDEN_URL_CHARS, FORBIDDEN_URL_SEQUENCES, HEX_RADIX, RELEASES_URL_PREFIX, SHA256_HEX_LEN,
    SUPPORTED_MANIFEST_SCHEMA,
};
use crate::platform::PlatformKey;

/// Erros ao interpretar ou validar o manifesto.
#[derive(Debug, Error)]
pub enum ManifestError {
    /// JSON malformado ou com campos obrigatórios ausentes.
    #[error("manifesto inválido: {0}")]
    Json(#[from] serde_json::Error),
    /// Manifesto não traz artefato para a plataforma pedida.
    #[error("manifesto sem artefato para a plataforma `{0}`")]
    MissingPlatform(&'static str),
    /// `url` do artefato fora do repositório fixado, sem HTTPS ou com travessia de caminho.
    #[error("URL de artefato recusada: {0}")]
    ForbiddenUrl(String),
    /// `sha256` do artefato não tem 64 dígitos hexadecimais.
    #[error("sha256 inválido no manifesto: {0}")]
    InvalidSha256(String),
}

/// Artefato de uma plataforma.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PlatformAsset {
    /// URL de download (precisa pertencer ao repositório fixado).
    pub url: String,
    /// SHA-256 do artefato em hexadecimal.
    pub sha256: String,
    /// Team ID da Apple; só existe se o build do macOS foi assinado.
    #[serde(default)]
    pub team_id: Option<String>,
}

/// Exige HTTPS dentro do `prefix` fixado e, no restante, sem `%`, `\\`, `@` (userinfo), `..`,
/// `//`, espaços nem caracteres de controle.
///
/// # Errors
/// [`ManifestError::ForbiddenUrl`] se a URL violar a política.
pub fn validate_url(url: &str, prefix: &str) -> Result<(), ManifestError> {
    let valid = url.strip_prefix(prefix).is_some_and(|rest| {
        // Inclui a última `/` do prefixo para pegar um `//` logo na emenda.
        let tail = &url[prefix.len().saturating_sub(1)..];
        !rest.is_empty()
            && !rest.contains(FORBIDDEN_URL_CHARS)
            && !FORBIDDEN_URL_SEQUENCES.iter().any(|seq| tail.contains(seq))
            && !rest.chars().any(|c| c.is_whitespace() || c.is_control())
    });
    if valid {
        Ok(())
    } else {
        Err(ManifestError::ForbiddenUrl(url.to_owned()))
    }
}

impl PlatformAsset {
    /// Valida URL (HTTPS, dentro do repositório, sem travessia) e formato do `sha256`.
    ///
    /// # Errors
    /// [`ManifestError::ForbiddenUrl`] ou [`ManifestError::InvalidSha256`].
    pub fn validate(&self) -> Result<(), ManifestError> {
        validate_url(&self.url, RELEASES_URL_PREFIX)?;
        let hex_ok = self.sha256.len() == SHA256_HEX_LEN
            && self.sha256.chars().all(|c| c.is_digit(HEX_RADIX));
        if !hex_ok {
            return Err(ManifestError::InvalidSha256(self.sha256.clone()));
        }
        Ok(())
    }
}

/// Manifesto `schema: 1`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    /// Versão do esquema do manifesto.
    pub schema: u32,
    /// Versão publicada (semver).
    pub version: String,
    /// Link das notas da versão.
    pub notes_url: String,
    /// Artefatos por chave de plataforma.
    pub platforms: HashMap<String, PlatformAsset>,
}

/// Só o campo `schema`, para decidir se o restante é compreensível.
#[derive(Deserialize)]
struct SchemaProbe {
    schema: Option<serde_json::Value>,
}

impl Manifest {
    /// Interpreta o manifesto. Esquema desconhecido (ou ausente) é **ignorado**, não é erro:
    /// retorna `Ok(None)` para o chamador seguir como "sem atualização".
    ///
    /// Em `schema: 1`, um `notes_url` fora das Releases do repositório fixado invalida o manifesto inteiro
    /// ([`ManifestError::ForbiddenUrl`]): o link aparece na UI, então não vale confiar nele.
    ///
    /// # Errors
    /// [`ManifestError::Json`] se o JSON for inválido ou o esquema suportado estiver incompleto.
    pub fn parse(bytes: &[u8]) -> Result<Option<Self>, ManifestError> {
        let probe: SchemaProbe = serde_json::from_slice(bytes)?;
        let supported = u64::from(SUPPORTED_MANIFEST_SCHEMA);
        if probe.schema.as_ref().and_then(serde_json::Value::as_u64) != Some(supported) {
            return Ok(None);
        }
        let manifest: Self = serde_json::from_slice(bytes)?;
        validate_url(&manifest.notes_url, RELEASES_URL_PREFIX)?;
        Ok(Some(manifest))
    }

    /// Artefato da plataforma, já validado.
    ///
    /// # Errors
    /// [`ManifestError::MissingPlatform`] ou erros de [`PlatformAsset::validate`].
    pub fn asset_for(&self, platform: PlatformKey) -> Result<&PlatformAsset, ManifestError> {
        let asset = self
            .platforms
            .get(platform.as_str())
            .ok_or(ManifestError::MissingPlatform(platform.as_str()))?;
        asset.validate()?;
        Ok(asset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn json(url: &str, sha: &str) -> String {
        format!(
            r#"{{
              "schema": 1,
              "version": "0.4.0",
              "notes_url": "https://github.com/dbmodeling/dbml/releases/tag/v0.4.0",
              "platforms": {{
                "windows-x86_64": {{ "url": "{url}", "sha256": "{sha}" }},
                "macos-aarch64": {{ "url": "{url}", "sha256": "{sha}", "team_id": "ABCDE12345" }},
                "linux-x86_64": {{ "url": "{url}", "sha256": "{sha}" }},
                "linux-x86_64-deb": {{ "url": "{url}", "sha256": "{sha}" }},
                "linux-x86_64-rpm": {{ "url": "{url}", "sha256": "{sha}" }}
              }}
            }}"#
        )
    }

    fn good_url() -> &'static str {
        "https://github.com/dbmodeling/dbml/releases/download/v0.4.0/db-modeling-linux-x86_64.tar.gz"
    }

    fn parsed() -> Manifest {
        Manifest::parse(json(good_url(), SHA).as_bytes())
            .expect("json válido")
            .expect("schema 1")
    }

    #[test]
    fn interpreta_manifesto_valido() {
        let m = parsed();
        assert_eq!(m.version, "0.4.0");
        assert_eq!(m.platforms.len(), 5);
        let mac = m.asset_for(PlatformKey::MacosAarch64).expect("mac");
        assert_eq!(mac.team_id.as_deref(), Some("ABCDE12345"));
        let linux = m.asset_for(PlatformKey::LinuxX86_64).expect("linux");
        assert_eq!(linux.team_id, None);
        assert!(m.asset_for(PlatformKey::LinuxX86_64Deb).is_ok());
        assert!(m.asset_for(PlatformKey::LinuxX86_64Rpm).is_ok());
    }

    #[test]
    fn manifesto_antigo_sem_pacotes_continua_valido() {
        let old = format!(
            r#"{{
              "schema": 1,
              "version": "0.4.0",
              "notes_url": "https://github.com/dbmodeling/dbml/releases/tag/v0.4.0",
              "platforms": {{
                "linux-x86_64": {{ "url": "{}", "sha256": "{SHA}" }}
              }}
            }}"#,
            good_url()
        );
        let m = Manifest::parse(old.as_bytes())
            .expect("json válido")
            .expect("schema 1");
        assert!(m.asset_for(PlatformKey::LinuxX86_64).is_ok());
        assert!(matches!(
            m.asset_for(PlatformKey::LinuxX86_64Deb),
            Err(ManifestError::MissingPlatform("linux-x86_64-deb"))
        ));
        assert!(matches!(
            m.asset_for(PlatformKey::LinuxX86_64Rpm),
            Err(ManifestError::MissingPlatform("linux-x86_64-rpm"))
        ));
    }

    #[test]
    fn esquema_desconhecido_e_ignorado() {
        let future = br#"{"schema": 2, "version": "9.0.0", "novo": true}"#;
        assert!(Manifest::parse(future).expect("não é erro").is_none());
    }

    #[test]
    fn esquema_ausente_e_ignorado() {
        assert!(
            Manifest::parse(br#"{"version":"1.0.0"}"#)
                .expect("ok")
                .is_none()
        );
    }

    #[test]
    fn esquema_de_tipo_inesperado_e_ignorado() {
        assert!(Manifest::parse(br#"{"schema":"1"}"#).expect("ok").is_none());
    }

    #[test]
    fn json_malformado_e_erro() {
        assert!(matches!(
            Manifest::parse(b"{nao e json"),
            Err(ManifestError::Json(_))
        ));
    }

    #[test]
    fn schema_1_incompleto_e_erro() {
        assert!(matches!(
            Manifest::parse(br#"{"schema":1,"version":"1.0.0"}"#),
            Err(ManifestError::Json(_))
        ));
    }

    #[test]
    fn campos_extras_sao_tolerados() {
        let text =
            json(good_url(), SHA).replace(r#""schema": 1,"#, r#""schema": 1, "extra": {"a": 1},"#);
        assert!(Manifest::parse(text.as_bytes()).expect("ok").is_some());
    }

    #[test]
    fn plataforma_ausente_e_erro() {
        let mut m = parsed();
        m.platforms.remove("linux-x86_64");
        assert!(matches!(
            m.asset_for(PlatformKey::LinuxX86_64),
            Err(ManifestError::MissingPlatform("linux-x86_64"))
        ));
    }

    #[test]
    fn url_fora_do_repositorio_e_recusada() {
        for url in [
            "https://evil.example/db-modeling.tar.gz",
            "http://github.com/dbmodeling/dbml/releases/download/v1/a.tar.gz",
            "https://github.com/outro/repo/releases/download/v1/a.tar.gz",
            "https://github.com/dbmodeling/dbml/releases/../../../evil/a.tar.gz",
        ] {
            let m = Manifest::parse(json(url, SHA).as_bytes())
                .expect("json")
                .expect("schema");
            assert!(
                matches!(
                    m.asset_for(PlatformKey::LinuxX86_64),
                    Err(ManifestError::ForbiddenUrl(_))
                ),
                "deveria recusar {url}"
            );
        }
    }

    #[test]
    fn url_com_escape_barra_invertida_userinfo_ou_barra_dupla_e_recusada() {
        let base = "https://github.com/dbmodeling/dbml/releases/download/v1/";
        for suffix in [
            "%2e%2e/evil.tar.gz",
            "..%2fevil.tar.gz",
            "a\\..\\b.tar.gz",
            "x@evil.example/a.tar.gz",
            "/a.tar.gz",
            "dir//a.tar.gz",
            "a b.tar.gz",
        ] {
            let url = format!("{base}{suffix}");
            // `json()` embute o texto cru: a barra invertida precisa de escape JSON.
            let m = Manifest::parse(json(&url.replace('\\', "\\\\"), SHA).as_bytes())
                .expect("json")
                .expect("schema");
            assert!(
                matches!(
                    m.asset_for(PlatformKey::LinuxX86_64),
                    Err(ManifestError::ForbiddenUrl(_))
                ),
                "deveria recusar {url}"
            );
        }
    }

    #[test]
    fn notes_url_fora_do_repositorio_invalida_o_manifesto() {
        // Decisão: `notes_url` inválido é erro de validação do manifesto inteiro.
        for notes in [
            "https://evil.example/notes",
            "http://github.com/dbmodeling/dbml/releases/tag/v1",
            "https://github.com/outro/repo/releases/tag/v1",
            "https://github.com/dbmodeling/dbml/../../evil",
            // dentro do repositório, mas fora de `/releases/`: o app descartaria o aviso
            "https://github.com/dbmodeling/dbml/issues/1",
            "https://github.com/dbmodeling/dbml/blob/main/README.md",
            "https://github.com/dbmodeling/dbml/releases",
            "https://github.com/dbmodeling/dbml/releases/%2e%2e/x",
        ] {
            let text = json(good_url(), SHA).replace(
                "https://github.com/dbmodeling/dbml/releases/tag/v0.4.0",
                notes,
            );
            assert!(
                matches!(
                    Manifest::parse(text.as_bytes()),
                    Err(ManifestError::ForbiddenUrl(_))
                ),
                "deveria recusar {notes}"
            );
        }
    }

    #[test]
    fn notes_url_sob_releases_e_aceito() {
        let text = json(good_url(), SHA);
        assert!(text.contains("/releases/tag/v0.4.0"));
        let manifest = Manifest::parse(text.as_bytes())
            .expect("json")
            .expect("schema");
        assert!(manifest.notes_url.starts_with(RELEASES_URL_PREFIX));
    }

    #[test]
    fn sha256_malformado_e_recusado() {
        for sha in ["", "abc", &"g".repeat(64), &"a".repeat(63)] {
            let m = Manifest::parse(json(good_url(), sha).as_bytes())
                .expect("json")
                .expect("schema");
            assert!(
                matches!(
                    m.asset_for(PlatformKey::LinuxX86_64),
                    Err(ManifestError::InvalidSha256(_))
                ),
                "deveria recusar `{sha}`"
            );
        }
    }
}
