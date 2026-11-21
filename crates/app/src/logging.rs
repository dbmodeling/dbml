//! Structured rotating file logger for the Desktop app built on `tracing` (#98).

use crate::constants::{
    LOG_FILE_NAME, LOG_ROTATED_FILE_NAME, LOG_TRUNCATED_SUFFIX, LOGS_DIR_NAME, MAX_LOG_FILE_BYTES,
    MAX_LOG_LINE_BYTES,
};
use std::borrow::Cow;
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

/// Returns the Desktop logs directory (`<config_dir>/logs`).
#[must_use]
pub fn logs_dir() -> Option<PathBuf> {
    crate::session::config_dir().map(|d| d.join(LOGS_DIR_NAME))
}

/// Masks any embedded credentials in URLs so passwords never appear in logs.
#[must_use]
pub fn redact_sensitive(input: &str) -> String {
    input
        .split_whitespace()
        .map(|tok| {
            if tok.contains("://") && tok.contains('@') {
                db_introspect::mask_url(tok)
            } else {
                tok.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

struct MessageVisitor {
    out: String,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if !self.out.is_empty() {
            self.out.push(' ');
        }
        if field.name() == "message" {
            let _ = write!(self.out, "{value:?}");
        } else {
            let _ = write!(self.out, "{}={value:?}", field.name());
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if !self.out.is_empty() {
            self.out.push(' ');
        }
        if field.name() == "message" {
            self.out.push_str(value);
        } else {
            let _ = write!(self.out, "{}={value}", field.name());
        }
    }
}

struct RotatingFileSubscriber {
    dir: PathBuf,
    lock: Mutex<()>,
}

impl RotatingFileSubscriber {
    fn new(dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        Self {
            dir,
            lock: Mutex::new(()),
        }
    }

    fn write_line(&self, line: &str) {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        write_rotating(&self.dir, line, MAX_LOG_FILE_BYTES);
    }
}

fn truncate_line(line: &str, max_bytes: usize) -> Cow<'_, str> {
    if line.len() <= max_bytes {
        return Cow::Borrowed(line);
    }
    let keep = max_bytes.saturating_sub(LOG_TRUNCATED_SUFFIX.len());
    let cut = line.floor_char_boundary(keep);
    let mut out = String::with_capacity(cut + LOG_TRUNCATED_SUFFIX.len());
    out.push_str(&line[..cut]);
    out.push_str(LOG_TRUNCATED_SUFFIX);
    Cow::Owned(out)
}

fn write_rotating(dir: &Path, line: &str, max_bytes: u64) {
    let _ = fs::create_dir_all(dir);
    let current = dir.join(LOG_FILE_NAME);
    let bounded = truncate_line(line, MAX_LOG_LINE_BYTES);
    let mut truncated_fallback = false;
    if fs::metadata(&current)
        .is_ok_and(|m| m.len().saturating_add(bounded.len() as u64) >= max_bytes)
    {
        let rotated = dir.join(LOG_ROTATED_FILE_NAME);
        // No Windows, `rename` pode falhar se o destino já existir; remove antes.
        let _ = fs::remove_file(&rotated);
        if fs::rename(&current, &rotated).is_err() {
            // Se o arquivo estiver travado (ex.: outro leitor), trunca no lugar para nunca estourar o limite.
            truncated_fallback = true;
        }
    }
    let mut opts = OpenOptions::new();
    opts.create(true).write(true);
    if truncated_fallback {
        opts.truncate(true);
    } else {
        opts.append(true);
    }
    if let Ok(mut file) = opts.open(&current) {
        let _ = writeln!(file, "{bounded}");
    }
}

impl Subscriber for RotatingFileSubscriber {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _span: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}
    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let meta = event.metadata();
        let mut visitor = MessageVisitor { out: String::new() };
        event.record(&mut visitor);
        let ts = crate::session::now_secs();
        let clean = redact_sensitive(&visitor.out);
        let line = format!("{ts} {} {}: {clean}", meta.level(), meta.target());
        self.write_line(&line);
    }

    fn enter(&self, _span: &Id) {}
    fn exit(&self, _span: &Id) {}
}

/// Installs the global rotating file `tracing` subscriber (idempotent).
pub fn init() {
    let Some(dir) = logs_dir() else { return };
    let _ = tracing::subscriber::set_global_default(RotatingFileSubscriber::new(dir));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_sensitive_strips_url_passwords() {
        let msg = "failed to connect to postgres://alice:super-secret-pw@db.example.com:5432/app";
        let redacted = redact_sensitive(msg);
        assert!(!redacted.contains("super-secret-pw"), "{redacted}");
        assert!(redacted.contains("db.example.com"), "{redacted}");
    }

    #[test]
    fn write_rotating_rolls_over_at_max_bytes() {
        let dir = std::env::temp_dir().join(format!("dbm_log_rot_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        write_rotating(&dir, "first-line-exceeding-limit", 10);
        write_rotating(&dir, "second-line", 10);
        write_rotating(&dir, "third-line", 10);
        assert!(dir.join(LOG_ROTATED_FILE_NAME).exists());
        assert_eq!(
            fs::read_to_string(dir.join(LOG_ROTATED_FILE_NAME))
                .unwrap()
                .trim(),
            "second-line"
        );
        assert_eq!(
            fs::read_to_string(dir.join(LOG_FILE_NAME)).unwrap().trim(),
            "third-line"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn truncate_line_caps_size_on_utf8_boundary() {
        let huge = "é".repeat(MAX_LOG_LINE_BYTES);
        let out = truncate_line(&huge, MAX_LOG_LINE_BYTES);
        assert!(out.len() <= MAX_LOG_LINE_BYTES);
        assert!(out.ends_with(LOG_TRUNCATED_SUFFIX));
    }
}
