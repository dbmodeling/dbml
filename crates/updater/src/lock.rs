//! Trava de arquivo exclusiva entre instâncias do app (`<cache>/db-modeling/updates/lock`).
//!
//! O app aceita várias instâncias; só quem obtém a trava atualiza naquela rodada. A trava é do
//! sistema operacional (`File::try_lock`), então some sozinha se o processo morrer.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

use crate::constants::{CACHE_APP_DIR, LOCK_FILE_NAME, UPDATES_DIR};

/// Diretório de updates (downloads, temporários e a trava) sob o cache do usuário.
#[must_use]
pub fn updates_dir(cache_dir: &Path) -> PathBuf {
    cache_dir.join(CACHE_APP_DIR).join(UPDATES_DIR)
}

/// Caminho do arquivo de trava.
#[must_use]
pub fn lock_path(cache_dir: &Path) -> PathBuf {
    updates_dir(cache_dir).join(LOCK_FILE_NAME)
}

/// Cria o diretório de updates, privado ao usuário.
///
/// Em Unix, só o diretório final recebe `0700` (forçado com `set_permissions`, inclusive se já
/// existia com outro modo), não pode ser um symlink e o dono precisa ser o usuário do processo;
/// pasta de outro usuário é erro. Os pais (`<cache>` e `db-modeling`) mantêm o modo padrão.
/// No Windows, a DACL restrita é aplicada por `updater_net::private_dir::ensure_private_dir`.
///
/// # Errors
/// Erro de E/S ao criar o diretório, ou `PermissionDenied` se for symlink ou pertencer a outro usuário.
pub fn ensure_updates_dir(cache_dir: &Path) -> io::Result<PathBuf> {
    let dir = updates_dir(cache_dir);
    if let Some(parent) = dir.parent() {
        fs::create_dir_all(parent)?;
    }
    ensure_private_unix_dir(&dir)?;
    Ok(dir)
}

/// Cria `dir` (o pai precisa existir) privado ao usuário atual e o força privado se já existia.
///
/// - Unix: recusa symlinks, confere o UID do dono e força modo `0700`.
/// - Windows: DACL protegida (sem herdar do pai) que só concede acesso total ao dono do objeto
///   (`OW`, o usuário que o criou) e ao `SYSTEM`.
///
/// # Errors
/// Erro de E/S ao criar o diretório, ou `PermissionDenied` se for symlink ou pertencer a outro usuário.
pub fn ensure_private_unix_dir(dir: &Path) -> io::Result<()> {
    match fs::create_dir(dir) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    #[cfg(unix)]
    unix_private::make_private(dir)?;
    #[cfg(windows)]
    windows_acl::restrict_to_current_user(dir)?;
    Ok(())
}

/// ACL restrita ao usuário no Windows (`D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)`), por FFI com `windows-sys`.
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
                &raw mut descriptor,
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
            GetSecurityDescriptorDacl(
                descriptor,
                &raw mut present,
                &raw mut dacl,
                &raw mut defaulted,
            )
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

#[cfg(unix)]
mod unix_private {
    use std::fs::{self, OpenOptions, Permissions};
    use std::io;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::Path;

    use crate::constants::{OWNER_PROBE_PREFIX, UPDATES_DIR_UNIX_MODE};

    /// Recusa symlinks, confere o dono e força o modo `0700` no diretório final.
    pub(super) fn make_private(dir: &Path) -> io::Result<()> {
        let meta = fs::symlink_metadata(dir)?;
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "diretório privado não pode ser symlink",
            ));
        }
        let own_uid = effective_uid(dir)?;
        check_owner(meta.uid(), own_uid)?;
        fs::set_permissions(dir, Permissions::from_mode(UPDATES_DIR_UNIX_MODE))
    }

    /// uid efetivo do processo: dono de um arquivo recém-criado por ele (sem depender de `libc`).
    fn effective_uid(dir: &Path) -> io::Result<u32> {
        let probe = dir.join(format!("{OWNER_PROBE_PREFIX}{}", std::process::id()));
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&probe)?;
        let uid = file.metadata().map(|meta| meta.uid());
        drop(file);
        let _ = fs::remove_file(&probe);
        uid
    }

    /// O dono do diretório precisa ser o usuário do processo.
    pub(super) fn check_owner(dir_uid: u32, own_uid: u32) -> io::Result<()> {
        if dir_uid == own_uid {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("diretório de updates pertence a outro usuário (uid {dir_uid})"),
            ))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn dono_diferente_do_processo_e_recusado() {
            assert!(check_owner(1000, 1000).is_ok());
            let error = check_owner(0, 1000).expect_err("outro dono");
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        }
    }
}

/// Trava obtida; é liberada ao descartar (`Drop`) ou quando o processo termina.
#[derive(Debug)]
pub struct UpdateLock {
    _file: File,
    updates_dir: PathBuf,
}

impl UpdateLock {
    /// Tenta obter a trava sem bloquear.
    ///
    /// Retorna `Ok(None)` se **outra instância** já a detém: quem não obtém a trava pula a
    /// rodada de atualização.
    ///
    /// # Errors
    /// Erro de E/S ao criar o diretório/arquivo ou ao travar por outro motivo que não "ocupado".
    pub fn try_acquire(cache_dir: &Path) -> io::Result<Option<Self>> {
        let updates_dir = ensure_updates_dir(cache_dir)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(lock_path(cache_dir))?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self {
                _file: file,
                updates_dir,
            })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => Err(error),
        }
    }

    /// Diretório de updates no cache protegido por esta trava (nunca a `updates\` do Windows
    /// em `{app}`). A limpeza de órfãos só age sobre ele.
    #[must_use]
    pub fn updates_dir(&self) -> &Path {
        &self.updates_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caminho_segue_o_layout_do_spec() {
        let path = lock_path(Path::new("/cache"));
        assert_eq!(path, Path::new("/cache/db-modeling/updates/lock"));
    }

    #[test]
    fn primeira_instancia_obtem_a_trava() {
        let cache = tempfile::tempdir().expect("tempdir");
        let lock = UpdateLock::try_acquire(cache.path()).expect("io");
        assert!(lock.is_some());
        assert!(lock_path(cache.path()).exists());
    }

    #[test]
    fn segunda_instancia_nao_obtem_a_trava_e_pula_a_rodada() {
        let cache = tempfile::tempdir().expect("tempdir");
        let first = UpdateLock::try_acquire(cache.path()).expect("io");
        assert!(first.is_some());
        let second = UpdateLock::try_acquire(cache.path()).expect("io");
        assert!(
            second.is_none(),
            "trava exclusiva: segunda instância não obtém"
        );
    }

    #[test]
    fn trava_e_liberada_ao_descartar() {
        let cache = tempfile::tempdir().expect("tempdir");
        let first = UpdateLock::try_acquire(cache.path()).expect("io");
        drop(first);
        assert!(UpdateLock::try_acquire(cache.path()).expect("io").is_some());
    }

    #[cfg(unix)]
    #[test]
    fn diretorio_existente_com_modo_aberto_e_forcado_a_0700() {
        use std::os::unix::fs::PermissionsExt;
        let cache = tempfile::tempdir().expect("tempdir");
        let dir = updates_dir(cache.path());
        fs::create_dir_all(&dir).expect("criar");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("chmod");
        ensure_updates_dir(cache.path()).expect("ensure");
        let mode = fs::metadata(&dir).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode, crate::constants::UPDATES_DIR_UNIX_MODE);
    }

    #[cfg(unix)]
    #[test]
    fn nao_sobra_arquivo_de_sonda_no_diretorio() {
        let cache = tempfile::tempdir().expect("tempdir");
        ensure_updates_dir(cache.path()).expect("ensure");
        assert_eq!(
            fs::read_dir(updates_dir(cache.path())).expect("ls").count(),
            0
        );
    }

    #[cfg(unix)]
    #[test]
    fn diretorio_de_updates_e_privado_em_unix() {
        use std::os::unix::fs::PermissionsExt;
        let cache = tempfile::tempdir().expect("tempdir");
        let dir = ensure_updates_dir(cache.path()).expect("criar");
        let mode = fs::metadata(&dir).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode, crate::constants::UPDATES_DIR_UNIX_MODE);
    }
}
