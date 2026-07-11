fn main() {
    napi_build::setup();

    // The library re-exports `#[napi]` bindings whose generated glue references
    // `napi_*` C symbols provided by the Node runtime at load time. Unit tests
    // resolve them via the `napi/noop` dev-dependency (feature-unified), but a
    // standalone `cargo bench` binary links the crate as a plain dependency
    // where `noop` is not unified, so those symbols are undefined at link. The
    // benched paths (minify / structural) are pure Rust and never call into
    // napi, so we let the bench linker leave the unreferenced napi symbols
    // unresolved. Scoped to bench targets only — the cdylib addon is untouched.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    match target_os.as_str() {
        "macos" | "ios" => {
            println!("cargo:rustc-link-arg-benches=-Wl,-undefined,dynamic_lookup");
        }
        "linux" | "android" => {
            println!("cargo:rustc-link-arg-benches=-Wl,--unresolved-symbols=ignore-all");
        }
        _ => {}
    }
}
