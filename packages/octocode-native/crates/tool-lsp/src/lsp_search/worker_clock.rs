//! The settle clock a test thread holds, handed to the blocking worker a
//! request spawns ([`super::blocking_cancellable`]): the worker settles files
//! as its caller does. A test hook: in production there is no such clock and
//! [`carried`] returns `work` as it is.

/// `work`, to run on another thread under the calling thread's clock.
#[cfg(test)]
pub(super) fn carried<T>(work: impl FnOnce() -> T) -> impl FnOnce() -> T {
    let through = octocode_engine::stamp::settled_through();
    move || octocode_engine::stamp::with_settled_through(through, work)
}

#[cfg(not(test))]
pub(super) fn carried<T>(work: impl FnOnce() -> T) -> impl FnOnce() -> T {
    work
}
