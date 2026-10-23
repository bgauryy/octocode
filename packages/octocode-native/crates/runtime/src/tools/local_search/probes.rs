//! Bounded probe searches of an empty page: other spellings of a literal
//! miss, and matches in the paths the walk skipped.

use super::leads::*;
use super::types::*;
use crate::execution::cancel::CancellationCheck;
use crate::num::u32_of_usize;
use crate::policy::path::PathPolicy;
use crate::policy::prune::DefaultsFlag;
use crate::response::result::ToolError;
use crate::tools::empty_walk::{Cause, Globs, Hits, Tagged, Verdict, Walk, diagnose};
use crate::tools::leads::skipped::{SkippedEntry, skipped_cause, skipped_paths};
use octocode_contract_types::SkippedPaths;
use octocode_engine::search::text_search::search_cancellable;
use octocode_engine::types::{
    CaseRule, PatternSyntax, SearchOutput, SpanValues, TextSearchOptions,
};
use serde_json::{Value, json};
use std::sync::Arc;

mod spellings;

pub(super) use spellings::*;

/// What the re-run of an empty search over its skipped paths found: the
/// files, each tagged with the causes that hid it, and what the path policy
/// withheld from that walk (symlinks it does not follow, credential files).
pub(super) struct Lifted {
    tagged: Tagged,
    withheld: crate::policy::discovery::Withheld,
}

/// Re-run an empty search over what the defaults leave out (ignored and
/// hidden entries, the default-excluded directories), each file found
/// tagged with the causes that hid it ([`probe_lifted`]).
pub(super) fn probe_unsearched(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    search_options: &TextSearchOptions,
    cancel: &impl CancellationCheck,
) -> Result<Lifted, ()> {
    let walk = query_walk(query);
    probe_lifted(
        search_options,
        walk.lifting(&walk.skipping()),
        paths,
        cancel,
    )
}

/// The walk the query ran: the causes it skipped paths by.
fn query_walk(query: &LocalSearchQuery) -> Walk {
    Walk {
        ignored: query.include_ignored == Some(true),
        hidden: query.include_hidden == Some(true),
        default_excludes: query.default_excludes.defaults(),
    }
}

/// Say which skipped paths the re-run of an empty search
/// ([`probe_unsearched`]) found a match in: the hint names the rule, entry
/// or directory, and the lead searches again with only those lifted.
/// Default-excluded directories the re-run could not finish are named with
/// the lead that searches them. Returns whether skipped paths were shown to
/// hold a match.
pub(super) fn disclose_unsearched(
    query: &LocalSearchQuery,
    search_options: &TextSearchOptions,
    lifted: Result<Lifted, ()>,
    skipped: Skipped<'_>,
    next: &mut Option<Value>,
    hints: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Result<Unsearched, ToolError> {
    let walk = query_walk(query);
    let root = std::path::PathBuf::from(&search_options.path);
    let verdict = diagnose(&root, walk, |relaxed| {
        Ok::<_, std::convert::Infallible>(lifted.as_ref().map_or_else(
            |()| Hits {
                cut: true,
                ..Hits::default()
            },
            |lifted| lifted.tagged.find(relaxed, None),
        ))
    });
    let Ok(verdict) = verdict;
    if let Verdict::Absent = verdict {
        if let Ok(lifted) = &lifted {
            absent(query, &skipped, &lifted.withheld, hints, warnings);
        }
        return Ok(Unsearched::default());
    }
    let subject = if query.result_view == LocalSearchQueryResultView::FilesWithout {
        ["file that lacks a match", "files that lack a match"]
    } else {
        ["file that matches", "files that match"]
    };
    let named = skipped.walk.counts();
    let Some(disclosure) = crate::tools::leads::skipped::disclose(
        &verdict,
        subject,
        crate::tools::leads::skipped::Unchecked {
            causes: &skipped.causes,
            named: &named,
        },
    ) else {
        return Ok(Unsearched::default());
    };
    insert_lead(
        next,
        crate::tools::leads::skipped::lead(
            crate::id::ToolId::LocalSearch,
            query,
            &disclosure.causes,
        ),
    );
    // A check that ran out of time names the ignored directories a fresh
    // bounded walk finds, with their rules; a finished one names the rules
    // behind the files it found.
    let (named, unchecked) = match &verdict {
        Verdict::Skipped(diagnosis) => (diagnosis.skips().to_vec(), false),
        _ => (unsearched_ignored(search_options, walk), true),
    };
    if unchecked {
        warnings.push(disclosure.text);
    } else {
        hints.insert(0, disclosure.text);
    }
    Ok(Unsearched {
        may_hold_matches: true,
        unchecked,
        observed: observed_paths(query, &named)?,
    })
}

/// The check of the skipped paths searched them and found nothing, so no
/// lead lifts them: the hint says so, and what the path policy withheld
/// from that walk (a skipped directory of symlinks) is counted in a warning,
/// since absence there is unproven.
fn absent(
    query: &LocalSearchQuery,
    skipped: &Skipped<'_>,
    withheld: &crate::policy::discovery::Withheld,
    hints: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    let counts = skipped.walk.counts();
    if counts.is_empty() {
        return;
    }
    let what = if query.result_view == LocalSearchQueryResultView::FilesWithout {
        "file lacking a match"
    } else {
        "match"
    };
    let hint = match withheld.notice() {
        Some(notice) => {
            warnings.push(format!("Skipped paths: {notice}"));
            format!(
                "Skipped paths ({counts}) hold no {what} outside what their check withheld (warnings)."
            )
        }
        None => format!("Skipped paths ({counts}) hold no {what} either; the check searched them."),
    };
    hints.insert(0, hint);
}

/// What a page's walk skipped, by cause, for the empty page's check of it.
pub(super) struct Skipped<'a> {
    pub(super) walk: &'a crate::policy::discovery::WalkSkips,
    /// The causes it skipped paths by: what a lead over them lifts.
    pub(super) causes: Vec<Cause>,
}

/// What the empty page's check of the skipped paths found.
#[derive(Default)]
pub(super) struct Unsearched {
    /// Skipped paths hold a match, or may: the check did not finish.
    pub(super) may_hold_matches: bool,
    /// The check did not finish: the skipped paths are unsearched scope
    /// ([`crate::tools::leads::skipped::SKIPPED_UNCHECKED`]).
    pub(super) unchecked: bool,
    /// The skipped paths the check named, each with its rule.
    pub(super) observed: Option<SkippedPaths>,
}

fn insert_lead(next: &mut Option<Value>, lead: Value) {
    if let Some(map) = next.get_or_insert_with(|| json!({})).as_object_mut() {
        map.insert("includeSkipped".into(), lead);
    }
}

/// The skipped paths a check named, one entry per cause and rule (its first
/// path, with `count` when the rule hides more), or every path with
/// `extras:["details"]`; the `includeSkipped` lead lifts them all either way.
/// A check's findings are samples (`sampled`).
fn observed_paths(
    query: &LocalSearchQuery,
    skipped: &[crate::tools::empty_walk::Skip],
) -> Result<Option<SkippedPaths>, ToolError> {
    skipped_paths(
        skipped.iter().map(|skip| {
            SkippedEntry::path(
                octocode_engine::text::slash_path(&skip.entry),
                skipped_cause(skip.cause),
            )
            .because(skip.reason.clone())
        }),
        lists_every_skipped_path(query),
        true,
    )
}

/// The ignored directories a walk skipped that could hold a path the
/// search's include globs match, named, within a fresh
/// [`PROBE_BUDGET`](crate::tools::empty_walk::PROBE_BUDGET) of their own
/// (the probe that ran out of time left none of the diagnosis' budget): the
/// rules the lead lifts.
fn unsearched_ignored(
    options: &TextSearchOptions,
    walk: Walk,
) -> Vec<crate::tools::empty_walk::Skip> {
    let root = std::path::Path::new(&options.path);
    let scope = octocode_engine::search::glob::IncludeScope::new(
        root,
        options.include.as_deref().unwrap_or_default(),
    );
    let started = std::time::Instant::now();
    let allowance = crate::tools::empty_walk::PROBE_BUDGET;
    let mut skips = crate::tools::empty_walk::ignored_directories(root, walk, &|| {
        started.elapsed() > allowance
    });
    skips.retain(|skip| {
        scope.reaches(
            &skip.entry,
            skip.entry.strip_prefix(root).unwrap_or(&skip.entry),
            true,
        )
    });
    skips
}

/// One bounded probe: `options` over the query's own walk (`paths`, the
/// default excludes when `defaults`, the include globs), stopped by `cancel`
/// or the probe budget. `None` when it cannot run; the flag says the budget
/// cut it, so every count it makes is a floor.
pub(super) fn probe_search(
    options: TextSearchOptions,
    paths: &PathPolicy,
    defaults: bool,
    cancel: &impl CancellationCheck,
) -> Option<(octocode_engine::types::TextSearchResult, bool)> {
    probe_walked(options, paths, defaults, cancel).map(|(found, cut, _)| (found, cut))
}

/// [`probe_search`] with what the path policy withheld from its walk.
fn probe_walked(
    options: TextSearchOptions,
    paths: &PathPolicy,
    defaults: bool,
    cancel: &impl CancellationCheck,
) -> Option<(
    octocode_engine::types::TextSearchResult,
    bool,
    crate::policy::discovery::Withheld,
)> {
    let walk = Arc::new(crate::policy::discovery::WalkAdmission::search(
        paths.clone(),
        std::path::PathBuf::from(&options.path),
        defaults,
        options.include.as_deref().unwrap_or_default(),
    ));
    let started = std::time::Instant::now();
    let deadline = crate::tools::empty_walk::probe_budget();
    let cut = std::sync::atomic::AtomicBool::new(false);
    let found = crate::tools::leads::best_effort(|| {
        search_cancellable(options, walk.clone(), &|| {
            let late = started.elapsed() > deadline;
            if late {
                cut.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            cancel.check().is_err() || late
        })
    })?
    .ok()?;
    Some((
        found,
        cut.load(std::sync::atomic::Ordering::Relaxed),
        walk.skipped().withheld,
    ))
}

/// The files `options`' search finds under the fully relaxed `walk` (files
/// only), tagged with the causes that hide each ([`Tagged::found`]). A search
/// that cannot run is `Err`; one cut by its deadline keeps what it found and
/// answers every relaxed walk as cut.
fn probe_lifted(
    options: &TextSearchOptions,
    walk: Walk,
    paths: &PathPolicy,
    cancel: &impl CancellationCheck,
) -> Result<Lifted, ()> {
    // The files the row's view lists: those without a match for a
    // `filesWithout` row, else those with one.
    let mut lifted = options.probe(if options.output == SearchOutput::FilesWithoutMatch {
        SearchOutput::FilesWithoutMatch
    } else {
        SearchOutput::FilesWithMatch
    });
    lifted.ignore = octocode_engine::search::walk::IgnorePolicy {
        include_ignored: walk.ignored,
        include_hidden: walk.hidden,
    };
    let root = std::path::PathBuf::from(&options.path);
    let globs = Globs {
        include: options.include.as_deref().unwrap_or_default(),
        exclude: options.exclude.as_deref().unwrap_or_default(),
    };
    let (searched, _, withheld) =
        probe_walked(lifted, paths, walk.default_excludes, cancel).ok_or(())?;
    let cut = !searched.stats.cap_reasons.is_empty();
    let tagged = Tagged::found(
        &root,
        globs,
        searched.files.iter().map(|file| root.join(&file.path)),
        cut,
    )
    .map_err(|_| ())?;
    Ok(Lifted { tagged, withheld })
}
