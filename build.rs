fn main() {
    println!(
        "cargo:rustc-env=SACHO_TARGET={}",
        std::env::var("TARGET").unwrap()
    );
}
