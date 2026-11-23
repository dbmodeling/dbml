//! Diretório privado ao usuário para os downloads: `0700` em Unix (com checagem de dono e recusa
//! de symlinks) e ACL só do usuário (mais o `SYSTEM`) no Windows.

use std::io;
use std::path::Path;

/// Cria `dir` (o pai precisa existir) privado ao usuário atual e o força privado se já existia.
///
/// Delega a [`updater::lock::ensure_private_unix_dir`], que aplica `0700` (e verifica dono/symlink)
/// em Unix e a DACL restrita ao dono + `SYSTEM` no Windows.
///
/// # Errors
/// Erro de E/S ao criar o diretório ou ao aplicar a permissão.
pub fn ensure_private_dir(dir: &Path) -> io::Result<()> {
    updater::lock::ensure_private_unix_dir(dir)
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
