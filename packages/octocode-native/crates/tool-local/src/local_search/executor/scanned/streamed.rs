//! A traversal search read as a stream, like `rg --sort path | head` that
//! continues.
//!
//! The default walk order is name-sorted and depth first, so a page needs
//! only the files up to its own end: the walk stops once the page is full
//! ([`crate::local_search::layout::page_fill`]) and the next page continues from
//! where this one ended. The cursor names that place: `walkOffset` (the
//! candidates before the first file with rows not yet shown), `rowOffset`
//! (rows of that file already shown), `scanOffset` (rows listed before), and
//! a `snapshot` that names where the page before began and whose digest pins
//! that page's candidates up to the cut, each by path and stamp. A
//! continuation walks from there, replays that page without searching it,
//! and goes on; a pin that no longer matches restarts the search rather than
//! skip or repeat a row. A file after the cut shows its current text.
use super::{ScanOutput, engine_error};
use crate::local_search::cursor::{SNAPSHOT_SCHEME, stale_snapshot};
use crate::local_search::layout::{PageRows, StreamPricer, page_fill};
use crate::local_search::local_search_query;
use crate::local_search::tally;
use octocode_contract_types::{ErrorCode, LocalSearchQuery, RegexEngine};
use octocode_engine::search::text_search::{
    Anchor, Landmark, Pin, Stream, Streamed, Streaming, search_stream,
};
use octocode_engine::types::{SearchSort, TextSearchFile, TextSearchOptions};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::policy::prune::DefaultsFlag;
use octocode_runtime_base::response::result::ToolError;
use std::sync::Arc;

/// Where a streamed page stands in the walk.
pub(crate) struct StreamFacts {
    /// One per file the stream returned.
    pub(crate) landmarks: Vec<Landmark>,
    /// The candidates before it are searched or passed.
    pub(crate) through: u32,
    /// The walk reached its end.
    pub(crate) ended: bool,
    /// The pin over the page's candidates before `through`.
    pub(crate) pin: String,
    /// Where the page began, for the page that continues it.
    pub(crate) anchor: Anchor,
    /// Rows of the first file earlier pages showed.
    pub(crate) start_row: usize,
    /// Rows (files in list views) earlier pages listed.
    pub(crate) listed: usize,
    /// What the pages before carried past their cut: whether rows were
    /// owed at this page's start (a page that finds none there finds them
    /// changed since), and what they counted.
    pub(crate) carried: Carried,
}

/// What a walk's cursor carries from the pages before to the page it
/// continues with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Carried {
    /// The page before found rows at its cut that it did not show.
    pub(crate) rows_owed: bool,
    /// What the pages before counted, up to the cut.
    pub(crate) counted: Counted,
}

/// What a walk's pages counted: each page adds its own scan to what the
/// pages before it counted, so a page states the files walked so far and
/// the last one the walk's totals.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counted {
    /// Files searched.
    pub(crate) files: u32,
    pub(crate) occurrences: u32,
    pub(crate) lines: u32,
    /// Masked secret spans in the searched files.
    pub(crate) masked: u32,
    /// Files holding a hit.
    pub(crate) matched: u32,
    /// Some page missed content (a cap, an unreadable file): the count of
    /// matched files is not final.
    pub(crate) gap: bool,
}

impl Counted {
    /// These counts and what one page's scan counted.
    pub(crate) fn with(self, scanned: &octocode_engine::types::TextSearchStats) -> Self {
        Self {
            files: self
                .files
                .saturating_add(scanned.files_searched.unwrap_or(0)),
            occurrences: self
                .occurrences
                .saturating_add(scanned.match_count.unwrap_or(0)),
            lines: self
                .lines
                .saturating_add(scanned.matched_lines.unwrap_or(0)),
            masked: self
                .masked
                .saturating_add(scanned.masked_spans.unwrap_or(0)),
            matched: self
                .matched
                .saturating_add(scanned.files_matched.unwrap_or(0)),
            gap: self.gap || scanned.capped == Some(true) || scanned.error_count.unwrap_or(0) > 0,
        }
    }

    /// These counts without one searched file's: the file the next page
    /// searches again.
    fn without(self, file: octocode_engine::types::FileCounts) -> Self {
        let matched = u32::from(file.occurrences > 0 || file.lines > 0);
        Self {
            files: self.files.saturating_sub(1),
            occurrences: self.occurrences.saturating_sub(file.occurrences),
            lines: self.lines.saturating_sub(file.lines),
            masked: self.masked.saturating_sub(file.masked),
            matched: self.matched.saturating_sub(matched),
            gap: self.gap,
        }
    }

    /// `scanned` (one page's scan) with the walk's counts through it in
    /// place of the page's own; a count the page's view does not keep stays
    /// absent.
    pub(crate) fn stated(
        self,
        scanned: &octocode_engine::types::TextSearchStats,
    ) -> octocode_engine::types::TextSearchStats {
        let walked = self.with(scanned);
        octocode_engine::types::TextSearchStats {
            files_searched: Some(walked.files),
            match_count: scanned.match_count.map(|_| walked.occurrences),
            matched_lines: scanned.matched_lines.map(|_| walked.lines),
            files_matched: scanned.files_matched.map(|_| walked.matched),
            masked_spans: scanned.masked_spans.map(|_| walked.masked),
            ..scanned.clone()
        }
    }

    fn encode(&self) -> String {
        format!(
            "{}.{}.{}.{}.{}.{}",
            self.files,
            self.occurrences,
            self.lines,
            self.masked,
            self.matched,
            u8::from(self.gap)
        )
    }

    fn decode(text: &str) -> Option<Self> {
        let mut parts = text.split('.').map(|part| {
            part.bytes()
                .all(|byte| byte.is_ascii_digit())
                .then(|| part.parse::<u32>().ok())
                .flatten()
        });
        let mut next = || parts.next().flatten();
        let counted = Self {
            files: next()?,
            occurrences: next()?,
            lines: next()?,
            masked: next()?,
            matched: next()?,
            gap: match next()? {
                0 => false,
                1 => true,
                _ => return None,
            },
        };
        parts.next().is_none().then_some(counted)
    }
}

/// Whether `query` reads as a stream: the walk order, a linear engine, a
/// view whose rows are decided per file, and no page or judgment that needs
/// the whole scope first. Count views, classification and counted
/// uniqueness rank or total the whole scope, and PCRE2 runs in windows
/// ([`crate::local_search::window`]).
pub(crate) fn streams(query: &LocalSearchQuery) -> bool {
    use local_search_query::View;
    local_search_query::sort(query) == SearchSort::Traversal
        && local_search_query::regex_mode(query) != RegexEngine::Pcre2
        && matches!(
            local_search_query::view(query),
            View::Content | View::MatchOnly | View::Files | View::FilesWithout
        )
        && !local_search_query::has_classification(query)
        && local_search_query::selection(query).is_none()
        && !tally::counts_all(query)
        && local_search_query::page(query) <= 1
}

/// The query's continuation: where its walk resumes.
struct Resume {
    from: u32,
    start_row: u32,
    listed: u32,
    /// The digest the snapshot carries, when the query has one.
    pin: Option<String>,
    /// Where the page before began.
    anchor: Anchor,
    /// What the pages before carried past their cut.
    carried: Carried,
}

fn resume_of(build: &str, query: &LocalSearchQuery) -> Result<Resume, ToolError> {
    let (from, start_row, listed) = (
        local_search_query::walk_offset(query).unwrap_or(0),
        local_search_query::row_offset(query).unwrap_or(0),
        local_search_query::scan_offset(query).unwrap_or(0),
    );
    let Some(snapshot) = local_search_query::snapshot(query) else {
        if from > 0 || start_row > 0 || listed > 0 {
            return Err(ToolError::new(
                ErrorCode::InvalidInput,
                "walkOffset, rowOffset and scanOffset continue a search from its snapshot; drop them to search again",
            ));
        }
        return Ok(Resume {
            from,
            start_row,
            listed,
            pin: None,
            anchor: Anchor::START,
            carried: Carried::default(),
        });
    };
    // Another build cut its pages otherwise: restart.
    let (digest, (anchor, carried)) = snapshot
        .strip_prefix(SNAPSHOT_SCHEME)
        .and_then(|rest| rest.split_once(':'))
        .filter(|(minted, _)| *minted == build)
        .and_then(|(_, pinned)| match pinned.split_once(':') {
            Some((digest, anchor)) => Some((digest.to_owned(), decode_anchor(anchor)?)),
            None => Some((pinned.to_owned(), (Anchor::START, Carried::default()))),
        })
        .ok_or_else(|| stale_snapshot(query))?;
    Ok(Resume {
        from,
        start_row,
        listed,
        pin: Some(digest),
        anchor,
        carried,
    })
}

/// Marks a snapshot whose page before found rows at the cut it did not show.
const ROWS_OWED: char = '+';
/// Starts what the pages before counted, after the anchor.
const COUNTED: char = ';';

/// The snapshot a continuation of this search names for the pin `hex` over
/// the page that began at `anchor`, with what the pages so far `carried`
/// past their cut.
pub(crate) fn snapshot_of(build: &str, hex: &str, anchor: &Anchor, carried: &Carried) -> String {
    let owed = if carried.rows_owed {
        ROWS_OWED.to_string()
    } else {
        String::new()
    };
    format!(
        "{SNAPSHOT_SCHEME}{}:{hex}:{}{owed}{COUNTED}{}",
        build,
        anchor.encode(),
        carried.counted.encode()
    )
}

/// The anchor part of a walk's snapshot as [`snapshot_of`] wrote it, and
/// what the pages before carried past their cut; `None` for any other text.
pub(crate) fn decode_anchor(text: &str) -> Option<(Anchor, Carried)> {
    let (anchor, counted) = match text.split_once(COUNTED) {
        Some((anchor, counted)) => (anchor, Counted::decode(counted)?),
        None => (text, Counted::default()),
    };
    let (anchor, rows_owed) = anchor
        .strip_suffix(ROWS_OWED)
        .map_or((anchor, false), |anchor| (anchor, true));
    Some((Anchor::decode(anchor)?, Carried { rows_owed, counted }))
}

/// Search `options` as a stream from the query's cursor, until its page is
/// full or the walk ends.
pub(super) fn scan_stream(
    build: &str,
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    options: TextSearchOptions,
    cancel: &impl CancellationCheck,
    (query_key, output_root, page_budget): (&str, &std::path::Path, usize),
    limits: octocode_runtime_base::security::SourceLimits,
) -> Result<ScanOutput, ToolError> {
    let resume = resume_of(build, query)?;
    let root = std::path::PathBuf::from(&options.path);
    let walk = Arc::new(
        octocode_runtime_base::policy::discovery::WalkAdmission::search(
            paths.clone(),
            root.clone(),
            local_search_query::default_excludes(query).defaults(),
            options.include.as_deref().unwrap_or_default(),
        ),
    );
    let recorder = Arc::new(crate::local_search::walked::Recorder::new(
        Arc::clone(&walk),
        paths.clone(),
        root.clone(),
        local_search_query::include_ignored(query) != Some(true),
        octocode_engine::memory::MEMORY.limit(),
    ));
    let start = resume.start_row as usize;
    let covers = resume.from + u32::from(start > 0);
    let stream = Stream {
        bind: query_key.to_owned(),
        from: resume.from,
        pin: resume.pin.map(|hex| Pin { covers, hex }),
        anchor: resume.anchor,
        enough: Box::new(page_fill(
            query,
            paths,
            output_root,
            (page_budget, start),
            stream_pricer(paths, output_root, limits),
        )),
        prepare: stream_preparer(query, paths, output_root, limits),
    };
    let streamed = search_stream(
        options,
        recorder.clone(),
        &|| cancel.check().is_err(),
        stream,
    )
    .map_err(|error| engine_error(query, &error, root.is_file()))?;
    let Streaming::Streamed(streamed) = streamed else {
        return Err(stale_snapshot(query));
    };
    #[cfg(test)]
    SEARCHED.with(|searched| {
        searched.set(searched.get() + u64::from(streamed.result.stats.files_searched.unwrap_or(0)));
    });
    let Streamed {
        result,
        landmarks,
        through,
        ended,
        pin,
        anchor,
    } = *streamed;
    cancel.check().map_err(ToolError::cancelled)?;
    // Only a whole walk that found nothing is offered to the probes of its
    // miss; a page of hits keeps nothing.
    let whole_miss = ended && resume.from == 0 && start == 0 && result.files.is_empty();
    Ok(ScanOutput {
        parsed: result,
        digests: None,
        skipped: Arc::new(walk.skipped()),
        stored: None,
        walked: whole_miss.then(|| recorder.walked()),
        paused: None,
        seek: None,
        stream: Some(StreamFacts {
            landmarks,
            through,
            ended,
            pin,
            anchor,
            start_row: start,
            listed: resume.listed as usize,
            carried: resume.carried,
        }),
    })
}

#[cfg(test)]
thread_local! {
    /// Files this thread's streamed pages searched, each page's whole scan,
    /// for tests that pin how much a walk searches again.
    static SEARCHED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Files streamed pages searched on this thread so far.
#[cfg(test)]
pub(crate) fn searched_on_this_thread() -> u64 {
    SEARCHED.with(std::cell::Cell::get)
}

/// What prices the enclosing names of each file the stream commits, as the
/// page cut will ([`crate::local_search::verify::stream_price`]).
fn stream_pricer(
    paths: &PathPolicy,
    output_root: &std::path::Path,
    limits: octocode_runtime_base::security::SourceLimits,
) -> StreamPricer {
    let (paths, output_root) = (paths.clone(), output_root.to_path_buf());
    let security = octocode_runtime_base::security::ContentSecurity::with_limits(limits);
    Box::new(move |file| {
        crate::local_search::verify::stream_price(&paths, &output_root, &security, file)
    })
}

/// What outlines each result file on the thread that searched it, ahead of
/// the pricer ([`crate::local_search::verify::prepare_outline`]): only a page
/// whose rows name their enclosing declarations needs outlines.
fn stream_preparer(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    output_root: &std::path::Path,
    limits: octocode_runtime_base::security::SourceLimits,
) -> Option<octocode_engine::search::text_search::Prepare> {
    if local_search_query::view(query) != local_search_query::View::Content {
        return None;
    }
    let (paths, output_root) = (paths.clone(), output_root.to_path_buf());
    let security = octocode_runtime_base::security::ContentSecurity::with_limits(limits);
    Some(Box::new(move |path| {
        crate::local_search::verify::prepare_outline(&paths, &output_root, &security, path);
    }))
}

/// Where the page after a streamed one continues, as the cursor names it.
pub(crate) struct WalkCut {
    /// Candidates before the first file with rows left to show.
    pub(crate) walk: u32,
    /// Rows of that file the pages so far showed.
    pub(crate) row: usize,
    /// The pin over what the next page walks again before its own start.
    pub(crate) pin: String,
    /// Where the page the cut ends began: the next page walks from there.
    pub(crate) anchor: Anchor,
    /// What the walk carries past the cut: whether rows were owed there,
    /// and what the pages so far counted up to it (set once the page is
    /// verified, [`Self::count`]).
    pub(crate) carried: Carried,
    /// What each returned file from the cut on counted: the next page
    /// searches them again.
    again: Vec<octocode_engine::types::FileCounts>,
    /// The files this page searched before the cut, when the cut is at a
    /// file it returned: the next page searches that file and every
    /// candidate after it again, those without a hit included.
    searched: Option<u32>,
}

impl WalkCut {
    /// What the pages through this one counted up to the cut: `before` (the
    /// pages before it) and `scanned` (its own scan, as verified), less the
    /// files from the cut on, which the next page searches again.
    pub(crate) fn count(
        &mut self,
        before: Counted,
        scanned: &octocode_engine::types::TextSearchStats,
    ) {
        let counted = self
            .again
            .iter()
            .fold(before.with(scanned), |counted, file| counted.without(*file));
        self.carried.counted = match self.searched {
            Some(searched) => Counted {
                files: before.files.saturating_add(searched),
                ..counted
            },
            None => counted,
        };
    }
}

/// The cut a streamed page ended at: the file it stopped inside, the first
/// file it did not show, or the walk's end past the last file it read.
/// `None` when nothing remains. `shown` are the page's rows by index into
/// `files`, in page order; `list` views show whole files.
pub(crate) fn walk_cut(
    facts: &StreamFacts,
    files: &[TextSearchFile],
    shown: &PageRows,
    list: bool,
) -> Option<WalkCut> {
    if let Some((index, rows)) = shown.last() {
        if !list && rows.end < files[*index].matches.len() {
            let landmark = &facts.landmarks[*index];
            return Some(WalkCut {
                walk: landmark.position,
                row: rows.end,
                pin: landmark.after.clone(),
                anchor: facts.anchor.clone(),
                carried: Carried {
                    rows_owed: true,
                    ..Carried::default()
                },
                again: files[*index..].iter().map(|file| file.counted).collect(),
                searched: Some(landmark.searched),
            });
        }
        if let Some(landmark) = facts.landmarks.get(index + 1) {
            return Some(WalkCut {
                walk: landmark.position,
                row: 0,
                pin: landmark.before.clone(),
                anchor: facts.anchor.clone(),
                carried: Carried {
                    rows_owed: true,
                    ..Carried::default()
                },
                again: files[index + 1..].iter().map(|file| file.counted).collect(),
                searched: Some(landmark.searched),
            });
        }
    }
    (!facts.ended).then(|| WalkCut {
        walk: facts.through,
        row: 0,
        pin: facts.pin.clone(),
        anchor: facts.anchor.clone(),
        carried: Carried::default(),
        again: Vec::new(),
        searched: None,
    })
}
