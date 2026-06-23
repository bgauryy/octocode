use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use ignore::overrides::{Override, OverrideBuilder};
use ignore::WalkBuilder;
use rayon::prelude::*;

use super::language::AgLanguage;
use super::octo::compile_matcher;
use super::query::{invalid_query_explanation, Prefilter, StructuralQuery};
use super::types::{
    structural_query_fingerprint, StructuralDetailedMatch, StructuralDiagnostic,
    StructuralSearchDetailedFileResult, StructuralSearchFileResult,
    StructuralSearchFilesDetailedResult, StructuralSearchFilesOptions, StructuralSearchFilesResult,
    STRUCTURAL_ANALYZER, STRUCTURAL_ANALYZER_VERSION,
};
use crate::signatures::languages;
use crate::types::RipgrepSearchOptions;

pub fn search_files(
    options: StructuralSearchFilesOptions,
) -> Result<StructuralSearchFilesResult, String> {
    let root = PathBuf::from(&options.path);
    let query = StructuralQuery::new(options.pattern.as_deref(), options.rule.as_deref())?;

    let include = options.include.unwrap_or_default();
    let exclude = options.exclude.unwrap_or_default();
    let exclude_dir = options.exclude_dir.unwrap_or_else(default_exclude_dirs);
    let hidden = options.hidden;
    let no_ignore = options.no_ignore;
    let max_depth = options.max_depth;
    let max_files = options.max_files.map(|n| n as usize).unwrap_or(2_000);
    let max_file_bytes = options
        .max_file_bytes
        .map(|n| n as u64)
        .unwrap_or(1_000_000);
    let prefilter = query.prefilter();

    let overrides = build_overrides(&root, &include, &exclude)?;
    let (candidate_files, skipped_by_pre_filter, skipped_unsupported) = match &prefilter {
        Prefilter::None => (
            collect_candidate_files(
                &root,
                overrides,
                &exclude_dir,
                max_files,
                hidden,
                no_ignore,
                max_depth,
            )?,
            0,
            0,
        ),
        Prefilter::Single(anchor) => {
            // `supported_only=false` so ripgrep searches every file — unsupported
            // extensions that textually contain the anchor must surface as
            // `skipped_unsupported`, not vanish into the prefilter lump.
            matching_anchor_candidate_files(
                &root,
                &include,
                &exclude,
                &exclude_dir,
                hidden,
                no_ignore,
                max_depth,
                overrides,
                anchor,
                max_files,
                false,
            )?
        }
        Prefilter::Union(anchors) => {
            // Union prefilter: files must contain at least one of the literals.
            // Uses regex alternation (safe because anchors are validated identifiers).
            matching_anchor_union_candidate_files(
                &root,
                &include,
                &exclude,
                &exclude_dir,
                hidden,
                no_ignore,
                max_depth,
                overrides,
                anchors,
                max_files,
            )?
        }
    };

    let mut by_ext: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    let mut skipped_unsupported_ext = skipped_unsupported;
    for path in candidate_files {
        let ext = extension_for_path(&path).unwrap_or_default();
        if languages::find_entry(&ext).is_none() {
            // Defense-in-depth: `matching_anchor_candidate_files` already
            // partitioned unsupported files out, but `collect_candidate_files`
            // (no-anchor path) returns supported-only, so this is a no-op there.
            skipped_unsupported_ext = skipped_unsupported_ext.saturating_add(1);
            continue;
        }
        by_ext.entry(ext).or_default().push(path);
    }

    let mut files = Vec::new();
    let mut total_matches = 0u32;
    let mut parsed_files = 0u32;
    let mut skipped_unreadable = 0u32;
    let mut skipped_large = 0u32;
    let mut warnings = Vec::new();

    enum SearchOutcome {
        Unreadable,
        Large,
        ParsedNoMatch,
        Matched(StructuralSearchFileResult),
    }

    for (ext, paths) in by_ext {
        let Some(entry) = languages::find_entry(&ext) else {
            // Unreachable (unsupported exts were partitioned above) but kept as
            // a guard so a future refactor can't silently re-introduce the drop.
            skipped_unsupported_ext = skipped_unsupported_ext.saturating_add(paths.len() as u32);
            continue;
        };
        let lang = AgLanguage::new(&ext, entry);
        let run = compile_matcher(&lang, query)?;

        let outcomes: Vec<SearchOutcome> = paths
            .into_par_iter()
            .map(|file_path| {
                let metadata = match fs::metadata(&file_path) {
                    Ok(m) => m,
                    Err(_) => return SearchOutcome::Unreadable,
                };
                if metadata.len() > max_file_bytes {
                    return SearchOutcome::Large;
                }
                let content = match fs::read_to_string(&file_path) {
                    Ok(c) => c,
                    Err(_) => return SearchOutcome::Unreadable,
                };
                let matches = run(&content);
                if matches.is_empty() {
                    SearchOutcome::ParsedNoMatch
                } else {
                    SearchOutcome::Matched(StructuralSearchFileResult {
                        path: file_path.to_string_lossy().to_string(),
                        matches,
                    })
                }
            })
            .collect();

        for outcome in outcomes {
            match outcome {
                SearchOutcome::Unreadable => skipped_unreadable += 1,
                SearchOutcome::Large => skipped_large += 1,
                SearchOutcome::ParsedNoMatch => parsed_files += 1,
                SearchOutcome::Matched(result) => {
                    parsed_files += 1;
                    total_matches =
                        total_matches.saturating_add(result.matches.len() as u32);
                    files.push(result);
                }
            }
        }
    }

    match &prefilter {
        Prefilter::None => {
            warnings.push(format!(
                "No literal anchor in the {} — parsed all {parsed_files} candidate file(s) with no text pre-filter.",
                if query.is_rule() { "rule" } else { "pattern" }
            ));
        }
        Prefilter::Single(_) if skipped_by_pre_filter > 0 => {
            warnings.push(format!(
                "Pre-filter skipped parsing {skipped_by_pre_filter} supported file(s) (literal anchor absent); parsed {parsed_files}."
            ));
        }
        Prefilter::Union(anchors) if skipped_by_pre_filter > 0 => {
            warnings.push(format!(
                "Union pre-filter ({} anchors: {}) skipped {skipped_by_pre_filter} supported file(s); parsed {parsed_files}.",
                anchors.len(),
                anchors.join("|")
            ));
        }
        _ => {}
    }
    if skipped_unsupported_ext > 0 {
        warnings.push(format!(
            "Skipped {skipped_unsupported_ext} candidate file(s) with unsupported extensions."
        ));
    }
    if skipped_unreadable > 0 {
        warnings.push(format!(
            "Skipped {skipped_unreadable} unreadable or vanished candidate file(s)."
        ));
    }
    if skipped_large > 0 {
        warnings.push(format!(
            "Skipped {skipped_large} candidate file(s) larger than {max_file_bytes} bytes."
        ));
    }

    Ok(StructuralSearchFilesResult {
        files,
        total_matches,
        parsed_files,
        skipped_by_pre_filter,
        skipped_unsupported: skipped_unsupported_ext,
        skipped_unreadable,
        skipped_large,
        warnings,
    })
}

pub fn search_files_detailed(
    options: StructuralSearchFilesOptions,
) -> Result<StructuralSearchFilesDetailedResult, String> {
    let StructuralSearchFilesOptions {
        path,
        pattern,
        rule,
        include,
        exclude,
        exclude_dir,
        hidden,
        no_ignore,
        max_depth,
        max_files,
        max_file_bytes,
    } = options;
    let pattern_ref = pattern.as_deref();
    let rule_ref = rule.as_deref();
    let query_fingerprint = structural_query_fingerprint(pattern_ref, rule_ref);
    let query = match StructuralQuery::new(pattern_ref, rule_ref) {
        Ok(query) => query,
        Err(message) => {
            let diagnostic = StructuralDiagnostic::new(
                "structural.query.invalid",
                "error",
                "match",
                message.clone(),
            )
            .with_path(path.clone())
            .with_recovery("Provide exactly one non-empty structural pattern or YAML rule.");
            return Ok(StructuralSearchFilesDetailedResult {
                files: Vec::new(),
                total_matches: 0,
                parsed_files: 0,
                skipped_by_pre_filter: 0,
                skipped_unsupported: 0,
                skipped_unreadable: 0,
                skipped_large: 0,
                analyzer: STRUCTURAL_ANALYZER.to_owned(),
                analyzer_version: STRUCTURAL_ANALYZER_VERSION.to_owned(),
                status: "parserFailed".to_owned(),
                query: invalid_query_explanation(pattern_ref, rule_ref, &message),
                diagnostics: vec![diagnostic],
                warnings: Vec::new(),
            });
        }
    };

    let root = PathBuf::from(&path);
    let include = include.unwrap_or_default();
    let exclude = exclude.unwrap_or_default();
    let exclude_dir = exclude_dir.unwrap_or_else(default_exclude_dirs);
    let max_files = max_files.map(|n| n as usize).unwrap_or(2_000);
    let max_file_bytes = max_file_bytes.map(|n| n as u64).unwrap_or(1_000_000);
    let prefilter = query.prefilter();
    let query_explanation = query.explanation();

    let overrides = build_overrides(&root, &include, &exclude)?;
    let candidate_files = collect_files(
        &root,
        overrides,
        &exclude_dir,
        max_files,
        false,
        hidden,
        no_ignore,
        max_depth,
    )?;
    let matching_paths: Option<HashSet<String>> = match &prefilter {
        Prefilter::None => None,
        Prefilter::Single(anchor) => Some(matching_anchor_paths(
            &root,
            &include,
            &exclude_dir,
            hidden,
            no_ignore,
            max_depth,
            anchor,
        )?),
        Prefilter::Union(anchors) => Some(matching_anchor_union_paths(
            &root,
            &include,
            &exclude_dir,
            hidden,
            no_ignore,
            max_depth,
            anchors,
        )?),
    };

    let mut matchers = BTreeMap::new();
    let mut files = Vec::new();
    let mut total_matches = 0u32;
    let mut parsed_files = 0u32;
    let mut skipped_by_pre_filter = 0u32;
    let mut skipped_unsupported = 0u32;
    let mut skipped_unreadable = 0u32;
    let mut skipped_large = 0u32;
    let mut compile_failures = 0u32;

    for file_path in candidate_files {
        let path_string = file_path.to_string_lossy().to_string();
        if matching_paths
            .as_ref()
            .is_some_and(|paths| !paths.contains(path_string.as_str()))
        {
            skipped_by_pre_filter += 1;
            files.push(skipped_file(
                path_string,
                "skippedByPreFilter",
                "preFilter",
                StructuralDiagnostic::new(
                    "structural.prefilter.skipped",
                    "info",
                    "scan",
                    "Literal anchor was absent, so AST parsing was skipped.",
                )
                .with_recovery("Remove the literal prefilter by using a rule with no safe anchor if every file must be parsed."),
            ));
            continue;
        }

        let ext = extension_for_path(&file_path).unwrap_or_default();
        let Some(entry) = languages::find_entry(&ext) else {
            skipped_unsupported += 1;
            files.push(skipped_file(
                path_string.clone(),
                "unsupported",
                "unsupportedExtension",
                StructuralDiagnostic::new(
                    "structural.language.unsupported",
                    "warning",
                    "parse",
                    format!("Structural search does not support .{ext} files."),
                )
                .with_path(path_string)
                .with_recovery(
                    "Use text search for this extension or add a tree-sitter grammar mapping.",
                ),
            ));
            continue;
        };

        let metadata = match fs::metadata(&file_path) {
            Ok(metadata) => metadata,
            Err(err) => {
                skipped_unreadable += 1;
                files.push(skipped_file(
                    path_string.clone(),
                    "unreadable",
                    "metadata",
                    StructuralDiagnostic::new(
                        "structural.file.unreadable",
                        "warning",
                        "scan",
                        format!("Could not read file metadata: {err}."),
                    )
                    .with_path(path_string)
                    .with_recovery(
                        "Retry if the file still exists and permissions allow reading it.",
                    ),
                ));
                continue;
            }
        };

        if metadata.len() > max_file_bytes {
            skipped_large += 1;
            files.push(skipped_file(
                path_string.clone(),
                "truncated",
                "maxFileBytes",
                StructuralDiagnostic::new(
                    "structural.file.tooLarge",
                    "warning",
                    "scan",
                    format!(
                        "File is {} bytes, above the structural search limit of {max_file_bytes} bytes.",
                        metadata.len()
                    ),
                )
                .with_path(path_string)
                .with_recovery("Raise maxFileBytes or inspect the file with a narrower text search first."),
            ));
            continue;
        }

        let content = match fs::read_to_string(&file_path) {
            Ok(content) => content,
            Err(err) => {
                skipped_unreadable += 1;
                files.push(skipped_file(
                    path_string.clone(),
                    "unreadable",
                    "read",
                    StructuralDiagnostic::new(
                        "structural.file.unreadable",
                        "warning",
                        "scan",
                        format!("Could not read file content as UTF-8: {err}."),
                    )
                    .with_path(path_string)
                    .with_recovery("Use binary inspection or text search for non-UTF-8 content."),
                ));
                continue;
            }
        };

        if !matchers.contains_key(&ext) {
            let lang = AgLanguage::new(&ext, entry);
            matchers.insert(ext.clone(), compile_matcher(&lang, query));
        }
        let Some(compiled) = matchers.get(&ext) else {
            compile_failures += 1;
            files.push(skipped_file(
                path_string.clone(),
                "parserFailed",
                "queryCompile",
                StructuralDiagnostic::new(
                    "structural.matcher.missing",
                    "error",
                    "match",
                    "Structural matcher was unavailable after compilation.",
                )
                .with_path(path_string)
                .with_recovery(
                    "Retry the search; this indicates an internal matcher lifecycle issue.",
                ),
            ));
            continue;
        };
        let run = match compiled {
            Ok(run) => run,
            Err(message) => {
                compile_failures += 1;
                files.push(skipped_file(
                    path_string.clone(),
                    "parserFailed",
                    "queryCompile",
                    StructuralDiagnostic::new(
                        "structural.query.compileFailed",
                        "error",
                        "match",
                        message.clone(),
                    )
                    .with_path(path_string)
                    .with_recovery("Check the structural pattern or YAML rule against this file's language grammar."),
                ));
                continue;
            }
        };

        let matches: Vec<StructuralDetailedMatch> = run(&content)
            .into_iter()
            .map(|matched| {
                StructuralDetailedMatch::from_match(&path_string, &query_fingerprint, matched)
            })
            .collect();
        parsed_files += 1;
        total_matches = total_matches.saturating_add(matches.len() as u32);
        files.push(StructuralSearchDetailedFileResult {
            path: path_string,
            status: "ok".to_owned(),
            language_id: entry.language_id.map(str::to_owned),
            skipped_reason: None,
            matches,
            diagnostics: Vec::new(),
        });
    }

    let mut warnings = Vec::new();
    match &prefilter {
        Prefilter::None => {
            warnings.push(format!(
                "No literal anchor in the {} — parsed all {parsed_files} supported candidate file(s) with no text pre-filter.",
                if query.is_rule() { "rule" } else { "pattern" }
            ));
        }
        Prefilter::Single(_) if skipped_by_pre_filter > 0 => {
            warnings.push(format!(
                "Pre-filter skipped parsing {skipped_by_pre_filter} file(s); parsed {parsed_files}."
            ));
        }
        Prefilter::Union(anchors) if skipped_by_pre_filter > 0 => {
            warnings.push(format!(
                "Union pre-filter ({} anchors: {}) skipped {skipped_by_pre_filter} file(s); parsed {parsed_files}.",
                anchors.len(),
                anchors.join("|")
            ));
        }
        _ => {}
    }
    if skipped_unsupported > 0 {
        warnings.push(format!(
            "Skipped {skipped_unsupported} candidate file(s) with unsupported extensions."
        ));
    }
    if skipped_unreadable > 0 {
        warnings.push(format!(
            "Skipped {skipped_unreadable} unreadable or vanished candidate file(s)."
        ));
    }
    if skipped_large > 0 {
        warnings.push(format!(
            "Skipped {skipped_large} candidate file(s) larger than {max_file_bytes} bytes."
        ));
    }

    let status = if compile_failures > 0 {
        "parserFailed"
    } else if parsed_files == 0
        && skipped_unsupported > 0
        && skipped_by_pre_filter == 0
        && skipped_unreadable == 0
        && skipped_large == 0
    {
        "unsupported"
    } else if skipped_unsupported > 0 || skipped_unreadable > 0 || skipped_large > 0 {
        "partial"
    } else {
        "ok"
    };

    Ok(StructuralSearchFilesDetailedResult {
        files,
        total_matches,
        parsed_files,
        skipped_by_pre_filter,
        skipped_unsupported,
        skipped_unreadable,
        skipped_large,
        analyzer: STRUCTURAL_ANALYZER.to_owned(),
        analyzer_version: STRUCTURAL_ANALYZER_VERSION.to_owned(),
        status: status.to_owned(),
        query: query_explanation,
        diagnostics: Vec::new(),
        warnings,
    })
}

fn matching_anchor_paths(
    root: &Path,
    include: &[String],
    exclude_dir: &[String],
    hidden: Option<bool>,
    no_ignore: Option<bool>,
    // Accepted for API uniformity; ripgrep-native has no max_depth, so the
    // anchor-prefilter path can't enforce it. The no-anchor walker path does.
    _max_depth: Option<u32>,
    anchor: &str,
) -> Result<HashSet<String>, String> {
    let result = crate::ripgrep_search::search(RipgrepSearchOptions {
        path: root.to_string_lossy().into_owned(),
        pattern: anchor.to_owned(),
        fixed_string: Some(true),
        case_sensitive: Some(true),
        files_only: Some(true),
        include: (!include.is_empty()).then(|| include.to_vec()),
        exclude_dir: (!exclude_dir.is_empty()).then(|| exclude_dir.to_vec()),
        hidden,
        no_ignore,
        // max_depth is not a RipgrepSearchOptions field — enforced by the
        // `collect_files` walker instead (this prefilter path holes it).
        sort: Some("path".to_owned()),
        ..RipgrepSearchOptions::default()
    })
    .map_err(|err| format!("literal prefilter failed for anchor '{anchor}': {err}"))?;

    Ok(result.files.into_iter().map(|file| file.path).collect())
}

// Both anchor-prefilter helpers thread the 6 scope fields the OQL contract
// requires (include/exclude/exclude_dir/hidden/no_ignore/max_depth); bundling
// into a `FileScope` struct is a future cleanup, not warranted for this fix.
#[allow(clippy::too_many_arguments)]
fn matching_anchor_candidate_files(
    root: &Path,
    include: &[String],
    exclude: &[String],
    exclude_dir: &[String],
    hidden: Option<bool>,
    no_ignore: Option<bool>,
    // Accepted for API uniformity; ripgrep-native has no max_depth, so the
    // anchor-prefilter path can't enforce it. The no-anchor walker path does.
    _max_depth: Option<u32>,
    overrides: Override,
    anchor: &str,
    max_files: usize,
    supported_only: bool,
) -> Result<(Vec<PathBuf>, u32, u32), String> {
    let search_include = anchor_search_include_globs(include, supported_only);
    let result = crate::ripgrep_search::search(RipgrepSearchOptions {
        path: root.to_string_lossy().into_owned(),
        pattern: anchor.to_owned(),
        fixed_string: Some(true),
        case_sensitive: Some(true),
        files_only: Some(true),
        include: (!search_include.is_empty()).then_some(search_include),
        exclude: (!exclude.is_empty()).then(|| exclude.to_vec()),
        exclude_dir: (!exclude_dir.is_empty()).then(|| exclude_dir.to_vec()),
        hidden,
        no_ignore,
        // max_depth is not a RipgrepSearchOptions field — enforced by the
        // `collect_files` walker instead (this prefilter path holes it).
        sort: Some("path".to_owned()),
        ..RipgrepSearchOptions::default()
    })
    .map_err(|err| format!("literal prefilter failed for anchor '{anchor}': {err}"))?;

    let files_searched = result.stats.files_searched.unwrap_or_default();
    let mut matched_supported = 0u32;
    let mut matched_unsupported = 0u32;
    let mut out = Vec::new();
    for file in result.files {
        let path = PathBuf::from(file.path);
        if overrides.matched(&path, false).is_ignore() {
            continue;
        }
        let is_supported =
            extension_for_path(&path).is_some_and(|ext| languages::find_entry(&ext).is_some());
        if is_supported {
            matched_supported = matched_supported.saturating_add(1);
            if out.len() < max_files {
                out.push(path);
            }
        } else {
            matched_unsupported = matched_unsupported.saturating_add(1);
        }
    }

    // `skipped_by_pre_filter` = supported files ripgrep searched but the anchor
    // was absent (proof of no match); `skipped_unsupported` = unsupported-ext
    // files that contained the anchor (not evaluated, not proof). Splitting
    // them keeps the legacy result honest about evidence kind.
    let skipped_by_pre_filter = files_searched
        .saturating_sub(matched_supported)
        .saturating_sub(matched_unsupported);
    Ok((out, skipped_by_pre_filter, matched_unsupported))
}

/// Union prefilter variant of [`matching_anchor_paths`]: a file qualifies if it
/// contains ANY of `anchors` (regex alternation with escaped literals).
fn matching_anchor_union_paths(
    root: &Path,
    include: &[String],
    exclude_dir: &[String],
    hidden: Option<bool>,
    no_ignore: Option<bool>,
    _max_depth: Option<u32>,
    anchors: &[&str],
) -> Result<HashSet<String>, String> {
    let pattern = anchors_to_regex(anchors);
    let result = crate::ripgrep_search::search(RipgrepSearchOptions {
        path: root.to_string_lossy().into_owned(),
        pattern,
        fixed_string: None, // regex alternation — not fixed-string
        case_sensitive: Some(true),
        files_only: Some(true),
        include: (!include.is_empty()).then(|| include.to_vec()),
        exclude_dir: (!exclude_dir.is_empty()).then(|| exclude_dir.to_vec()),
        hidden,
        no_ignore,
        sort: Some("path".to_owned()),
        ..RipgrepSearchOptions::default()
    })
    .map_err(|err| format!("union prefilter failed for anchors {anchors:?}: {err}"))?;
    Ok(result.files.into_iter().map(|f| f.path).collect())
}

/// Union prefilter variant of [`matching_anchor_candidate_files`].
#[allow(clippy::too_many_arguments)]
fn matching_anchor_union_candidate_files(
    root: &Path,
    include: &[String],
    exclude: &[String],
    exclude_dir: &[String],
    hidden: Option<bool>,
    no_ignore: Option<bool>,
    _max_depth: Option<u32>,
    overrides: Override,
    anchors: &[&str],
    max_files: usize,
) -> Result<(Vec<PathBuf>, u32, u32), String> {
    let search_include = anchor_search_include_globs(include, false);
    let pattern = anchors_to_regex(anchors);
    let result = crate::ripgrep_search::search(RipgrepSearchOptions {
        path: root.to_string_lossy().into_owned(),
        pattern,
        fixed_string: None, // regex alternation
        case_sensitive: Some(true),
        files_only: Some(true),
        include: (!search_include.is_empty()).then_some(search_include),
        exclude: (!exclude.is_empty()).then(|| exclude.to_vec()),
        exclude_dir: (!exclude_dir.is_empty()).then(|| exclude_dir.to_vec()),
        hidden,
        no_ignore,
        sort: Some("path".to_owned()),
        ..RipgrepSearchOptions::default()
    })
    .map_err(|err| format!("union prefilter failed for anchors {anchors:?}: {err}"))?;

    let files_searched = result.stats.files_searched.unwrap_or_default();
    let mut matched_supported = 0u32;
    let mut matched_unsupported = 0u32;
    let mut out = Vec::new();
    for file in result.files {
        let path = PathBuf::from(file.path);
        if overrides.matched(&path, false).is_ignore() {
            continue;
        }
        let is_supported =
            extension_for_path(&path).is_some_and(|ext| languages::find_entry(&ext).is_some());
        if is_supported {
            matched_supported = matched_supported.saturating_add(1);
            if out.len() < max_files {
                out.push(path);
            }
        } else {
            matched_unsupported = matched_unsupported.saturating_add(1);
        }
    }
    let skipped_by_pre_filter = files_searched
        .saturating_sub(matched_supported)
        .saturating_sub(matched_unsupported);
    Ok((out, skipped_by_pre_filter, matched_unsupported))
}

/// Build a ripgrep regex alternation from anchor literals.
/// Each anchor is escaped so operator characters (`&&`, `||`, etc.) are treated
/// as literals, not regex metacharacters.
fn anchors_to_regex(anchors: &[&str]) -> String {
    anchors
        .iter()
        .map(|a| regex_escape_anchor(a))
        .collect::<Vec<_>>()
        .join("|")
}

/// Escape regex metacharacters in an anchor string.
/// Safe anchors from `derive_literal_anchor` contain only alphanumeric, `_`,
/// and a small set of operator characters — all of which are safe after escaping.
fn regex_escape_anchor(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for ch in s.chars() {
        if matches!(
            ch,
            '.' | '+'
                | '*'
                | '?'
                | '^'
                | '$'
                | '{'
                | '}'
                | '('
                | ')'
                | '|'
                | '['
                | ']'
                | '\\'
                | '&'
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn anchor_search_include_globs(include: &[String], supported_only: bool) -> Vec<String> {
    if !include.is_empty() || !supported_only {
        return include.to_vec();
    }
    languages::supported_extensions()
        .into_iter()
        .map(|ext| format!("*.{ext}"))
        .collect()
}

fn default_exclude_dirs() -> Vec<String> {
    [
        "node_modules",
        "dist",
        ".git",
        "build",
        "coverage",
        ".next",
        "out",
        "target",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

/// Compile `include` + `exclude` into a gitignore-style override set, rooted at
/// the search path so relative globs like `src/**/*.ts` resolve as users expect.
/// `exclude` globs are added negated (`!glob`) so they drop files that `include`
/// would otherwise match — mirroring `localSearchCode.exclude`。
fn build_overrides(
    root: &Path,
    include: &[String],
    exclude: &[String],
) -> Result<Override, String> {
    let mut builder = OverrideBuilder::new(root);
    for glob in include {
        builder
            .add(glob)
            .map_err(|err| format!("invalid include glob '{glob}': {err}"))?;
    }
    for glob in exclude {
        let negated = if glob.starts_with('!') {
            glob.to_owned()
        } else {
            format!("!{glob}")
        };
        builder
            .add(&negated)
            .map_err(|err| format!("invalid exclude glob '{glob}': {err}"))?;
    }
    builder
        .build()
        .map_err(|err| format!("failed to compile include/exclude globs: {err}"))
}

/// Walk `root` with ripgrep's own `ignore` engine and yields deterministic
/// candidate paths whose extension a grammar can parse.
fn collect_candidate_files(
    root: &Path,
    overrides: Override,
    exclude_dir: &[String],
    max_files: usize,
    hidden: Option<bool>,
    no_ignore: Option<bool>,
    max_depth: Option<u32>,
) -> Result<Vec<PathBuf>, String> {
    collect_files(
        root,
        overrides,
        exclude_dir,
        max_files,
        true,
        hidden,
        no_ignore,
        max_depth,
    )
}

// Walker threads the 6 scope fields the OQL contract requires; a `FileScope`
// struct would reduce the param count but is out of scope for this parity fix.
#[allow(clippy::too_many_arguments)]
fn collect_files(
    root: &Path,
    overrides: Override,
    exclude_dir: &[String],
    max_files: usize,
    supported_only: bool,
    hidden: Option<bool>,
    no_ignore: Option<bool>,
    max_depth: Option<u32>,
) -> Result<Vec<PathBuf>, String> {
    let metadata = fs::metadata(root).map_err(|err| {
        format!(
            "Cannot access structural search path '{}': {err}",
            root.display()
        )
    })?;

    if metadata.is_file() {
        return Ok(if file_is_candidate(root, &overrides, supported_only) {
            vec![root.to_path_buf()]
        } else {
            Vec::new()
        });
    }
    if !metadata.is_dir() {
        return Ok(Vec::new());
    }

    let excluded: HashSet<String> = exclude_dir.iter().cloned().collect();
    let mut builder = WalkBuilder::new(root);
    builder
        .overrides(overrides)
        // `hidden`/`no_ignore`/`max_depth` mirror the text lane (RipgrepSearchOptions)
        // so OQL `scope` parity holds on the structural walker too. Default
        // (None) preserves the ignore crate's standard behavior.
        .hidden(hidden != Some(true))
        .git_ignore(no_ignore != Some(true))
        .ignore(no_ignore != Some(true))
        .max_depth(max_depth.map(|n| n as usize))
        .sort_by_file_path(|a, b| a.cmp(b))
        .filter_entry(move |entry| {
            if entry.depth() == 0 {
                return true;
            }
            if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                let name = entry.file_name().to_string_lossy();
                return !excluded.contains(name.as_ref());
            }
            true
        });

    let mut out = Vec::new();
    for result in builder.build() {
        if out.len() >= max_files {
            break;
        }
        let Ok(entry) = result else { continue };
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.into_path();
        if !supported_only
            || extension_for_path(&path).is_some_and(|ext| languages::find_entry(&ext).is_some())
        {
            out.push(path);
        }
    }
    Ok(out)
}

fn file_is_candidate(path: &Path, overrides: &Override, supported_only: bool) -> bool {
    !overrides.matched(path, false).is_ignore()
        && (!supported_only
            || extension_for_path(path).is_some_and(|ext| languages::find_entry(&ext).is_some()))
}

fn extension_for_path(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
}

fn skipped_file(
    path: String,
    status: &str,
    skipped_reason: &str,
    diagnostic: StructuralDiagnostic,
) -> StructuralSearchDetailedFileResult {
    StructuralSearchDetailedFileResult {
        path,
        status: status.to_owned(),
        language_id: None,
        skipped_reason: Some(skipped_reason.to_owned()),
        matches: Vec::new(),
        diagnostics: vec![diagnostic],
    }
}
