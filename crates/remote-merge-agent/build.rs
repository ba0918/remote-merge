fn main() {
    let target = std::env::var("TARGET").expect("cargo did not set TARGET environment variable");
    println!("cargo:rustc-env=TARGET={target}");
}
