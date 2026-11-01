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
        max_files: crate::tools::ast_search::scan_window::window_files(
            ast_search_query_symbols::scan_offset(q),
        ),
        skip_files: Some(ast_search_query_symbols::scan_offset(q)),
        max_file_bytes: u32::try_from(crate::tools::ast_search::source::MAX_PARSE_SOURCE_BYTES)
            .ok(),
        language_globs: ast_search_query_symbols::language_globs(q)
            .as_ref()
            .map(crate::tools::ast_rule::language_globs),
        // The ignore rules `match` and the text tools walk by, off only with
        // `includeIgnored`.
        ignore: octocode_engine::types::IgnorePolicy::new(q.include_ignored, q.include_hidden),
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
    let scoped = crate::tools::test_code::Scoped::of(q.test_code, paths);
    let walk = walk_directory(q, canonical, (paths, scoped.as_ref()), cancel)?;
    outline_walked(q, canonical, (walk, scoped.as_ref()), cancel)
}

/// The candidate files a directory outline scans, in walk order, and the
/// entries the path policy withheld from that walk.
pub(super) struct DirectoryWalk {
    pub(super) files: octocode_engine::types::FileSystemQueryResult,
    withheld: crate::policy::discovery::Withheld,
    /// Entries the walk could not read, and how many for want of
    /// permission.
    walk_errors: (u32, u32),
}

/// Walk a directory outline's candidates once: the memo fingerprint reads
/// the walk, and a cold page scans it without walking again.
pub(super) fn walk_directory(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
    (paths, scoped): (&PathPolicy, Option<&crate::tools::test_code::Scoped>),
    cancel: &dyn CancellationCheck,
) -> Result<DirectoryWalk, ToolError> {
    let outlined = octocode_engine::signatures::languages::RegistryView::installed()
        .extensions(octocode_engine::text::analysis::AnalysisOperation::Declarations);
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
            let outlinable = || {
                outlined.iter().any(|extension| {
                    octocode_engine::text::extension_of(&path.to_string_lossy(), true, "")
                        == *extension
                })
            };
            // Only a directory or an outlinable file leaves absence unproven
            // when the policy withholds it or it is a link (the walk follows
            // none). The outline walk hides dot entries unless
            // `includeHidden`: one out of view is not disclosed.
            admission.admit(path, file_type, |is_dir| {
                (q.include_hidden == Some(true) || !crate::tools::ast_search::is_dot_entry(path))
                    && (is_dir || outlinable())
            }) && scoped.is_none_or(|scoped| {
                // Only an outlinable file is one the scope leaves unread.
                let is_dir = file_type.map_or_else(|| path.is_dir(), |kind| kind.is_dir());
                (!is_dir && !outlinable()) || scoped.reads(path, is_dir)
            })
        },
        &|| cancel.check().is_err(),
    )
    .map_err(|error| ToolError::graph_scan(&error, cancel))?;
    Ok(DirectoryWalk {
        walk_errors: (files.skipped, files.permission_denied),
        files,
        withheld: admission.skipped().withheld,
    })
}

/// The declarations of a walked directory's files. A name filter is a byte
/// prefilter first: a file that never spells any filter text cannot declare
/// a matching name, so it is read but not parsed.
pub(super) fn outline_walked(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
    (walk, scoped): (DirectoryWalk, Option<&crate::tools::test_code::Scoped>),
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
    let failure = std::sync::Mutex::new(None);
    let scan = octocode_engine::fact_scan::scan_walked_graph_facts(
        directory_scan_options(q, canonical),
        walk.files,
        octocode_engine::fact_scan::FactsSource::Declarations(&|source, path, parser| {
            declarations(source, &path.to_string_lossy(), parser)
        }),
        &keep,
        &|view| {
            let project = || -> Result<Option<OutlineFile>, ToolError> {
                let cancelled = || cancel.check().is_err();
                let mut maps = super::super::source::mapping_budget(cancel, &cancelled);
                let mut evidence =
                    super::super::evidence::FileEvidence::new(view.source, &mut maps, cancel);
                let file = canonical.join(view.relative_path);
                let (rows, off_kind, notes) = reduce_document(
                    q,
                    (
                        filter.as_ref(),
                        scoped.map(|scoped| (scoped, file.as_path())),
                    ),
                    view.facts,
                    || Some(view.content.to_owned()),
                    &mut evidence,
                )?;
                let source_descriptors = evidence.finish()?;
                Ok(
                    (!rows.is_empty() || !off_kind.is_empty() || !notes.is_empty()).then(|| {
                        OutlineFile {
                            path: rooted(view.relative_path),
                            rows,
                            off_kind,
                            notes,
                            source_descriptors,
                        }
                    }),
                )
            };
            match project() {
                Ok(file) => file,
                Err(error) => {
                    failure
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .get_or_insert(error);
                    None
                }
            }
        },
        &|| {
            cancel.check().is_err()
                || failure
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .is_some()
        },
    );
    if let Some(error) = failure
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
    {
        return Err(error);
    }
    let scan = scan.map_err(|error| ToolError::graph_scan(&error, cancel))?;
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
        walk_errors: walk.walk_errors,
        test_code_excluded: scoped
            .map(crate::tools::test_code::Scoped::excluded)
            .unwrap_or_default(),
    })
}

/// The files a directory query's `parserGlobs` map to a grammar, compiled
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
        "parserGlobs",
    )
    .map_err(crate::tools::ast_search::errors::native_error)
}

/// Whether the outline scan parses `path`: an outlined extension, or a
/// `parserGlobs` match.
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
