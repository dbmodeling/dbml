//! Download do artefato em diretório privado e verificação do SHA-256.
//!
//! Fluxo: `<updates>/<versão>/<nome>.part` → SHA-256 conferido → renomeado para `<nome>` → SHA-256
//! **recalculado** imediatamente antes de devolver o caminho. Um `.part` nunca é devolvido nem
//! usado como pronto; se o hash divergir, o arquivo é apagado.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use semver::Version;
use thiserror::Error;
use updater::constants::RELEASES_URL_PREFIX;
use updater::manifest::PlatformAsset;
use updater::transport::{Transport, TransportError};
use updater::verify::{VerifyError, sha256_hex_file, verify_sha256};

use crate::constants::{
    FILE_NAME_EXTRA_CHARS, MAX_DOWNLOAD_BYTES, MIN_FREE_SPACE_BYTES, PART_EXTENSION,
};
use crate::private_dir::ensure_private_dir;
use crate::url_policy::{UrlRejection, check_asset_url};

/// Espaço livre em disco (injetável para teste).
pub trait FreeSpace: Send + Sync {
    /// Bytes disponíveis ao usuário no volume de `dir`.
    ///
    /// # Errors
    /// Erro de E/S ao consultar o volume.
    fn available(&self, dir: &Path) -> io::Result<u64>;
}

/// Espaço livre real do volume, via `fs4`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemFreeSpace;

impl FreeSpace for SystemFreeSpace {
    fn available(&self, dir: &Path) -> io::Result<u64> {
        fs4::available_space(dir)
    }
}

impl<F> FreeSpace for F
where
    F: Fn(&Path) -> io::Result<u64> + Send + Sync,
{
    fn available(&self, dir: &Path) -> io::Result<u64> {
        self(dir)
    }
}

/// Falhas ao baixar e verificar o artefato.
#[derive(Debug, Error)]
pub enum FetchError {
    /// URL do artefato recusada pela política (fora do repositório, HTTP, travessia...).
    #[error("URL do artefato recusada: {0}")]
    Url(#[from] UrlRejection),
    /// O nome do arquivo derivado da URL não é seguro.
    #[error("nome de arquivo inválido na URL do artefato: `{0}`")]
    InvalidFileName(String),
    /// Menos espaço livre que o mínimo exigido.
    #[error("espaço livre insuficiente: {available} bytes (mínimo {required})")]
    InsufficientSpace {
        /// Bytes livres.
        available: u64,
        /// Bytes exigidos.
        required: u64,
    },
    /// Erro de E/S no diretório de downloads.
    #[error("erro de E/S: {0}")]
    Io(#[from] io::Error),
    /// Falha de rede, status, limite de tamanho ou disco ao baixar.
    #[error("falha ao baixar: {0}")]
    Transport(#[from] TransportError),
    /// O hash do arquivo baixado diverge do manifesto.
    #[error("hash do download diverge do manifesto: {0}")]
    HashMismatch(VerifyError),
    /// O hash recalculado imediatamente antes de entregar o caminho diverge (arquivo trocado).
    #[error("hash recalculado diverge (arquivo alterado após o download): {0}")]
    RecheckMismatch(VerifyError),
}

/// Artefato baixado e com SHA-256 conferido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedAsset {
    /// Caminho do arquivo final (nunca um `.part`).
    pub path: PathBuf,
    /// SHA-256 (hexadecimal minúsculo) recalculado imediatamente antes de devolver.
    pub sha256: String,
}

/// Último segmento do caminho da URL, restrito a `[A-Za-z0-9._-]` e sem começar por ponto.
fn safe_file_name(url: &url::Url) -> Result<String, FetchError> {
    let name = url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .unwrap_or_default();
    let valid = !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || FILE_NAME_EXTRA_CHARS.contains(&c));
    if valid {
        Ok(name.to_owned())
    } else {
        Err(FetchError::InvalidFileName(name.to_owned()))
    }
}

fn part_path(final_path: &Path) -> PathBuf {
    let mut name = final_path.as_os_str().to_owned();
    name.push(".");
    name.push(PART_EXTENSION);
    PathBuf::from(name)
}

/// Confere o `.part`, promove para o nome final e **recalcula** o hash antes de devolver.
/// `before_recheck` é um ponto de observação (os testes adulteram o arquivo ali).
fn finalize(
    part: &Path,
    final_path: &Path,
    expected_sha256: &str,
    before_recheck: impl FnOnce(&Path),
) -> Result<DownloadedAsset, FetchError> {
    let downloaded = sha256_hex_file(part)?;
    if let Err(error) = verify_sha256(&downloaded, expected_sha256) {
        let _ = fs::remove_file(part);
        return Err(FetchError::HashMismatch(error));
    }
    // Windows não renomeia por cima de um arquivo existente.
    let _ = fs::remove_file(final_path);
    fs::rename(part, final_path)?;
    before_recheck(final_path);
    let recomputed = sha256_hex_file(final_path)?;
    if let Err(error) = verify_sha256(&recomputed, expected_sha256) {
        let _ = fs::remove_file(final_path);
        return Err(FetchError::RecheckMismatch(error));
    }
    Ok(DownloadedAsset {
        path: final_path.to_path_buf(),
        sha256: recomputed,
    })
}

/// Baixa o artefato de `version` em `<updates_dir>/<versão>/` e o verifica.
///
/// Reaproveita um arquivo final já verificado de uma rodada anterior (instalação que falhou),
/// retoma um `.part` por `Range` e nunca devolve um arquivo parcial.
///
/// # Errors
/// [`FetchError`]; em qualquer falha de hash o arquivo é apagado.
pub fn download_verified(
    transport: &dyn Transport,
    free_space: &dyn FreeSpace,
    updates_dir: &Path,
    version: &Version,
    asset: &PlatformAsset,
) -> Result<DownloadedAsset, FetchError> {
    let url = check_asset_url(&asset.url, RELEASES_URL_PREFIX)?;
    let name = safe_file_name(&url)?;
    let dir = updates_dir.join(version.to_string());
    ensure_private_dir(&dir)?;
    let final_path = dir.join(&name);

    // Arquivo final de uma rodada anterior: vale se o hash ainda bate; senão é descartado.
    if final_path.is_file() {
        let existing = sha256_hex_file(&final_path)?;
        if verify_sha256(&existing, &asset.sha256).is_ok() {
            return Ok(DownloadedAsset {
                path: final_path,
                sha256: existing,
            });
        }
        fs::remove_file(&final_path)?;
    }

    let available = free_space.available(&dir)?;
    if available < MIN_FREE_SPACE_BYTES {
        return Err(FetchError::InsufficientSpace {
            available,
            required: MIN_FREE_SPACE_BYTES,
        });
    }

    let part = part_path(&final_path);
    transport.download(url.as_str(), &part, MAX_DOWNLOAD_BYTES)?;
    finalize(&part, &final_path, &asset.sha256, |_| {})
}

/// Recalcula o SHA-256 de `path` e o compara ao esperado. É o que cada instalador chama
/// imediatamente antes de montar, extrair ou executar o artefato.
///
/// # Errors
/// [`FetchError::RecheckMismatch`] se divergir; [`FetchError::Io`] se não puder ler.
pub fn recheck(path: &Path, expected_sha256: &str) -> Result<(), FetchError> {
    let actual = sha256_hex_file(path)?;
    verify_sha256(&actual, expected_sha256).map_err(FetchError::RecheckMismatch)
}

#[cfg(test)]
mod tests {
    use updater::transport::fake::FakeTransport;
    use updater::verify::sha256_hex;

    use super::*;

    const ASSET_URL: &str = "https://github.com/dbmodeling/dbml/releases/download/v0.4.0/db-modeling-linux-x86_64.tar.gz";
    const BODY: &[u8] = b"conteudo do artefato";

    fn asset(url: &str, body: &[u8]) -> PlatformAsset {
        PlatformAsset {
            url: url.to_owned(),
            sha256: sha256_hex(body),
            team_id: None,
        }
    }

    fn version() -> Version {
        Version::parse("0.4.0").expect("semver")
    }

    /// Espaço livre fixo.
    struct Space(u64);

    impl FreeSpace for Space {
        fn available(&self, _: &Path) -> io::Result<u64> {
            Ok(self.0)
        }
    }

    const PLENTY: Space = Space(MIN_FREE_SPACE_BYTES);

    #[test]
    fn baixa_confere_e_devolve_o_caminho_final() {
        let updates = tempfile::tempdir().expect("tempdir");
        let transport = FakeTransport::new().with_body(ASSET_URL, BODY);
        let got = download_verified(
            &transport,
            &PLENTY,
            updates.path(),
            &version(),
            &asset(ASSET_URL, BODY),
        )
        .expect("ok");
        assert_eq!(got.sha256, sha256_hex(BODY));
        assert_eq!(fs::read(&got.path).expect("ler"), BODY);
        assert!(got.path.ends_with("0.4.0/db-modeling-linux-x86_64.tar.gz"));
        assert!(!part_path(&got.path).exists(), "o .part some ao concluir");
    }

    #[test]
    fn hash_divergente_ao_baixar_apaga_o_arquivo_e_falha() {
        let updates = tempfile::tempdir().expect("tempdir");
        let transport = FakeTransport::new().with_body(ASSET_URL, b"outro conteudo");
        let error = download_verified(
            &transport,
            &PLENTY,
            updates.path(),
            &version(),
            &asset(ASSET_URL, BODY),
        )
        .expect_err("hash");
        assert!(matches!(error, FetchError::HashMismatch(_)), "{error}");
        let dir = updates.path().join("0.4.0");
        assert_eq!(fs::read_dir(dir).expect("ls").count(), 0, "nada sobra");
    }

    #[test]
    fn hash_divergente_no_recalculo_e_recusado_e_apagado() {
        let updates = tempfile::tempdir().expect("tempdir");
        let dir = updates.path().join("0.4.0");
        fs::create_dir_all(&dir).expect("dir");
        let final_path = dir.join("app.bin");
        let part = part_path(&final_path);
        fs::write(&part, BODY).expect("part");
        let error = finalize(&part, &final_path, &sha256_hex(BODY), |path| {
            // Troca o arquivo entre o primeiro hash e o recálculo.
            fs::write(path, b"trocado").expect("adulterar");
        })
        .expect_err("recálculo");
        assert!(matches!(error, FetchError::RecheckMismatch(_)), "{error}");
        assert!(!final_path.exists(), "arquivo adulterado é apagado");
    }

    #[test]
    fn recheck_publico_detecta_adulteracao_posterior() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("a.bin");
        fs::write(&path, BODY).expect("escrever");
        assert!(recheck(&path, &sha256_hex(BODY)).is_ok());
        fs::write(&path, b"x").expect("adulterar");
        assert!(matches!(
            recheck(&path, &sha256_hex(BODY)),
            Err(FetchError::RecheckMismatch(_))
        ));
    }

    #[test]
    fn url_fora_do_repositorio_e_recusada_sem_baixar() {
        let updates = tempfile::tempdir().expect("tempdir");
        let outside = "https://github.com/outro/repo/releases/download/v1/app.tar.gz";
        let transport = FakeTransport::new().with_body(outside, BODY);
        let error = download_verified(
            &transport,
            &PLENTY,
            updates.path(),
            &version(),
            &asset(outside, BODY),
        )
        .expect_err("fora do repo");
        assert!(matches!(error, FetchError::Url(_)), "{error}");
        assert_eq!(transport.requests().len(), 0);
    }

    #[test]
    fn espaco_livre_insuficiente_nao_baixa() {
        let updates = tempfile::tempdir().expect("tempdir");
        let transport = FakeTransport::new().with_body(ASSET_URL, BODY);
        let tiny = Space(MIN_FREE_SPACE_BYTES - 1);
        let error = download_verified(
            &transport,
            &tiny,
            updates.path(),
            &version(),
            &asset(ASSET_URL, BODY),
        )
        .expect_err("espaço");
        assert!(matches!(error, FetchError::InsufficientSpace { .. }));
        assert_eq!(transport.requests().len(), 0);
    }

    #[test]
    fn arquivo_final_ja_verificado_e_reaproveitado_sem_nova_requisicao() {
        let updates = tempfile::tempdir().expect("tempdir");
        let transport = FakeTransport::new().with_body(ASSET_URL, BODY);
        let first = download_verified(
            &transport,
            &PLENTY,
            updates.path(),
            &version(),
            &asset(ASSET_URL, BODY),
        )
        .expect("1");
        let second = download_verified(
            &transport,
            &PLENTY,
            updates.path(),
            &version(),
            &asset(ASSET_URL, BODY),
        )
        .expect("2");
        assert_eq!(first, second);
        assert_eq!(transport.requests().len(), 1);
    }

    #[test]
    fn arquivo_final_corrompido_e_descartado_e_baixado_de_novo() {
        let updates = tempfile::tempdir().expect("tempdir");
        let dir = updates.path().join("0.4.0");
        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join("db-modeling-linux-x86_64.tar.gz"), b"corrompido").expect("lixo");
        let transport = FakeTransport::new().with_body(ASSET_URL, BODY);
        let got = download_verified(
            &transport,
            &PLENTY,
            updates.path(),
            &version(),
            &asset(ASSET_URL, BODY),
        )
        .expect("ok");
        assert_eq!(fs::read(got.path).expect("ler"), BODY);
        assert_eq!(transport.requests().len(), 1);
    }

    #[test]
    fn nome_de_arquivo_inseguro_e_recusado() {
        for url in [
            "https://github.com/dbmodeling/dbml/releases/download/v1/",
            "https://github.com/dbmodeling/dbml/releases/download/v1/.hidden",
            "https://github.com/dbmodeling/dbml/releases/download/v1/a:b",
        ] {
            let parsed = url::Url::parse(url).expect("url");
            assert!(safe_file_name(&parsed).is_err(), "{url}");
        }
    }

    #[test]
    fn falha_de_rede_e_propagada_sem_devolver_arquivo() {
        let updates = tempfile::tempdir().expect("tempdir");
        let transport =
            FakeTransport::new().with_error(ASSET_URL, TransportError::Network("queda".into()));
        let error = download_verified(
            &transport,
            &PLENTY,
            updates.path(),
            &version(),
            &asset(ASSET_URL, BODY),
        )
        .expect_err("rede");
        assert!(matches!(error, FetchError::Transport(_)));
    }
}
