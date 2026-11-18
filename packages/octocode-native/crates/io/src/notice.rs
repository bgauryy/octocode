//! Warnings a call says on stderr, outside its result, each line a `# `
//! note ([`as_note`]).
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
//! runtime's row work and row fan-out, the clasify and ranked-read workers,
//! and the GitHub budget's blocking state writes. A notice raised outside
//! every scope cannot name its call: when it is about the process
//! ([`kind`]), every call in flight relays it; a notice about a request goes
//! only to stderr, never into an unrelated call's reply.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, PoisonError, RwLock, Weak};

/// One call's notices: the first of each kind, in the order raised.
type Notices = Mutex<Vec<(&'static str, String)>>;

/// Where one call's progress lines go while it runs (see
/// [`Scope::with_progress`]).
pub type Progress = Arc<dyn Fn(&str) + Send + Sync>;

/// The notices one call raised, and the host's listener for its progress
/// lines when the host shows them live (an MCP client that sent a progress
/// token). Clones share both.
#[derive(Clone)]
pub struct Scope(Arc<Notices>, Option<Progress>);

thread_local! {
    static THREAD: RefCell<Option<Scope>> = const { RefCell::new(None) };
    static RESULT: RefCell<Option<ResultNotes>> = const { RefCell::new(None) };
}

/// What one call says in its own result rather than on stderr: the first
/// note of each kind, in the order raised ([`raise_in_result`]). The runtime
/// enters one per call where its rows run and states what it holds in the
/// response's `warnings`, so the line never lands outside the envelope.
/// Work handed to another thread carries it as it carries the [`Scope`]
/// ([`carried`]).
#[derive(Clone, Default)]
pub struct ResultNotes(Arc<Notices>);

impl ResultNotes {
    /// Make these the current thread's result notes until the guard drops.
    #[must_use = "the notes stop collecting when the guard drops"]
    pub fn enter(&self) -> EnteredResult {
        let previous = RESULT.with(|thread| thread.replace(Some(self.clone())));
        EnteredResult {
            previous,
            _thread: std::marker::PhantomData,
        }
    }

    fn current() -> Option<Self> {
        RESULT.with(|thread| thread.borrow().clone())
    }

    /// The notes said so far, removed: each is stated once.
    #[must_use]
    pub fn take(&self) -> Vec<String> {
        std::mem::take(&mut *lock(&self.0))
            .into_iter()
            .map(|(_, text)| text)
            .collect()
    }
}

/// The thread's [`ResultNotes`], entered by [`ResultNotes::enter`]; dropping
/// it restores the ones the thread had before.
pub struct EnteredResult {
    previous: Option<ResultNotes>,
    _thread: std::marker::PhantomData<*const ()>,
}

impl Drop for EnteredResult {
    fn drop(&mut self) {
        let previous = self.previous.take();
        RESULT.with(|thread| *thread.borrow_mut() = previous);
    }
}

/// Say `text` in the result of the call running this code, once per kind
/// ([`ResultNotes`]); never on stderr. Code that runs for no call (no notes
/// entered) has no result to say it in, so it is said as [`raise`] says it.
pub fn raise_in_result(kind: &'static str, text: impl Into<String>) {
    match ResultNotes::current() {
        Some(notes) => record(&notes.0, kind, &scrubbed(text.into())),
        None => raise(kind, text),
    }
}

/// `text` through the installed scrubber ([`set_scrubber`]).
fn scrubbed(text: String) -> String {
    match SCRUBBER.get() {
        Some(scrub) => scrub(&text),
        None => text,
    }
}

/// The notice kinds about the whole process, not one call: each call in
/// flight relays one raised outside every scope. Every other kind belongs to
/// the call that raised it.
pub mod kind {
    /// The shared GitHub rate-limit record cannot be used; this process
    /// paces alone.
    pub const GITHUB_BUDGET_UNSHARED: &str = "github-budget-unshared";
    /// No GitHub credential and no usable credential store: requests run
    /// unauthenticated.
    pub const GITHUB_ANONYMOUS: &str = "github-anonymous";

    /// Every process-level kind.
    pub(super) const PROCESS: &[&str] = &[GITHUB_BUDGET_UNSHARED, GITHUB_ANONYMOUS];
}

/// The scopes of the calls in flight, for a process-level notice raised
/// outside them.
static LIVE: Mutex<Vec<Weak<Notices>>> = Mutex::new(Vec::new());
/// Where progress lines go when the running code is in no [`Scope`]: the
/// CLI's stderr, installed by [`report_progress_to`].
static PROCESS_PROGRESS: RwLock<Option<Progress>> = RwLock::new(None);
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
        Self(notices, None)
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

    /// A new scope whose call's progress lines go to `progress` as they
    /// happen.
    #[must_use]
    pub fn with_progress(progress: impl Fn(&str) + Send + Sync + 'static) -> Self {
        Self(Self::default().0, Some(Arc::new(progress)))
    }

    /// The listener for this scope's progress lines, when its host gave one.
    #[must_use]
    pub fn progress(&self) -> Option<Progress> {
        self.1.clone()
    }

    /// The notices raised in this scope, the first of each kind, in the
    /// order raised: what a process that ran only this call would have
    /// printed.
    #[must_use]
    pub fn notices(&self) -> Vec<String> {
        lock(&self.0).iter().map(|(_, text)| text.clone()).collect()
    }
}

/// Send every progress line raised outside a [`Scope`] to `sink`: how an
/// in-process CLI call shows what a long wait or build is doing.
pub fn report_progress_to(sink: impl Fn(&str) + Send + Sync + 'static) {
    *PROCESS_PROGRESS
        .write()
        .unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(sink));
}

/// Where the running call's progress lines go, or `None` when nothing
/// listens (an MCP call without a progress token, an unscoped library
/// call): its scope's listener (the warm server's, which writes each line
/// to the client as a frame while the call runs), else its scope's notices
/// (relayed with a reply), else the process sink
/// ([`report_progress_to`]). Take it where the call runs: work carried to
/// another thread uses the sink taken here.
#[must_use]
pub fn progress_sink() -> Option<Progress> {
    let Some(scope) = Scope::current() else {
        return PROCESS_PROGRESS
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
    };
    Some(
        scope
            .progress()
            .unwrap_or_else(|| Arc::new(move |line: &str| emit(line, Some(&scope)))),
    )
}

/// `work`, made to run in the caller's [`Scope`] on whichever thread runs
/// it: how work handed to another thread keeps its call's notices.
pub fn carried<R>(work: impl FnOnce() -> R) -> impl FnOnce() -> R {
    let (scope, notes) = (Scope::current(), ResultNotes::current());
    move || {
        let _in = scope.as_ref().map(Scope::enter);
        let _noting = notes.as_ref().map(ResultNotes::enter);
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

/// `text` as stderr says it: every physical line a `# ` note, the one form
/// of a stderr line beside a call's result, so a reader that merges the
/// streams tells them from content.
#[must_use]
pub fn as_note(text: &str) -> String {
    text.split('\n')
        .map(|line| format!("# {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Say `text` on stderr unless this process already said a notice of this
/// `kind`, and record it in the current [`Scope`]. Outside every scope a
/// [`kind::PROCESS`] kind is recorded in every live one; any other kind has
/// no call to name and is only said on stderr.
pub fn raise(kind: &'static str, text: impl Into<String>) {
    raise_among(kind, text, kind::PROCESS);
}

#[expect(
    clippy::print_stderr,
    reason = "the notice owner: stderr is where a notice is said"
)]
fn raise_among(kind: &'static str, text: impl Into<String>, process_kinds: &[&str]) {
    let text = scrubbed(text.into());
    match Scope::current() {
        Some(scope) => record(&scope.0, kind, &text),
        None if process_kinds.contains(&kind) => {
            let live: Vec<_> = lock(&LIVE).iter().filter_map(Weak::upgrade).collect();
            for notices in live {
                record(&notices, kind, &text);
            }
        }
        None => {}
    }
    if lock(&PRINTED).insert(kind) {
        eprintln!("{}", as_note(&text));
    }
}

/// Emit a repeatable diagnostic to its captured scope, outside result data. A scoped caller's
/// warm-server reply relays it just like a notice, without deduplication.
/// Unscoped diagnostics go only to stderr, never to unrelated live calls.
#[expect(
    clippy::print_stderr,
    reason = "the notice owner: stderr is where a diagnostic is said"
)]
pub fn emit(text: impl Into<String>, scope: Option<&Scope>) {
    let text = scrubbed(text.into());
    if let Some(scope) = scope {
        lock(&scope.0).push(("", text.clone()));
    }
    eprintln!("{}", as_note(&text));
}

#[cfg(test)]
mod tests;
