//! Warnings a call says on stderr, outside its result.
//!
//! Stdout carries CLI results and MCP JSON-RPC, so a condition that is not
//! part of any result (no usable credential store, an unwritable shared
//! state file) is said on stderr. [`raise`] says each kind of notice once per
//! process: for a CLI call, once per call. The CLI's warm server answers
//! many calls in one process with a null stderr, so it runs each call in its
//! own [`Scope`] and relays that scope's [`Scope::notices`], which the client
//! prints as an in-process call would have.
//!
//! A notice belongs to the scope the raising code runs in: its future's
//! ([`Scope::run`]), else its thread's ([`Scope::enter`]). Work handed to
//! another thread carries its caller's scope there ([`carried`]): the
//! runtime's row work and row fan-out, and the GitHub budget's blocking
//! state writes. A notice raised outside every scope cannot name its call,
//! so every call in flight relays it: a notice is never dropped.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, PoisonError, Weak};

/// One call's notices: the first of each kind, in the order raised.
type Notices = Mutex<Vec<(&'static str, String)>>;

/// The notices one call raised. Clones share them.
#[derive(Clone)]
pub struct Scope(Arc<Notices>);

thread_local! {
    static THREAD: RefCell<Option<Scope>> = const { RefCell::new(None) };
}

/// The scopes of the calls in flight, for a notice raised outside them.
static LIVE: Mutex<Vec<Weak<Notices>>> = Mutex::new(Vec::new());
/// Kinds this process already said on stderr.
static PRINTED: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());

/// How every notice's text is cleaned before it is recorded or printed,
/// installed once by the runtime (this crate sits below the security rules).
static SCRUBBER: std::sync::OnceLock<fn(&str) -> String> = std::sync::OnceLock::new();

/// Install the scrubber every notice passes through; the first call wins.
pub fn set_scrubber(scrub: fn(&str) -> String) {
    let _ = SCRUBBER.set(scrub);
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Default for Scope {
    /// A new scope, live until its last clone drops.
    fn default() -> Self {
        let notices = Arc::new(Notices::default());
        let mut live = lock(&LIVE);
        live.retain(|scope| scope.strong_count() > 0);
        live.push(Arc::downgrade(&notices));
        Self(notices)
    }
}

impl Scope {
    /// Run `future` in this scope: what it raises while it is polled is
    /// this scope's, though other calls' futures share its thread.
    pub async fn run<F: Future>(&self, future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        std::future::poll_fn(|context| {
            let _in = self.enter();
            future.as_mut().poll(context)
        })
        .await
    }

    /// Make this scope the current thread's until the guard drops.
    #[must_use = "the scope ends when the guard drops"]
    pub fn enter(&self) -> Entered {
        let previous = THREAD.with(|thread| thread.replace(Some(self.clone())));
        Entered {
            previous,
            _thread: std::marker::PhantomData,
        }
    }

    /// The scope the running code is in: the one its future runs in
    /// ([`Self::run`]), else its thread's ([`Self::enter`]).
    #[must_use]
    pub fn current() -> Option<Self> {
        THREAD.with(|thread| thread.borrow().clone())
    }

    /// The notices raised in this scope, the first of each kind, in the
    /// order raised: what a process that ran only this call would have
    /// printed.
    #[must_use]
    pub fn notices(&self) -> Vec<String> {
        lock(&self.0).iter().map(|(_, text)| text.clone()).collect()
    }
}

/// `work`, made to run in the caller's [`Scope`] on whichever thread runs
/// it: how work handed to another thread keeps its call's notices.
pub fn carried<R>(work: impl FnOnce() -> R) -> impl FnOnce() -> R {
    let scope = Scope::current();
    move || {
        let _in = scope.as_ref().map(Scope::enter);
        work()
    }
}

fn record(notices: &Notices, kind: &'static str, text: &str) {
    let mut notices = lock(notices);
    if !notices.iter().any(|(seen, _)| *seen == kind) {
        notices.push((kind, text.to_owned()));
    }
}

/// A thread's scope, entered by [`Scope::enter`]; dropping it restores the
/// scope the thread had before. It stays on the thread that entered it.
pub struct Entered {
    previous: Option<Scope>,
    _thread: std::marker::PhantomData<*const ()>,
}

impl Drop for Entered {
    fn drop(&mut self) {
        let previous = self.previous.take();
        THREAD.with(|thread| *thread.borrow_mut() = previous);
    }
}

/// Say `text` on stderr unless this process already said a notice of this
/// `kind`, and record it in the current [`Scope`] (outside every scope: in
/// every live one).
pub fn raise(kind: &'static str, text: impl Into<String>) {
    let text: String = text.into();
    let text = match SCRUBBER.get() {
        Some(scrub) => scrub(&text),
        None => text,
    };
    match Scope::current() {
        Some(scope) => record(&scope.0, kind, &text),
        None => {
            let live: Vec<_> = lock(&LIVE).iter().filter_map(Weak::upgrade).collect();
            for notices in live {
                record(&notices, kind, &text);
            }
        }
    }
    if lock(&PRINTED).insert(kind) {
        eprintln!("{text}");
    }
}

/// Emit a repeatable diagnostic to its captured scope, outside result data. A scoped caller's
/// warm-server reply relays it just like a notice, without deduplication.
/// Unscoped diagnostics go only to stderr, never to unrelated live calls.
pub fn emit(text: impl Into<String>, scope: Option<&Scope>) {
    let text = text.into();
    let text = match SCRUBBER.get() {
        Some(scrub) => scrub(&text),
        None => text,
    };
    if let Some(scope) = scope {
        lock(&scope.0).push(("", text.clone()));
    }
    eprintln!("{text}");
}

#[cfg(test)]
mod tests {
    use super::{Scope, carried, raise};

    #[tokio::test]
    async fn repeated_diagnostics_are_scoped_and_unscoped_ones_are_not_broadcast() {
        let a = Scope::default();
        let b = Scope::default();
        a.run(async {
            super::emit("trace-repeat", Scope::current().as_ref());
            super::emit("trace-repeat", Scope::current().as_ref());
        })
        .await;
        b.run(async { super::emit("trace-unscoped", None) }).await;
        assert_eq!(of(&a, "trace-repeat"), ["trace-repeat", "trace-repeat"]);
        assert!(of(&a, "trace-unscoped").is_empty());
        assert!(of(&b, "trace-repeat").is_empty());
        assert!(of(&b, "trace-unscoped").is_empty());
    }

    fn of(scope: &Scope, kind: &str) -> Vec<String> {
        scope
            .notices()
            .into_iter()
            .filter(|text| text.starts_with(kind))
            .collect()
    }

    /// One test: a raise outside every scope reaches every live scope, so
    /// separate tests running at once would see each other's.
    #[tokio::test]
    async fn a_notice_belongs_to_the_call_that_raised_it() {
        let (a, b) = (Scope::default(), Scope::default());
        // In a task's scope: that call's only, the first of a kind once.
        a.run(async {
            raise("notice-test-task", "notice-test-task: one");
            tokio::task::yield_now().await;
            raise("notice-test-task", "notice-test-task: two");
        })
        .await;
        assert_eq!(of(&a, "notice-test-task"), ["notice-test-task: one"]);
        assert!(of(&b, "notice-test-task").is_empty(), "not another call's");

        // Work carried to another thread keeps its caller's scope.
        let carried = b
            .run(async {
                std::thread::spawn(carried(|| {
                    raise("notice-test-thread", "notice-test-thread: b");
                }))
            })
            .await;
        carried.join().expect("thread");
        assert_eq!(of(&b, "notice-test-thread"), ["notice-test-thread: b"]);
        assert!(of(&a, "notice-test-thread").is_empty());

        // Leaving the guard restores the thread's previous scope.
        {
            let _in = a.enter();
            {
                let _inner = b.enter();
            }
            raise("notice-test-restored", "notice-test-restored: a");
        }
        assert_eq!(of(&a, "notice-test-restored").len(), 1);
        assert!(of(&b, "notice-test-restored").is_empty());

        // Raised outside every scope: every call in flight relays it.
        std::thread::spawn(|| raise("notice-test-unscoped", "notice-test-unscoped: all"))
            .join()
            .expect("thread");
        assert_eq!(of(&a, "notice-test-unscoped").len(), 1);
        assert_eq!(of(&b, "notice-test-unscoped").len(), 1);

        // A scope that ended takes no more; a later call that raises the
        // kind again relays it again, though the process says it once.
        drop(b);
        let later = Scope::default();
        later
            .run(async { raise("notice-test-task", "notice-test-task: one") })
            .await;
        assert_eq!(of(&later, "notice-test-task"), ["notice-test-task: one"]);
    }
}
