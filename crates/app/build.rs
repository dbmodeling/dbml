//! Embute o ícone do app no .exe (só quando o alvo é Windows).

fn main() {
    println!("cargo:rerun-if-changed=assets/brand/app.rc");
    println!("cargo:rerun-if-changed=assets/brand/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        #[expect(
            clippy::unwrap_used,
            reason = "build script aborts if Windows resource compilation fails"
        )]
        embed_resource::compile("assets/brand/app.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
