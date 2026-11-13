fn main() {
    println!("cargo:rerun-if-changed=assets/hypatia.png");
    println!("cargo:rerun-if-changed=assets/hypatia.ico");
    println!("cargo:rerun-if-changed=hypatia.rc");

    // English is the source language; `lang/<code>/LC_MESSAGES/hypatia.po` holds the rest.
    let config = slint_build::CompilerConfiguration::new().with_bundled_translations("lang");
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint UI failed to compile");

    #[cfg(windows)]
    if std::env::var_os("CARGO_CFG_TARGET_OS").as_deref() == Some(std::ffi::OsStr::new("windows")) {
        embed_resource::compile("hypatia.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("Failed to embed Windows application icon resource");
    }
}
