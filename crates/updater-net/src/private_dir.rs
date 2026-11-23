//! Diretório privado ao usuário para os downloads: `0700` em Unix (com checagem de dono e recusa
//! de symlinks) e ACL só do usuário (mais o `SYSTEM`) no Windows.

#[cfg(not(unix))]
use std::fs;
use std::io;
use std::path::Path;

/// Cria `dir` (o pai precisa existir) privado ao usuário atual e o força privado se já existia.
///
/// - Unix: delega a [`updater::lock::ensure_private_unix_dir`] (recusa symlinks, confere UID do
///   dono e força modo `0700`).
/// - Windows: DACL protegida (sem herdar do pai) que só concede acesso total ao dono do objeto
///   (`OW`, o usuário que o criou) e ao `SYSTEM`. **Não verificado em Windows** (ver
///   `windows_acl`).
///
/// # Errors
/// Erro de E/S ao criar o diretório ou ao aplicar a permissão.
pub fn ensure_private_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        updater::lock::ensure_private_unix_dir(dir)
    }
    #[cfg(not(unix))]
    {
        create_dir(dir)?;
        restrict(dir)
    }
}

#[cfg(not(unix))]
fn create_dir(dir: &Path) -> io::Result<()> {
    match fs::create_dir(dir) {
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        other => other,
    }
}

#[cfg(windows)]
fn restrict(dir: &Path) -> io::Result<()> {
    windows_acl::restrict_to_current_user(dir)
}

#[cfg(not(any(unix, windows)))]
fn restrict(_dir: &Path) -> io::Result<()> {
    Ok(())
}

/// ACL restrita ao usuário, por FFI mínima com `windows-sys`.
///
/// **NÃO VERIFICADO EM WINDOWS**: este módulo foi escrito sem compilador nem teste no Windows
/// (o desenvolvimento foi em Linux). O `windows-latest` do CI compila (e o teste abaixo roda), mas
/// a semântica da ACL só se confirma na verificação manual do ticket 03:
/// `icacls <cache>\db-modeling\updates` deve listar apenas o usuário (via dono) e `SYSTEM`, sem
/// herança.
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows_acl {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr;

    use crate::constants::DIR_ACL_SDDL;
    use windows_sys::Win32::Foundation::{ERROR_SUCCESS, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1, SE_FILE_OBJECT,
        SetNamedSecurityInfoW,
    };
    use windows_sys::Win32::Security::{
        ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl,
        PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
    };

    fn wide(text: &str) -> Vec<u16> {
        std::ffi::OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub(super) fn restrict_to_current_user(dir: &Path) -> io::Result<()> {
        let sddl = wide(DIR_ACL_SDDL);
        let path: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        // SAFETY: `sddl` termina em NUL e vive durante a chamada; `descriptor` é um ponteiro de
        // saída válido que recebe memória alocada pelo sistema (liberada com `LocalFree`).
        let converted = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            )
        };
        if converted == 0 {
            return Err(io::Error::last_os_error());
        }
        let result = apply(&path, descriptor);
        // SAFETY: `descriptor` veio de `ConvertStringSecurityDescriptor...` e não é usado depois.
        unsafe {
            LocalFree(descriptor);
        }
        result
    }

    fn apply(path: &[u16], descriptor: PSECURITY_DESCRIPTOR) -> io::Result<()> {
        let mut present = 0;
        let mut defaulted = 0;
        let mut dacl: *mut ACL = ptr::null_mut();
        // SAFETY: `descriptor` é válido (acabou de ser convertido) e os três ponteiros de saída
        // apontam para variáveis locais.
        let got = unsafe {
            GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted)
        };
        if got == 0 || present == 0 || dacl.is_null() {
            return Err(io::Error::other("DACL ausente no descritor de segurança"));
        }
        // SAFETY: `path` termina em NUL; `dacl` aponta para dentro de `descriptor`, ainda vivo.
        let status = unsafe {
            SetNamedSecurityInfoW(
                path.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                dacl,
                ptr::null(),
            )
        };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(
                i32::try_from(status).unwrap_or(i32::MAX),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cria_e_aceita_diretorio_ja_existente() {
        let root = tempfile::tempdir().expect("tempdir");
        let dir = root.path().join("privado");
        ensure_private_dir(&dir).expect("criar");
        ensure_private_dir(&dir).expect("idempotente");
        assert!(dir.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn modo_e_0700_mesmo_se_o_diretorio_existia_aberto() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().expect("tempdir");
        let dir = root.path().join("aberto");
        fs::create_dir(&dir).expect("criar");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("chmod");
        ensure_private_dir(&dir).expect("ensure");
        let mode = fs::metadata(&dir).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode, updater::constants::UPDATES_DIR_UNIX_MODE);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_e_recusado_como_diretorio_privado() {
        let root = tempfile::tempdir().expect("tempdir");
        let target = root.path().join("alvo");
        std::fs::create_dir(&target).expect("criar alvo");
        let link = root.path().join("link");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");
        let err = ensure_private_dir(&link).expect_err("symlink deve falhar");
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    }
}
