//! The scan of one file or a directory, and the order of its files.
use super::continuations::tree_repair;
use super::query::MatchQuery;
use super::rows::{diagnostic_error, error_diagnostic, match_display_path};
use super::snapshot::directory_source_digest;
use super::types::{Scan, ScannedFile, Scope};
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

mod walk_log;

use walk_log::WalkLog;

pub(super) fn scan_file(
    q: MatchQuery<'_>,
    scope: &Scope,
    cancel: &dyn CancellationCheck,
) -> Result<Scan, ToolError> {
    let canonical = &scope.canonical;
    super::super::language::validate_file_language(
        canonical,
        scope.language.as_deref(),
        octocode_engine::text::analysis::AnalysisOperation::StructuralSearch,
    )?;
    let Some(bytes) = super::super::source::read_parse_source(canonical)? else {
        let mut error = ToolError::new(
            ErrorCode::FileTooLarge,
            "Source exceeds the native parser byte limit.",
        );
        if let Some(lead) = q
            .pattern_text()
            .as_deref()
            .and_then(super::super::source::pattern_literal)
            .and_then(|literal| {
                super::super::source::text_search(&canonical.to_string_lossy(), literal)
            })
        {
            error.calls = Some(Box::new(serde_json::json!({ "findText": lead })));
        }
        return Err(error);
    };
    // Parse the file as the directory scan does: admitted by the one text
    // rule, raw. Output strings are redacted by the response stage.
    let stop = || cancel.check().is_err();
    let mut maps = super::super::source::mapping_budget(&stop);
    let admitted = super::super::source::admitted(canonical, &bytes, &mut maps)?;
    let source = admitted.text();
    let source_path = canonical.to_string_lossy();
    let mut r = octocode_engine::structural::search_detailed(
        source,
        &source_path,
        super::super::language::parser_override(canonical, scope.language.as_deref()).as_deref(),
        None,
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
    let mut binder = octocode_engine::structural::SourceBinder::with_budget(&admitted, maps);
    for matched in &mut r.matches {
        binder.bind_match(matched).map_err(ToolError::source)?;
    }
    if let Some(region) = q.scope() {
        super::super::region::select(region, source, &mut r.matches)?;
    }
    if let Some(note) =
        super::super::language::cpp_header_note(canonical, q.language().as_deref(), source)
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
            path: canonical.to_string_lossy().into_owned(),
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
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(scope.canonical.clone());
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
    let ignore_rules = q.include_ignored() != Some(true);
    let log = WalkLog::new(
        scope,
        paths,
        &admission,
        ignore_rules,
        q.include_hidden() == Some(true),
    );
    let projection_failure = std::sync::Mutex::new(None);
    let price = |m: &StructuralDetailedMatch| match match_price(q, m, cancel) {
        Ok(price) => price,
        Err(error) => {
            projection_failure
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get_or_insert(error);
            0
        }
    };
    let r = octocode_engine::structural::search_files(
        StructuralSearchFilesOptions {
            path: scope.canonical.to_string_lossy().into_owned(),
            pattern: None,
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
        &|path| log.admit(path),
        &|path| parser_extension(scope, path),
        &|| {
            cancel.check().is_err()
                || projection_failure
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .is_some()
        },
    );
    if let Some(error) = projection_failure
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
    {
        return Err(error);
    }
    let r = r.map_err(super::super::errors::native_error)?;
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
                    candidate: &|path| parses(scope, path),
                    max_depth: q.max_depth(),
                    admit: &admit,
                    stop: &stop,
                },
                &q.to_value(),
            )
        });
    let logged = log.finish();
    // Preserve the scan-level coverage signals the group projection drops.
    Ok(Scan {
        unwalked,
        sources,
        source_digest,
        ignore_rules: logged.ignore_rules,
        repository_markers: if ignore_rules {
            crate::tools::page_memo::repository_markers(&scope.canonical)
        } else {
            Vec::new()
        },
        untracked_ignore_dependencies: logged.untracked_ignore_dependencies,
        discovery_sources: logged.discovery_sources,
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
        unparsed: logged.unparsed,
        census: logged.census,
    })
}

/// Whether a directory scan parses `path`: by its own extension, or by the
/// rule that names its parser.
fn parses(scope: &Scope, path: &std::path::Path) -> bool {
    scope
        .extensions
        .as_ref()
        .is_none_or(|extensions| has_extension_in(path, extensions))
        || scope.parsers.extension(path).is_some()
}

/// The parser a directory scan reads `path` with: the grammar a
/// `parserGlobs` entry names, else the one its shared extension resolves
/// to under `parser`, else its own extension.
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
    let by_count = q.sort() == Some("matchCount");
    let descending = q.reverse().unwrap_or(false);
    files.sort_by(|a, b| {
        let ordering = if by_count {
            a.matches.len().cmp(&b.matches.len())
        } else {
            a.path.cmp(&b.path)
        };
        let ordering = if descending {
            ordering.reverse()
        } else {
            ordering
        };
        ordering.then_with(|| a.path.cmp(&b.path))
    });
}
