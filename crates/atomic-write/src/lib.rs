#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Escrita atômica de arquivos compartilhada por Desktop e CLI Web.
//!
//! [`atomic_write`] grava num arquivo temporário vizinho e faz `rename` sobre o destino, mas
//! sem os efeitos colaterais da versão ingênua: o link simbólico é resolvido (o alvo é
//! atualizado, o link continua link), as permissões do arquivo original são copiadas para o
//! temporário e, quando o `rename` não é possível (diretório sem permissão de escrita, destino
//! em mount especial), a gravação cai para escrita direta no próprio arquivo.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Máximo de saltos ao seguir uma cadeia de links simbólicos.
const MAX_SYMLINK_HOPS: usize = 40;

/// Grava `bytes` em `path` de forma atômica, criando diretórios ausentes.
///
/// - Link simbólico em `path`: o destino final do link é que é substituído.
/// - Arquivo existente: suas permissões são preservadas no novo conteúdo.
/// - Se criar o temporário ou renomear falhar e `path` já for um arquivo comum gravável,
///   o conteúdo é escrito diretamente nele (sem atomicidade, mas sem perder o arquivo).
///
/// Limitação conhecida: o `rename` cria um novo inode, então o dono do arquivo (quando
/// diferente do usuário atual) e hard links existentes não são preservados no caminho atômico.
///
/// # Errors
///
/// Devolve o erro de E/S se nem a gravação atômica nem o fallback direto funcionarem.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let target = resolve_symlinks(path)?;
    let parent = match target.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    fs::create_dir_all(&parent)?;
    let existing = fs::metadata(&target).ok().filter(fs::Metadata::is_file);
    match write_via_temp(&target, &parent, bytes, existing.as_ref()) {
        Ok(()) => Ok(()),
        Err(err) if existing.is_some() => write_in_place(&target, bytes).map_err(|_| err),
        Err(err) => Err(err),
    }
}

/// Segue links simbólicos até o arquivo final; link pendente resolve para o alvo ainda inexistente.
fn resolve_symlinks(path: &Path) -> io::Result<PathBuf> {
    let mut current = path.to_path_buf();
    for _ in 0..MAX_SYMLINK_HOPS {
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let link = fs::read_link(&current)?;
                current = if link.is_absolute() {
                    link
                } else {
                    current.parent().unwrap_or_else(|| Path::new("")).join(link)
                };
            }
            _ => return Ok(current),
        }
    }
    Err(io::Error::other("too many levels of symbolic links"))
}

fn write_via_temp(
    target: &Path,
    parent: &Path,
    bytes: &[u8],
    existing: Option<&fs::Metadata>,
) -> io::Result<()> {
    let file_name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let (tmp_path, mut file) = loop {
        let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let tmp = parent.join(format!(".{file_name}.tmp-{}-{seq}", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // Arquivo novo segue a umask; existente começa restrito até copiarmos o modo.
            options.mode(if existing.is_some() { 0o600 } else { 0o666 });
        }
        match options.open(&tmp) {
            Ok(file) => break (tmp, file),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
            Err(err) => return Err(err),
        }
    };
    let result = (|| {
        file.write_all(bytes)?;
        if let Some(meta) = existing {
            file.set_permissions(meta.permissions())?;
        }
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp_path, target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    result?;
    // Persiste a entrada de diretório do rename (não existe em Windows; melhor esforço).
    #[cfg(unix)]
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

fn write_in_place(target: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file: File = OpenOptions::new().write(true).truncate(true).open(target)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "atomic_write_{name}_{}_{}",
                std::process::id(),
                TMP_SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
            }
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp-"))
            .collect()
    }

    #[test]
    fn creates_missing_directories_and_file() {
        let s = Scratch::new("create");
        let target = s.0.join("a/b/schema.dbml");
        atomic_write(&target, b"hello").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"hello");
        assert_eq!(leftovers(&s.0.join("a/b")), Vec::<String>::new());
    }

    #[test]
    fn replaces_content() {
        let s = Scratch::new("replace");
        let target = s.0.join("schema.dbml");
        fs::write(&target, "old old old").unwrap();
        atomic_write(&target, b"new").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(leftovers(&s.0), Vec::<String>::new());
    }

    #[cfg(unix)]
    #[test]
    fn keeps_symlink_and_updates_its_target() {
        let s = Scratch::new("symlink");
        let real = s.0.join("real.dbml");
        let link = s.0.join("link.dbml");
        fs::write(&real, "old").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        atomic_write(&link, b"new").unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "o link foi trocado por arquivo comum"
        );
        assert_eq!(fs::read(&real).unwrap(), b"new");
    }

    #[cfg(unix)]
    #[test]
    fn follows_relative_and_dangling_symlinks() {
        let s = Scratch::new("relative");
        fs::create_dir_all(s.0.join("sub")).unwrap();
        std::os::unix::fs::symlink("sub/real.dbml", s.0.join("link.dbml")).unwrap();
        atomic_write(&s.0.join("link.dbml"), b"x").unwrap();
        assert_eq!(fs::read(s.0.join("sub/real.dbml")).unwrap(), b"x");
    }

    #[cfg(unix)]
    #[test]
    fn preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let s = Scratch::new("mode");
        let target = s.0.join("run.dbml");
        fs::write(&target, "old").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        atomic_write(&target, b"new").unwrap();
        let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
    }

    #[cfg(unix)]
    #[test]
    fn falls_back_to_in_place_when_directory_is_read_only() {
        use std::os::unix::fs::PermissionsExt;
        let s = Scratch::new("readonly_dir");
        let target = s.0.join("schema.dbml");
        fs::write(&target, "old").unwrap();
        fs::set_permissions(&s.0, fs::Permissions::from_mode(0o555)).unwrap();
        atomic_write(&target, b"new").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
    }

    #[test]
    fn rejects_symlink_loops() {
        #[cfg(unix)]
        {
            let s = Scratch::new("loop");
            let a = s.0.join("a");
            let b = s.0.join("b");
            std::os::unix::fs::symlink(&b, &a).unwrap();
            std::os::unix::fs::symlink(&a, &b).unwrap();
            assert!(atomic_write(&a, b"x").is_err());
        }
    }
}
