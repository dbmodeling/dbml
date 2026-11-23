//! Manifesto gerado por `scripts/generate-update-manifest.sh` passa pelo parser REAL do cliente.
//!
//! As fixtures em `tests/fixtures/` são a saída do script para o `release-dist` fictício de
//! `scripts/tests/update_manifest_test.sh` (que confere, byte a byte, que o script ainda gera o
//! mesmo texto). Aqui validamos o outro lado: `Manifest::parse` + `asset_for` nas 3 plataformas.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use updater::constants::{RELEASES_URL_PREFIX, SHA256_HEX_LEN, SUPPORTED_MANIFEST_SCHEMA};
use updater::manifest::Manifest;
use updater::platform::PlatformKey;

const FIXTURE_SIGNED_MACOS: &str = include_str!("fixtures/latest.json");
const FIXTURE_UNSIGNED_MACOS: &str = include_str!("fixtures/latest-unsigned-macos.json");
const FIXTURE_VERSION: &str = "0.4.0";
const FIXTURE_TAG: &str = "v0.4.0";
const FIXTURE_TEAM_ID: &str = "ABCDE12345";
const NOTES_URL: &str = "https://github.com/dbmodeling/dbml/releases/tag/v0.4.0";
const ASSET_WINDOWS: &str = "db-modeling-windows-setup.exe";
const ASSET_MACOS: &str = "db-modeling-macos-arm64.dmg";
const ASSET_LINUX: &str = "db-modeling-linux-x86_64.tar.gz";

fn parse(text: &str) -> Manifest {
    Manifest::parse(text.as_bytes())
        .expect("o manifesto gerado precisa ser JSON válido")
        .expect("o manifesto gerado precisa ser schema suportado")
}

fn expected_url(asset: &str) -> String {
    format!("{RELEASES_URL_PREFIX}download/{FIXTURE_TAG}/{asset}")
}

#[test]
fn manifesto_gerado_e_aceito_pelo_parser_do_cliente() {
    let manifest = parse(FIXTURE_SIGNED_MACOS);
    assert_eq!(manifest.schema, SUPPORTED_MANIFEST_SCHEMA);
    assert_eq!(manifest.version, FIXTURE_VERSION);
    assert_eq!(manifest.notes_url, NOTES_URL);
    assert_eq!(manifest.platforms.len(), 3);
}

#[test]
fn asset_for_aceita_as_tres_plataformas_com_url_e_sha_do_script() {
    let manifest = parse(FIXTURE_SIGNED_MACOS);
    for (platform, asset) in [
        (PlatformKey::WindowsX86_64, ASSET_WINDOWS),
        (PlatformKey::MacosAarch64, ASSET_MACOS),
        (PlatformKey::LinuxX86_64, ASSET_LINUX),
    ] {
        let found = manifest
            .asset_for(platform)
            .unwrap_or_else(|e| panic!("{} recusada: {e}", platform.as_str()));
        assert_eq!(found.url, expected_url(asset));
        assert_eq!(found.sha256.len(), SHA256_HEX_LEN);
    }
}

#[test]
fn team_id_so_existe_no_macos_do_build_assinado() {
    let signed = parse(FIXTURE_SIGNED_MACOS);
    let mac = signed.asset_for(PlatformKey::MacosAarch64).unwrap();
    assert_eq!(mac.team_id.as_deref(), Some(FIXTURE_TEAM_ID));
    for platform in [PlatformKey::WindowsX86_64, PlatformKey::LinuxX86_64] {
        assert_eq!(signed.asset_for(platform).unwrap().team_id, None);
    }

    let unsigned = parse(FIXTURE_UNSIGNED_MACOS);
    for platform in [
        PlatformKey::WindowsX86_64,
        PlatformKey::MacosAarch64,
        PlatformKey::LinuxX86_64,
    ] {
        let found = unsigned.asset_for(platform).unwrap();
        assert_eq!(found.team_id, None, "{}", platform.as_str());
    }
}

/// `latest.json.sig` da fixture foi gerada por `rsign sign` padrão (prehashed, rsign2 0.6.7) com
/// um par descartável: a chave privada foi apagada e só a pública está em `tests/fixtures/`.
/// Garante que o formato que o workflow produz passa no verificador do cliente
/// (`allow_legacy = false`). O gancho de chave injetável só existe em teste/debug.
#[cfg(debug_assertions)]
#[test]
fn assinatura_prehashed_do_rsign_passa_no_verificador_do_cliente() {
    use updater::verify::{VerifyError, verify_signature_with_keys};

    const SIGNATURE: &str = include_str!("fixtures/latest.json.sig");
    const TEST_PUBLIC_KEY: &str = include_str!("fixtures/test-public-key.txt");

    let key = TEST_PUBLIC_KEY.trim();
    verify_signature_with_keys(FIXTURE_SIGNED_MACOS.as_bytes(), SIGNATURE, &[key])
        .expect("a assinatura do rsign precisa validar");

    let tampered = FIXTURE_SIGNED_MACOS.replace(FIXTURE_VERSION, "9.9.9");
    assert!(matches!(
        verify_signature_with_keys(tampered.as_bytes(), SIGNATURE, &[key]),
        Err(VerifyError::BadSignature)
    ));
}
