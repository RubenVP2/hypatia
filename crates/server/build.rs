use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=../web/dist");

    let dist_index = Path::new("../web/dist/index.html");
    if !dist_index.exists() {
        panic!(
            "\n\n====================================================================\n\
            ERREUR: crates/web/dist/index.html est introuvable !\n\
            Le binaire hypatia-server requiert les assets web compilés via rust-embed.\n\
            Veuillez d'abord compiler le frontend WebAssembly :\n  \
            wsl -e bash -lc 'cd crates/web && trunk build --release'\n\
            ou exécuter le script global : .\\build.ps1\n\
            ====================================================================\n\n"
        );
    }
}
