#![cfg_attr(windows, windows_subsystem = "windows")]

//! `auto_update_helper.exe`: ver a documentação da biblioteca.

fn main() -> std::process::ExitCode {
    auto_update_helper::main_entry()
}
