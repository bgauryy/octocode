//! Contained panics: work that runs under [`contain`] reports a panic as a
//! [`Contained`] (its phase and source location) instead of printing it.
//!
//! The default panic hook writes to stderr, which an MCP host never shows
//! the model, and it names no input. A scope records the first panic of its
//! work, including one an inner `catch_unwind` recovers from (a parser
//! fallback, a sanitizer failing closed), so the caller can name what was
//! not assessed. The location is kept, never the message: a message can
//! quote private input. A panic outside every scope reaches the hook this
//! one replaced, as before.
use std::cell::RefCell;
use std::sync::{Arc, Mutex, Once, PoisonError};

/// What a contained piece of work was doing when it panicked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Reading a source file's bytes.
    Read,
    /// Admitting a source's bytes as text (decoding).
    Decode,
    /// A caller's prefilter of a source's text.
    Keep,
    /// Parsing a source into a syntax tree.
    Parse,
    /// Extracting a file's facts and their source correspondence.
    Extract,
    /// Mapping a file's facts to what the caller keeps.
    Map,
    /// Weighing what the caller kept of a file against its budget.
    Weigh,
    /// Scanning a source for secrets before it is kept.
    SecretScan,
    /// Linking a file's facts into a graph.
    Link,
    /// Admitting and answering one request, outside its rows.
    Request,
    /// Executing one tool row.
    Row,
    /// Warming what the rows of a batch share.
    Warm,
}

impl Phase {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Decode => "decode",
            Self::Keep => "keep",
            Self::Parse => "parse",
            Self::Extract => "extract",
            Self::Map => "map",
            Self::Weigh => "weigh",
            Self::SecretScan => "secret scan",
            Self::Link => "link",
            Self::Request => "request",
            Self::Row => "row",
            Self::Warm => "warm-up",
        }
    }
}

/// A panic a scope caught or saw recovered: its phase and location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contained {
    pub phase: Phase,
    /// `file:line:column` of the panic, when the runtime reported one.
    pub site: Option<String>,
}

impl std::fmt::Display for Contained {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.site {
            Some(site) => write!(f, "{} phase panicked at {site}", self.phase.label()),
            None => write!(f, "{} phase panicked", self.phase.label()),
        }
    }
}

/// The scope a thread runs under: its current phase and where its first
/// panic is recorded. Work handed to another thread carries it ([`current`]).
#[derive(Clone, Debug)]
pub struct Scope {
    phase: Phase,
    first: Arc<Mutex<Option<Contained>>>,
}

thread_local! {
    static SCOPE: RefCell<Option<Scope>> = const { RefCell::new(None) };
}

fn install() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let site = info
                .location()
                .map(|at| format!("{}:{}:{}", portable(at.file()), at.line(), at.column()));
            if !record(site) {
                previous(info);
            }
        }));
    });
}

/// The source file of a panic site without the build machine's directories:
/// an absolute path (a dependency's registry checkout, the toolchain's
/// sources) keeps its crate directory, the parent of its last `src`, and
/// below, else its file name; a relative path (this workspace) is kept.
fn portable(file: &str) -> String {
    let path = std::path::Path::new(file);
    if !path.has_root() {
        return file.to_owned();
    }
    let parts = path
        .components()
        .filter_map(|part| match part {
            std::path::Component::Normal(name) => Some(name.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let start = parts
        .iter()
        .rposition(|part| part == "src")
        .filter(|&at| at + 1 < parts.len())
        .and_then(|at| at.checked_sub(1))
        .unwrap_or(parts.len().saturating_sub(1));
    parts.get(start..).unwrap_or_default().join("/")
}

/// Records a panic at `site` in this thread's scope; `false` when the thread
/// runs under none, so the replaced hook reports it.
fn record(site: Option<String>) -> bool {
    SCOPE.with(|slot| {
        // A panic while the slot is borrowed is never this module's: let
        // the replaced hook report it.
        let Ok(scope) = slot.try_borrow() else {
            return false;
        };
        let Some(scope) = scope.as_ref() else {
            return false;
        };
        scope
            .first
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_or_insert(Contained {
                phase: scope.phase,
                site,
            });
        true
    })
}

/// Restores the thread's previous scope when dropped.
struct Restore(Option<Scope>);

impl Drop for Restore {
    fn drop(&mut self) {
        let previous = self.0.take();
        SCOPE.with(|slot| *slot.borrow_mut() = previous);
    }
}

fn enter(scope: Scope) -> Restore {
    Restore(SCOPE.with(|slot| slot.replace(Some(scope))))
}

/// Runs `work` in `phase`, containing a panic: `Err` when it panicked or an
/// inner catch recovered from a panic of its work, so its result is not a
/// full assessment. Nothing is printed.
pub fn contain<T>(phase: Phase, work: impl FnOnce() -> T) -> Result<T, Contained> {
    match observe(phase, work)? {
        (value, None) => Ok(value),
        (_, Some(recovered)) => Err(recovered),
    }
}

/// [`contain`] that fails only when `work` itself unwinds: a panic an inner
/// catch recovered from is not printed, and its result stands.
pub fn catch<T>(phase: Phase, work: impl FnOnce() -> T) -> Result<T, Contained> {
    observe(phase, work).map(|(value, _)| value)
}

/// Runs `work` in `phase`: `Err` when it unwound, else its result and the
/// first panic an inner catch recovered from, for a caller whose work has a
/// designed fallback to judge whether that result still stands.
pub fn observe<T>(
    phase: Phase,
    work: impl FnOnce() -> T,
) -> Result<(T, Option<Contained>), Contained> {
    install();
    let first = Arc::new(Mutex::new(None));
    let result = {
        let _scope = enter(Scope {
            phase,
            first: Arc::clone(&first),
        });
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
    };
    let seen = first.lock().unwrap_or_else(PoisonError::into_inner).take();
    match result {
        Ok(value) => Ok((value, seen)),
        Err(_) => Err(seen.unwrap_or(Contained { phase, site: None })),
    }
}

/// Runs `work` as `phase` of the enclosing scope (none: as it is), so a
/// panic in it is attributed to that phase.
pub fn within<T>(phase: Phase, work: impl FnOnce() -> T) -> T {
    match current() {
        Some(scope) => {
            let _scope = enter(Scope { phase, ..scope });
            work()
        }
        None => work(),
    }
}

/// This thread's scope, for work it hands to another thread ([`adopt`]).
#[must_use]
pub fn current() -> Option<Scope> {
    SCOPE.with(|slot| slot.borrow().clone())
}

/// Runs `work` under `scope`, taken from the thread that handed it over:
/// its panics are recorded there, not printed.
pub fn adopt<T>(scope: Option<Scope>, work: impl FnOnce() -> T) -> T {
    match scope {
        Some(scope) => {
            let _scope = enter(scope);
            work()
        }
        None => work(),
    }
}

#[cfg(test)]
mod tests;
