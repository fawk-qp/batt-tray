use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let res = dir.join("..").join("app.res");
    println!("cargo:rerun-if-changed={}", res.display());
    if res.exists() {
        println!("cargo:rustc-link-arg-bins={}", res.display());
    }
}
