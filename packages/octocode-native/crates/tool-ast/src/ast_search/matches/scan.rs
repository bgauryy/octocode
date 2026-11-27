//! The scan of one file or a directory, and the order of its files.
use super::super::scan_window::window_files;
use super::continuations::tree_repair;
use super::groups::header_chars;
use super::query::MatchQuery;
use super::rows::{diagnostic_error, error_diagnostic};
use super::snapshot::directory_source_digest;
use super::stream::PageFill;
use super::types::{Scan, ScannedFile, Scope};
use super::{demand, file_list, match_price};
use octocode_engine::structural::{
    FileMatches, PageStarts, StructuralDetailedMatch, StructuralDiagnostic,
    StructuralSearchDetailedFileResult, StructuralSearchFilesOptions,
};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::policy::prune;
use octocode_runtime_base::response::result::ToolError;
use octocode_tools_shared::ast_rule::has_extension_in;
use octocode_tools_shared::test_code::Scoped;

mod walk_log;

use walk_log::WalkLog;

pub(super) fn scan_file(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<Scan, ToolError> {
    let canonical = &scope.canonical;
    super::super::language::validate_file_language(
        canonical,
        scope.language.as_deref(),
        octocode_engine::text::analysis::AnalysisOperation::StructuralSearch,
    )?;
    let Some(bytes) = scope.parse_source(canonical)? else {
        let pattern = q.pattern_text();
        return Err(super::super::source::too_large_error(
            canonical,
            &canonical.to_string_lossy(),
            pattern
                .as_deref()
                .and_then(super::super::source::pattern_literal),
        ));
    };
    // Parse the file as the directory scan does: admitted by the one text
    // rule, as a read shows it ([`super::super::source::admitted_for_match`]).
    let stop = || cancel.check().is_err();
    let mut maps = super::super::source::mapping_budget(cancel, &stop);
    let (admitted, shown) = super::super::source::admitted_for_match(canonical, &bytes, &mut maps)?;
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
    // Only the matches of the scope are mapped to their source: a narrower
    // scope is what a file past its mapping limits needs.
    if let Some(region) = q.scope() {
        super::super::region::select(region, source, &mut r.matches)?;
    }
    let mut binder = octocode_engine::structural::SourceBinder::with_budget(&admitted, maps);
    for matched in &mut r.matches {
        binder.bind_match(matched).map_err(|error| match error {
            octocode_engine::text::analysis::SourceError::LimitExceeded(limit) => {
                source_limit_error(q, canonical, limit)
            }
            error => ToolError::source(error),
        })?;
    }
    if let Some(shown) = &shown {
        octocode_engine::structural::restore_texts(&mut r.matches, source, shown);
    }
    // A test-code scope keeps one side of a single file's matches too.
    let mut test_code_excluded = octocode_engine::types::TestCodeExcluded::default();
    let mut test_code_unread = Vec::new();
    let mut verdict_inputs = Vec::new();
    if let Some(scoped) = octocode_tools_shared::test_code::Scoped::of(q.test_code(), paths) {
        r.matches
            .retain(|matched| scoped.keeps(canonical, matched.start_line));
        test_code_excluded = scoped.excluded();
        test_code_unread = scoped.unread();
        verdict_inputs = scoped.watched();
    }
    if let Some(warning) = admitted.warning() {
        r.diagnostics
            .push(StructuralDiagnostic::decoded(&source_path, warning));
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
            recovered_bodies: r.recovered_bodies,
            source: None,
        }],
        // Inline source is no file a kept scan could watch.
        sources: match scope.inline {
            Some(_) => Vec::new(),
            None => vec![canonical.clone()],
        },
        source_digest: octocode_engine::digest::sha256(&bytes),
        ignore_rules: verdict_inputs,
        test_code_excluded,
        test_code_unread,
        ..Scan::default()
    })
}

/// The refusal of a requested file that passes the source-map limit `limit`:
/// it names the file and the limit, and leads to the query over part of it.
fn source_limit_error(q: MatchQuery<'_>, file: &std::path::Path, limit: &str) -> ToolError {
    let path = file.to_string_lossy();
    let mut error = ToolError::new(
        octocode_contract_types::ErrorCode::ScopeTooBroad,
        format!(
            "{path} {}: narrow the search of this file with a smaller scope.",
            octocode_engine::text::analysis::MapLimits::default().passed(limit)
        ),
    );
    error.calls = super::continuations::narrow_leads(q, file, &path).map(|[first, rest]| {
        Box::new(serde_json::json!({ "narrowSearch": first, "narrowRest": rest }))
    });
    error
}

/// The scan of a directory's window: from `q`'s `scanOffset` until the time
/// budget cuts it, or with `through` the candidates before that position,
/// from the first, as an explicit file window the engine never cuts by time
/// (what a ranked scan lost, [`super::standing`]).
pub(super) fn scan_directory(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    through: Option<u32>,
) -> Result<Scan, ToolError> {
    scan_directory_keeping(q, scope, paths, cancel, through, Keeps::Page(None))
}

/// What a directory scan keeps of its matches: what the page shows, or a
/// census's counts, or whether any match exists at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Keeps {
    /// The page's own demand ([`demand`]): its rows are priced. With the
    /// page's budget, a content window in walk order ends at a start of its
    /// pages ([`super::stream::PageFill`], and
    /// [`octocode_engine::structural::PagedWindow`]): its pages are whole,
    /// and the next window's page 1 follows its last.
    Page(Option<usize>),
    /// Each file's match count: a census that states how many.
    Counts,
    /// Counts until a chunk of files holds a match: a census that asks
    /// only whether the scope holds any.
    First,
}

/// [`scan_directory`] keeping what `keeps` asks for.
pub(super) fn scan_directory_keeping(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    through: Option<u32>,
    keeps: Keeps,
) -> Result<Scan, ToolError> {
    #[cfg(test)]
    DIRECTORY_SCANS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(scope.canonical.clone());
    let include = q.include().unwrap_or_default();
    let admission = octocode_runtime_base::policy::discovery::WalkAdmission::new(
        paths.discovery_walk(),
        &scope.canonical,
        octocode_runtime_base::policy::discovery::WalkRules {
            defaults: q.default_excludes(),
            include: &include,
            generated: true,
            engine_prunes: true,
        },
    );
    let ignore_rules = q.include_ignored() != Some(true);
    // A test-code scope leaves out, unparsed, every file whose path puts it
    // on the other side (the walk's log decides it, so its census counts
    // only what is read), and every match on the other side of a file that
    // may mix both (its spans read once, on its first match).
    let scoped = octocode_tools_shared::test_code::Scoped::of(q.test_code(), paths);
    let log = WalkLog::new(
        scope,
        paths,
        &admission,
        q.include_hidden() == Some(true),
        scoped.as_ref(),
    );
    // Only a walk-order content window is cut by its page: a file view
    // lists every file's count, a ranked order needs every file, and an
    // explicit window is the caller's.
    let page_budget = match keeps {
        Keeps::Page(budget) if through.is_none() && !file_list(q) && !ranks_whole_scope(q) => {
            budget
        }
        _ => None,
    };
    let row_paths = super::super::RowPaths::new(paths, &scope.canonical, scope.is_file);
    // A window the one before ended inside its first file lists that file
    // from where the window before stopped.
    let carry = super::snapshot::listed_before(q).carry;
    let fill = page_budget.map(|budget| {
        std::sync::Mutex::new(PageFill::of(
            q,
            budget,
            octocode_engine::num::saturating_usize(carry),
        ))
    });
    let page_starts = |files: &[(usize, &StructuralSearchDetailedFileResult)]| {
        let Some(fill) = fill.as_ref() else {
            return Vec::new();
        };
        let mut fill = fill
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for (position, file) in files {
            if let FileMatches::Priced(prices) = &file.matches
                && !prices.is_empty()
            {
                let shown = super::super::rooted_display(
                    &scope.canonical,
                    std::path::Path::new(&file.path),
                );
                fill.add(
                    *position,
                    header_chars(&shown, row_paths),
                    prices.iter().map(|price| *price as usize),
                );
            }
        }
        fill.new_starts()
    };
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
    let keep_line = |path: &std::path::Path, line: u32| {
        scoped
            .as_ref()
            .is_none_or(|scoped| scoped.keeps(path, line))
    };
    let options = StructuralSearchFilesOptions {
        path: scope.canonical.to_string_lossy().into_owned(),
        pattern: None,
        rule: q.rule(),
        include: q.include(),
        exclude: q.exclude(),
        default_exclude_dir: Some(prune::walk_names(q.default_excludes(), &include)),
        ignore: octocode_engine::types::IgnorePolicy::new(q.include_ignored(), q.include_hidden()),
        max_depth: q.max_depth(),
        max_files: through.or_else(|| window_files(q.scan_offset())),
        skip_files: Some(through.map_or(q.scan_offset(), |_| 0)),
        max_file_bytes: u32::try_from(
            octocode_runtime_base::security::SourceLimits::REAL.whole_source,
        )
        .ok(),
        judge: Some(super::super::source::judge()),
        global_ignore: paths.global_ignore(),
        // Each file's outcome is kept across scans of this query: the
        // row price's inputs are the scan key's.
        kept: Some(super::memo::scan_key(q, paths)),
    };
    let admit =
        |path: &std::path::Path, file_type: Option<std::fs::FileType>| log.admit(path, file_type);
    let stop = || {
        cancel.check().is_err()
            || projection_failure
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_some()
    };
    // The scans of this scope beside this one share its walk and its parses
    // ([`super::shared_scope`]).
    let walk = super::shared_scope::walk_key(paths, &scope.canonical, &options);
    let r = super::shared_scope::scan(
        &walk,
        (&scope.canonical, paths),
        &|| cancel.check().is_err(),
        |share| {
            octocode_engine::structural::search_files_shared(
                options,
                match keeps {
                    Keeps::Page(_) => demand(
                        q,
                        &price,
                        page_budget.map(|_| (&page_starts as &PageStarts<'_>, carry > 0)),
                    ),
                    Keeps::Counts => octocode_engine::structural::StructuralDemand::Count,
                    Keeps::First => octocode_engine::structural::StructuralDemand::Exists,
                },
                (
                    &admit,
                    scoped
                        .as_ref()
                        .map(|_| &keep_line as &octocode_engine::structural::KeepLine),
                ),
                &|path| parser_extension(scope, path),
                &stop,
                share,
            )
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
    // What the walk's own rules left out joins the admission's ledger, as
    // a text search's walk records it: a file only when the scan reads it.
    let left_out = &r.walk_skipped;
    admission.record_left_out(
        (&left_out.ignored, &left_out.hidden, &left_out.pruned),
        |path| log.reads(path),
    );
    let logged = log.finish();
    let mut rules = if ignore_rules {
        octocode_engine::search::walk::external_ignore_inputs_with(
            &scope.canonical,
            paths.global_ignore(),
        )
    } else {
        Vec::new()
    };
    // What a `testCode` scope's verdicts read, a parent module outside this
    // scope too, proves a kept scan as the ignore rules do.
    if let Some(scoped) = &scoped {
        rules.extend(scoped.watched());
    }
    // Preserve the scan-level coverage signals the group projection drops.
    Ok(Scan {
        sources,
        source_digest,
        ignore_rules: rules,
        watches_ignore_files: ignore_rules,
        repository_markers: if ignore_rules {
            octocode_tools_shared::page_memo::repository_markers(&scope.canonical)
        } else {
            Vec::new()
        },
        discovery_sources: logged.discovery_sources,
        files: r
            .files
            .into_iter()
            .map(|f| ScannedFile {
                path: super::super::rooted_display(&scope.canonical, std::path::Path::new(&f.path)),
                parser: parser_extension(scope, std::path::Path::new(&f.path)),
                source: f
                    .source_sha256
                    .map(|digest| (std::path::PathBuf::from(&f.path), digest)),
                matches: f.matches,
                diagnostics: f.diagnostics,
                status: f.status,
                recovered: f.recovered,
                recovered_bodies: f.recovered_bodies,
            })
            .collect(),
        truncated: r.scan_truncated,
        filled: r.page_filled,
        page_split: r.page_split,
        resumed_inside: r.resumed_inside,
        scan_resume: r.scan_resume,
        listed_before: r.listed_before,
        listed_through: r.listed_through,
        earlier_files: r.earlier_files,
        diagnostics: r.diagnostics,
        skips: (r.skipped_unsupported, r.skipped_unreadable, r.skipped_large),
        walk_errors: r.walk_errors,
        skipped_by_prefilter: r.skipped_by_pre_filter,
        skipped: admission.skipped(),
        unparsed: logged.unparsed,
        census: logged.census,
        present: logged.present,
        test_code_excluded: scoped.as_ref().map(Scoped::excluded).unwrap_or_default(),
        test_code_walked: scoped.as_ref().map_or(0, Scoped::left_unread),
        test_code_unread: scoped.as_ref().map(Scoped::unread).unwrap_or_default(),
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

/// Files in the walk's own order: by path component, so a directory's files
/// precede its siblings that share its name as a prefix (`a/x.rs` before
/// `a-b.rs`, as text order would not). A scan cut into windows lists each
/// window in walk order; only this order makes their concatenation the
/// order of the whole scan.
fn walk_order(a: &ScannedFile, b: &ScannedFile) -> std::cmp::Ordering {
    std::path::Path::new(&a.path).cmp(std::path::Path::new(&b.path))
}

/// Whether `q`'s order ranks the scope's whole file set: every order but walk
/// order, the only one in which a cut scan's windows, each listed in turn,
/// list the files in the order of the whole scan.
pub(super) fn ranks_whole_scope(q: MatchQuery<'_>) -> bool {
    q.sort() == Some("matchCount") || q.reverse().unwrap_or(false)
}

pub(super) fn sort_files(q: MatchQuery<'_>, files: &mut [ScannedFile]) {
    let by_count = q.sort() == Some("matchCount");
    let descending = q.reverse().unwrap_or(false);
    files.sort_by(|a, b| {
        let ordering = if by_count {
            a.matches.len().cmp(&b.matches.len())
        } else {
            walk_order(a, b)
        };
        let ordering = if descending {
            ordering.reverse()
        } else {
            ordering
        };
        ordering.then_with(|| walk_order(a, b))
    });
}
