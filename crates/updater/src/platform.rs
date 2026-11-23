//! Escolha da chave de plataforma do manifesto por SO e arquitetura.

use crate::constants::{
    ARCH_AARCH64, ARCH_X86_64, OS_LINUX, OS_MACOS, OS_WINDOWS, PLATFORM_LINUX_X86_64,
    PLATFORM_MACOS_AARCH64, PLATFORM_WINDOWS_X86_64,
};

/// Sistema operacional suportado pelo updater (usado também pela política).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOs {
    /// Windows.
    Windows,
    /// macOS.
    MacOs,
    /// Linux.
    Linux,
}

impl TargetOs {
    /// SO desta compilação; `None` se não for um dos suportados.
    #[must_use]
    pub fn current() -> Option<Self> {
        Self::from_name(std::env::consts::OS)
    }

    /// Converte o nome de `std::env::consts::OS`.
    #[must_use]
    pub fn from_name(os: &str) -> Option<Self> {
        match os {
            OS_WINDOWS => Some(Self::Windows),
            OS_MACOS => Some(Self::MacOs),
            OS_LINUX => Some(Self::Linux),
            _ => None,
        }
    }
}

/// Plataforma com artefato de atualização publicado no manifesto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlatformKey {
    /// `windows-x86_64`.
    WindowsX86_64,
    /// `macos-aarch64`.
    MacosAarch64,
    /// `linux-x86_64`.
    LinuxX86_64,
}

impl PlatformKey {
    /// Plataforma desta compilação (`None` em combinações sem artefato, como macOS Intel).
    #[must_use]
    pub fn current() -> Option<Self> {
        Self::detect(std::env::consts::OS, std::env::consts::ARCH)
    }

    /// Escolhe a chave por nomes de SO/arquitetura; injetável para teste.
    #[must_use]
    pub fn detect(os: &str, arch: &str) -> Option<Self> {
        match (os, arch) {
            (OS_WINDOWS, ARCH_X86_64) => Some(Self::WindowsX86_64),
            (OS_MACOS, ARCH_AARCH64) => Some(Self::MacosAarch64),
            (OS_LINUX, ARCH_X86_64) => Some(Self::LinuxX86_64),
            _ => None,
        }
    }

    /// Chave textual usada em `platforms` do manifesto.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WindowsX86_64 => PLATFORM_WINDOWS_X86_64,
            Self::MacosAarch64 => PLATFORM_MACOS_AARCH64,
            Self::LinuxX86_64 => PLATFORM_LINUX_X86_64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escolhe_chave_para_cada_plataforma_suportada() {
        assert_eq!(
            PlatformKey::detect("windows", "x86_64"),
            Some(PlatformKey::WindowsX86_64)
        );
        assert_eq!(
            PlatformKey::detect("macos", "aarch64"),
            Some(PlatformKey::MacosAarch64)
        );
        assert_eq!(
            PlatformKey::detect("linux", "x86_64"),
            Some(PlatformKey::LinuxX86_64)
        );
    }

    #[test]
    fn combinacoes_fora_do_escopo_nao_tem_chave() {
        assert_eq!(PlatformKey::detect("macos", "x86_64"), None);
        assert_eq!(PlatformKey::detect("linux", "aarch64"), None);
        assert_eq!(PlatformKey::detect("windows", "aarch64"), None);
        assert_eq!(PlatformKey::detect("freebsd", "x86_64"), None);
    }

    #[test]
    fn chaves_textuais_batem_com_o_manifesto() {
        assert_eq!(PlatformKey::WindowsX86_64.as_str(), "windows-x86_64");
        assert_eq!(PlatformKey::MacosAarch64.as_str(), "macos-aarch64");
        assert_eq!(PlatformKey::LinuxX86_64.as_str(), "linux-x86_64");
    }

    #[test]
    fn so_atual_e_reconhecido_pelo_nome() {
        assert_eq!(TargetOs::from_name("windows"), Some(TargetOs::Windows));
        assert_eq!(TargetOs::from_name("macos"), Some(TargetOs::MacOs));
        assert_eq!(TargetOs::from_name("linux"), Some(TargetOs::Linux));
        assert_eq!(TargetOs::from_name("plan9"), None);
    }
}
