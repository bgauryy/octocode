//! The source location of a contained row panic. The default panic hook
//! prints it to stderr, which a warm server discards; the row's own failure
//! carries the location (never the message, which can quote private input).
use std::cell::RefCell;
use std::sync::Once;

thread_local! {
    static SITE: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Chains a hook that records each panic's location for the panicking
/// thread, then runs the hook it replaced.
fn install() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let site = info
                .location()
                .map(|at| format!("{}:{}:{}", at.file(), at.line(), at.column()));
            SITE.with(|slot| *slot.borrow_mut() = site);
            previous(info);
        }));
    });
}

/// Runs `work`, containing a panic; `Err` holds the panic's location.
pub(super) fn contain<T>(work: impl FnOnce() -> T) -> Result<T, Option<String>> {
    install();
    SITE.with(|slot| slot.borrow_mut().take());
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
        .map_err(|_| SITE.with(|slot| slot.borrow_mut().take()))
}
