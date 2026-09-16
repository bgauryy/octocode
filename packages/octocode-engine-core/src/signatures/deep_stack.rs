/// Run a parser or recursive AST walk on a dedicated thread with substantially
/// more native stack than napi's calling thread provides.
pub(crate) fn run_on_deep_stack<T: Send + Default + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> T {
    const STACK_SIZE: usize = 64 * 1024 * 1024;
    match std::thread::Builder::new().stack_size(STACK_SIZE).spawn(f) {
        Ok(handle) => handle.join().unwrap_or_default(),
        Err(_) => T::default(),
    }
}
