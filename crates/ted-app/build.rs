//! Embeds the Windows resources (the executable icon, `assets/ted.rc`) into `ted.exe`, as
//! Postino does. Nothing to do on the other platforms.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/ted.rc");
        println!("cargo:rerun-if-changed=assets/ted.ico");
        if let Err(error) =
            embed_resource::compile("assets/ted.rc", embed_resource::NONE).manifest_optional()
        {
            panic!("failed to embed the Windows resources: {error}");
        }
    }
}
