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
mod tests {
    use super::*;

    #[test]
    fn a_panic_is_contained_with_its_phase_and_site() {
        let contained =
            contain(Phase::Extract, || -> u8 { panic!("quoted input") }).expect_err("panicked");
        assert_eq!(contained.phase, Phase::Extract);
        let site = contained.site.as_deref().unwrap_or_default();
        assert!(site.contains("contain.rs"), "{site}");
        assert!(!contained.to_string().contains("quoted input"));
    }

    #[test]
    fn a_recovered_inner_panic_still_marks_the_work_unassessed() {
        let contained = contain(Phase::Extract, || {
            within(Phase::Parse, || {
                std::panic::catch_unwind(|| -> u8 { panic!("inner") }).unwrap_or(0)
            })
        })
        .expect_err("an inner catch recovered");
        assert_eq!(contained.phase, Phase::Parse);
    }

    #[test]
    fn catch_keeps_a_result_an_inner_catch_recovered() {
        let value = catch(Phase::Extract, || {
            std::panic::catch_unwind(|| -> u8 { panic!("inner") }).unwrap_or(7)
        });
        assert_eq!(value, Ok(7));
        assert!(catch(Phase::Extract, || -> u8 { panic!("outer") }).is_err());
    }

    #[test]
    fn work_on_another_thread_reports_to_the_scope_that_handed_it_over() {
        let contained = contain(Phase::SecretScan, || {
            let scope = current();
            std::thread::spawn(move || {
                adopt(scope, || std::panic::catch_unwind(|| -> u8 { panic!("x") }))
            })
            .join()
            .map(|_| ())
        })
        .expect_err("the worker panicked");
        assert_eq!(contained.phase, Phase::SecretScan);
    }

    #[test]
    fn a_site_keeps_no_build_machine_directory() {
        let registry = "/home/u/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/oxc_parser-0.30.0/src/lexer/mod.rs";
        assert_eq!(portable(registry), "oxc_parser-0.30.0/src/lexer/mod.rs");
        assert_eq!(
            portable("/rustc/0a1b2c/library/core/src/option.rs"),
            "core/src/option.rs"
        );
        assert_eq!(portable("/tmp/build/out/generated.rs"), "generated.rs");
        assert_eq!(
            portable("crates/engine/src/contain.rs"),
            "crates/engine/src/contain.rs"
        );
    }

    /// Contained panics, one on a thread that adopted the scope; run only as
    /// the child of [`a_contained_panic_prints_nothing`].
    #[test]
    #[ignore = "the child process of a_contained_panic_prints_nothing"]
    fn contained_panics_in_a_child() {
        let caught = contain(Phase::Extract, || -> u8 { panic!("quoted input") });
        let recovered = catch(Phase::Parse, || {
            let scope = current();
            std::thread::spawn(move || {
                adopt(scope, || {
                    std::panic::catch_unwind(|| -> u8 { panic!("inner") })
                })
            })
            .join()
            .map(|_| ())
        });
        assert!(caught.is_err() && recovered.is_ok());
    }

    /// A contained panic prints nothing: run in a child process with the
    /// test harness's capture off, so the hook's output would reach stderr.
    #[test]
    fn a_contained_panic_prints_nothing() {
        let exe = std::env::current_exe().expect("test binary");
        let out = std::process::Command::new(exe)
            .args([
                "--exact",
                "contain::tests::contained_panics_in_a_child",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .output()
            .expect("run the child");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{stderr}");
        assert!(
            String::from_utf8_lossy(&out.stdout).contains("1 passed"),
            "the child ran the test"
        );
        assert!(
            !stderr.contains("panicked") && !stderr.contains("quoted input"),
            "{stderr}"
        );
    }

    #[test]
    fn a_panic_under_no_scope_reaches_the_replaced_hook() {
        assert!(!record(None));
        assert!(contain(Phase::Link, || 1).is_ok());
        assert!(current().is_none());
    }
}
