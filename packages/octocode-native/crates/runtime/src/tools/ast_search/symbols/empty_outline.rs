use super::ast_search_query_symbols;
use super::directory::outlinable;
use crate::id::ToolId;
use crate::policy::prune::DefaultsFlag;
use crate::tools::empty_walk::{FileProbe, Globs, Walk, diagnose};
use crate::tools::leads::skipped;
use crate::{execution::cancel::CancellationCheck, policy::path::PathPolicy};
use ignore::gitignore::Gitignore;
use octocode_contract_types::AstSearchQuerySymbols;
use serde_json::Value;

/// An empty directory outline cannot prove absence in what its walk skipped:
/// the scan honors `.gitignore` and the default prune, and hides dot entries.
/// When files of an outlined language spell the searched name behind them,
/// the hint names why, and `includeSkipped` (as `match` and the text tools
/// offer it) is the same scope with the fewest flags that reach them
/// (`includeIgnored`, `includeHidden`, `defaultExcludes:false`), probed with
/// its own filters first, so it is never offered empty.
pub(super) fn note_skipped(
    q: &AstSearchQuerySymbols,
    root: &std::path::Path,
    mapped: Option<&Gitignore>,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    out: &mut Value,
) {
    if out["status"] != "empty" {
        return;
    }
    let outlined = octocode_engine::signatures::graph_facts::graph_fact_extensions();
    let filter = ast_search_query_symbols::name_filter(q);
    let budget = usize::MAX;
    let exclude = ast_search_query_symbols::exclude(q).unwrap_or_default();
    let discovery = paths.discovery_walk();
    let permits = |path: &std::path::Path| discovery.permits(path);
    let stop = || cancel.check().is_err();
    // A file counts when it can declare the name: an outlined language that
    // spells it, like the scan's own prefilter.
    let keep = |path: &std::path::Path| {
        outlinable(&outlined, mapped, path)
            && filter.as_ref().is_none_or(|filter| {
                crate::tools::ast_search::source::read_parse_source(path)
                    .ok()
                    .flatten()
                    .is_some_and(|bytes| {
                        crate::tools::ast_search::source::parse_text(path, &bytes)
                            .is_ok_and(|text| filter.spelled_in(&text))
                    })
            })
    };
    // What the outline scan itself would reach from `at` under `walk`: it
    // applies the ignore rules found from its own root (`at`), which a probe
    // rooted there does not read for files below the root.
    let probe_at = |at: &std::path::Path, exclude: &[String], walk: Walk| {
        let rules = (!walk.ignored && at.is_dir())
            .then(|| octocode_engine::search::walk::IgnoreRules::new(at));
        let permitted = |path: &std::path::Path| {
            permits(path)
                && rules
                    .as_ref()
                    .is_none_or(|rules| !rules.is_ignored(path, path.is_dir()))
        };
        FileProbe::new(at, budget, &permitted, &keep)
            .stopping(&stop)
            .within(Globs {
                include: &[],
                exclude,
            })
            .find(walk)
            .is_ok_and(|found| found.count > 0)
    };
    let walk = Walk {
        ignored: q.include_ignored.unwrap_or(false),
        hidden: q.include_hidden.unwrap_or(false),
        default_excludes: q.default_excludes.defaults(),
    };
    let scanned = FileProbe::new(root, budget, &permits, &keep)
        .stopping(&stop)
        .within(Globs {
            include: &[],
            exclude: &exclude,
        })
        .beyond(walk);
    let Ok(verdict) = diagnose(root, walk, |relaxed| scanned.find(relaxed)) else {
        return;
    };
    // A check cut before it found anything proved nothing: every cause the
    // walk skipped by is unproven.
    let skipping = walk.skipping();
    let named = skipping
        .iter()
        .map(|cause| cause.label())
        .collect::<Vec<_>>()
        .join(", ");
    let Some(disclosure) = skipped::disclose(
        &verdict,
        ["outlinable file", "outlinable files"],
        skipped::Unchecked {
            causes: &skipping,
            named: &named,
        },
    ) else {
        return;
    };
    // The same scope with exactly the flags the diagnosis found the files
    // behind, probed as the outline scan walks it, so it is never empty.
    if disclosure.unchecked || probe_at(root, &exclude, walk.lifting(&disclosure.causes)) {
        let lead = skipped::lead(ToolId::AstSearch, q, &disclosure.causes);
        skipped::attach(out, &disclosure, lead);
    } else {
        crate::tools::leads::push_hint(out, disclosure.text);
    }
    // The response keeps the first leads of `next`: the text fallback goes
    // after the leads that lift what the outline skipped.
    if let Some(next) = out["calls"].as_object_mut()
        && let Some(fallback) = next.shift_remove("findText")
    {
        next.insert("findText".to_owned(), fallback);
    }
}
