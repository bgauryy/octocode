//! A PCRE2 search cut into windows by time, each resuming where the last
//! one paused.
//!
//! Nothing bounds the time of one PCRE2 match, so a `-P` search over a large
//! tree can outlast its caller. A window has an end ([`SearchWindow::ends`]):
//! past it the search pauses after the last file finished in walk order, so
//! everything before the pause is searched whole and nothing after it is
//! searched at all ([`Pause`]). The next window resumes at that position
//! ([`Resume`]), with what the paused windows searched when the caller still
//! holds it, and otherwise searches those files again before its own end
//! applies: every window makes progress, and the window that ends the search
//! returns exactly what one unpaused search returns.
use super::state::CollectResult;
use crate::types::TextSearchResult;
use std::sync::Arc;
use std::time::Instant;

/// What a window's end is compared with after each finished file.
pub type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

/// How one window of a search is bounded and where it starts. The default
/// window has no end and starts at the first file: the whole search.
#[derive(Default)]
pub struct SearchWindow {
    /// When the window ends; `None` runs until the search ends or the caller
    /// cancels it. Only a PCRE2 search pauses: a linear engine needs no end.
    pub ends: Option<Instant>,
    /// The time `ends` is compared with each time a file is finished;
    /// `None` reads the monotonic clock.
    pub clock: Option<Clock>,
    /// The windows before this one.
    pub resume: Option<Resume>,
}

/// Where a paused search resumes.
pub struct Resume {
    /// Candidates, in walk order, the windows before this one searched.
    pub position: u32,
    /// What they kept, when the caller still holds it.
    pub earlier: Option<Earlier>,
}

/// What the paused windows searched, under the pin of the candidates they
/// passed ([`Pause::pin`]). It serves only while those candidates are the
/// same files, unchanged; otherwise the window searches them again.
pub struct Earlier {
    pub pin: String,
    pub searched: Searched,
}

/// The files a search finished so far, kept whole for the window that
/// resumes it.
pub struct Searched(pub(super) CollectResult);

impl Searched {
    /// Rough heap bytes of what is kept.
    #[must_use]
    pub fn weight(&self) -> usize {
        self.0.weight()
    }

    /// Files searched so far.
    #[must_use]
    pub fn files_searched(&self) -> u64 {
        self.0.files_searched
    }

    /// Matches found so far.
    #[must_use]
    pub fn matches(&self) -> u64 {
        self.0.submatches
    }

    /// Lines with a match found so far.
    #[must_use]
    pub fn matched_lines(&self) -> u64 {
        self.0.matched_lines
    }
}

/// A window paused at its end: the candidates before `position`, in walk
/// order, are searched; none after it is.
pub struct Pause {
    pub position: u32,
    /// The digest of those candidates' paths and stamps, in walk order, as
    /// the search read them.
    pub pin: String,
    pub searched: Searched,
}

/// How a window ended.
pub enum Windowed {
    /// The search reached its last file (or the caller cancelled it).
    Whole(TextSearchResult),
    Paused(Pause),
}
