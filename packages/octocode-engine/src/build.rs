fn main() {
    #[cfg(feature = "napi-addon")]
    napi_build::setup();

    // napi-build applies dynamic lookup only to cdylibs. The explicit test
    // feature also links an executable that resolves these symbols at runtime.
    #[cfg(feature = "napi-test")]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        #[allow(clippy::print_stdout)]
        {
            println!("cargo:rustc-link-arg=-Wl,-undefined,dynamic_lookup");
        }
    }
}
