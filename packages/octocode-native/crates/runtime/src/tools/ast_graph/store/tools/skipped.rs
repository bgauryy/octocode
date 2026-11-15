//! What a build's walk left out of the graph: the files no graph grammar
//! parses (`leads::unparsed`'s record and sentence, as astSearch names the
//! ones no AST grammar does), and, when the scan parsed no file, what the
//! walk skipped, as every local tool discloses it (`empty_walk`'s diagnosis,
//! `leads::skipped`'s sentence and lead).
use super::*;
use crate::id::ToolId;
use crate::tools::empty_walk::{FileProbe, Globs, Walk, diagnose_shown};
use crate::tools::leads::skipped;
use crate::tools::leads::unparsed::{Reader, Unparsed};

/// A build's reading of a file no grammar parses: no node or edge holds it.
const BUILD: Reader = Reader {
    grammar: "graph grammar",
    unread: ["it is not in the graph", "they are not in the graph"],
};

/// The files of the build's own `walk` of `root` that `keep` leaves out and
/// the build does not read as script-bearing markup (its scan counts those
/// itself, `markupNotLinked`); `None` when `stop` ended the walk, which then
/// proves no count.
fn unparsed(
    root: &Path,
    walk: Walk,
    exclude: &[String],
    paths: &PathPolicy,
    permits: &dyn Fn(&Path, Option<std::fs::FileType>) -> bool,
    keep: &dyn Fn(&Path) -> bool,
    stop: &dyn Fn() -> bool,
) -> Option<Unparsed> {
    let markup = octocode_engine::signatures::languages::SCRIPT_BEARING_MARKUP;
    let entries = walk
        .walker(
            root,
            &crate::policy::prune::walk_names(walk.default_excludes, &[]),
            Globs {
                include: &[],
                exclude,
            },
        )
        .ok()?;
    let mut found = Unparsed::default();
    // The walk is depth first: a store directory's entries follow it.
    let mut store: Option<PathBuf> = None;
    for entry in entries {
        if stop() {
            return None;
        }
        let Ok(entry) = entry else { continue };
        let (path, kind) = (entry.path(), entry.file_type());
        // Octocode's own graph store is no part of any scope, whatever the
        // walk's flags (`prune::is_octocode_artifact`), as the build's walk
        // leaves it out.
        if store.as_ref().is_some_and(|store| path.starts_with(store)) {
            continue;
        }
        let is_dir = kind.is_some_and(|kind| kind.is_dir());
        if crate::policy::prune::is_octocode_artifact(root, path, is_dir) {
            store = Some(path.to_path_buf());
            continue;
        }
        // Every directory on the way is shown to the discovery policy, which
        // decides what lies in it from its real path.
        if !permits(path, kind) || !kind.is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let extension = octocode_engine::text::extension_of(&path.to_string_lossy(), true, "");
        if keep(path) || markup.contains(&extension.as_str()) {
            continue;
        }
        found.record(
            path,
            paths
                .workspace_relative(path)
                .unwrap_or_else(|| path.display().to_string()),
        );
    }
    Some(found)
}

/// The hints of the files a build left out for want of a graph grammar.
/// A build that parsed no file names every one of them; beside parsed files
/// the ones of no known type are named (they could be source) and the rest
/// are a count per type.
fn unparsed_hints(found: &Unparsed, parsed_any: bool) -> Vec<String> {
    let (named, typed) = found.disclosed(&BUILD, parsed_any, None);
    let typed = typed.as_ref().and_then(Value::as_object).map(|counts| {
        let listed = counts
            .iter()
            .map(|(name, files)| format!("{name} {files}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("Files of types with no graph grammar are not in the graph: {listed}.")
    });
    named.into_iter().chain(typed).collect()
}

/// Names what the build of `root` left out. The files its walk met that no
/// graph grammar parses are a hint. When it scanned no file, the graph
/// source files behind its walk's skipped paths (ignore rules, dot entries,
/// the default excludes) are a hint too, with the graphBuild lead that
/// builds them. Both honor the build's `exclude` and `parserGlobs`.
pub(super) fn note_skipped(
    query: &GraphBuildQuery,
    root: &Path,
    paths: &PathPolicy,
    context: &ExecutionContext,
    out: &mut Value,
) {
    if out["id"].is_null() {
        return;
    }
    let parsed_any = out["scan"]["filesScanned"].as_u64().unwrap_or(0) > 0;
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
    if let Some(found) = unparsed(root, walk, &options.exclude, paths, &permits, &keep, &stop) {
        crate::response::paging::push_warnings(out, unparsed_hints(&found, parsed_any));
    }
    if parsed_any {
        return;
    }
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
    crate::response::paging::push_warnings(out, [disclosure.text]);
    crate::response::result::attach_related(out, lead);
}
