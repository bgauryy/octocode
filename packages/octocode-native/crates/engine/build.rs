fn main() {
    #[cfg(feature = "napi-addon")]
    napi_build::setup();

    // napi-build applies dynamic lookup only to cdylibs. The explicit test
    // feature also links executables (unit and integration tests) whose napi
    // symbols are resolved by Node at runtime and never called in tests.
    #[cfg(feature = "napi-test")]
    {
        let link_arg = match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
            Ok("macos") => Some("-Wl,-undefined,dynamic_lookup"),
            // GNU ld and lld; zig's cc wrapper rejects this flag, so
            // `cargo zigbuild --tests --all-features` cannot link these tests.
            Ok("linux") => Some("-Wl,--unresolved-symbols=ignore-all"),
            _ => None,
        };
        if let Some(link_arg) = link_arg {
            #[allow(clippy::print_stdout)]
            {
                println!("cargo:rustc-link-arg={link_arg}");
            }
        }
    }
}
