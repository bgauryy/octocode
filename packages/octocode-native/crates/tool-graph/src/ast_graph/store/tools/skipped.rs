//! What a build's walk left out of the graph: the files no graph grammar
//! parses (`leads::unparsed`'s record and sentence, as astSearch names the
//! ones no AST grammar does), and, when the scan parsed no file, what the
//! walk skipped, as every local tool discloses it (the walk's ledger,
//! `leads::skipped`'s sentence and lead).
use super::*;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::ToolError;
use octocode_tools_shared::leads::skipped;
use octocode_tools_shared::leads::unparsed::{Reader, SourceTypes, Unparsed};

/// A build's reading of a file no grammar parses: no node or edge holds it.
const BUILD: Reader = Reader {
    grammar: "graph grammar",
    unread: ["it is not in the graph", "they are not in the graph"],
};

/// The build's own walk of `root` (its flags, `exclude` and default prune),
/// listing every file: the files `keep` leaves out that the build does not
/// read as script-bearing markup (its scan counts those itself,
/// `markupNotLinked`), and what the walk left out of the graph source files
/// it could hold (ignored, dot and default-excluded paths, as every local
/// walk records them). `None` when `stop` ended the walk, which then proves
/// no count.
fn walk_scope(
    root: &Path,
    options: &IngestOptions,
    paths: &PathPolicy,
    keep: &dyn Fn(&Path) -> bool,
    stop: &(dyn Fn() -> bool + Sync),
) -> Option<(
    Unparsed,
    octocode_runtime_base::policy::discovery::WalkSkips,
)> {
    let markup = octocode_engine::signatures::languages::SCRIPT_BEARING_MARKUP;
    let admission = crate::ast_graph::walk_scope::build_admission(paths, root);
    let walked = octocode_engine::search::fs_query::query_file_system(
        octocode_engine::types::FileSystemQueryOptions {
            path: root.to_string_lossy().into_owned(),
            recursive: Some(true),
            ignore: options.walk.ignore,
            global_ignore: paths.global_ignore(),
            entry_type: Some(octocode_engine::types::EntryType::File),
            default_exclude_dir: Some(octocode_runtime_base::policy::prune::walk_names(
                options.walk.default_excludes,
                &[],
            )),
            exclude: (!options.exclude.is_empty()).then(|| options.exclude.clone()),
            ..Default::default()
        },
        &|path: &Path, kind, _| admission.admit(path, kind, |_| true),
        stop,
    )
    .ok()?;
    admission.record_left_out((&walked.ignored, &walked.hidden, &walked.pruned_dirs), keep);
    let mut found = Unparsed::default();
    for entry in &walked.entries {
        let path = Path::new(&entry.path);
        let extension = octocode_engine::text::extension_of(&entry.path, true, "");
        if keep(path) || markup.contains(&extension.as_str()) {
            continue;
        }
        found.record(path, shown(paths, root, path));
    }
    Some((found, admission.skipped()))
}

/// `path`, under the snapshot `root`, as the build's rows name it: from the
/// workspace, or, outside it, from the root the receipt states.
fn shown(paths: &PathPolicy, root: &Path, path: &Path) -> String {
    paths.workspace_relative(path).unwrap_or_else(|| {
        path.strip_prefix(root).map_or_else(
            |_| path.display().to_string(),
            octocode_engine::text::slash_path,
        )
    })
}

/// The source types a build's disclosure judges unparsed files by: every
/// grammar's beside parsed files; none when it parsed no file, so every
/// such file is named.
fn read_by(parsed_any: bool) -> Option<SourceTypes> {
    parsed_any.then(|| SourceTypes::read_by(None))
}

/// The hints of the files a build left out for want of a graph grammar.
/// A build that parsed no file names every one of them; beside parsed files
/// the ones of no known type are named (they could be source) and the rest
/// are a count per type.
fn unparsed_hints(found: &Unparsed, read: Option<&SourceTypes>) -> Vec<String> {
    let (named, typed) = found.disclosed(&BUILD, read, None);
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
/// graph grammar parses are a hint. When it scanned no file, the paths its
/// walk skipped (ignore rules, dot entries, the default excludes) that could
/// hold a graph source file are named too, with the graphBuild lead that
/// builds them; nothing behind them is read. Both honor the build's
/// `exclude` and `parserGlobs`.
pub(super) fn note_skipped(
    query: &GraphBuildQuery,
    root: &Path,
    paths: &PathPolicy,
    context: &ExecutionContext,
    out: &mut Value,
) -> Result<(), ToolError> {
    if out["id"].is_null() {
        return Ok(());
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
        return Ok(());
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
    let stop = || context.check().is_err();
    let Some((found, walked)) = walk_scope(root, &options, paths, &keep, &stop) else {
        return Ok(());
    };
    let read = read_by(parsed_any);
    octocode_runtime_base::response::paging::push_warnings(
        out,
        unparsed_hints(&found, read.as_ref()),
    );
    // Each file the hint counts is its own `unparsed` entry; the walk
    // finished, so the list is whole.
    // Inside the workspace the response names each entry from it.
    let entries = found
        .skipped(read.as_ref())
        .into_iter()
        .map(|mut entry| {
            if paths.workspace_relative(&entry.path).is_none() {
                entry.path = shown(paths, root, Path::new(&entry.path));
            }
            entry
        })
        .collect();
    if let Some(skipped) = skipped::unparsed_paths(entries, false)? {
        out["skippedPaths"] = serde_json::to_value(skipped).map_err(skipped::skipped_path_error)?;
    }
    if parsed_any {
        return Ok(());
    }
    let Some(disclosure) = skipped::unsearched_ledger(&walked) else {
        return Ok(());
    };
    // The same build with exactly the skipping causes lifted.
    let lead = skipped::lead(ToolId::GraphBuild, query, &disclosure.causes);
    octocode_runtime_base::response::paging::push_warnings(out, [disclosure.text]);
    octocode_runtime_base::response::result::attach_related(out, lead);
    Ok(())
}
