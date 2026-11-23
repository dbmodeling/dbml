//! Regra de versão: só atualiza para semver maior e diferente da já instalada.

use semver::{BuildMetadata, Version};
use thiserror::Error;

/// Motivo pelo qual uma versão do manifesto não deve ser instalada.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VersionRefusal {
    /// Versão do manifesto é igual ou menor que a em execução (inclui rebaixamento).
    #[error("versão {remote} não é maior que a em execução ({current})")]
    NotNewer {
        /// Versão em execução.
        current: Version,
        /// Versão do manifesto.
        remote: Version,
    },
    /// Versão remota é pré-release e o app em execução é estável (só o canal `stable`).
    #[error("versão {0} é pré-release; só o canal estável é aceito")]
    PreRelease(Version),
    /// Esta versão já foi instalada por este app e aguarda reinício.
    #[error("versão {0} já instalada e aguardando reinício")]
    AlreadyInstalled(Version),
}

/// Erro ao interpretar uma versão.
#[derive(Debug, Error)]
#[error("versão inválida `{input}`: {source}")]
pub struct VersionParseError {
    input: String,
    #[source]
    source: semver::Error,
}

/// Interpreta uma versão semver (aceita prefixo `v` e espaços nas pontas).
///
/// # Errors
/// Retorna [`VersionParseError`] se o texto não for semver válido.
pub fn parse_version(text: &str) -> Result<Version, VersionParseError> {
    let trimmed = text.trim();
    let bare = trimmed.strip_prefix('v').unwrap_or(trimmed);
    Version::parse(bare).map_err(|source| VersionParseError {
        input: text.to_owned(),
        source,
    })
}

/// Versão sem build metadata (`+ci.7`), que não participa da precedência semver.
fn without_build(version: &Version) -> Version {
    Version {
        build: BuildMetadata::EMPTY,
        ..version.clone()
    }
}

/// Decide se `remote` deve ser instalada.
///
/// Atualiza só se `remote > current` (ignorando build metadata nos dois lados), `remote` for
/// diferente da versão que este app já instalou e aguarda reinício (`pending`) e, quando o app
/// em execução é estável, `remote` também for estável (canal único `stable`).
///
/// # Errors
/// [`VersionRefusal`] explica a recusa.
pub fn should_update(
    current: &Version,
    remote: &Version,
    pending: Option<&Version>,
) -> Result<(), VersionRefusal> {
    let (current, remote) = (&without_build(current), &without_build(remote));
    let pending = pending.map(without_build);
    if remote <= current {
        return Err(VersionRefusal::NotNewer {
            current: current.clone(),
            remote: remote.clone(),
        });
    }
    if current.pre.is_empty() && !remote.pre.is_empty() {
        return Err(VersionRefusal::PreRelease(remote.clone()));
    }
    if pending.as_ref() == Some(remote) {
        return Err(VersionRefusal::AlreadyInstalled(remote.clone()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        parse_version(s).expect("versão de teste válida")
    }

    #[test]
    fn versao_maior_atualiza() {
        assert_eq!(should_update(&v("0.3.4"), &v("0.4.0"), None), Ok(()));
        assert_eq!(should_update(&v("0.3.4"), &v("0.3.5"), None), Ok(()));
    }

    #[test]
    fn versao_igual_e_recusada() {
        assert!(matches!(
            should_update(&v("0.3.4"), &v("0.3.4"), None),
            Err(VersionRefusal::NotNewer { .. })
        ));
    }

    #[test]
    fn rebaixamento_e_recusado() {
        assert!(matches!(
            should_update(&v("0.4.0"), &v("0.3.9"), None),
            Err(VersionRefusal::NotNewer { .. })
        ));
    }

    #[test]
    fn pre_release_vale_menos_que_a_estavel() {
        assert!(should_update(&v("0.4.0-rc.1"), &v("0.4.0"), None).is_ok());
        assert!(should_update(&v("0.4.0"), &v("0.4.0-rc.1"), None).is_err());
    }

    #[test]
    fn versao_ja_instalada_aguardando_reinicio_e_recusada() {
        assert_eq!(
            should_update(&v("0.3.4"), &v("0.4.0"), Some(&v("0.4.0"))),
            Err(VersionRefusal::AlreadyInstalled(v("0.4.0")))
        );
    }

    #[test]
    fn versao_maior_que_a_pendente_ainda_atualiza() {
        assert_eq!(
            should_update(&v("0.3.4"), &v("0.4.1"), Some(&v("0.4.0"))),
            Ok(())
        );
    }

    #[test]
    fn build_metadata_e_ignorada_na_comparacao() {
        for (current, remote) in [
            ("0.3.4+ci.7", "0.3.4"),
            ("0.3.4", "0.3.4+ci.9"),
            ("0.3.4+ci.7", "0.3.4+ci.8"),
        ] {
            assert!(
                matches!(
                    should_update(&v(current), &v(remote), None),
                    Err(VersionRefusal::NotNewer { .. })
                ),
                "{current} -> {remote} não deve atualizar"
            );
        }
        assert_eq!(should_update(&v("0.3.4+ci.7"), &v("0.3.5"), None), Ok(()));
    }

    #[test]
    fn pendente_e_comparada_sem_build_metadata() {
        assert_eq!(
            should_update(&v("0.3.4"), &v("0.4.0+a"), Some(&v("0.4.0+b"))),
            Err(VersionRefusal::AlreadyInstalled(v("0.4.0")))
        );
    }

    #[test]
    fn pre_release_remota_e_recusada_quando_o_app_e_estavel() {
        assert_eq!(
            should_update(&v("0.4.0"), &v("0.5.0-rc.1"), None),
            Err(VersionRefusal::PreRelease(v("0.5.0-rc.1")))
        );
    }

    #[test]
    fn pre_release_remota_e_aceita_quando_o_app_ja_e_pre_release() {
        assert_eq!(
            should_update(&v("0.5.0-rc.1"), &v("0.5.0-rc.2"), None),
            Ok(())
        );
    }

    #[test]
    fn parse_aceita_prefixo_v_e_rejeita_lixo() {
        assert_eq!(v("v1.2.3"), Version::new(1, 2, 3));
        assert!(parse_version("abc").is_err());
        assert!(parse_version("").is_err());
    }
}
