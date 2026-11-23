//! Escolha da chave de plataforma do manifesto por SO e arquitetura.

use crate::constants::{
    ARCH_AARCH64, ARCH_X86_64, OS_LINUX, OS_MACOS, OS_WINDOWS, PLATFORM_LINUX_X86_64,
    PLATFORM_LINUX_X86_64_DEB, PLATFORM_LINUX_X86_64_RPM, PLATFORM_MACOS_AARCH64,
    PLATFORM_WINDOWS_X86_64,
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

/// Formato de pacote do sistema com o qual o app foi instalado no Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageFormat {
    /// `.deb` (Debian/Ubuntu).
    Deb,
    /// `.rpm` (Fedora).
    Rpm,
}

/// Plataforma com artefato de atualização publicado no manifesto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlatformKey {
    /// `windows-x86_64`.
    WindowsX86_64,
    /// `macos-aarch64`.
    MacosAarch64,
    /// `linux-x86_64` (tarball / `install.sh`).
    LinuxX86_64,
    /// `linux-x86_64-deb` (pacote `.deb`, Debian/Ubuntu).
    LinuxX86_64Deb,
    /// `linux-x86_64-rpm` (pacote `.rpm`, Fedora).
    LinuxX86_64Rpm,
}

impl PlatformKey {
    /// Plataforma desta compilação (`None` em combinações sem artefato, como macOS Intel). No
    /// Linux devolve o tarball; quem sabe como o app foi instalado (`.deb`/`.rpm`) escolhe a
    /// variante com [`PlatformKey::for_package`].
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

    /// Variante de pacote da mesma plataforma: `Deb`/`Rpm` só existem em Linux `x86_64`; as demais
    /// chaves não têm variante e voltam como estão.
    #[must_use]
    pub fn for_package(self, package: PackageFormat) -> Self {
        match (self, package) {
            (Self::LinuxX86_64, PackageFormat::Deb) => Self::LinuxX86_64Deb,
            (Self::LinuxX86_64, PackageFormat::Rpm) => Self::LinuxX86_64Rpm,
            (other, _) => other,
        }
    }

    /// Chave textual usada em `platforms` do manifesto.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WindowsX86_64 => PLATFORM_WINDOWS_X86_64,
            Self::MacosAarch64 => PLATFORM_MACOS_AARCH64,
            Self::LinuxX86_64 => PLATFORM_LINUX_X86_64,
            Self::LinuxX86_64Deb => PLATFORM_LINUX_X86_64_DEB,
            Self::LinuxX86_64Rpm => PLATFORM_LINUX_X86_64_RPM,
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
        assert_eq!(PlatformKey::LinuxX86_64Deb.as_str(), "linux-x86_64-deb");
        assert_eq!(PlatformKey::LinuxX86_64Rpm.as_str(), "linux-x86_64-rpm");
    }

    #[test]
    fn pacote_escolhe_a_variante_linux() {
        let linux = PlatformKey::LinuxX86_64;
        assert_eq!(
            linux.for_package(PackageFormat::Deb),
            PlatformKey::LinuxX86_64Deb
        );
        assert_eq!(
            linux.for_package(PackageFormat::Rpm),
            PlatformKey::LinuxX86_64Rpm
        );
        // fora do Linux não há variante
        assert_eq!(
            PlatformKey::WindowsX86_64.for_package(PackageFormat::Deb),
            PlatformKey::WindowsX86_64
        );
        assert_eq!(
            PlatformKey::MacosAarch64.for_package(PackageFormat::Rpm),
            PlatformKey::MacosAarch64
        );
    }

    #[test]
    fn so_atual_e_reconhecido_pelo_nome() {
        assert_eq!(TargetOs::from_name("windows"), Some(TargetOs::Windows));
        assert_eq!(TargetOs::from_name("macos"), Some(TargetOs::MacOs));
        assert_eq!(TargetOs::from_name("linux"), Some(TargetOs::Linux));
        assert_eq!(TargetOs::from_name("plan9"), None);
    }
}
