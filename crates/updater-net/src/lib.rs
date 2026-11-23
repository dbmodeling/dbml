#![deny(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
//! Rede, download e verificação do auto-update do App Desktop.
//!
//! Camada impura sobre a crate pura [`updater`]: [`ureq_transport::UreqTransport`] implementa
//! [`updater::transport::Transport`] com `ureq` + `rustls`, e [`round::run_round`] orquestra uma
//! rodada (política → trava → manifesto assinado → regra de versão → download → SHA-256) e termina
//! entregando [`round::VerifiedUpdate`] aos instaladores de cada plataforma (tickets 05–07).
//!
//! O **gancho de teste** (`DBM_UPDATE_URL`, chave pública de teste, origem local) só é compilado
//! em debug ou com a feature `harness`; nunca em release (ver [`TEST_HOOKS_COMPILED`]).

pub mod constants;
pub mod etag;
pub mod fetch;
pub mod keys;
pub mod private_dir;
pub mod round;
pub mod ureq_transport;
pub mod url_policy;

/// `true` se o gancho de teste foi compilado nesta build (debug ou feature `harness`).
/// Em release precisa ser `false`; o teste `tests/release_has_no_hook.rs` confere o binário.
pub const TEST_HOOKS_COMPILED: bool = cfg!(any(debug_assertions, feature = "harness"));
