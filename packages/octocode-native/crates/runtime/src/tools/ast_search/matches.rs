pub use crate::contracts::tool_types::{AstSearchQueryMatchPattern, AstSearchQueryMatchRule};
use crate::tools::ast_rule::{
    GrammarChoice, choose_grammar, compile_check, has_extension_in, language_extensions,
    present_grammars,
};
use crate::tools::id::ToolId;
use crate::tools::id::query_limits::ast_search::r#match as limits;
use crate::tools::num::u32_of;
use crate::tools::result::ToolError;
use crate::{
    policy::{path::PathPolicy, prune::PruneMode},
    security::ContentSecurity,
    tools::cancel::CancellationCheck,
};
use octocode_engine::structural::{
    StructuralDetailedMatch, StructuralDiagnostic, StructuralSearchFilesOptions,
};
use serde_json::{Value, json};

/// A `match` query in either of its generated forms (pattern or rule).
#[derive(Clone, Copy, Debug)]
pub enum MatchQuery<'a> {
    Pattern(&'a AstSearchQueryMatchPattern),
    Rule(&'a AstSearchQueryMatchRule),
}

/// Binds `$field` from whichever form `$query` is.
macro_rules! either_form {
    ($query:expr, $field:ident => $value:expr) => {
        match $query {
            MatchQuery::Pattern(AstSearchQueryMatchPattern { $field, .. }) => $value,
            MatchQuery::Rule(AstSearchQueryMatchRule { $field, .. }) => $value,
        }
    };
}

fn non_empty(values: &[String]) -> Option<Vec<String>> {
    (!values.is_empty()).then(|| values.to_vec())
}

/// Form-independent views in the engine's units.
impl MatchQuery<'_> {
    pub fn path(self) -> String {
        either_form!(self, path => path.to_string())
    }
    pub fn pattern(self) -> Option<String> {
        match self {
            Self::Pattern(query) => Some(query.pattern.to_string()),
            Self::Rule(_) => None,
        }
    }
    pub fn rule(self) -> Option<String> {
        match self {
            Self::Pattern(_) => None,
            // An object rule serializes as JSON, which is valid YAML for the
            // same rule.
            Self::Rule(query) => Some(match serde_json::to_value(&query.rule) {
                Ok(Value::String(text)) => text,
                Ok(mut value) => {
                    crate::tools::result::remove_nulls(&mut value);
                    value.to_string()
                }
                Err(_) => String::new(),
            }),
        }
    }
    pub fn include(self) -> Option<Vec<String>> {
        either_form!(self, include => non_empty(include).map(|globs| crate::policy::include::include_globs(&globs)))
    }
    pub fn exclude(self) -> Option<Vec<String>> {
        either_form!(self, exclude => non_empty(exclude))
    }
    pub fn default_excludes(self) -> bool {
        use crate::policy::prune::DefaultsFlag;
        either_form!(self, default_excludes => default_excludes.defaults())
    }
    pub fn hidden(self) -> Option<bool> {
        either_form!(self, hidden => *hidden)
    }
    pub fn no_ignore(self) -> Option<bool> {
        either_form!(self, no_ignore => *no_ignore)
    }
    pub fn reverse(self) -> Option<bool> {
        either_form!(self, reverse => *reverse)
    }
    pub fn capture_text(self) -> Option<bool> {
        either_form!(self, capture_text => *capture_text)
    }
    /// Levels below `path` (1 = its children), as the wire counts them.
    pub fn max_depth(self) -> Option<u32> {
        either_form!(self, max_depth => max_depth.map(|depth| u32::try_from(depth.get()).unwrap_or(u32::MAX)))
    }
    pub fn max_files(self) -> u32 {
        either_form!(self, max_files => u32_of(*max_files))
    }
    /// Leading candidates (walk order) an earlier window evaluated.
    pub fn scan_offset(self) -> u32 {
        either_form!(self, scan_offset => scan_offset.map_or(0, |offset| u32::try_from(offset.get()).unwrap_or(u32::MAX)))
    }
    pub fn match_page_size(self) -> u32 {
        either_form!(self, match_page_size => u32_of(*match_page_size))
    }
    pub fn match_content_length(self) -> Option<u32> {
        either_form!(self, match_content_length => Some(u32_of(*match_content_length)))
    }
    pub fn language(self) -> Option<String> {
        either_form!(self, language => language.as_ref().map(ToString::to_string))
    }
    pub fn sort(self) -> Option<String> {
        either_form!(self, sort => Some(sort.to_string()))
    }
    pub fn result_view(self) -> Option<String> {
        either_form!(self, result_view => Some(result_view.to_string()))
    }
    pub fn page(self) -> u32 {
        either_form!(self, page => u32_of(*page))
    }
    pub fn match_page(self) -> u32 {
        either_form!(self, match_page => u32_of(*match_page))
    }
    pub fn page_size(self) -> u32 {
        either_form!(self, page_size => u32_of(*page_size))
    }
    pub fn snapshot(self) -> Option<String> {
        either_form!(self, snapshot => snapshot.as_ref().map(ToString::to_string))
    }
    /// The query as a continuation copies it; an object rule keeps only the
    /// matchers the caller set.
    fn to_value(self) -> Value {
        let mut value = match self {
            Self::Pattern(query) => serde_json::to_value(query),
            Self::Rule(query) => serde_json::to_value(query),
        }
        .unwrap_or_else(|_| json!({}));
        if let Some(rule) = value.get_mut("rule") {
            crate::tools::result::remove_nulls(rule);
        }
        value
    }
}

pub fn execute_match(
    q: MatchQuery<'_>,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> super::AstResult {
    let out = execute_match_inner(q, paths, security, cancel).map_err(|mut error| {
        // A YAML rule that fails to compile needs a rule-shaped recovery, not
        // the pattern advice (terminators, bodies) the generic hint gives.
        if q.rule().is_some() && error.code == "invalidPattern" && error.hints.is_empty() {
            error.hints.push(RULE_COMPILE_HINT.to_owned());
        }
        error
    })?;
    Ok(retry_omitted_parts(q, out, paths, security, cancel))
}

/// A declaration pattern matches exactly, so `function $N($$$A) { $$$B }`
/// misses every function with a return type and Rust `fn …` every `pub`
/// item. Re-run it once as a rule over the pattern plus the variants that
/// spell those parts. A union that finds more replaces the result as
/// written, with a warning naming the variants; its continuations replay
/// that rule.
fn retry_omitted_parts(
    q: MatchQuery<'_>,
    out: Value,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> Value {
    let (MatchQuery::Pattern(query), Some(pattern)) = (q, q.pattern()) else {
        return out;
    };
    if out["status"] == "error" {
        return out;
    }
    let language = out["inferredLanguage"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| q.language());
    let extensions = match language.as_deref() {
        Some(language) => language_extensions(language).unwrap_or_default(),
        None => Some(octocode_engine::text::extension_of(&q.path(), true, ""))
            .filter(|extension| !extension.is_empty())
            .into_iter()
            .collect(),
    };
    let variants = omitted_part_variants(&pattern, &extensions);
    if variants.is_empty() {
        return out;
    }
    let mut row = serde_json::to_value(query).unwrap_or_default();
    let Some(fields) = row.as_object_mut() else {
        return out;
    };
    fields.retain(|key, value| key != "pattern" && !value.is_null());
    let any = std::iter::once(&pattern)
        .chain(&variants)
        .map(|variant| json!({"pattern": variant}))
        .collect::<Vec<_>>();
    fields.insert("rule".into(), json!({"any": any}));
    let Ok(rule) = serde_json::from_value::<AstSearchQueryMatchRule>(row) else {
        return out;
    };
    let Ok(mut retried) = execute_match_inner(MatchQuery::Rule(&rule), paths, security, cancel)
    else {
        return out;
    };
    let written = out["stats"]["matchCount"].as_u64().unwrap_or(0);
    if retried["stats"]["matchCount"].as_u64().unwrap_or(0) <= written {
        return out;
    }
    let note = json!({
        "code":"structural.pattern.relaxed",
        "severity":"warning",
        "stage":"match",
        "message":format!("{written} matches as written; the rest match `{}`.", variants.join("` or `")),
    });
    match retried["diagnostics"].as_array_mut() {
        Some(diagnostics) => diagnostics.insert(0, note),
        None => retried["diagnostics"] = json!([note]),
    }
    retried
}

/// Variants of a declaration pattern that spell what it omitted: a return
/// type (TypeScript, Rust, Python, Go) and, for Rust items, `pub`.
fn omitted_part_variants(
    pattern: &str,
    extensions: &std::collections::BTreeSet<String>,
) -> Vec<String> {
    let has = |ext: &str| extensions.contains(ext);
    let (keyword, slot, body) = if has("ts") || has("tsx") {
        ("function", ": $_", '{')
    } else if has("rs") {
        ("fn", " -> $_", '{')
    } else if has("py") {
        ("def", " -> $_", ':')
    } else if has("go") {
        ("func", " $_", '{')
    } else {
        return Vec::new();
    };
    let returned = return_slot(pattern, keyword, slot, body);
    let first = pattern.split_whitespace().next().unwrap_or_default();
    let visible =
        (has("rs") && RUST_VISIBLE_ITEMS.contains(&first)).then(|| format!("pub {pattern}"));
    let both = returned
        .as_ref()
        .filter(|_| visible.is_some())
        .map(|returned| format!("pub {returned}"));
    [returned, visible, both].into_iter().flatten().collect()
}

/// `pattern` with `slot` after the parameter list that `body` directly
/// follows, when the declaration after `keyword` spells no return type.
fn return_slot(pattern: &str, keyword: &str, slot: &str, body: char) -> Option<String> {
    let start = pattern.match_indices(keyword).find_map(|(index, _)| {
        let before = pattern[..index].chars().next_back();
        let after = pattern[index + keyword.len()..].chars().next();
        let boundary = |ch: Option<char>| {
            ch.is_none_or(|ch| !(ch.is_alphanumeric() || ch == '_' || ch == '$'))
        };
        (boundary(before) && boundary(after)).then_some(index + keyword.len())
    })?;
    let mut depth = 0_i32;
    let mut close = None;
    for (offset, ch) in pattern[start..].char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => {
                depth -= 1;
                if depth == 0 && ch == ')' {
                    close = Some(start + offset);
                }
            }
            _ if depth == 0 && ch == body => {
                let close = close?;
                // Only whitespace between `)` and the body: no return type.
                return pattern[close + 1..start + offset]
                    .trim()
                    .is_empty()
                    .then(|| format!("{}{slot}{}", &pattern[..=close], &pattern[close + 1..]));
            }
            _ => {}
        }
    }
    None
}

const RULE_COMPILE_HINT: &str = "Fix the YAML rule where the error points (keys: kind pattern regex has inside all any not stopBy); test patterns alone.";

/// Rust item keywords a visibility modifier (`pub`, `pub(crate)`) can precede.
const RUST_VISIBLE_ITEMS: &[&str] = &[
    "fn", "async", "unsafe", "const", "static", "struct", "enum", "union", "trait", "type", "mod",
    "extern", "use",
];

/// The validated scope of one `match` query and the grammar it runs with.
struct Scope {
    canonical: std::path::PathBuf,
    is_file: bool,
    /// `language`, else the grammar inferred for a directory.
    language: Option<String>,
    /// Set when `language` was inferred; continuations pin it.
    inferred: Option<String>,
    extensions: Option<std::collections::BTreeSet<String>>,
}

/// One scanned file: display path, matches, diagnostics, engine status.
type ScannedFile = (
    String,
    Vec<StructuralDetailedMatch>,
    Vec<StructuralDiagnostic>,
    String,
);

/// The files a scan evaluated plus its corpus-coverage signals. A
/// single-file scan leaves the signals at their complete defaults.
#[derive(Default)]
struct Scan {
    files: Vec<ScannedFile>,
    truncated: bool,
    diagnostics: Vec<StructuralDiagnostic>,
    /// Unsupported, unreadable and over-large files skipped.
    skips: (u32, u32, u32),
    skipped_by_prefilter: u32,
    /// Every file the scan evaluated (absolute), for the memo's stamps.
    sources: Vec<std::path::PathBuf>,
    /// Candidate entries the security path policy withheld from the walk.
    withheld: crate::policy::discovery::Withheld,
}

/// Scans kept for the continuation pages of a multi-page result.
static SCANS: super::memo::ScanMemo<Scan> = super::memo::ScanMemo::new();

/// The scan inputs a `match` snapshot does not digest; a stored scan is
/// served only to a page with the same ones.
fn scan_key(q: MatchQuery<'_>, paths: &PathPolicy) -> String {
    crate::digest::json_sha256(&json!([
        paths.identity(),
        q.default_excludes(),
        super::MAX_PARSE_SOURCE_BYTES
    ]))
}

/// Rough heap bytes of a stored scan, its memo weight.
fn scan_bytes(scan: &Scan) -> usize {
    let diagnostic =
        |d: &StructuralDiagnostic| 64 + d.message.len() + d.path.as_ref().map_or(0, String::len);
    scan.files
        .iter()
        .map(|(path, matches, diagnostics, _)| {
            96 + path.len()
                + matches
                    .iter()
                    .map(|m| {
                        128 + m.text.len()
                            + m.header.as_ref().map_or(0, String::len)
                            + m.metavars
                                .values()
                                .flatten()
                                .map(|value| 24 + value.len())
                                .sum::<usize>()
                            + m.metavar_ranges
                                .values()
                                .flatten()
                                .map(|range| 48 + range.text.len())
                                .sum::<usize>()
                    })
                    .sum::<usize>()
                + diagnostics.iter().map(diagnostic).sum::<usize>()
        })
        .sum::<usize>()
        + scan.diagnostics.iter().map(diagnostic).sum::<usize>()
        + scan
            .sources
            .iter()
            .map(|path| 64 + path.as_os_str().len())
            .sum::<usize>()
}

/// One file's row on the result page, with what its match page withheld.
struct Group {
    row: Value,
    more_matches: bool,
    truncated_captures: bool,
    /// The longest value this match page shortened, in characters.
    cut_chars: Option<usize>,
    /// Line spans of this match page's rows whose shown text was cut.
    clipped: Vec<(u64, u64)>,
}

/// Rows per file plus per-file coverage: files that parsed, files whose
/// query failed to compile, and files cut short (execution limit,
/// unreadable, unsupported).
#[derive(Default)]
struct Grouped {
    groups: Vec<Group>,
    /// The first file that parsed (its row path).
    first_parsed: Option<String>,
    diagnostics: Vec<Value>,
    total_matches: u64,
    parsed_files: u32,
    compile_error: Option<ToolError>,
    compile_failed_files: u32,
    unevaluated_files: u32,
}

fn execute_match_inner(
    q: MatchQuery<'_>,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> super::AstResult {
    cancel.check().map_err(ToolError::cancelled)?;
    let scope = resolve_scope(q, paths, cancel)?;
    let continues = q.page() > 1 || q.match_page() > 1;
    let key = scan_key(q, paths);
    // A continuation page reuses the scan its first page stored when nothing
    // it read changed; the snapshot below still decides whether it serves.
    let stored = q
        .snapshot()
        .filter(|_| continues)
        .and_then(|snapshot| SCANS.get(&snapshot, &key));
    let fresh = stored.is_none();
    let started = std::time::SystemTime::now();
    let scan = match stored {
        Some(scan) => scan,
        None => {
            let mut scan = if scope.is_file {
                scan_file(q, &scope, security)?
            } else {
                scan_directory(q, &scope, paths, security, cancel)?
            };
            cancel.check().map_err(ToolError::cancelled)?;
            sort_files(q, &mut scan.files);
            std::sync::Arc::new(scan)
        }
    };
    let snapshot = match_snapshot(q, &scope, &scan.files);
    if continues && q.snapshot().as_deref() != Some(&snapshot) {
        if !fresh {
            // A stored scan is only a candidate: a page whose query no longer
            // matches it is answered by a fresh scan, exactly as without one.
            if let Some(stale) = q.snapshot() {
                SCANS.evict(&stale);
            }
            return execute_match_inner(q, paths, security, cancel);
        }
        if let Some(error) = interrupted_rescan(q, &scan) {
            return Err(error);
        }
        return Ok(crate::tools::result::stale_snapshot(continuation_with(
            q,
            json!({"page":1,"matchPage":1,"snapshot":null}),
            &snapshot,
        )));
    }
    let grouped = group_files(q, &scan.files);
    if grouped.total_matches == 0
        && grouped.parsed_files == 0
        && grouped.compile_failed_files > 0
        && let Some(error) = grouped.compile_error
    {
        // The query compiled in no candidate file: an empty result would read
        // as proof of absence, so surface the compile failure instead.
        return Err(error);
    }
    let out = render_match_page(q, &scope, &scan, grouped, &snapshot);
    if fresh
        && out["next"]
            .as_object()
            .is_some_and(|next| next.contains_key("nextPage") || next.contains_key("nextMatchPage"))
    {
        SCANS.put(
            (snapshot, key),
            &scope.canonical,
            &scan.sources,
            started,
            std::sync::Arc::clone(&scan),
            scan_bytes(&scan),
        );
    }
    Ok(out)
}

/// Validates the path and picks the grammar: `language`, else for a
/// directory the one grammar that both occurs in the scope and parses the
/// query.
fn resolve_scope(
    q: MatchQuery<'_>,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<Scope, ToolError> {
    let requested = q
        .language()
        .as_deref()
        .map(|language| {
            language_extensions(language).ok_or_else(|| {
                ToolError::new(
                    "languageUnsupported",
                    crate::tools::ast_rule::unsupported_language_message(language),
                )
            })
        })
        .transpose()?;
    let p = paths.validate(q.path()).map_err(super::policy_error)?;
    let meta =
        std::fs::metadata(&p.canonical).map_err(|error| super::io_error(&q.path(), error))?;
    let inferred = if meta.is_dir() && q.language().is_none() {
        Some(infer_directory_language(q, &p.canonical, paths, cancel)?)
    } else {
        None
    };
    let language = q.language().or_else(|| inferred.clone());
    let extensions = match (requested, &language) {
        (None, Some(language)) => language_extensions(language),
        (extensions, _) => extensions,
    };
    // The literal prefilter skips files without the pattern's anchor
    // unparsed, so an unparseable pattern (`foo(`) would otherwise report
    // `complete` and empty wherever the anchor is absent.
    if meta.is_dir()
        && let Some(extensions) = &extensions
    {
        compile_check(extensions, q.pattern().as_deref(), q.rule().as_deref())
            .map_err(|error| ToolError::new(error.code, error.message))?;
    }
    Ok(Scope {
        canonical: p.canonical,
        is_file: meta.is_file(),
        language,
        inferred,
        extensions,
    })
}

fn scan_file(
    q: MatchQuery<'_>,
    scope: &Scope,
    security: &ContentSecurity,
) -> Result<Scan, ToolError> {
    let canonical = &scope.canonical;
    super::validate_file_language(canonical, scope.language.as_deref())?;
    let Some(bytes) = super::read_parse_source(canonical)? else {
        return Err(ToolError::new(
            "fileTooLarge",
            "Source exceeds the native parser byte limit.",
        ));
    };
    // Parse the file as the directory scan does: raw. Output strings are
    // redacted by the response stage.
    let source = security
        .decode_source_bytes(&bytes, super::MAX_PARSE_SOURCE_BYTES)
        .map_err(super::policy_error)?;
    let source_path = canonical.to_string_lossy();
    let r = if super::cpp_header_override(canonical, scope.language.as_deref()) {
        octocode_engine::portable::structural_search_detailed_with_extension(
            &source,
            &source_path,
            "cpp",
            q.pattern().as_deref(),
            q.rule().as_deref(),
        )
    } else {
        octocode_engine::portable::structural_search_detailed(
            &source,
            &source_path,
            q.pattern().as_deref(),
            q.rule().as_deref(),
        )
    }
    .map_err(super::native_error)?;
    if let Some(error) = diagnostic_error(&r.diagnostics) {
        return Err(error);
    }
    Ok(Scan {
        files: vec![(
            super::display_name(canonical),
            r.matches,
            r.diagnostics,
            r.status,
        )],
        sources: vec![canonical.clone()],
        ..Scan::default()
    })
}

fn scan_directory(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> Result<Scan, ToolError> {
    let extensions = scope.extensions.as_ref();
    let withheld = std::sync::Mutex::new(crate::policy::discovery::Withheld::default());
    let r = octocode_engine::portable::structural_search_files_detailed_filtered_with_extension(
        StructuralSearchFilesOptions {
            path: scope.canonical.to_string_lossy().into_owned(),
            pattern: q.pattern(),
            rule: q.rule(),
            include: q.include().or_else(|| {
                extensions
                    .map(|extensions| extensions.iter().map(|ext| format!("*.{ext}")).collect())
            }),
            exclude: q.exclude(),
            exclude_dir: Some(
                PruneMode::SyntaxVisible.directories_before_policy(q.default_excludes()),
            ),
            hidden: q.hidden(),
            no_ignore: q.no_ignore(),
            max_depth: q.max_depth(),
            max_files: Some(q.max_files()),
            skip_files: Some(q.scan_offset()),
            max_file_bytes: u32::try_from(super::MAX_PARSE_SOURCE_BYTES).ok(),
        },
        &|path| {
            let allowed = super::allow_discovery(path, paths, cancel)?;
            // Only a directory or a file of the scanned grammar could have
            // matched, so only those leave absence unproven.
            let is_dir = path.is_dir();
            if !allowed
                && (is_dir
                    || extensions.is_none_or(|extensions| has_extension_in(path, extensions)))
            {
                withheld
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .record(path, is_dir);
            }
            // Explicit include globs are intersected with language: files
            // outside the selected grammar are never candidates.
            Ok(allowed
                && (q.include().is_none()
                    || extensions.is_none_or(|extensions| {
                        !path.is_file() || has_extension_in(path, extensions)
                    })))
        },
        &|path| {
            if super::cpp_header_override(path, scope.language.as_deref()) {
                "cpp".to_owned()
            } else {
                octocode_engine::text::extension_of(&path.to_string_lossy(), true, "")
            }
        },
    )
    .map_err(super::native_error)?;
    if let Some(error) = diagnostic_error(&r.diagnostics) {
        return Err(error);
    }
    let sources = r
        .files
        .iter()
        .map(|f| std::path::PathBuf::from(&f.path))
        .collect();
    // Preserve the scan-level coverage signals the group projection drops.
    Ok(Scan {
        sources,
        files: r
            .files
            .into_iter()
            .map(|f| {
                (
                    security
                        .sanitize_text(&match_display_path(&scope.canonical, &f.path), None)
                        .content,
                    f.matches,
                    f.diagnostics,
                    f.status,
                )
            })
            .collect(),
        truncated: r.scan_truncated,
        diagnostics: r.diagnostics,
        skips: (r.skipped_unsupported, r.skipped_unreadable, r.skipped_large),
        skipped_by_prefilter: r.skipped_by_pre_filter,
        withheld: withheld
            .into_inner()
            .unwrap_or_else(|error| error.into_inner()),
    })
}

fn sort_files(q: MatchQuery<'_>, files: &mut [ScannedFile]) {
    files.sort_by(|a, b| {
        let ordering = if q.sort().as_deref() == Some("matchCount") {
            b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0))
        } else {
            a.0.cmp(&b.0)
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
fn interrupted_rescan(q: MatchQuery<'_>, scan: &Scan) -> Option<ToolError> {
    let interrupted = scan.files.iter().any(|(_, _, diagnostics, _)| {
        diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.code.as_str(),
                "structural.match.deadline" | "structural.parse.interrupted"
            )
        })
    });
    if !interrupted {
        return None;
    }
    let mut error = ToolError::new(
        "timeout",
        "The rescan for this page stopped at the structural match deadline, so it cannot be checked against the earlier pages; the source is unchanged. Retry the same page.",
    );
    error.hints = vec!["Retry next.retry; a narrower path or pattern parses less per page.".into()];
    error.next = Some(Box::new(json!({
        "retry": crate::tools::result::Continuation::new(ToolId::AstSearch, q.to_value())
            .confidence("exact")
            .build()
    })));
    Some(error)
}

/// The match-row shape a snapshot was cut under; bumped by every row rename.
const MATCH_ROWS_VERSION: u32 = 4;

/// Fingerprint over the query shape plus the ordered result set (each file
/// and its match count). Continuation cursors (page>1 or matchPage>1) that
/// carry a stale snapshot are rejected with `staleSnapshot`. Fields the
/// continuation normalizes to a default are digested by their effective
/// value so a fresh page matches.
fn match_snapshot(q: MatchQuery<'_>, scope: &Scope, files: &[ScannedFile]) -> String {
    let ordered = files
        .iter()
        .map(|f| (f.0.clone(), f.1.len()))
        .collect::<Vec<_>>();
    crate::digest::json_sha256(&json!([
        // Row shape version (D1 v4: `stats.matchCount`, object rows, 1-based columns); an older cursor
        // restarts instead of mixing shapes across pages.
        MATCH_ROWS_VERSION,
        scope.canonical.to_string_lossy(),
        q.pattern(),
        q.rule(),
        q.include(),
        q.exclude(),
        q.hidden(),
        q.no_ignore(),
        q.max_depth(),
        scope.language,
        q.reverse(),
        // Always present: validation stamps the contract defaults.
        q.sort().as_deref().unwrap_or_default(),
        q.result_view().as_deref().unwrap_or_default(),
        q.max_files(),
        q.scan_offset(),
        ordered
    ]))
}

/// The match page size, the requested match page and its first row.
fn match_window(q: MatchQuery<'_>) -> (usize, usize, usize) {
    let per_page = (q.match_page_size() as usize).clamp(1, limits::MATCH_PAGE_SIZE_MAXIMUM);
    let page = q.match_page().max(1) as usize;
    (per_page, page, (page - 1).saturating_mul(per_page))
}

fn group_files(q: MatchQuery<'_>, files: &[ScannedFile]) -> Grouped {
    let mut grouped = Grouped::default();
    let file_list = matches!(q.result_view().as_deref(), Some("files" | "countMatches"));
    let content_length = q
        .match_content_length()
        .map_or(limits::MATCH_CONTENT_LENGTH_DEFAULT, |length| {
            length as usize
        })
        .clamp(1, limits::MATCH_CONTENT_LENGTH_MAXIMUM);
    for (path, matches, diagnostics, status) in files {
        match status.as_str() {
            "ok" => {
                grouped.parsed_files += 1;
                grouped.first_parsed.get_or_insert_with(|| path.clone());
            }
            "skippedByPreFilter" => {}
            _ => {
                if let Some(diagnostic) = diagnostics
                    .iter()
                    .find(|diagnostic| diagnostic.code == "structural.query.compileFailed")
                {
                    grouped.compile_failed_files += 1;
                    grouped.compile_error.get_or_insert_with(|| {
                        ToolError::new(
                            crate::tools::ast_rule::INVALID_PATTERN,
                            crate::tools::ast_rule::untagged(&diagnostic.message),
                        )
                    });
                } else {
                    grouped.unevaluated_files += 1;
                }
            }
        }
        let rows = matches
            .iter()
            .map(|value| match_value(value, q.capture_text().unwrap_or(false), content_length))
            .collect::<Vec<_>>();
        if !rows.is_empty() {
            grouped.total_matches += rows.len() as u64;
            grouped.groups.push(if file_list {
                listed_file(q, path.clone(), rows.len())
            } else {
                match_group(q, path.clone(), rows)
            });
        }
        if status != "ok" || !diagnostics.is_empty() {
            // A literal prefilter can skip thousands of files. Preserve the
            // coverage fact once instead of one identical diagnostic for
            // every skipped path.
            grouped.diagnostics.extend(
                diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.code != "structural.prefilter.skipped")
                    .map(diag),
            );
        }
    }
    grouped
}

/// A `files`/`countMatches` view row: the file, with its count when asked.
fn listed_file(q: MatchQuery<'_>, path: String, total: usize) -> Group {
    let row = if q.result_view().as_deref() == Some("countMatches") {
        json!({"path":path,"matchCount":total})
    } else {
        json!({"path":path})
    };
    Group {
        row,
        more_matches: false,
        truncated_captures: false,
        cut_chars: None,
        clipped: vec![],
    }
}

/// One file's match page, with per-file pagination when it is a subset.
fn match_group(q: MatchQuery<'_>, path: String, rows: Vec<MatchRow>) -> Group {
    let (per_page, match_page, start) = match_window(q);
    let total = rows.len();
    let end = start.saturating_add(per_page).min(total);
    let page_rows = rows.get(start..end).unwrap_or(&[]);
    // Rows whose text was cut to a header or that hide capture text:
    // `captureText:true` (next.expandCaptures) returns them whole.
    let truncated_captures = page_rows.iter().any(|row| row.withheld);
    let cut_chars = page_rows.iter().filter_map(|row| row.cut).max();
    let clipped = page_rows
        .iter()
        .filter(|row| row.shown_cut)
        .map(|row| row.lines)
        .collect();
    let selected = page_rows
        .iter()
        .map(|row| row.value.clone())
        .collect::<Vec<_>>();
    let out_of_range = start >= total;
    let returned = selected.len();
    let mut row = json!({"path":path,"matches":selected});
    // Counts only add information when this match page is a subset.
    if returned != total || out_of_range {
        row["totalMatchRows"] = json!(total);
        row["returnedMatchRows"] = json!(returned);
    }
    if total > per_page || out_of_range {
        let facts = crate::response::pages::PageFacts::counted(match_page, per_page, total)
            .out_of_range(out_of_range);
        let more = match_page < total.div_ceil(per_page).max(1);
        row["pagination"] = facts.to_value();
        if more && match_page < limits::MATCH_PAGE_MAXIMUM {
            row["pagination"]["nextMatchPage"] = json!(match_page + 1);
        }
    }
    Group {
        row,
        more_matches: end < total,
        truncated_captures,
        cut_chars,
        clipped,
    }
}

/// Scan-level coverage diagnostics the per-file groups do not carry.
fn scan_diagnostics(q: MatchQuery<'_>, scope: &Scope, scan: &Scan) -> Vec<Value> {
    let mut diagnostics = vec![];
    if scan.skipped_by_prefilter > 0 {
        diagnostics.push(json!({
            "code":"structural.prefilter.skipped",
            "severity":"info",
            "stage":"scan",
            "message":format!("Literal-anchor prefilter skipped AST parsing for {} file(s) that cannot contain the anchor.", scan.skipped_by_prefilter),
            "recovery":"Use a YAML rule with no safe literal anchor only when every supported candidate must be parsed."
        }));
    }
    diagnostics.extend(scan.diagnostics.iter().map(diag));
    if scan.truncated {
        let limit = q.max_files();
        diagnostics.push(json!({
            "code":"structural.scan.truncated",
            "severity":"warning",
            "stage":"scan",
            "message":format!("Candidate scan hit the maxFiles limit ({limit}); files beyond it were not evaluated. Results are a bounded subset, not the full corpus."),
            "path":scope.canonical.to_string_lossy(),
            "recovery":"Follow next.expandScan, or narrow the scope with include or exclude globs."
        }));
    }
    diagnostics
}

fn render_match_page(
    q: MatchQuery<'_>,
    scope: &Scope,
    scan: &Scan,
    grouped: Grouped,
    snapshot: &str,
) -> Value {
    let (skips, truncated) = (scan.skips, scan.truncated);
    let mut diagnostics = grouped.diagnostics;
    diagnostics.extend(scan_diagnostics(q, scope, scan));
    let parsed = grouped.parsed_files;
    let groups = grouped.groups;
    let size = (q.page_size() as usize).clamp(1, limits::PAGE_SIZE_MAXIMUM);
    let page = q.page().max(1) as usize;
    let start = (page - 1) * size;
    let page_groups = groups
        .get(start..(start + size).min(groups.len()))
        .unwrap_or(&[]);
    let more = start + size < groups.len();
    let mut out = json!({"searchEngine":"structural","snapshot":snapshot,"stats":{"matchCount":grouped.total_matches}});
    if let Some(inferred) = &scope.inferred {
        out["inferredLanguage"] = json!(inferred);
    }
    if !groups.is_empty() {
        out["files"] = Value::Array(page_groups.iter().map(|group| group.row.clone()).collect());
        out["pagination"] =
            crate::response::pages::PageFacts::counted(page, size, groups.len()).to_value();
    }
    if !diagnostics.is_empty() {
        out["diagnostics"] = json!(diagnostics)
    }
    // Withheld entries are disclosed once, like localSearch's walk.
    let withheld = scan.withheld.notice();
    if let Some(notice) = &withheld {
        out["warnings"] = json!([notice]);
    }
    if groups.is_empty() && q.pattern().is_some() && diagnostics.is_empty() {
        out["diagnostics"] = json!([{
            "code":"structural.query.noMatches",
            "severity":"info",
            "stage":"match",
            "message":format!("0 structural matches for the requested pattern in {parsed} parsed file{}. Patterns must be complete parseable nodes with their relevant bodies (`$$$BODY`), return types, or decorators; trailing punctuation the pattern omits is not required. Confirm the node shape with operation:\"syntaxTree\" (next.viewTree); use an explicit YAML rule for partial or relational constraints.", if parsed == 1 { "" } else { "s" }),
            "path":scope.canonical.to_string_lossy()
        }]);
    }
    let skipped = skips.0 + skips.1 + skips.2 > 0;
    let incomplete =
        truncated || skipped || grouped.compile_failed_files > 0 || grouped.unevaluated_files > 0;
    out["truncated"] = json!(truncated);
    let partial_reasons = [
        (truncated, "maxFiles"),
        (skipped, "filesSkipped"),
        (grouped.compile_failed_files > 0, "compileFailed"),
        (grouped.unevaluated_files > 0, "filesUnevaluated"),
    ]
    .into_iter()
    .filter_map(|(cut, reason)| cut.then_some(reason))
    .collect::<Vec<_>>();
    if !partial_reasons.is_empty() {
        out["isPartial"] = json!(true);
        out["partialReasons"] = json!(partial_reasons);
    }
    let empty = groups.is_empty() && !more && !incomplete;
    if empty {
        out["status"] = json!("empty");
    }
    // Every file and match is listed and the scan saw its whole scope: the
    // row's expansions and reads are drill-downs, not unread rest.
    if !groups.is_empty()
        && !more
        && !incomplete
        && withheld.is_none()
        && !page_groups.iter().any(|group| group.more_matches)
    {
        out["complete"] = json!(true);
    }
    attach_match_continuations(q, scope, &mut out, page_groups, truncated, more, snapshot);
    // An empty match leads to the syntax tree of a parsed file, where the
    // pattern's node shape can be compared with the source.
    if empty
        && let Some(file) = grouped
            .first_parsed
            .and_then(|path| row_file(scope, &json!({"path": path})))
    {
        let mut tree = json!({"operation":"syntaxTree","path":file.to_string_lossy()});
        if scope.is_file
            && let Some(language) = q.language()
        {
            tree["language"] = json!(language);
        }
        out["next"]["viewTree"] = crate::tools::result::Continuation::new(ToolId::AstSearch, tree)
            .why("Compare the pattern with a parsed file's node shapes.")
            .confidence("medium")
            .build();
    }
    out
}

/// `next` pages, the scan and value expansions, and the read of the top
/// file's hits.
fn attach_match_continuations(
    q: MatchQuery<'_>,
    scope: &Scope,
    out: &mut Value,
    page_groups: &[Group],
    truncated: bool,
    more: bool,
    snapshot: &str,
) {
    let page = q.page().max(1) as usize;
    let (per_page, match_page, _) = match_window(q);
    let has_more_matches = page_groups.iter().any(|group| group.more_matches);
    let has_truncated_captures = page_groups.iter().any(|group| group.truncated_captures);
    let cut_chars = page_groups.iter().filter_map(|group| group.cut_chars).max();
    let max_content_length = limits::MATCH_CONTENT_LENGTH_MAXIMUM;
    let content_length = q
        .match_content_length()
        .map_or(limits::MATCH_CONTENT_LENGTH_DEFAULT, |length| {
            length as usize
        })
        .clamp(1, limits::MATCH_CONTENT_LENGTH_MAXIMUM);
    // A `maxFiles` cut below the schema maximum is raisable: expandScan
    // scans on with a doubled bound, so only the maximum is terminal. It
    // resumes after the files this window evaluated (`scanOffset`), so it is
    // offered once the window's last file page is reached.
    let max_files = q.max_files() as usize;
    let expand_scan = (truncated && max_files < limits::MAX_FILES_MAXIMUM)
        .then(|| max_files.saturating_mul(2).min(limits::MAX_FILES_MAXIMUM));
    if (truncated && !more && expand_scan.is_none())
        || (more && page >= limits::PAGE_MAXIMUM)
        || (has_more_matches && match_page >= limits::MATCH_PAGE_MAXIMUM)
    {
        out["terminalLimit"] = json!(true);
    }
    let pinned = scope
        .inferred
        .as_ref()
        .map_or_else(|| json!({}), |language| json!({"language":language}));
    let with = |changes: Value| {
        let mut merged = pinned.clone();
        if let (Some(target), Some(changes)) = (merged.as_object_mut(), changes.as_object()) {
            target.extend(changes.clone());
        }
        continuation_with(q, merged, snapshot)
    };
    if more && page < limits::PAGE_MAXIMUM {
        // A new file page restarts per-file match pagination.
        out["next"] = json!({"nextPage":with(json!({"page":page + 1,"matchPage":1}))})
    }
    if has_more_matches && match_page < limits::MATCH_PAGE_MAXIMUM {
        out["next"]["nextMatchPage"] =
            with(json!({"matchPageSize":per_page,"matchPage":match_page+1}));
    }
    if let Some(bound) = expand_scan.filter(|_| !more) {
        out["next"]["expandScan"] = with(json!({
            "maxFiles":bound,"scanOffset":max_files,"page":1,"matchPage":1,"snapshot":null
        }));
    }
    // A clipped value is display clipping of a listed match, not a coverage
    // gap: expandCaptures re-runs with captures and the whole length when
    // captures are hidden; otherwise (or past the length maximum) the
    // clipped rows' lines are read whole.
    let whole_length = cut_chars.map(|chars| chars.min(max_content_length));
    let expand_captures = has_truncated_captures && !q.capture_text().unwrap_or(false);
    if expand_captures {
        let mut expand = json!({"captureText":true});
        if let Some(length) = whole_length.filter(|length| *length > content_length) {
            expand["matchContentLength"] = json!(length);
        }
        out["next"]["expandCaptures"] = with(expand);
    }
    if !expand_captures || cut_chars.is_some_and(|chars| chars > max_content_length) {
        let reads = page_groups
            .iter()
            .flat_map(|group| read_clipped_values(scope, group))
            .enumerate();
        for (index, read) in reads {
            let name = match index {
                0 => "expandValues".to_owned(),
                n => format!("expandValues{}", n + 1),
            };
            out["next"][name] = read;
        }
    }
    if let Some(read) = page_groups
        .first()
        .and_then(|top| read_top_hits(scope, &top.row))
    {
        out["next"]["read"] = read;
    }
}

/// Lines of context around each hit in the `read` lead.
const READ_CONTEXT: u64 = 3;
/// Hit windows one `read` lead may name.
const READ_MAX_RANGES: usize = 5;
/// Longest match the top read shows whole.
const READ_MAX_SPAN: u64 = 400;

/// The file a match row names.
fn row_file(scope: &Scope, row: &Value) -> Option<std::path::PathBuf> {
    let shown = row["path"].as_str()?;
    Some(if scope.is_file {
        scope.canonical.clone()
    } else {
        scope
            .canonical
            .parent()
            .unwrap_or(&scope.canonical)
            .join(shown)
    })
}

/// Reads of one file's clipped rows, whole: their merged line spans, at most
/// [`MAX_READ_RANGES`](crate::tools::local_fetch::MAX_READ_RANGES) per read.
fn read_clipped_values(scope: &Scope, group: &Group) -> Vec<Value> {
    let Some(file) = row_file(scope, &group.row).filter(|_| !group.clipped.is_empty()) else {
        return vec![];
    };
    let spans = crate::tools::line_spans::merge_spans(group.clipped.iter().copied());
    spans
        .chunks(crate::tools::local_fetch::MAX_READ_RANGES)
        .map(|chunk| {
            let ranges = chunk
                .iter()
                .map(|(start, end)| format!("{start}-{end}"))
                .collect::<Vec<_>>();
            crate::tools::result::Continuation::new(
                ToolId::LocalFetch,
                json!({"path": file.to_string_lossy(), "ranges": ranges}),
            )
            .why("Read the clipped match values whole.")
            .confidence("high")
            .build()
        })
        .collect()
}

/// The natural next step after a match: read the top file's hit lines in
/// context. Offered when they fit [`READ_MAX_RANGES`] windows.
fn read_top_hits(scope: &Scope, row: &Value) -> Option<Value> {
    let file = row_file(scope, row)?;
    // A row shows a multi-line match only up to its header, so its window
    // spans the whole match; past READ_MAX_SPAN lines, its first line.
    let mut hits = vec![];
    for hit in row["matches"].as_array()? {
        let line = hit["line"].as_u64()?;
        let end = hit["endLine"]
            .as_u64()
            .filter(|end| *end >= line && end - line < READ_MAX_SPAN)
            .unwrap_or(line);
        hits.push((line, end));
    }
    let windows = crate::tools::line_spans::merge_spans(
        hits.into_iter()
            .map(|(line, end)| (line.saturating_sub(READ_CONTEXT).max(1), end + READ_CONTEXT)),
    );
    if windows.is_empty() || windows.len() > READ_MAX_RANGES {
        return None;
    }
    let ranges = windows
        .iter()
        .map(|(start, end)| format!("{start}-{end}"))
        .collect::<Vec<_>>();
    Some(
        crate::tools::result::Continuation::new(
            ToolId::LocalFetch,
            json!({"path": file.to_string_lossy(), "ranges": ranges}),
        )
        .why("Read the top file's hits in context.")
        .confidence("high")
        .build(),
    )
}

/// One match row, an object: `line`, `column` (1-based), `endLine` for a
/// multi-line match, and `value` (whitespace-normalized text). With
/// `captureText` the row also carries `endColumn` for a multi-line span, and each capture once in `metavarRanges` (text plus
/// 1-based position; the engine `metavars` map is a fallback for a capture
/// without a range).
struct MatchRow {
    value: Value,
    /// The lean row withheld something `captureText:true` returns: a
    /// multi-line match cut to its header (signature) followed by `…`, or
    /// capture text the value does not show.
    withheld: bool,
    /// The shown text was cut at `matchContentLength`.
    shown_cut: bool,
    /// The match's 1-based start and end lines.
    lines: (u64, u64),
    /// The whole value's characters when they exceed `matchContentLength`,
    /// so the shown or the `captureText` value is cut.
    cut: Option<usize>,
}

fn match_value(m: &StructuralDetailedMatch, capture_text: bool, content_length: usize) -> MatchRow {
    let lines = (u64::from(m.start_line), u64::from(m.end_line));
    let whole_chars = normalized_chars(&m.text);
    let cut = (whole_chars > content_length).then_some(whole_chars);
    let header = m.header.as_deref().filter(|_| !capture_text);
    let (text, shown_cut) = match header {
        Some(header) => {
            let (text, shown_cut) = compact_match(header, content_length.saturating_sub(2).max(1));
            (format!("{text} …"), shown_cut)
        }
        None => compact_match(&m.text, content_length),
    };
    if !capture_text {
        let hidden = |capture: &str| {
            let capture = capture.split_whitespace().collect::<Vec<_>>().join(" ");
            !capture.is_empty() && !text.contains(&capture)
        };
        let withheld = header.is_some()
            || m.metavar_ranges
                .values()
                .flatten()
                .any(|range| hidden(&range.text))
            || m.metavars.values().flatten().any(|value| hidden(value));
        // X1: a row is an object named like the inputs that take it; a
        // multi-line match states its last line.
        let mut value = json!({
            "line": m.start_line,
            "column": crate::tools::num::one_based_column(m.start_col),
        });
        if m.end_line != m.start_line {
            value["endLine"] = json!(m.end_line);
        }
        value["value"] = json!(text);
        return MatchRow {
            value,
            withheld,
            shown_cut,
            lines,
            cut,
        };
    }
    use crate::tools::num::one_based_column;
    let mut value =
        json!({"line":m.start_line,"value":text,"column":one_based_column(m.start_col)});
    if shown_cut {
        value["truncated"] = json!(true);
    }
    if m.end_line != m.start_line {
        value["endLine"] = json!(m.end_line);
        value["endColumn"] = json!(one_based_column(m.end_col));
    }
    let mut ranges = serde_json::Map::new();
    for (name, values) in &m.metavar_ranges {
        let rows = values
            .iter()
            .map(|range| {
                let mut row = json!({
                    "text":range.text,
                    "line":range.line,
                    "column":one_based_column(range.column),
                    "endColumn":one_based_column(range.end_column)
                });
                if range.end_line != range.line {
                    row["endLine"] = json!(range.end_line);
                }
                row
            })
            .collect::<Vec<_>>();
        if !rows.is_empty() {
            ranges.insert(name.clone(), Value::Array(rows));
        }
    }
    // Capture maps are unordered; emit names sorted so output is deterministic.
    let mut captures: Vec<_> = m.metavars.iter().collect();
    captures.sort_by(|a, b| a.0.cmp(b.0));
    let mut metavars = serde_json::Map::new();
    for (name, values) in captures {
        if !ranges.contains_key(name) && !values.is_empty() {
            metavars.insert(name.clone(), json!(values));
        }
    }
    if !metavars.is_empty() {
        value["metavars"] = Value::Object(metavars);
    }
    if !ranges.is_empty() {
        value["metavarRanges"] = Value::Object(ranges);
    }
    MatchRow {
        value,
        withheld: false,
        shown_cut,
        lines,
        cut,
    }
}

/// Whitespace-normalized match text bounded to `limit` characters
/// (`matchContentLength`), including the trailing ellipsis when cut, and
/// whether it was cut.
fn compact_match(text: &str, limit: usize) -> (String, bool) {
    let normalized = normalize_match(text);
    if normalized.chars().count() > limit {
        let mut output = normalized
            .chars()
            .take(limit.saturating_sub(1))
            .collect::<String>();
        output.push('…');
        (output, true)
    } else {
        (normalized, false)
    }
}

/// Match text on one line: whitespace runs become one space, except that a
/// line holding a line comment (`//`, `#`) keeps its line break, so the
/// comment cannot swallow the code after it (`{ // note\n return x }`).
fn normalize_match(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let mut words = line.split_whitespace().peekable();
        if words.peek().is_none() {
            continue;
        }
        if !output.is_empty() && !output.ends_with('\n') {
            output.push(' ');
        }
        for (index, word) in words.enumerate() {
            if index > 0 {
                output.push(' ');
            }
            output.push_str(word);
        }
        if lines.peek().is_some() && (line.contains("//") || line.contains('#')) {
            output.push('\n');
        }
    }
    output.truncate(output.trim_end().len());
    output
}

/// Characters of `text` once normalized, as [`compact_match`] measures it.
fn normalized_chars(text: &str) -> usize {
    normalize_match(text).chars().count()
}

fn match_display_path(root: &std::path::Path, path: &str) -> String {
    let candidate = std::path::Path::new(path);
    let relative = candidate.strip_prefix(root).unwrap_or(candidate);
    super::rooted_display(root, relative)
}

fn continuation_with(q: MatchQuery<'_>, changes: Value, snapshot: &str) -> Value {
    let mut query = q.to_value();
    query["snapshot"] = json!(snapshot);
    // A `null` change drops that field.
    if let (Some(target), Some(changes)) = (query.as_object_mut(), changes.as_object()) {
        target.extend(changes.clone());
        for (field, _) in changes.iter().filter(|(_, value)| value.is_null()) {
            target.remove(field);
        }
    }
    crate::tools::result::Continuation::new(ToolId::AstSearch, query)
        .confidence("exact")
        .build()
}

/// The single grammar for a directory `match` without language: present in
/// the scope and able to compile the query. Several candidates (or none
/// present) fail with `ast.language.required`, naming what was found.
fn infer_directory_language(
    q: MatchQuery<'_>,
    root: &std::path::Path,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<String, ToolError> {
    let prune = PruneMode::SyntaxVisible.directories(q.default_excludes());
    let present = present_grammars(
        root,
        &prune,
        q.hidden().unwrap_or(false),
        q.no_ignore().unwrap_or(false),
        q.max_files() as usize,
        &|path| super::allow_discovery(path, paths, cancel),
    )
    .map_err(super::native_error)?;
    let (pattern, rule) = (q.pattern(), q.rule());
    let choice = choose_grammar(present.into_iter(), |_, extensions| {
        compile_check(extensions, pattern.as_deref(), rule.as_deref())
    });
    match choice {
        GrammarChoice::One(language) => Ok(language),
        GrammarChoice::Invalid(error) => Err(ToolError::new(error.code, error.message)),
        GrammarChoice::Absent => Err(ToolError::new(
            "languageRequired",
            "No source file of a supported structural grammar was found under path; set language.",
        )),
        GrammarChoice::Several(several) => {
            let mut error = ToolError::new(
                "languageRequired",
                format!(
                    "Several grammars in this directory parse the query ({}); set language to one of them.",
                    several.join(", ")
                ),
            );
            error
                .hints
                .push("Or narrow path/include to files of one language.".to_owned());
            // One runnable lead per grammar (`next.withRust`, …).
            let base = q.to_value();
            let mut leads = serde_json::Map::new();
            for language in &several {
                let mut row = base.clone();
                row["language"] = json!(language);
                let mut name = String::from("with");
                let mut chars = language.chars();
                if let Some(first) = chars.next() {
                    name.extend(first.to_uppercase());
                    name.extend(chars);
                }
                leads.insert(
                    name,
                    crate::tools::result::Continuation::new(ToolId::AstSearch, row)
                        .why(format!("Match the {language} files."))
                        .confidence("exact")
                        .build(),
                );
            }
            error.next = Some(Box::new(Value::Object(leads)));
            Err(error)
        }
    }
}

fn diag(d: &StructuralDiagnostic) -> Value {
    json!({"code":d.code,"severity":d.severity,"stage":d.stage,"message":d.message,"path":d.path,"recovery":d.recovery})
}

fn diagnostic_error(diagnostics: &[StructuralDiagnostic]) -> Option<ToolError> {
    diagnostics
        .iter()
        .find(|diagnostic| diagnostic.severity == "error")
        .map(|diagnostic| {
            let code = match diagnostic.code.as_str() {
                "structural.query.compileFailed" | "structural.query.invalid" => "invalidPattern",
                other => other,
            };
            ToolError::new(code, crate::tools::ast_rule::untagged(&diagnostic.message))
        })
}

#[cfg(test)]
mod language_glob_tests {
    fn language_include_globs(language: &str) -> Option<Vec<String>> {
        super::language_extensions(language).map(|extensions| {
            extensions
                .into_iter()
                .map(|extension| format!("*.{extension}"))
                .collect()
        })
    }

    #[test]
    fn directory_language_globs_derive_from_the_canonical_grammar_registry() {
        let cuda_enabled = octocode_engine::portable::grammar_capabilities()
            .iter()
            .any(|capability| {
                capability.structural_search && capability.extensions.iter().any(|ext| ext == "cu")
            });
        assert_eq!(
            language_include_globs("cuda"),
            cuda_enabled.then(|| vec!["*.cu".to_owned(), "*.cuh".to_owned()])
        );
        assert_eq!(
            language_include_globs("assembly"),
            Some(vec![
                "*.asm".to_owned(),
                "*.assembly".to_owned(),
                "*.s".to_owned()
            ])
        );
        assert_eq!(
            language_include_globs("typescript"),
            Some(vec![
                "*.cts".to_owned(),
                "*.mts".to_owned(),
                "*.ts".to_owned(),
                "*.tsx".to_owned()
            ])
        );
        assert_eq!(language_include_globs("unsupported"), None);
    }
}

#[cfg(test)]
mod normalize_tests {
    use super::{compact_match, normalize_match, normalized_chars};

    #[test]
    fn a_line_comment_keeps_its_line_break_so_later_code_stays_visible() {
        let go = "if err != nil { // The only error can be a bad pattern.\n\treturn fmt.Errorf(\"x\")\n}";
        assert_eq!(
            normalize_match(go),
            "if err != nil { // The only error can be a bad pattern.\nreturn fmt.Errorf(\"x\") }"
        );
        let rust = "builder\n    .worker_threads(1) // no timer!\n    .build()\n    .unwrap()";
        assert_eq!(
            normalize_match(rust),
            "builder .worker_threads(1) // no timer!\n.build() .unwrap()"
        );
        // Without comments the match stays on one line.
        assert_eq!(normalize_match("a\n   .b()\n\n  .c()"), "a .b() .c()");
        assert_eq!(
            normalized_chars(rust),
            normalize_match(rust).chars().count()
        );
        assert_eq!(compact_match(rust, 12).0.chars().count(), 12);
    }
}

#[cfg(test)]
mod memo_tests {
    use super::*;
    use crate::tools::cancel::NeverCancel;
    use crate::tools::test_support::{Fixture, simple_policy};

    fn run(query: &Value, paths: &PathPolicy, security: &ContentSecurity) -> Value {
        let query: super::super::AstSearchQuery =
            serde_json::from_value(query.clone()).expect("typed astSearch row");
        super::super::execute_ast(&query, paths, security, &NeverCancel).expect("astSearch")
    }

    fn next_query(out: &Value, name: &str) -> Value {
        out["next"][name]["query"]["queries"][0].clone()
    }

    /// A continuation page served from the first page's stored scan is
    /// byte-identical to the page a fresh scan gives; an edit drops the
    /// stored scan and the page restarts as before.
    #[test]
    fn continuation_pages_reuse_the_settled_scan_with_identical_output() {
        let root = Fixture::new();
        for (name, calls) in [("a.ts", 3), ("b.ts", 1), ("c.ts", 2)] {
            let body = (0..calls)
                .map(|i| format!("console.log({i});\n"))
                .collect::<String>();
            std::fs::write(root.0.join(name), body).expect("file");
        }
        std::fs::write(root.0.join("d.ts"), "other();\n").expect("prefilter skip");
        let (paths, security) = simple_policy(&root.0);
        std::thread::sleep(
            octocode_engine::graph::SourceStamp::SETTLE + std::time::Duration::from_millis(200),
        );
        let first = run(
            &json!({"operation":"match","path":root.0,"pattern":"console.log($A)","language":"typescript","pageSize":1,"matchPageSize":2}),
            &paths,
            &security,
        );
        let snapshot = first["snapshot"].as_str().expect("snapshot").to_owned();
        assert!(
            SCANS.holds(&snapshot),
            "a multi-page scan is stored: {first}"
        );
        for name in ["nextPage", "nextMatchPage"] {
            let query = next_query(&first, name);
            assert!(query.is_object(), "{name}: {first}");
            let stored = run(&query, &paths, &security);
            SCANS.evict(&snapshot);
            let fresh = run(&query, &paths, &security);
            assert_eq!(
                serde_json::to_string(&stored).expect("json"),
                serde_json::to_string(&fresh).expect("json"),
                "{name}"
            );
            // The rescan stores the scan again for the pages after it.
            assert!(SCANS.holds(&snapshot));
        }
        // An edit drops the stored scan: the page restarts from page 1.
        std::fs::write(root.0.join("c.ts"), "console.log(9);\n").expect("edit");
        let page = run(&next_query(&first, "nextPage"), &paths, &security);
        assert_eq!(page["errorCode"], "staleSnapshot", "{page}");
        assert!(!SCANS.holds(&snapshot));
    }
}

#[cfg(test)]
mod interrupted_rescan_tests {
    use super::*;

    /// A rescan cut by the match deadline (zero matches, status truncated)
    /// is a retryable timeout for the same page, not a stale snapshot; a
    /// complete rescan that differs stays stale.
    #[test]
    fn a_deadline_cut_rescan_is_a_retryable_timeout() {
        let query: AstSearchQueryMatchPattern = serde_json::from_value(json!({
            "operation":"match","path":"/repo/a.min.js","pattern":"f($A)",
            "matchPage":9,"snapshot":"earlier"
        }))
        .expect("typed row");
        let q = MatchQuery::Pattern(&query);
        let cut = Scan {
            files: vec![(
                "a.min.js".to_owned(),
                vec![],
                vec![StructuralDiagnostic {
                    code: "structural.match.deadline".to_owned(),
                    severity: "warning".to_owned(),
                    stage: "match".to_owned(),
                    message: "Structural matching exceeded its execution deadline".to_owned(),
                    path: None,
                    recovery: None,
                }],
                "truncated".to_owned(),
            )],
            ..Scan::default()
        };
        let error = interrupted_rescan(q, &cut).expect("a deadline cut is a timeout");
        assert_eq!(error.code, "timeout");
        let retry = &error.next.expect("retry")["retry"]["query"]["queries"][0];
        assert_eq!(retry["matchPage"], 9);
        assert_eq!(retry["snapshot"], "earlier");
        let changed = Scan {
            files: vec![("a.min.js".to_owned(), vec![], vec![], "ok".to_owned())],
            ..Scan::default()
        };
        assert!(interrupted_rescan(q, &changed).is_none());
    }
}
