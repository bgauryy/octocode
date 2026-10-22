//! The scan of one file or a directory, its stored memo, and the snapshot
//! that names it.
use super::*;
use crate::response::result::Confidence;
use octocode_contract_types::ErrorCode;

pub(super) fn scan_file(
    q: MatchQuery<'_>,
    scope: &Scope,
    security: &ContentSecurity,
) -> Result<Scan, ToolError> {
    let canonical = &scope.canonical;
    super::super::validate_file_language(canonical, scope.language.as_deref())?;
    let Some(bytes) = super::super::read_parse_source(canonical)? else {
        let mut error = ToolError::new(
            ErrorCode::FileTooLarge,
            "Source exceeds the native parser byte limit.",
        );
        if let Some(lead) = q
            .pattern()
            .as_deref()
            .and_then(super::super::pattern_literal)
            .and_then(|literal| super::super::text_search(&canonical.to_string_lossy(), literal))
        {
            error.next = Some(Box::new(serde_json::json!({ "findText": lead })));
        }
        return Err(error);
    };
    // Parse the file as the directory scan does: raw. Output strings are
    // redacted by the response stage.
    let source = security
        .decode_source_bytes(&bytes, super::super::MAX_PARSE_SOURCE_BYTES)
        .map_err(super::super::policy_error)?;
    let source_path = canonical.to_string_lossy();
    let mut r = octocode_engine::structural::search_detailed(
        &source,
        &source_path,
        super::super::parser_override(canonical, scope.language.as_deref()).as_deref(),
        q.pattern().as_deref(),
        q.rule().as_deref(),
    );
    if let Some(diagnostic) = error_diagnostic(&r.diagnostics) {
        let error = diagnostic_error(diagnostic);
        // A rule that does not parse as a rule names its bad key; no syntax
        // tree shows a rule's keys.
        return Err(
            if diagnostic.code == octocode_engine::structural::RULE_INVALID {
                error
            } else {
                tree_repair(error, q, scope)
            },
        );
    }
    if let Some(region) = q.scope() {
        super::super::region::select(region, &source, &mut r.matches)?;
    }
    if let Some(note) = super::super::cpp_header_note(canonical, q.language().as_deref(), &source) {
        r.diagnostics.push(StructuralDiagnostic {
            code: super::super::CPP_HEADER_NOTE.to_owned(),
            severity: "warning".to_owned(),
            stage: "parse".to_owned(),
            message: note,
            path: Some(source_path.into_owned()),
            recovery: None,
        });
    }
    Ok(Scan {
        files: vec![ScannedFile {
            path: super::super::display_name(canonical),
            parser: String::new(),
            matches: FileMatches::Held(r.matches),
            diagnostics: r.diagnostics,
            status: r.status,
            recovered: r.recovered,
            source: None,
        }],
        sources: vec![canonical.clone()],
        source_digest: octocode_engine::digest::sha256(&bytes),
        ..Scan::default()
    })
}

/// A query that does not compile against one file leads to that file's
/// syntax tree, where the node shapes a pattern or rule names are read.
pub(super) fn tree_repair(mut error: ToolError, q: MatchQuery<'_>, scope: &Scope) -> ToolError {
    if error.code == ErrorCode::InvalidPattern && error.next.is_none() {
        error.next = Some(Box::new(
            json!({ "viewTree": tree_lead(q, scope, &scope.canonical) }),
        ));
    }
    error
}

/// The syntaxTree read of `file` (its grammar pinned when one was named
/// for a single file).
pub(super) fn tree_lead(q: MatchQuery<'_>, scope: &Scope, file: &std::path::Path) -> Value {
    let mut tree = json!({"operation":"syntaxTree","path":file.to_string_lossy()});
    if scope.is_file
        && let Some(language) = q.language()
    {
        tree["language"] = json!(language);
    }
    crate::response::result::Continuation::new(ToolId::AstSearch, tree)
        .confidence(Confidence::Medium)
        .build()
}

pub(super) fn scan_directory(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<Scan, ToolError> {
    #[cfg(test)]
    DIRECTORY_SCANS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .push(scope.canonical.clone());
    let extensions = scope.extensions.as_ref();
    // A file parses by its own extension or by the rule that names its parser.
    let parses = |path: &std::path::Path| {
        extensions.is_none_or(|extensions| has_extension_in(path, extensions))
            || scope.parsers.extension(path).is_some()
    };
    let grammars = structural_extensions();
    let unparsed = std::sync::Mutex::new(NamedFiles::default());
    let observed = std::sync::Mutex::new(std::collections::BTreeSet::new());
    let ignore_rules = q.include_ignored() != Some(true);
    let external_rules = std::sync::Mutex::new(if ignore_rules {
        octocode_engine::search::walk::external_ignore_inputs(&scope.canonical)
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
    } else {
        std::collections::BTreeSet::new()
    });
    let untracked_ignore_dependencies = std::sync::atomic::AtomicBool::new(false);
    let census = std::sync::Mutex::new(std::collections::BTreeMap::<String, usize>::new());
    let counted = std::sync::Mutex::new(std::collections::HashSet::<std::path::PathBuf>::new());
    let withheld = std::sync::Mutex::new(crate::policy::discovery::Withheld::default());
    let discovery = paths.discovery_walk();
    let price = |m: &StructuralDetailedMatch| match_price(q, m);
    let r = octocode_engine::structural::search_files(
        StructuralSearchFilesOptions {
            path: scope.canonical.to_string_lossy().into_owned(),
            pattern: q.pattern(),
            rule: q.rule(),
            include: q.include(),
            exclude: q.exclude(),
            exclude_dir: None,
            default_exclude_dir: Some(prune::directories_before_policy(q.default_excludes())),
            ignore: octocode_engine::search::walk::IgnorePolicy::new(
                q.include_ignored(),
                q.include_hidden(),
            ),
            max_depth: q.max_depth(),
            max_files: q.max_files(),
            skip_files: Some(q.scan_offset()),
            max_file_bytes: u32::try_from(super::super::MAX_PARSE_SOURCE_BYTES).ok(),
        },
        demand(q, &price),
        &|path| {
            // Only a directory or a file of the scanned grammar could have
            // matched, so only those leave absence unproven.
            // A dot entry out of view (no `hidden`) is neither walked nor
            // disclosed, as structureSearch tree does.
            let in_view = q.include_hidden() == Some(true) || !super::super::is_dot_entry(path);
            // The walk follows no link: one naming a directory or a file of
            // the scanned grammar is disclosed as not followed, in or out of
            // the allowed roots.
            if path.is_symlink() {
                let is_dir = path.is_dir();
                if in_view && (is_dir || parses(path)) {
                    withheld
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .record(path, is_dir);
                }
                return false;
            }
            let allowed = discovery.permits(path);
            let is_dir = path.is_dir();
            if allowed && is_dir {
                let mut watched = observed.lock().unwrap_or_else(|error| error.into_inner());
                watched.insert(path.to_path_buf());
                if ignore_rules {
                    let inputs = crate::tools::page_memo::directory_ignore_inputs(
                        path,
                        &scope.canonical,
                        |ignore| discovery.permits(ignore),
                    );
                    external_rules
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .extend(inputs.external);
                    if inputs.untracked {
                        untracked_ignore_dependencies
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    watched.extend(inputs.files);
                }
            }
            if !allowed && in_view && (is_dir || parses(path)) {
                withheld
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .record(path, is_dir);
            }
            let unparsed_file = !parses(path) && path.is_file();
            // A file no grammar parses is disclosed, not silently dropped, when
            // the scope is every grammar's files. Under `language` the scope is
            // that language's files: a file of any other kind (a grammar's own
            // or none) was never in it, so it is not skipped evidence.
            if allowed
                && unparsed_file
                && scope.language.is_none()
                && !has_extension_in(path, &grammars)
            {
                // Named as a row path reads after the response stage: from
                // the workspace root, else absolute.
                unparsed
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .record(
                        paths
                            .workspace_relative(path)
                            .unwrap_or_else(|| path.to_string_lossy().into_owned()),
                    );
            }
            let admitted = allowed && !unparsed_file;
            // The engine asks again for each file it reads: count a path once.
            if admitted
                && scope.language.is_none()
                && path.is_file()
                && counted
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .insert(path.to_path_buf())
            {
                let extension = scope.parsers.extension(path).map_or_else(
                    || octocode_engine::text::extension_of(&path.to_string_lossy(), true, ""),
                    str::to_owned,
                );
                *census
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .entry(extension)
                    .or_default() += 1;
            }
            // Grammar filtering is not an explicit include override: ignored
            // source files remain ignored unless the caller overrides them.
            admitted
        },
        &|path| parser_extension(scope, path),
        &|| cancel.check().is_err(),
    )
    .map_err(super::super::native_error)?;
    if let Some(diagnostic) = error_diagnostic(&r.diagnostics) {
        return Err(diagnostic_error(diagnostic));
    }
    let sources = r
        .files
        .iter()
        .map(|f| std::path::PathBuf::from(&f.path))
        .collect();
    // Preserve source identity before the row projection drops internal metadata.
    let source_digest = directory_source_digest(
        r.files
            .iter()
            .map(|file| (file.path.as_str(), file.source_sha256.as_deref())),
    );
    // A walk stopped at its cap names where it stopped: the unread scopes.
    let stop = || cancel.check().is_err();
    let admit = |path: &std::path::Path, file_type| discovery.permits_entry(path, file_type);
    let unwalked = r
        .scan_frontier
        .as_deref()
        .filter(|_| r.scan_truncated && super::super::unwalked::at_ceiling(q.max_files()))
        .and_then(|frontier| {
            super::super::unwalked::Unwalked::of(
                &super::super::unwalked::Rules {
                    root: &scope.canonical,
                    frontier,
                    ignore: octocode_engine::search::walk::IgnorePolicy::new(
                        q.include_ignored(),
                        q.include_hidden(),
                    ),
                    prune: prune::directories_before_policy(q.default_excludes()),
                    include: q.include(),
                    exclude: q.exclude(),
                    candidate: &parses,
                    max_depth: q.max_depth(),
                    admit: &admit,
                    stop: &stop,
                },
                &q.to_value(),
            )
        });
    // Preserve the scan-level coverage signals the group projection drops.
    Ok(Scan {
        unwalked,
        sources,
        source_digest,
        ignore_rules: external_rules
            .into_inner()
            .unwrap_or_else(|error| error.into_inner())
            .into_iter()
            .collect(),
        repository_markers: if ignore_rules {
            crate::tools::page_memo::repository_markers(&scope.canonical)
        } else {
            Vec::new()
        },
        untracked_ignore_dependencies: untracked_ignore_dependencies.into_inner(),
        discovery_sources: observed
            .into_inner()
            .unwrap_or_else(|error| error.into_inner())
            .into_iter()
            .collect(),
        files: r
            .files
            .into_iter()
            .map(|f| ScannedFile {
                path: match_display_path(&scope.canonical, &f.path),
                parser: parser_extension(scope, std::path::Path::new(&f.path)),
                source: f
                    .source_sha256
                    .map(|digest| (std::path::PathBuf::from(&f.path), digest)),
                matches: f.matches,
                diagnostics: f.diagnostics,
                status: f.status,
                recovered: f.recovered,
            })
            .collect(),
        truncated: r.scan_truncated,
        scan_resume: r.scan_resume,
        diagnostics: r.diagnostics,
        skips: (r.skipped_unsupported, r.skipped_unreadable, r.skipped_large),
        skipped_by_prefilter: r.skipped_by_pre_filter,
        withheld: withheld
            .into_inner()
            .unwrap_or_else(|error| error.into_inner()),
        unparsed: unparsed
            .into_inner()
            .unwrap_or_else(|error| error.into_inner()),
        census: census
            .into_inner()
            .unwrap_or_else(|error| error.into_inner()),
    })
}

/// The parser a directory scan reads `path` with: the grammar a
/// `languageGlobs` entry names, else the one its shared extension resolves
/// to under `language`, else its own extension.
pub(super) fn parser_extension(scope: &Scope, path: &std::path::Path) -> String {
    if let Some(extension) = scope.parsers.extension(path) {
        extension.to_owned()
    } else if let Some(parser) =
        super::super::shared_extension_parser(path, scope.language.as_deref())
    {
        parser
    } else {
        octocode_engine::text::extension_of(&path.to_string_lossy(), true, "")
    }
}

/// Every directory scan's root, for the tests that count scans.
#[cfg(test)]
pub(super) static DIRECTORY_SCANS: std::sync::Mutex<Vec<std::path::PathBuf>> =
    std::sync::Mutex::new(Vec::new());

pub(super) fn directory_source_digest<'a>(
    files: impl Iterator<Item = (&'a str, Option<&'a str>)>,
) -> String {
    let identities = files.collect::<std::collections::BTreeMap<_, _>>();
    crate::digest::json_sha256(&json!(identities))
}

#[cfg(unix)]
pub(super) fn memo_source_unchanged(_: &Scope, _: &Scan) -> bool {
    // StampSet already checked change time and inode as well as size and mtime.
    true
}

#[cfg(not(unix))]
pub(super) fn memo_source_unchanged(scope: &Scope, scan: &Scan) -> bool {
    // Without Unix change time, restored mtime cannot prove source identity.
    // Reuse parsing only after bounded reads prove the saved source digest.
    let digests = scan
        .sources
        .iter()
        .map(|path| {
            let digest = super::super::read_parse_source(path)
                .ok()
                .flatten()
                .map(|bytes| octocode_engine::digest::sha256(&bytes));
            (path.to_string_lossy().into_owned(), digest)
        })
        .collect::<Vec<_>>();
    if scope.is_file {
        digests.first().and_then(|(_, digest)| digest.as_deref())
            == Some(scan.source_digest.as_str())
    } else {
        directory_source_digest(
            digests
                .iter()
                .map(|(path, digest)| (path.as_str(), digest.as_deref())),
        ) == scan.source_digest
    }
}

pub(super) fn sort_files(q: MatchQuery<'_>, files: &mut [ScannedFile]) {
    files.sort_by(|a, b| {
        let ordering = if q.sort().as_deref() == Some("matchCount") {
            b.matches
                .len()
                .cmp(&a.matches.len())
                .then_with(|| a.path.cmp(&b.path))
        } else {
            a.path.cmp(&b.path)
        };
        if q.reverse().unwrap_or(false) {
            ordering.reverse()
        } else {
            ordering
        }
    });
}

/// A continuation page whose fresh rescan stopped at a parse or match
/// deadline differs from its first page because the scan was cut, not
/// because the source changed: a retryable `timeout` with the same page as
/// `next.retry`, never a `staleSnapshot` that discards the pages read.
pub(super) fn interrupted_rescan(q: MatchQuery<'_>, scan: &Scan) -> Option<ToolError> {
    let interrupted = scan.files.iter().any(|file| {
        file.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.code.as_str(),
                octocode_engine::structural::MATCH_DEADLINE
                    | octocode_engine::structural::PARSE_INTERRUPTED
            )
        })
    });
    if !interrupted {
        return None;
    }
    let mut error = ToolError::new(
        ErrorCode::Timeout,
        "The rescan for this page stopped at the structural match deadline, so it cannot be checked against the earlier pages; the source is unchanged.",
    );
    error.next = Some(Box::new(json!({
        "retry": crate::response::result::Continuation::new(ToolId::AstSearch, q.to_value())
            .confidence(Confidence::Exact)
            .build()
    })));
    Some(error)
}

/// The match-row shape a snapshot was cut under; bumped by every row rename.
pub(super) const MATCH_ROWS_VERSION: u32 = 4;

/// Fingerprint over the query shape, ordered result set and scanned source bytes.
/// Continuation cursors (page>1 or matchPage>1) that
/// carry a stale snapshot are rejected with `staleSnapshot`. Fields the
/// continuation normalizes to a default are digested by their effective
/// value so a fresh page matches.
pub(super) fn match_snapshot(q: MatchQuery<'_>, scope: &Scope, scan: &Scan) -> String {
    let ordered = scan
        .files
        .iter()
        .map(|f| (f.path.clone(), f.matches.len()))
        .collect::<Vec<_>>();
    crate::digest::json_sha256(&json!([
        // Row shape version (v4: `matchCount`, object rows, 1-based columns); an older cursor
        // restarts instead of mixing shapes across pages.
        MATCH_ROWS_VERSION,
        scope.canonical.to_string_lossy(),
        q.pattern(),
        q.rule(),
        q.scope(),
        q.include(),
        q.exclude(),
        q.include_hidden(),
        q.include_ignored(),
        q.max_depth(),
        scope.language,
        q.language_globs(),
        q.reverse(),
        // Always present: validation stamps the contract defaults.
        q.sort().as_deref().unwrap_or_default(),
        q.result_view().as_deref().unwrap_or_default(),
        q.max_files(),
        q.scan_offset(),
        &scan.source_digest,
        ordered
    ]))
}
