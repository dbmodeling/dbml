//! Operações de arquivo atrás de uma trait, para injetar falhas nos testes.

use std::fs::{self, OpenOptions};
use std::io;
use std::path::Path;

/// Sistema de arquivos usado pelos jobs.
pub trait Fs {
    /// O caminho existe (arquivo ou pasta)?
    fn exists(&self, path: &Path) -> bool;
    /// O caminho é uma pasta?
    fn is_dir(&self, path: &Path) -> bool;
    /// Cria a pasta e os pais (idempotente).
    ///
    /// # Errors
    /// Erro de E/S.
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    /// Move/renomeia `from` para `to` (mesmo volume; é o que permite trocar um `.exe` em uso).
    ///
    /// # Errors
    /// Erro de E/S; `NotFound` se `from` não existe.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Remove uma pasta **vazia**.
    ///
    /// # Errors
    /// Erro de E/S (inclusive "não vazia").
    fn remove_dir(&self, path: &Path) -> io::Result<()>;
    /// Remove a pasta e tudo dentro (ou o arquivo).
    ///
    /// # Errors
    /// Erro de E/S.
    fn remove_all(&self, path: &Path) -> io::Result<()>;
    /// Lê um arquivo de texto.
    ///
    /// # Errors
    /// Erro de E/S.
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    /// O arquivo está em uso por outro processo (não dá para abrir para escrita)? Arquivo
    /// inexistente não está em uso.
    fn is_in_use(&self, path: &Path) -> bool;
}

/// Sistema de arquivos real.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealFs;

impl Fs for RealFs {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir(path)
    }

    fn remove_all(&self, path: &Path) -> io::Result<()> {
        if path.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        }
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        fs::read_to_string(path)
    }

    fn is_in_use(&self, path: &Path) -> bool {
        // abrir para escrita sem criar nem truncar: falha enquanto o `.exe` está em execução
        match OpenOptions::new().write(true).open(path) {
            Ok(_) => false,
            Err(error) => error.kind() != io::ErrorKind::NotFound,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arquivo_livre_ou_inexistente_nao_esta_em_uso() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("dbml.exe");
        assert!(!RealFs.is_in_use(&file));
        fs::write(&file, b"x").unwrap();
        assert!(!RealFs.is_in_use(&file));
        // a sondagem não pode alterar o arquivo
        assert_eq!(fs::read(&file).unwrap(), b"x");
    }

    #[test]
    fn remove_all_apaga_pasta_e_arquivo() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("a/b");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("f"), b"1").unwrap();
        RealFs.remove_all(&dir.path().join("a")).unwrap();
        assert!(!dir.path().join("a").exists());
        let file = dir.path().join("g");
        fs::write(&file, b"2").unwrap();
        RealFs.remove_all(&file).unwrap();
        assert!(!file.exists());
    }
}
