use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=../web/dist");

    let dist_index = Path::new("../web/dist/index.html");
    if !dist_index.exists() {
        panic!(
            "\n\n====================================================================\n\
            ERROR: crates/web/dist/index.html not found!\n\
            The hypatia-server binary requires web assets compiled with rust-embed.\n\
            Please compile the WebAssembly frontend first:\n  \
            wsl -e bash -lc 'cd crates/web && trunk build --release'\n\
            or run the global script: .\\build.ps1\n\
            ====================================================================\n\n"
        );
    }
}
