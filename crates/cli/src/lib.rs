#![forbid(unsafe_code)]

pub mod args;
pub mod assets;
pub mod constants;
pub mod diff;
pub mod git_base;
pub mod introspect_api;
pub mod lifecycle;
pub mod net;
pub mod server;
pub mod session;
pub mod state;
pub mod watcher;

#[cfg(test)]
mod tests {
    /// `forbid` no topo da lib e do binário: nenhum `unsafe` entra sem uma decisão explícita.
    #[test]
    fn unsafe_code_is_forbidden_in_lib_and_bin() {
        for (name, src) in [
            ("lib.rs", include_str!("lib.rs")),
            ("main.rs", include_str!("main.rs")),
        ] {
            assert!(
                src.lines()
                    .next()
                    .is_some_and(|l| l.trim() == "#![forbid(unsafe_code)]"),
                "{name} precisa começar com #![forbid(unsafe_code)]"
            );
        }
    }
}
