//! Embeds the application icon in the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/icon/ircnext.ico");
    println!("cargo:rerun-if-changed=../../assets/icon/windows.rc");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("../../assets/icon/windows.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("Windows resources");
    }
}
