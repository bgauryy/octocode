//! An empty build's skipped paths: a scan that parsed no file says what its
//! walk skipped, as every local tool discloses it (`empty_walk`'s diagnosis,
//! `leads::skipped`'s sentence and lead).
use super::*;
use crate::id::ToolId;
use crate::tools::empty_walk::{FileProbe, Globs, Walk, diagnose_shown};
use crate::tools::leads::skipped;

/// When the build of `root` scanned no file, the graph source files behind
/// its walk's skipped paths (ignore rules, dot entries, the default
/// excludes), named as a hint with a structureSearch lead that lists them.
/// The probe honors the build's `exclude` and `parserGlobs`.
pub(super) fn note_skipped(
    query: &GraphBuildQuery,
    root: &Path,
    paths: &PathPolicy,
    context: &ExecutionContext,
    out: &mut Value,
) {
    if out["id"].is_null() || out["scan"]["filesScanned"].as_u64().unwrap_or(0) > 0 {
        return;
    }
    let options = IngestOptions::of_query(query, root.to_path_buf());
    let Ok(mapped) = octocode_engine::search::glob::set(
        root,
        options
            .language_globs
            .values()
            .flatten()
            .map(String::as_str),
        "parserGlobs",
    ) else {
        return;
    };
    let parsed = octocode_engine::signatures::graph_facts::graph_fact_extensions();
    let keep = |path: &Path| {
        parsed.contains(&octocode_engine::text::extension_of(
            &path.to_string_lossy(),
            true,
            "",
        )) || mapped
            .as_ref()
            .is_some_and(|set| octocode_engine::search::glob::matches(set, path, false))
    };
    let discovery = paths.discovery_walk();
    let permits =
        |path: &Path, kind: Option<std::fs::FileType>| discovery.permits_entry(path, kind);
    let stop = || context.check().is_err();
    // The build's own walk, as the query set it (`graph::scan::scan_options`):
    // only what it still skips is named.
    let walk = Walk {
        ignored: options.walk.ignore.include_ignored,
        hidden: options.walk.ignore.include_hidden,
        default_excludes: options.walk.default_excludes,
    };
    let probe = FileProbe::new(root, &permits, &keep)
        .stopping(&stop)
        .within(Globs {
            include: &[],
            exclude: &options.exclude,
        })
        .beyond(walk);
    // The rules are named as the receipt names its root: workspace-relative.
    let shown = |path: &Path| {
        paths
            .workspace_relative(path)
            .unwrap_or_else(|| path.display().to_string())
    };
    let Ok(verdict) = diagnose_shown(root, walk, &shown, |relaxed| probe.find(relaxed)) else {
        return;
    };
    let skipping = walk.skipping();
    let named = skipping
        .iter()
        .map(|cause| cause.label())
        .collect::<Vec<_>>()
        .join(", ");
    let Some(disclosure) = skipped::disclose(
        &verdict,
        ["graph source file", "graph source files"],
        skipped::Unchecked {
            causes: &skipping,
            named: &named,
        },
    ) else {
        return;
    };
    // The same build with exactly the skipping causes lifted.
    let lead = skipped::lead(ToolId::GraphBuild, query, &disclosure.causes);
    crate::response::result::add_hints(out, [disclosure.text]);
    crate::response::result::attach_related(out, lead);
}
