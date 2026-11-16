//! A traversal search read as a stream, like `rg --sort path | head` that
//! continues.
//!
//! The default walk order is name-sorted and depth first, so a page needs
//! only the files up to its own end: the walk stops once the page is full
//! ([`crate::tools::local_search::layout::page_fill`]) and the next page continues from
//! where this one ended. The cursor names that place: `walkOffset` (the
//! candidates before the first file with rows not yet shown), `rowOffset`
//! (rows of that file already shown), `scanOffset` (rows listed before), and
//! a `snapshot` whose digest pins the candidates before the cut, each by
//! path and stamp. A continuation walks that prefix again without searching
//! it; a pin that no longer matches restarts the search rather than skip or
//! repeat a row. A file after the cut shows its current text.
use super::{ScanOutput, engine_error};
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::policy::prune::DefaultsFlag;
use crate::response::result::ToolError;
use crate::tools::local_search::cursor::{SNAPSHOT_SCHEME, stale_snapshot};
use crate::tools::local_search::layout::{PageRows, page_fill};
use crate::tools::local_search::local_search_query;
use crate::tools::local_search::tally;
use octocode_contract_types::{ErrorCode, LocalSearchQuery, RegexEngine};
use octocode_engine::search::text_search::{
    Landmark, Pin, Stream, Streamed, Streaming, search_stream,
};
use octocode_engine::types::{SearchSort, TextSearchFile, TextSearchOptions};
use std::sync::Arc;

/// Where a streamed page stands in the walk.
pub(crate) struct StreamFacts {
    /// One per file the stream returned.
    pub(crate) landmarks: Vec<Landmark>,
    /// The candidates before it are searched or passed.
    pub(crate) through: u32,
    /// The walk reached its end.
    pub(crate) ended: bool,
    /// The pin over the candidates before `through`.
    pub(crate) pin: String,
    /// Rows of the first file earlier pages showed.
    pub(crate) start_row: usize,
    /// Rows (files in list views) earlier pages listed.
    pub(crate) listed: usize,
}

/// Whether `query` reads as a stream: the walk order, a linear engine, a
/// view whose rows are decided per file, and no page or judgment that needs
/// the whole scope first. Count views, classification and counted
/// uniqueness rank or total the whole scope, and PCRE2 runs in windows
/// ([`crate::tools::local_search::window`]).
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
        && local_search_query::match_page(query) <= 1
}

/// The query's continuation: where its walk resumes.
struct Resume {
    from: u32,
    start_row: u32,
    listed: u32,
    /// The digest the snapshot carries, when the query has one.
    pin: Option<String>,
}

fn resume_of(query: &LocalSearchQuery) -> Result<Resume, ToolError> {
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
        });
    };
    // Another build cut its pages otherwise: restart.
    let digest = snapshot
        .strip_prefix(SNAPSHOT_SCHEME)
        .and_then(|rest| rest.split_once(':'))
        .filter(|(build, _)| *build == crate::build_identity::BUILD_IDENTITY)
        .map(|(_, digest)| digest.to_owned())
        .ok_or_else(|| stale_snapshot(query))?;
    Ok(Resume {
        from,
        start_row,
        listed,
        pin: Some(digest),
    })
}

/// The snapshot a continuation of this search names for `hex`.
pub(crate) fn snapshot_of(hex: &str) -> String {
    format!(
        "{SNAPSHOT_SCHEME}{}:{hex}",
        crate::build_identity::BUILD_IDENTITY
    )
}

/// Search `options` as a stream from the query's cursor, until its page is
/// full or the walk ends.
pub(super) fn scan_stream(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    options: TextSearchOptions,
    cancel: &impl CancellationCheck,
    (query_key, output_root, page_budget): (&str, &std::path::Path, usize),
) -> Result<ScanOutput, ToolError> {
    let resume = resume_of(query)?;
    let root = std::path::PathBuf::from(&options.path);
    let walk = Arc::new(crate::policy::discovery::WalkAdmission::search(
        paths.clone(),
        root.clone(),
        local_search_query::default_excludes(query).defaults(),
        options.include.as_deref().unwrap_or_default(),
    ));
    let recorder = Arc::new(crate::tools::local_search::walked::Recorder::new(
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
        enough: Box::new(page_fill(query, paths, output_root, (page_budget, start))),
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
    let Streamed {
        result,
        landmarks,
        through,
        ended,
        pin,
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
            start_row: start,
            listed: resume.listed as usize,
        }),
    })
}

/// Where the page after a streamed one continues, as the cursor names it.
pub(crate) struct WalkCut {
    /// Candidates before the first file with rows left to show.
    pub(crate) walk: u32,
    /// Rows of that file the pages so far showed.
    pub(crate) row: usize,
    /// The pin over what the next page walks again before its own start.
    pub(crate) pin: String,
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
            });
        }
        if let Some(landmark) = facts.landmarks.get(index + 1) {
            return Some(WalkCut {
                walk: landmark.position,
                row: 0,
                pin: landmark.before.clone(),
            });
        }
    }
    (!facts.ended).then(|| WalkCut {
        walk: facts.through,
        row: 0,
        pin: facts.pin.clone(),
    })
}
