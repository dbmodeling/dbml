//! Log do helper: uma linha por evento, anexada a `tools\auto_update_helper.log`.

use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Linha do log: segundos desde a época Unix e a mensagem.
#[must_use]
pub fn format_line(now: SystemTime, message: &str) -> String {
    let secs = now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    format!("[{secs}] {message}\n")
}

/// Anexa `message` ao arquivo (cria a pasta se preciso).
///
/// # Errors
/// Erro de E/S; o chamador ignora (log é melhor esforço).
pub fn append(path: &Path, message: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(format_line(SystemTime::now(), message).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn linha_tem_o_tempo_e_termina_com_quebra() {
        let line = format_line(UNIX_EPOCH + Duration::from_secs(42), "oi");
        assert_eq!(line, "[42] oi\n");
    }

    #[test]
    fn anexa_sem_apagar_o_que_ja_existe() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tools").join("x.log");
        append(&path, "a").unwrap();
        append(&path, "b").unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.lines().next().unwrap().ends_with("] a"));
    }
}
