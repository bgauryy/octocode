fn main() {
    #[cfg(feature = "napi-addon")]
    napi_build::setup();
}
