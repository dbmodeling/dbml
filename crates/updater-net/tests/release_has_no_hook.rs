//! Garante que o gancho de teste (`DBM_UPDATE_URL` e a chave pública de teste) só existe em debug
//! ou com a feature `harness`.
//!
//! Rodar também em release: `cargo test -p updater-net --release` (sem `harness`). O `release.yml`
//! NÃO pode compilar com a feature `harness` (verificação manual: `grep -n harness
//! .github/workflows/release.yml` não deve achar nada; este ticket não edita workflows).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

/// Nomes do gancho, **invertidos**: assim o literal do gancho não aparece neste binário de teste
/// (que também é escaneado) e o teste não se auto-denuncia.
const REVERSED_HOOK_NAMES: [&str; 2] = ["LRU_ETADPU_MBD", "YEK_CILBUP_TSET_ETADPU_MBD"];

fn unreversed(reversed: &str) -> Vec<u8> {
    reversed.chars().rev().collect::<String>().into_bytes()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Bytes do binário de teste. Os pontos de entrada `from_environment()` (os que o app usa) são
/// referenciados aqui, então qualquer gancho escrito neles entra no executável e é achado.
fn scanned_binary() -> Vec<u8> {
    let _ = std::hint::black_box(updater_net::ureq_transport::TransportConfig::from_environment());
    let _ = std::hint::black_box(updater_net::keys::KeySet::from_environment());
    fs::read(std::env::current_exe().expect("exe")).expect("ler o binário de teste")
}

#[test]
fn flag_de_compilacao_acompanha_o_cfg() {
    assert_eq!(
        updater_net::TEST_HOOKS_COMPILED,
        cfg!(any(debug_assertions, feature = "harness"))
    );
}

/// Em release sem `harness`, o binário não contém os nomes do gancho.
#[cfg(not(any(debug_assertions, feature = "harness")))]
#[test]
fn build_de_release_nao_expoe_o_gancho() {
    let binary = scanned_binary();
    for reversed in REVERSED_HOOK_NAMES {
        let name = unreversed(reversed);
        assert!(
            !contains(&binary, &name),
            "o gancho `{}` não pode existir em release",
            String::from_utf8_lossy(&name)
        );
    }
    assert!(!updater_net::TEST_HOOKS_COMPILED);
}

/// Contraprova do método: com o gancho compilado, o binário **contém** os nomes (senão o teste
/// de release acima não provaria nada).
#[cfg(any(debug_assertions, feature = "harness"))]
#[test]
fn contraprova_o_scan_acha_o_gancho_quando_ele_existe() {
    let binary = scanned_binary();
    for reversed in REVERSED_HOOK_NAMES {
        let name = unreversed(reversed);
        assert!(
            contains(&binary, &name),
            "o scan deveria achar `{}` neste build",
            String::from_utf8_lossy(&name)
        );
    }
}
