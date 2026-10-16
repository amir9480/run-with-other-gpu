fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-env-changed=APP_VERSION");
    embed_resource::compile("assets/app.rc", embed_resource::NONE);

    // Delay-load the Visual C++ runtime so that, if it isn't installed, the exe still
    // starts and src/vcredist.rs can show a download link instead of a loader error.
    for dll in ["VCRUNTIME140.dll", "VCRUNTIME140_1.dll", "MSVCP140.dll"] {
        println!("cargo:rustc-link-arg-bins=/DELAYLOAD:{dll}");
    }
    println!("cargo:rustc-link-arg-bins=delayimp.lib");
}
