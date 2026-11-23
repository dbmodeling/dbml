#![deny(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
//! Núcleo puro do auto-update do App Desktop (modelo Zed).
//!
//! Sem GPUI e sem rede concreta: manifesto assinado, regra de versão, escolha de plataforma,
//! política de desativação, verificação (SHA-256 e minisign com duas chaves), máquina de
//! estados, trava entre instâncias e agendamento. A rede entra pela trait [`transport::Transport`].
//! Strings e números ficam em [`constants`].

pub mod constants;
pub mod lock;
pub mod manifest;
pub mod platform;
pub mod policy;
pub mod schedule;
pub mod state;
pub mod transport;
pub mod verify;
pub mod version;
