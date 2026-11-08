//! Structural compile and match recurse once per pattern level, so they run
//! on the engine's large-stack pool ([`crate::deep_stack`]), never on the
//! caller's stack, and [`MAX_STRUCTURAL_DEPTH`] is what that stack holds.

use super::error::CompileError;
use super::matching::ExecutionError;
use crate::deep_stack::{STACK_SIZE, run_borrowing_on_deep_stack};

/// Stack one pattern level may take in the matcher (a `match_node` and the
/// child list it reads), in the build that takes the most: measured
/// unoptimized at about 4 KiB (a 500-level match needed 2.2 MB of thread
/// stack), doubled for the frames that cost more.
const STACK_BYTES_PER_LEVEL: usize = 8 * 1024;

/// The share of [`STACK_SIZE`] a pattern's nesting may use: the rest is the
/// margin for the frames below the match.
const NESTING_SHARE_NUMERATOR: usize = 3;
const NESTING_SHARE_DENOMINATOR: usize = 4;

/// The deepest pattern level the matcher reads before refusing with
/// `structural.match.depthLimit`: what the large stack holds, not a guess.
pub(in crate::structural) const MAX_STRUCTURAL_DEPTH: usize =
    STACK_SIZE / NESTING_SHARE_DENOMINATOR * NESTING_SHARE_NUMERATOR / STACK_BYTES_PER_LEVEL;

/// Run `f` on the large-stack pool and wait for it; `None` when no pool
/// could be built. A test's injected compile interruption follows it there,
/// and one it left unread comes back.
pub(in crate::structural) fn on_deep_stack<T: Send>(f: impl FnOnce() -> T + Send) -> Option<T> {
    #[cfg(test)]
    let interrupt = super::INTERRUPT_NEXT_COMPILE_PARSE.with(|flag| flag.replace(false));
    let (value, _left, _steps) = run_borrowing_on_deep_stack(move || {
        #[cfg(test)]
        super::INTERRUPT_NEXT_COMPILE_PARSE.with(|flag| flag.set(interrupt));
        #[cfg(test)]
        super::ANCESTOR_STEPS.with(|steps| steps.set(0));
        let value = f();
        #[cfg(test)]
        let left = super::INTERRUPT_NEXT_COMPILE_PARSE.with(|flag| flag.replace(false));
        #[cfg(not(test))]
        let left = false;
        #[cfg(test)]
        let steps = super::ANCESTOR_STEPS.with(std::cell::Cell::get);
        #[cfg(not(test))]
        let steps = 0;
        (value, left, steps)
    })?;
    #[cfg(test)]
    {
        super::INTERRUPT_NEXT_COMPILE_PARSE.with(|flag| flag.set(_left));
        // The pool's thread did the work; the caller's test reads its count.
        super::ANCESTOR_STEPS.with(|steps| steps.set(steps.get() + _steps));
    }
    Some(value)
}

/// Why nothing ran: the large-stack pool could not be built.
pub(super) fn stack_unavailable() -> ExecutionError {
    ExecutionError::limit(
        "structural.match.stackUnavailable",
        "match",
        "Structural matching could not start its large-stack worker, so nothing was matched",
    )
}

/// [`on_deep_stack`] for a compile.
pub(super) fn compile_on_deep_stack<T: Send>(
    f: impl FnOnce() -> Result<T, CompileError> + Send,
) -> Result<T, CompileError> {
    on_deep_stack(f).unwrap_or_else(|| Err(CompileError::Parse(stack_unavailable())))
}
