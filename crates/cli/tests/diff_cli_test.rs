//! Integração de `dbml diff`: executa o binário e confere stdout, stderr e código de saída.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dbml_diff_cli_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, name: &str, content: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, content).unwrap();
    p
}

fn dbml_diff(args: &[&Path], extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dbml"))
        .arg("diff")
        .args(args)
        .args(extra)
        .output()
        .unwrap()
}

const OLD: &str = "Table users { id int [pk] }\nTable orders { id int [pk]\n user_id int }\n";
const NEW: &str = "Table users { id int [pk]\n email varchar }\nTable orders { id int [pk]\n user_id int }\nRef: orders.user_id > users.id\n";

#[test]
fn exits_1_and_prints_text_when_files_differ() {
    let dir = fixture_dir("differ");
    let (old, new) = (write(&dir, "old.dbml", OLD), write(&dir, "new.dbml", NEW));
    let out = dbml_diff(&[&old, &new], &[]);
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("+ column email: varchar"), "{stdout}");
    assert!(stdout.contains("+ orders.user_id > users.id"), "{stdout}");
    assert!(
        stdout.contains("Summary: 2 added, 0 removed, 0 modified"),
        "{stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exits_0_when_files_are_equivalent() {
    let dir = fixture_dir("same");
    let (old, new) = (write(&dir, "old.dbml", OLD), write(&dir, "new.dbml", OLD));
    let out = dbml_diff(&[&old, &new], &[]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "No differences.\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn json_format_prints_the_schema_diff() {
    let dir = fixture_dir("json");
    let (old, new) = (write(&dir, "old.dbml", OLD), write(&dir, "new.dbml", NEW));
    let out = dbml_diff(&[&old, &new], &["--format", "json"]);
    assert_eq!(out.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["refs"][0]["kind"], "added");
    assert_eq!(v["tables"][0]["kind"], "modified");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn syntax_error_exits_2_naming_the_file() {
    let dir = fixture_dir("syntax");
    let old = write(&dir, "old.dbml", OLD);
    let bad = write(&dir, "broken.dbml", "Table users { id int");
    let out = dbml_diff(&[&old, &bad], &[]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(out.stdout.len(), 0);
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("broken.dbml"), "{stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_file_exits_2() {
    let dir = fixture_dir("missing");
    let old = write(&dir, "old.dbml", OLD);
    let out = dbml_diff(&[&old, &dir.join("nope.dbml")], &[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8(out.stderr).unwrap().contains("nope.dbml"));
    let _ = std::fs::remove_dir_all(&dir);
}
