//! Embeds the icon and version information in `forma.exe` on Windows, so Explorer, the taskbar
//! and the installer show them.

fn main() {
    println!("cargo:rerun-if-changed=packaging/windows/forma.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("packaging/windows/forma.ico")
            .set("FileDescription", "Forma")
            .set("ProductName", "Forma")
            .set("LegalCopyright", "GPL-3.0-or-later");
        res.compile()
            .expect("could not embed the Windows resources");
    }
}
