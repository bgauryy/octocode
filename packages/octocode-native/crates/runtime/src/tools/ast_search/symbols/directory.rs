use super::ast_search_query_symbols;
use super::outline::{Outline, OutlineFile, declarations, reduce_document};
use crate::policy::prune::DefaultsFlag;
use crate::response::result::ToolError;
use crate::{
    execution::cancel::CancellationCheck,
    policy::{path::PathPolicy, prune},
};
use ignore::gitignore::Gitignore;
use octocode_contract_types::AstSearchQuerySymbols;
use octocode_engine::types::GraphFactsScanOptions;
use serde_json::json;

/// The engine walk options a directory outline scans with.
fn directory_scan_options(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
) -> GraphFactsScanOptions {
    GraphFactsScanOptions {
        path: canonical.to_string_lossy().into_owned(),
        exclude_dir: None,
        default_exclude_dir: Some(prune::walk_names(q.default_excludes.defaults(), &[])),
        exclude: ast_search_query_symbols::exclude(q),
        max_files: ast_search_query_symbols::max_files(q),
        skip_files: Some(ast_search_query_symbols::scan_offset(q)),
        max_file_bytes: u32::try_from(crate::tools::ast_search::source::MAX_PARSE_SOURCE_BYTES)
            .ok(),
        language_globs: ast_search_query_symbols::language_globs(q)
            .map(crate::tools::ast_rule::language_globs),
        // The ignore rules `match` and the text tools walk by, off only with
        // `includeIgnored`.
        ignore: octocode_engine::search::walk::IgnorePolicy::new(
            q.include_ignored,
            q.include_hidden,
        ),
    }
}

/// Every file's declarations under a directory (see [`outline_walked`]).
#[cfg(test)]
pub(crate) fn outline_directory(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<Outline, ToolError> {
    let walk = walk_directory(q, canonical, paths, cancel)?;
    outline_walked(q, canonical, walk, cancel)
}

/// The candidate files a directory outline scans, in walk order, and the
/// entries the path policy withheld from that walk.
pub(super) struct DirectoryWalk {
    pub(super) files: octocode_engine::types::FileSystemQueryResult,
    withheld: crate::policy::discovery::Withheld,
    /// What a walk cut at its file cap never reached.
    unwalked: Option<crate::tools::ast_search::unwalked::Unwalked>,
    /// Entries the walk could not read, and how many for want of
    /// permission.
    walk_errors: (u32, u32),
}

/// Walk a directory outline's candidates once: the memo fingerprint reads
/// the walk, and a cold page scans it without walking again.
pub(super) fn walk_directory(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<DirectoryWalk, ToolError> {
    let outlined = octocode_engine::signatures::graph_facts::graph_fact_extensions();
    let admission = crate::policy::discovery::WalkAdmission::new(
        paths.discovery_walk(),
        canonical,
        crate::policy::discovery::WalkRules {
            defaults: q.default_excludes.defaults(),
            include: &[],
            generated: true,
            engine_prunes: true,
        },
    );
    let files = octocode_engine::fact_scan::walk_graph_fact_files(
        &directory_scan_options(q, canonical),
        &|path, file_type| {
            // Only a directory or an outlinable file leaves absence unproven
            // when the policy withholds it or it is a link (the walk follows
            // none). The outline walk hides dot entries unless
            // `includeHidden`: one out of view is not disclosed.
            admission.admit(path, file_type, |is_dir| {
                (q.include_hidden == Some(true) || !crate::tools::ast_search::is_dot_entry(path))
                    && (is_dir
                        || outlined.iter().any(|extension| {
                            octocode_engine::text::extension_of(&path.to_string_lossy(), true, "")
                                == *extension
                        }))
            })
        },
        &|| cancel.check().is_err(),
    )
    .map_err(|error| ToolError::graph_scan(&error, cancel))?;
    let unwalked = unwalked_scopes(q, canonical, &files, paths, cancel);
    Ok(DirectoryWalk {
        unwalked,
        walk_errors: (files.skipped, files.permission_denied),
        files,
        withheld: admission.skipped().withheld,
    })
}

/// The scopes a directory walk stopped at its file cap never reached, when
/// that cap is the walk ceiling (a narrower `path` is the only way to them).
fn unwalked_scopes(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
    files: &octocode_engine::types::FileSystemQueryResult,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Option<crate::tools::ast_search::unwalked::Unwalked> {
    let frontier = files.first_unlisted.as_deref().filter(|_| {
        files.was_capped
            && crate::tools::ast_search::unwalked::at_ceiling(ast_search_query_symbols::max_files(
                q,
            ))
    })?;
    let discovery = paths.discovery_walk();
    let admit = |path: &std::path::Path, file_type| discovery.permits_entry(path, file_type);
    let stop = || cancel.check().is_err();
    // The walk's own candidates: a file of an outlined grammar, or one a
    // `languageGlobs` rule names.
    let parsers = crate::tools::ast_rule::GlobParsers::compile(
        canonical,
        ast_search_query_symbols::language_globs(q),
    )
    .ok()?;
    let outlined: std::collections::BTreeSet<String> =
        octocode_engine::signatures::graph_facts::graph_fact_extensions()
            .into_iter()
            .collect();
    let candidate = |path: &std::path::Path| {
        crate::tools::ast_rule::has_extension_in(path, &outlined) || parsers.names(path)
    };
    crate::tools::ast_search::unwalked::Unwalked::of(
        &crate::tools::ast_search::unwalked::Rules {
            root: canonical,
            frontier: std::path::Path::new(frontier),
            ignore: octocode_engine::search::walk::IgnorePolicy::new(
                q.include_ignored,
                q.include_hidden,
            ),
            prune: prune::walk_names(q.default_excludes.defaults(), &[]),
            include: None,
            exclude: ast_search_query_symbols::exclude(q),
            candidate: &candidate,
            max_depth: None,
            admit: &admit,
            stop: &stop,
        },
        &crate::response::result::query_row(q, &[]),
    )
}

/// The declarations of a walked directory's files. A name filter is a byte
/// prefilter first: a file that never spells any filter text cannot declare
/// a matching name, so it is read but not parsed.
pub(super) fn outline_walked(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
    walk: DirectoryWalk,
    cancel: &dyn CancellationCheck,
) -> Result<Outline, ToolError> {
    let filter = ast_search_query_symbols::name_filter(q);
    let passed_over = std::sync::atomic::AtomicUsize::new(0);
    let keep = |source: &str| {
        let spelled = filter
            .as_ref()
            .is_none_or(|filter| filter.spelled_in(source));
        if !spelled {
            passed_over.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        spelled
    };
    let rooted = |relative: &str| {
        crate::tools::ast_search::rooted_display(canonical, std::path::Path::new(relative))
    };
    let scan = octocode_engine::fact_scan::scan_walked_graph_facts(
        directory_scan_options(q, canonical),
        walk.files,
        octocode_engine::fact_scan::FactsSource::Declarations(&|source, path, parser| {
            declarations(source, &path.to_string_lossy(), parser)
        }),
        &keep,
        &|view| {
            let (rows, off_kind, notes) = reduce_document(q, filter.as_ref(), view.facts, || {
                Some(view.content.to_owned())
            });
            (!rows.is_empty() || !off_kind.is_empty() || !notes.is_empty()).then(|| OutlineFile {
                path: rooted(view.relative_path),
                rows,
                off_kind,
                notes,
            })
        },
        &|| cancel.check().is_err(),
    )
    .map_err(|error| ToolError::graph_scan(&error, cancel))?;
    let diagnostics = scan
        .skipped
        .iter()
        .map(|d| json!({"path":rooted(&d.relative_path),"message":format!("{}: {}",d.code,d.message)}))
        .collect();
    let files = scan
        .kept
        .into_iter()
        .map(|(_, file)| file)
        .collect::<Vec<_>>();
    Ok(Outline {
        scanned: scan.parsed + passed_over.into_inner(),
        files,
        truncated: scan.truncated,
        scan_resume: scan.scan_resume,
        skipped: scan.files_skipped,
        diagnostics,
        withheld: walk.withheld,
        pruned: scan.pruned_dirs,
        unwalked: walk.unwalked,
        walk_errors: walk.walk_errors,
    })
}

/// The files a directory query's `languageGlobs` map to a grammar, compiled
/// against the scanned root in the one local glob dialect the outline scan
/// uses; `None` without globs. A glob that does not compile is `invalidGlob`.
pub(super) fn mapped_files(
    q: &AstSearchQuerySymbols,
    root: &std::path::Path,
) -> Result<Option<Gitignore>, ToolError> {
    let Some(globs) = ast_search_query_symbols::language_globs(q) else {
        return Ok(None);
    };
    octocode_engine::search::glob::set(
        root,
        globs.values().flatten().map(String::as_str),
        "languageGlobs",
    )
    .map_err(crate::tools::ast_search::errors::native_error)
}

/// Whether the outline scan parses `path`: an outlined extension, or a
/// `languageGlobs` match.
pub(super) fn outlinable(
    outlined: &[String],
    mapped: Option<&Gitignore>,
    path: &std::path::Path,
) -> bool {
    outlined.contains(&octocode_engine::text::extension_of(
        &path.to_string_lossy(),
        true,
        "",
    )) || mapped.is_some_and(|set| octocode_engine::search::glob::matches(set, path, false))
}
