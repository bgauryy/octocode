const STACK_SIZE: usize = 64 * 1024 * 1024;

/// Run a parser or recursive AST walk on a dedicated thread with substantially
/// more native stack than napi's calling thread provides.
pub(crate) fn run_on_deep_stack<T: Send + Default + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> T {
    match std::thread::Builder::new().stack_size(STACK_SIZE).spawn(f) {
        Ok(handle) => handle.join().unwrap_or_default(),
        Err(_) => T::default(),
    }
}

/// Like [`run_on_deep_stack`], but bounds the caller's wait by a wall-clock
/// deadline. The oxc parser runs as a single non-cooperative call that cannot be
/// interrupted from outside, so — unlike the tree-sitter path which polls an
/// `Instant` deadline during traversal — this puts a hard ceiling on how long a
/// pathological input can stall the caller. On timeout the worker is detached
/// (it finishes on its own; its result is discarded) and `T::default()` is
/// returned, matching the panic/spawn-failure fallbacks.
pub(crate) fn run_on_deep_stack_with_timeout<T: Send + Default + 'static>(
    timeout: std::time::Duration,
    f: impl FnOnce() -> T + Send + 'static,
) -> T {
    let (tx, rx) = std::sync::mpsc::sync_channel::<T>(1);
    match std::thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(move || {
            // If the receiver already timed out and dropped, this send fails
            // harmlessly; the worker simply exits.
            let _ = tx.send(f());
        }) {
        Ok(_handle) => rx.recv_timeout(timeout).unwrap_or_default(),
        Err(_) => T::default(),
    }
}
