//! Verificação de integridade: SHA-256 e assinatura minisign com duas chaves aceitas.

use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use minisign_verify::{PublicKey, Signature};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::constants::{HASH_READ_BUFFER_BYTES, HEX_DIGITS_PER_BYTE, PUBLIC_KEYS};

/// Erros de verificação.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum VerifyError {
    /// SHA-256 calculado difere do esperado.
    #[error("SHA-256 diverge: esperado {expected}, obtido {actual}")]
    HashMismatch {
        /// Hash esperado (do manifesto).
        expected: String,
        /// Hash calculado.
        actual: String,
    },
    /// O texto da assinatura não é uma assinatura minisign válida.
    #[error("assinatura minisign malformada")]
    MalformedSignature,
    /// Uma chave pública embutida não pôde ser interpretada (bug de configuração).
    #[error("chave pública embutida inválida")]
    InvalidPublicKey,
    /// Nenhuma das chaves aceitas validou a assinatura.
    #[error("assinatura inválida para todas as chaves públicas aceitas")]
    BadSignature,
}

/// SHA-256 dos bytes, em hexadecimal minúsculo.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    to_hex(&Sha256::digest(bytes))
}

/// SHA-256 de um leitor (em blocos, sem carregar tudo na memória).
///
/// # Errors
/// Erro de E/S na leitura.
pub fn sha256_hex_reader<R: Read>(mut reader: R) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; HASH_READ_BUFFER_BYTES];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(to_hex(&hasher.finalize()))
}

/// SHA-256 de um arquivo. Recalcule imediatamente antes de montar, extrair ou executar.
///
/// # Errors
/// Erro de E/S ao abrir ou ler o arquivo.
pub fn sha256_hex_file(path: &Path) -> io::Result<String> {
    sha256_hex_reader(File::open(path)?)
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(
        String::with_capacity(bytes.len() * HEX_DIGITS_PER_BYTE),
        |mut out, byte| {
            // `write!` em `String` nunca falha.
            let _ = write!(out, "{byte:02x}");
            out
        },
    )
}

/// Confere o hash calculado contra o esperado (hexadecimal, sem diferenciar caixa).
///
/// # Errors
/// [`VerifyError::HashMismatch`] se divergirem.
pub fn verify_sha256(actual_hex: &str, expected_hex: &str) -> Result<(), VerifyError> {
    if actual_hex.eq_ignore_ascii_case(expected_hex) {
        Ok(())
    } else {
        Err(VerifyError::HashMismatch {
            expected: expected_hex.to_lowercase(),
            actual: actual_hex.to_lowercase(),
        })
    }
}

/// Verifica `data` contra a assinatura minisign (texto completo do `.sig`) aceitando **qualquer
/// uma** das duas chaves públicas embutidas (primária ou reserva).
///
/// # Errors
/// [`VerifyError`]; [`VerifyError::BadSignature`] se nenhuma chave validar.
pub fn verify_signature(data: &[u8], signature: &str) -> Result<(), VerifyError> {
    verify_with_keys(data, signature, &PUBLIC_KEYS)
}

/// Como [`verify_signature`], com a lista de chaves (base64) explícita. Só existe em testes,
/// debug ou com a feature `harness`; **nunca** em release.
///
/// # Errors
/// [`VerifyError`]; [`VerifyError::BadSignature`] se nenhuma chave validar.
#[cfg(any(test, debug_assertions, feature = "harness"))]
pub fn verify_signature_with_keys(
    data: &[u8],
    signature: &str,
    public_keys: &[&str],
) -> Result<(), VerifyError> {
    verify_with_keys(data, signature, public_keys)
}

/// Uma chave que não interpreta é ignorada (`continue`) e a próxima é tentada: perder a
/// primária por defeito não pode impedir a reserva. Se **nenhuma** chave interpreta, o erro é
/// [`VerifyError::InvalidPublicKey`].
fn verify_with_keys(data: &[u8], signature: &str, public_keys: &[&str]) -> Result<(), VerifyError> {
    let signature = Signature::decode(signature).map_err(|_| VerifyError::MalformedSignature)?;
    let mut any_valid_key = false;
    for key in public_keys {
        let Ok(key) = PublicKey::from_base64(key) else {
            continue;
        };
        any_valid_key = true;
        if key.verify(data, &signature, false).is_ok() {
            return Ok(());
        }
    }
    Err(if any_valid_key {
        VerifyError::BadSignature
    } else {
        VerifyError::InvalidPublicKey
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{PUBLIC_KEY_PRIMARY, PUBLIC_KEY_RESERVE};

    // ATENÇÃO: pares de chaves descartáveis, gerados só para TESTE (nunca usados em produção).
    // As chaves privadas não foram guardadas; só as públicas e as assinaturas ficam aqui.
    /// Mensagem assinada nas fixtures.
    const MESSAGE: &[u8] = b"{\"schema\":1,\"version\":\"9.9.9\"}\n";
    /// TESTE: "primária" de teste A.
    const TEST_KEY_A: &str = "RWQDQ6vBpUsQVYCrbxiszASO2ILfueAtMPwRmGdPa3f2ZztPBWM6YGsH";
    /// TESTE: "reserva" de teste B.
    const TEST_KEY_B: &str = "RWTPapS9Jv/BGWqqdILpAoHysdbpAuUq65FB74aVXDokBmDXxRYdgNZM";
    /// TESTE: assinatura da MESSAGE pela chave A.
    const SIG_BY_A: &str = "untrusted comment: signature from rsign secret key\nRUQDQ6vBpUsQVeFf/xHpFqSo2qfPDg4Xngfq+dQ3fgAbg6HW0l3GyOlL38ml7cG3q2Mv6Oi4HelkyNeUxPd65d8lLS+MXiycIgk=\ntrusted comment: TESTE\neJbbSwLWEPJmv9MeUkQfOM+xrwj+4p2rndwjDFWrtPVjBqKyTEzKfYQwvX2Devp2khYBxny5qxKwVjjfhZ9+Cg==\n";
    /// TESTE: assinatura da MESSAGE pela chave B.
    const SIG_BY_B: &str = "untrusted comment: signature from rsign secret key\nRUTPapS9Jv/BGaHSeBqQA5VfOgX6UXpLt8MRdMU3R/OcXVYiKtSv6VePACpjsaiOoXMJA+DRPGV/Ugl1zmEd+0r6mW5lePHjDQY=\ntrusted comment: TESTE\n5ySYCgpIvnOgZ23Xf0cTN2xwBL8SUZ08MAKMgbvxGu+Iaeys4If5CHaJ7/0pmqotZvT5By3ZFJAPlTCH6z9OCw==\n";
    /// TESTE: assinatura da MESSAGE por uma terceira chave C, fora da lista aceita.
    const SIG_BY_C: &str = "untrusted comment: signature from rsign secret key\nRUSE5aX83W5+VzWlgESOKiisA5V7qzbKh32hbJo5VFaKn382N5A6+X1PhVMJpgArTG1ssFfnuWoUlGBX16cXfGitsv5u+7XukwQ=\ntrusted comment: TESTE\nIg7lr2Ig4cxyYYUEa0uCEonTeJoxbyqXZhoZo+RDrCgGLkHkcAl1daOWEhPzgVrKuPjoVdq1s//wNSj9BWO8CA==\n";

    const KEYS: [&str; 2] = [TEST_KEY_A, TEST_KEY_B];

    #[test]
    fn assinatura_valida_pela_primaria_e_aceita() {
        assert_eq!(verify_signature_with_keys(MESSAGE, SIG_BY_A, &KEYS), Ok(()));
    }

    #[test]
    fn assinatura_valida_so_pela_reserva_tambem_e_aceita() {
        // A chave B é a única da lista que valida esta assinatura.
        assert_eq!(
            verify_signature_with_keys(MESSAGE, SIG_BY_B, &[TEST_KEY_A, TEST_KEY_B]),
            Ok(())
        );
        assert_eq!(
            verify_signature_with_keys(MESSAGE, SIG_BY_B, &[TEST_KEY_A]),
            Err(VerifyError::BadSignature),
            "sem a reserva na lista, a assinatura da reserva não vale"
        );
    }

    #[test]
    fn chave_invalida_na_lista_nao_aborta_e_a_proxima_e_tentada() {
        let invalid = "nao-e-uma-chave";
        assert_eq!(
            verify_signature_with_keys(MESSAGE, SIG_BY_B, &[invalid, TEST_KEY_B]),
            Ok(()),
            "primária inválida: a reserva ainda valida"
        );
        assert_eq!(
            verify_signature_with_keys(MESSAGE, SIG_BY_C, &[invalid, TEST_KEY_B]),
            Err(VerifyError::BadSignature)
        );
        assert_eq!(
            verify_signature_with_keys(MESSAGE, SIG_BY_A, &[invalid, invalid]),
            Err(VerifyError::InvalidPublicKey)
        );
    }

    #[test]
    fn assinatura_de_chave_desconhecida_e_recusada_pelas_duas() {
        assert_eq!(
            verify_signature_with_keys(MESSAGE, SIG_BY_C, &KEYS),
            Err(VerifyError::BadSignature)
        );
    }

    #[test]
    fn manifesto_adulterado_e_recusado() {
        let tampered = b"{\"schema\":1,\"version\":\"9.9.8\"}\n";
        assert_eq!(
            verify_signature_with_keys(tampered, SIG_BY_A, &KEYS),
            Err(VerifyError::BadSignature)
        );
        assert_eq!(
            verify_signature_with_keys(tampered, SIG_BY_B, &KEYS),
            Err(VerifyError::BadSignature)
        );
    }

    #[test]
    fn assinatura_malformada_e_recusada() {
        assert_eq!(
            verify_signature_with_keys(MESSAGE, "lixo", &KEYS),
            Err(VerifyError::MalformedSignature)
        );
    }

    #[test]
    fn chaves_de_teste_nao_validam_com_as_chaves_embutidas_de_producao() {
        // `verify_signature` usa as duas chaves reais: uma assinatura de teste não passa.
        assert_eq!(
            verify_signature(MESSAGE, SIG_BY_A),
            Err(VerifyError::BadSignature)
        );
        assert_eq!(
            verify_signature(MESSAGE, SIG_BY_B),
            Err(VerifyError::BadSignature)
        );
    }

    #[test]
    fn chaves_embutidas_sao_interpretaveis_e_distintas() {
        assert_ne!(PUBLIC_KEY_PRIMARY, PUBLIC_KEY_RESERVE);
        for key in PUBLIC_KEYS {
            assert!(PublicKey::from_base64(key).is_ok(), "chave inválida: {key}");
        }
    }

    #[test]
    fn sha256_de_vetor_conhecido() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_de_leitor_e_arquivo_batem_com_o_de_bytes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("artefato.bin");
        let data = vec![7u8; HASH_READ_BUFFER_BYTES * 2 + 13];
        std::fs::write(&path, &data).expect("escrever");
        let expected = sha256_hex(&data);
        assert_eq!(sha256_hex_file(&path).expect("arquivo"), expected);
        assert_eq!(sha256_hex_reader(&data[..]).expect("leitor"), expected);
    }

    #[test]
    fn verify_sha256_ignora_caixa_e_recusa_divergencia() {
        let hash = sha256_hex(b"abc");
        assert_eq!(verify_sha256(&hash, &hash.to_uppercase()), Ok(()));
        assert!(matches!(
            verify_sha256(&hash, &sha256_hex(b"abd")),
            Err(VerifyError::HashMismatch { .. })
        ));
    }
}
