//! Test-only pause points of one thread's ingest, to prove what runs
//! outside the build lock.
use std::cell::RefCell;
use std::path::Path;

/// Where an ingest stops to call the thread's hook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::tools::ast_graph::store) enum Point {
    /// Before scanning, parsing and projecting.
    Build,
    /// Built and projected, before taking the build lock.
    Publish,
}

type Hook = Box<dyn FnMut(Point, &Path)>;

thread_local! {
    static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
}

/// Calls `hook` at each [`Point`] this thread's ingests reach.
pub(in crate::tools::ast_graph::store) fn set(hook: impl FnMut(Point, &Path) + 'static) {
    HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

pub(super) fn reach(point: Point, home: &Path) {
    HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().as_mut() {
            hook(point, home);
        }
    });
}
