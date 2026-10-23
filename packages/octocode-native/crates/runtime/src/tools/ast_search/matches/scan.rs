//! The scan of one file or a directory, and the order of its files.
use super::continuations::tree_repair;
use super::query::MatchQuery;
use super::rows::{diagnostic_error, error_diagnostic, match_display_path};
use super::scope::structural_extensions;
use super::snapshot::directory_source_digest;
use super::types::{NamedFiles, Scan, ScannedFile, Scope};
use super::{demand, match_price};
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::policy::prune;
use crate::response::result::ToolError;
use crate::tools::ast_rule::has_extension_in;
use octocode_contract_types::ErrorCode;
use octocode_engine::structural::{
    FileMatches, StructuralDetailedMatch, StructuralDiagnostic, StructuralSearchFilesOptions,
};

pub(super) fn scan_file(q: MatchQuery<'_>, scope: &Scope) -> Result<Scan, ToolError> {
    let canonical = &scope.canonical;
    super::super::language::validate_file_language(canonical, scope.language.as_deref())?;
    let Some(bytes) = super::super::source::read_parse_source(canonical)? else {
        let mut error = ToolError::new(
            ErrorCode::FileTooLarge,
            "Source exceeds the native parser byte limit.",
        );
        if let Some(lead) = q
            .pattern()
            .as_deref()
            .and_then(super::super::source::pattern_literal)
            .and_then(|literal| {
                super::super::source::text_search(&canonical.to_string_lossy(), literal)
            })
        {
            error.next = Some(Box::new(serde_json::json!({ "findText": lead })));
        }
        return Err(error);
    };
    // Parse the file as the directory scan does: admitted by the one text
    // rule, raw. Output strings are redacted by the response stage.
    let source = super::super::source::parse_text(canonical, &bytes)?;
    let source_path = canonical.to_string_lossy();
    let mut r = octocode_engine::structural::search_detailed(
        &source,
        &source_path,
        super::super::language::parser_override(canonical, scope.language.as_deref()).as_deref(),
        q.pattern().as_deref(),
        q.rule().as_ref(),
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
    if let Some(note) =
        super::super::language::cpp_header_note(canonical, q.language().as_deref(), &source)
    {
        r.diagnostics.push(StructuralDiagnostic {
            code: super::super::language::CPP_HEADER_NOTE.to_owned(),
            severity: "warning".to_owned(),
            stage: "parse".to_owned(),
            message: note,
            path: Some(source_path.into_owned()),
            recovery: None,
        });
    }
    Ok(Scan {
        files: vec![ScannedFile {
            path: crate::tools::display_name(canonical),
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
    let include = q.include().unwrap_or_default();
    let admission = crate::policy::discovery::WalkAdmission::new(
        paths.discovery_walk(),
        &scope.canonical,
        crate::policy::discovery::WalkRules {
            defaults: q.default_excludes(),
            include: &include,
            generated: true,
            engine_prunes: true,
        },
    );
    let price = |m: &StructuralDetailedMatch| match_price(q, m);
    let r = octocode_engine::structural::search_files(
        StructuralSearchFilesOptions {
            path: scope.canonical.to_string_lossy().into_owned(),
            pattern: q.pattern(),
            rule: q.rule(),
            include: q.include(),
            exclude: q.exclude(),
            exclude_dir: None,
            default_exclude_dir: Some(prune::walk_names(q.default_excludes(), &include)),
            ignore: octocode_engine::search::walk::IgnorePolicy::new(
                q.include_ignored(),
                q.include_hidden(),
            ),
            max_depth: q.max_depth(),
            max_files: q.max_files(),
            skip_files: Some(q.scan_offset()),
            max_file_bytes: u32::try_from(super::super::source::MAX_PARSE_SOURCE_BYTES).ok(),
        },
        demand(q, &price),
        &|path| {
            // Only a directory or a file of the scanned grammar could have
            // matched, so only those leave absence unproven when the policy
            // withholds them or they are links (the walk follows none).
            // A dot entry out of view (no `hidden`) is neither walked nor
            // disclosed, as structureSearch tree does.
            let in_view = q.include_hidden() == Some(true) || !super::super::is_dot_entry(path);
            if !admission.admit(path, None, |is_dir| in_view && (is_dir || parses(path))) {
                return false;
            }
            if path.is_dir() {
                let mut watched = observed.lock().unwrap_or_else(|error| error.into_inner());
                watched.insert(path.to_path_buf());
                if ignore_rules {
                    let inputs = crate::tools::page_memo::directory_ignore_inputs(
                        path,
                        &scope.canonical,
                        |ignore| admission.permits(ignore, None),
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
            let unparsed_file = !parses(path) && path.is_file();
            // A file no grammar parses is disclosed, not silently dropped, when
            // the scope is every grammar's files. Under `language` the scope is
            // that language's files: a file of any other kind (a grammar's own
            // or none) was never in it, so it is not skipped evidence.
            if unparsed_file && scope.language.is_none() && !has_extension_in(path, &grammars) {
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
            let admitted = !unparsed_file;
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
    .map_err(super::super::errors::native_error)?;
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
    let admit = |path: &std::path::Path, file_type| admission.permits(path, file_type);
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
                    prune: prune::walk_names(q.default_excludes(), &include),
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
        walk_errors: r.walk_errors,
        skipped_by_prefilter: r.skipped_by_pre_filter,
        withheld: admission.skipped().withheld,
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
        super::super::language::shared_extension_parser(path, scope.language.as_deref())
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
