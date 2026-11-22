fn main() {
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo::rustc-link-lib=delayimp");
        println!("cargo::rustc-link-arg=/DELAYLOAD:powrprof.dll");
    }
}
