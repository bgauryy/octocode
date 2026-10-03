pub use crate::contracts::tool_types::{AstSearchQueryMatchPattern, AstSearchQueryMatchRule};
use crate::tools::id::ToolId;
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

fn u32_of(value: std::num::NonZeroU64) -> u32 {
    u32::try_from(value.get()).unwrap_or(u32::MAX)
}

/// Contract `keyword` (`"default"`/`"maximum"`) of a match-operation field;
/// an undeclared value leaves the bound open (validation enforces it).
fn match_schema(field: &str, keyword: &str) -> u32 {
    crate::contracts::query_schema_number(ToolId::AstSearch, Some("match"), field, keyword)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(u32::MAX)
}

fn non_empty(values: &[String]) -> Option<Vec<String>> {
    (!values.is_empty()).then(|| values.to_vec())
}

/// The typed rule serializes unset optional matchers as `null`; the engine
/// reads only the keys a caller set.
fn without_nulls(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            fields.retain(|_, field| !field.is_null());
            fields.values_mut().for_each(without_nulls);
        }
        Value::Array(items) => items.iter_mut().for_each(without_nulls),
        _ => {}
    }
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
                    without_nulls(&mut value);
                    value.to_string()
                }
                Err(_) => String::new(),
            }),
        }
    }
    pub fn include(self) -> Option<Vec<String>> {
        either_form!(self, include => non_empty(include))
    }
    pub fn exclude(self) -> Option<Vec<String>> {
        either_form!(self, exclude => non_empty(exclude))
    }
    pub fn exclude_dir(self) -> Option<Vec<String>> {
        either_form!(self, exclude_dir => non_empty(exclude_dir))
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
    pub fn max_depth(self) -> Option<u32> {
        either_form!(self, max_depth => max_depth.map(|depth| u32::try_from(depth.max(0)).unwrap_or(u32::MAX)))
    }
    pub fn max_files(self) -> u32 {
        either_form!(self, max_files => u32_of(*max_files))
    }
    pub fn max_matches_per_file(self) -> u32 {
        either_form!(self, max_matches_per_file => u32_of(*max_matches_per_file))
    }
    pub fn match_content_length(self) -> Option<u32> {
        either_form!(self, match_content_length => Some(u32_of(*match_content_length)))
    }
    pub fn lang_type(self) -> Option<String> {
        either_form!(self, lang_type => lang_type.as_ref().map(ToString::to_string))
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
            without_nulls(rule);
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
    execute_match_inner(q, paths, security, cancel).map_err(|mut error| {
        // A YAML rule that fails to compile needs a rule-shaped recovery, not
        // the pattern advice (terminators, bodies) the generic hint gives.
        if q.rule().is_some()
            && error.code == "structural.query.compileFailed"
            && error.hints.is_empty()
        {
            error.hints.push(RULE_COMPILE_HINT.to_owned());
        }
        error
    })
}

const RULE_COMPILE_HINT: &str = "Fix the YAML rule at the position the error names: `rule:` takes keys such as kind, pattern, regex, has, inside, follows, precedes, all, any, not, matches (list values are YAML sequences). Check a nested pattern alone as a pattern query first.";

/// Rust item keywords a visibility modifier (`pub`, `pub(crate)`) can precede.
const RUST_VISIBLE_ITEMS: &[&str] = &[
    "fn", "async", "unsafe", "const", "static", "struct", "enum", "union", "trait", "type", "mod",
    "extern", "use",
];

/// ast-grep matches a Rust item pattern exactly: the visibility modifier is a
/// named child, so `fn $N()` never matches `pub fn`. A complete result for such
/// a pattern still excludes every visible item; say so.
fn rust_visibility_note(pattern: &str, rust: bool) -> Option<Value> {
    let first = pattern.split_whitespace().next()?;
    (rust && RUST_VISIBLE_ITEMS.contains(&first)).then(|| {
        json!({
            "code":"structural.pattern.visibilityExact",
            "severity":"warning",
            "stage":"match",
            "message":format!("Pattern starts with `{first}` and has no visibility modifier; structural matching is exact, so items declared `pub`/`pub(crate)` are not matched."),
            "recovery":format!("Re-run with `pub {first} …` (or `pub($V) {first} …`), or use a YAML rule on the item kind (e.g. `kind: function_item`) with has/regex constraints to match every visibility.")
        })
    })
}

fn execute_match_inner(
    q: MatchQuery<'_>,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> super::AstResult {
    cancel.check().map_err(super::cancelled)?;
    let lang_extensions = q
        .lang_type()
        .as_deref()
        .map(|language| {
            language_extensions(language).ok_or_else(|| {
                super::AstError::new(
                    "ast.language.unsupported",
                    format!(
                        "langType \"{language}\" is not a supported structural grammar. Use a language name, id, or extension such as \"typescript\", \"rust\", or \"py\"."
                    ),
                )
            })
        })
        .transpose()?;
    let p = paths.validate(q.path()).map_err(super::AstError::from)?;
    let meta = std::fs::metadata(&p.canonical).map_err(super::io_error)?;
    // A directory without langType uses the one grammar that both occurs in
    // the scope and parses the query; continuations pin it as langType.
    let inferred = if meta.is_dir() && q.lang_type().is_none() {
        Some(infer_directory_language(q, &p.canonical, paths, cancel)?)
    } else {
        None
    };
    let language = q.lang_type().or_else(|| inferred.clone());
    let lang_extensions = match (&lang_extensions, &language) {
        (None, Some(language)) => language_extensions(language),
        (extensions, _) => extensions.clone(),
    };
    // Corpus-coverage signals from the directory scan. The directory branch
    // fills these; the single-file branch leaves them at their complete defaults.
    let mut scan_truncated = false;
    let mut scan_diagnostics: Vec<StructuralDiagnostic> = Vec::new();
    let mut scan_skips = (0_u32, 0_u32, 0_u32);
    let mut skipped_by_prefilter = 0_u32;
    if meta.is_dir()
        && let Some(extensions) = &lang_extensions
    {
        compile_check(extensions, q)?;
    }
    let mut files = if meta.is_file() {
        super::validate_file_language(&p.canonical, language.as_deref())?;
        let bytes = std::fs::read(&p.canonical).map_err(super::io_error)?;
        // Parse the file as the directory scan does: raw. Output strings
        // are redacted by the response stage.
        let source = security
            .decode_source_bytes(&bytes, super::MAX_PARSE_SOURCE_BYTES)
            .map_err(super::AstError::from)?;
        let source_path = p.canonical.to_string_lossy();
        let r = if super::cpp_header_override(&p.canonical, language.as_deref()) {
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
        vec![(
            p.canonical
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            r.matches,
            r.diagnostics,
            r.status,
        )]
    } else {
        let r =
            octocode_engine::portable::structural_search_files_detailed_filtered_with_extension(
                StructuralSearchFilesOptions {
                    path: p.canonical.to_string_lossy().into_owned(),
                    pattern: q.pattern(),
                    rule: q.rule(),
                    include: q.include().or_else(|| {
                        lang_extensions.as_ref().map(|extensions| {
                            extensions.iter().map(|ext| format!("*.{ext}")).collect()
                        })
                    }),
                    exclude: q.exclude(),
                    exclude_dir: Some(
                        PruneMode::SyntaxVisible.directories(
                            &q.exclude_dir().unwrap_or_default(),
                            q.default_excludes(),
                        ),
                    ),
                    hidden: q.hidden(),
                    no_ignore: q.no_ignore(),
                    max_depth: q.max_depth().map(|depth| depth.saturating_add(1)),
                    max_files: Some(q.max_files()),
                    max_file_bytes: u32::try_from(super::MAX_PARSE_SOURCE_BYTES).ok(),
                },
                &|path| {
                    // Explicit include globs are intersected with langType: files
                    // outside the selected grammar are never candidates.
                    Ok(super::allow_discovery(path, paths, cancel)?
                        && (q.include().is_none()
                            || lang_extensions.as_ref().is_none_or(|extensions| {
                                !path.is_file() || has_extension_in(path, extensions)
                            })))
                },
                &|path| {
                    if super::cpp_header_override(path, language.as_deref()) {
                        "cpp".to_owned()
                    } else {
                        path.extension()
                            .and_then(|extension| extension.to_str())
                            .unwrap_or_default()
                            .to_ascii_lowercase()
                    }
                },
            )
            .map_err(super::native_error)?;
        if let Some(error) = diagnostic_error(&r.diagnostics) {
            return Err(error);
        }
        // Preserve the scan-level coverage signals the group projection drops.
        scan_truncated = r.scan_truncated;
        scan_skips = (r.skipped_unsupported, r.skipped_unreadable, r.skipped_large);
        skipped_by_prefilter = r.skipped_by_pre_filter;
        scan_diagnostics = r.diagnostics;
        r.files
            .into_iter()
            .map(|f| {
                (
                    security
                        .sanitize_text(&match_display_path(&p.canonical, &f.path), None)
                        .content,
                    f.matches,
                    f.diagnostics,
                    f.status,
                )
            })
            .collect()
    };
    cancel.check().map_err(super::cancelled)?;
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
    // Snapshot fingerprint over the query shape plus the ordered result set
    // (each file and its match count). Continuation cursors (page>1 or
    // matchPage>1) that carry a stale snapshot are rejected with
    // `ast.snapshot.changed`. Fields the continuation normalizes to a default
    // are digested by their effective value so a fresh page matches.
    let ordered = files
        .iter()
        .map(|f| (f.0.clone(), f.1.len()))
        .collect::<Vec<_>>();
    let snapshot = super::syntax::digest(&json!([
        p.canonical.to_string_lossy(),
        q.pattern(),
        q.rule(),
        q.include(),
        q.exclude(),
        q.exclude_dir(),
        q.hidden(),
        q.no_ignore(),
        q.max_depth(),
        language,
        q.reverse(),
        // Always present: validation stamps the contract defaults.
        q.sort().as_deref().unwrap_or_default(),
        q.result_view().as_deref().unwrap_or_default(),
        q.max_files(),
        ordered
    ]));
    if (q.page() > 1 || q.match_page() > 1) && q.snapshot().as_deref() != Some(&snapshot) {
        return Ok(super::snapshot_changed(&snapshot));
    }
    let mut groups = vec![];
    let mut all_diagnostics = vec![];
    let mut total_matches = 0_u64;
    let file_list = matches!(q.result_view().as_deref(), Some("files" | "countMatches"));
    let matches_per_page = q
        .max_matches_per_file()
        .clamp(1, match_schema("maxMatchesPerFile", "maximum")) as usize;
    let match_page = q.match_page().max(1) as usize;
    let match_start = (match_page - 1).saturating_mul(matches_per_page);
    let content_length = q
        .match_content_length()
        .unwrap_or_else(|| match_schema("matchContentLength", "default"))
        .clamp(1, match_schema("matchContentLength", "maximum")) as usize;
    // Per-file coverage: files that parsed, files whose query failed to
    // compile, and files cut short (execution limit, unreadable, unsupported).
    let mut parsed_files = 0_u32;
    let mut compile_error: Option<super::AstError> = None;
    let mut compile_failed_files = 0_u32;
    let mut unevaluated_files = 0_u32;
    for (path, matches, diagnostics, status) in files {
        match status.as_str() {
            "ok" => parsed_files += 1,
            "skippedByPreFilter" => {}
            _ => {
                if let Some(diagnostic) = diagnostics
                    .iter()
                    .find(|diagnostic| diagnostic.code == "structural.query.compileFailed")
                {
                    compile_failed_files += 1;
                    compile_error.get_or_insert_with(|| {
                        super::AstError::new(diagnostic.code.clone(), diagnostic.message.clone())
                    });
                } else {
                    unevaluated_files += 1;
                }
            }
        }
        let matches = matches
            .into_iter()
            .map(|value| match_value(value, q.capture_text().unwrap_or(false), content_length))
            .collect::<Vec<_>>();
        // Rows whose text was cut to a header or that hide capture text:
        // `captureText:true` (next.expandCaptures) returns them whole.
        let header_rows = matches.iter().map(|row| row.withheld).collect::<Vec<_>>();
        let full_chars = matches.iter().map(|row| row.cut).collect::<Vec<_>>();
        let shown_cuts = matches.iter().map(|row| row.shown_cut).collect::<Vec<_>>();
        let matches = matches.into_iter().map(|row| row.value).collect::<Vec<_>>();
        if !matches.is_empty() {
            let total = matches.len();
            total_matches += total as u64;
            if file_list {
                if q.result_view().as_deref() == Some("countMatches") {
                    groups.push((
                        json!({"path":path,"totalOccurrences":total}),
                        false,
                        false,
                        None,
                        false,
                    ));
                } else {
                    groups.push((json!({"path":path}), false, false, None, false));
                }
                continue;
            }
            let match_end = match_start.saturating_add(matches_per_page).min(total);
            let selected = matches.get(match_start..match_end).unwrap_or(&[]).to_vec();
            let more_matches = match_end < total;
            let truncated_captures = header_rows
                .get(match_start..match_end)
                .is_some_and(|rows| rows.contains(&true));
            let cut_chars = full_chars
                .get(match_start..match_end)
                .and_then(|rows| rows.iter().flatten().max().copied());
            let shown_cut = shown_cuts
                .get(match_start..match_end)
                .is_some_and(|rows| rows.contains(&true));
            let out_of_range = match_start >= total;
            let returned = selected.len();
            let mut group = json!({"path":path,"matches":selected});
            // Counts only add information when this match page is a subset.
            if returned != total || out_of_range {
                group["totalMatchRows"] = json!(total);
                group["returnedMatchRows"] = json!(returned);
            }
            if total > matches_per_page || out_of_range {
                let total_pages = total.div_ceil(matches_per_page).max(1);
                let more = match_page < total_pages;
                group["pagination"] = json!({
                    "currentPage":match_page,
                    "totalPages":total_pages,
                    "matchesPerPage":matches_per_page,
                    "totalMatches":total,
                    "hasMore":more
                });
                if more && match_page < 1_000 {
                    group["pagination"]["nextMatchPage"] = json!(match_page + 1);
                }
                if out_of_range {
                    group["pagination"]["outOfRange"] = json!(true);
                }
            }
            groups.push((
                group,
                more_matches,
                truncated_captures,
                cut_chars,
                shown_cut,
            ));
        }
        if status != "ok" || !diagnostics.is_empty() {
            // A literal prefilter can skip thousands of files. Preserve the
            // coverage fact once below instead of returning one identical
            // diagnostic for every skipped path.
            all_diagnostics.extend(
                diagnostics
                    .into_iter()
                    .filter(|diagnostic| diagnostic.code != "structural.prefilter.skipped")
                    .map(diag),
            );
        }
    }
    if total_matches == 0
        && parsed_files == 0
        && compile_failed_files > 0
        && let Some(error) = compile_error
    {
        // The query compiled in no candidate file: an empty result would read
        // as proof of absence, so surface the compile failure instead.
        return Err(error);
    }
    if skipped_by_prefilter > 0 {
        all_diagnostics.push(json!({
            "code":"structural.prefilter.skipped",
            "severity":"info",
            "stage":"scan",
            "message":format!("Literal-anchor prefilter skipped AST parsing for {skipped_by_prefilter} file(s) that cannot contain the anchor."),
            "recovery":"Use a YAML rule with no safe literal anchor only when every supported candidate must be parsed."
        }));
    }
    // Surface scan-level coverage signals (skips, unsupported extensions) that
    // the group projection would otherwise discard.
    all_diagnostics.extend(scan_diagnostics.into_iter().map(diag));
    let (skipped_unsupported, skipped_unreadable, skipped_large) = scan_skips;
    if scan_truncated {
        let limit = q.max_files();
        all_diagnostics.push(json!({
            "code":"structural.scan.truncated",
            "severity":"warning",
            "stage":"scan",
            "message":format!("Candidate scan hit the maxFiles limit ({limit}); files beyond it were not evaluated. Results are a bounded subset, not the full corpus."),
            "path":super::display_name(&p.canonical),
            "recovery":"Follow next.expandScan, or narrow the scope with include globs or excludeDir."
        }));
    }
    let size = q.page_size().clamp(1, match_schema("pageSize", "maximum")) as usize;
    let page = q.page().max(1) as usize;
    let start = (page - 1) * size;
    let page_groups = groups
        .get(start..(start + size).min(groups.len()))
        .unwrap_or(&[]);
    // Match-level pagination is page-local: only files shown on this page can
    // continue with nextMatchPage.
    let has_more_matches = page_groups.iter().any(|group| group.1);
    let has_truncated_captures = page_groups.iter().any(|group| group.2);
    // The longest whole value a row on this page shortened: a continuation at
    // that `matchContentLength` returns every cut value whole.
    let cut_chars = page_groups.iter().filter_map(|group| group.3).max();
    let shown_cut = page_groups.iter().any(|group| group.4);
    let max_content_length = match_schema("matchContentLength", "maximum") as usize;
    if let Some(chars) = cut_chars.filter(|chars| *chars > max_content_length) {
        all_diagnostics.push(json!({
            "code":"structural.match.valueClipped",
            "severity":"warning",
            "stage":"match",
            "message":format!("A match value has {chars} characters, past the matchContentLength maximum ({max_content_length}); its row shows the first {max_content_length}."),
            "path":super::display_name(&p.canonical),
            "recovery":"Read the row's line range with localFetch for the whole text."
        }));
    }
    let selected = page_groups
        .iter()
        .map(|group| group.0.clone())
        .collect::<Vec<_>>();
    let more = start + size < groups.len();
    let mut out = json!({"searchEngine":"structural","snapshot":snapshot,"stats":{"totalStructuralMatches":total_matches}});
    if let Some(inferred) = &inferred {
        out["inferredLangType"] = json!(inferred);
    }
    let pinned = inferred
        .as_ref()
        .map_or_else(|| json!({}), |language| json!({"langType":language}));
    if !groups.is_empty() {
        out["files"] = json!(selected);
        out["pagination"] = json!({"currentPage":page,"totalPages":groups.len().div_ceil(size).max(1),"filesPerPage":size,"totalFiles":groups.len()});
        if q.result_view().as_deref() != Some("files") {
            out["pagination"]["totalMatches"] = json!(total_matches);
        }
        out["pagination"]["hasMore"] = json!(more);
    }
    if !all_diagnostics.is_empty() {
        out["diagnostics"] = json!(all_diagnostics)
    }
    if groups.is_empty() && q.pattern().is_some() && all_diagnostics.is_empty() {
        let path = p
            .canonical
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        out["diagnostics"] = json!([{
            "code":"structural.query.noMatches",
            "severity":"info",
            "stage":"match",
            "message":"0 structural matches for the requested pattern in this scope. Patterns must be complete parseable nodes with their relevant bodies (`$$$BODY`), return types, or decorators; trailing punctuation the pattern omits is not required. Confirm the node shape with operation:\"syntaxTree\"; use an explicit YAML rule for partial or relational constraints.",
            "path":path
        }]);
    }
    let rust = language
        .as_deref()
        .and_then(language_extensions)
        .map_or_else(
            || p.canonical.extension().is_some_and(|ext| ext == "rs"),
            |extensions| extensions.contains("rs"),
        );
    if let Some(note) = q
        .pattern()
        .and_then(|pattern| rust_visibility_note(&pattern, rust))
    {
        match out["diagnostics"].as_array_mut() {
            Some(diagnostics) => diagnostics.push(note),
            None => out["diagnostics"] = json!([note]),
        }
    }
    let incomplete = scan_truncated
        || skipped_unsupported > 0
        || skipped_unreadable > 0
        || skipped_large > 0
        || compile_failed_files > 0
        || unevaluated_files > 0;
    out["truncated"] = json!(scan_truncated);
    out["complete"] = json!(!more && !incomplete && !has_more_matches && !shown_cut);
    if groups.is_empty() && !more && !incomplete {
        out["status"] = json!("empty");
    }
    // A `maxFiles` cut below the schema maximum is raisable: expandScan
    // re-runs the scan with a doubled bound, so only the maximum is terminal.
    let max_files = match_schema("maxFiles", "maximum");
    let expand_scan = (scan_truncated && q.max_files() < max_files)
        .then(|| q.max_files().saturating_mul(2).min(max_files));
    if (scan_truncated && !more && expand_scan.is_none())
        || (more && page >= 1_000)
        || (has_more_matches && match_page >= 1_000)
    {
        out["terminalLimit"] = json!(true);
    }
    let with = |changes: Value| {
        let mut merged = pinned.clone();
        if let (Some(target), Some(changes)) = (merged.as_object_mut(), changes.as_object()) {
            target.extend(changes.clone());
        }
        continuation_with(q, merged, &snapshot)
    };
    if more && page < 1_000 {
        // A new file page restarts per-file match pagination.
        out["next"] = json!({"nextPage":with(json!({"page":page + 1,"matchPage":1}))})
    }
    if has_more_matches && match_page < 1_000 {
        out["next"]["nextMatchPage"] =
            with(json!({"maxMatchesPerFile":matches_per_page,"matchPage":match_page+1}));
    }
    if let Some(bound) = expand_scan {
        let mut expand = with(json!({"maxFiles":bound,"page":1,"matchPage":1}));
        if let Some(query) = expand["query"].as_object_mut() {
            query.remove("snapshot");
        }
        out["next"]["expandScan"] = expand;
    }
    let whole_length = cut_chars.map(|chars| chars.min(max_content_length));
    if has_truncated_captures && !q.capture_text().unwrap_or(false) {
        let mut expand = json!({"captureText":true});
        if let Some(length) = whole_length {
            expand["matchContentLength"] = json!(length);
        }
        out["next"]["expandCaptures"] = with(expand);
    } else if let Some(length) = whole_length.filter(|length| *length > content_length) {
        out["next"]["expandValues"] = with(json!({"matchContentLength":length}));
    }
    Ok(out)
}
/// One match row. By default a lean string `"<line>[-<endLine>]\t<value>"`
/// (1-based lines, whitespace-normalized text). With `captureText` an object:
/// `line`, `column` (0-based) and `value`, `endLine` and `endColumn` for a
/// multi-line span, and each capture once in `metavarRanges` (text plus
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
    /// The whole value's characters when they exceed `matchContentLength`,
    /// so the shown or the `captureText` value is cut.
    cut: Option<usize>,
}

fn match_value(m: StructuralDetailedMatch, capture_text: bool, content_length: usize) -> MatchRow {
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
        let lines = if m.end_line == m.start_line {
            m.start_line.to_string()
        } else {
            format!("{}-{}", m.start_line, m.end_line)
        };
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
        return MatchRow {
            value: json!(format!("{lines}\t{text}")),
            withheld,
            shown_cut,
            cut,
        };
    }
    let mut value = json!({"line":m.start_line,"value":text,"column":m.start_col});
    if m.end_line != m.start_line {
        value["endLine"] = json!(m.end_line);
        value["endColumn"] = json!(m.end_col);
    }
    let mut ranges = serde_json::Map::new();
    for (name, values) in m.metavar_ranges {
        let rows = values
            .into_iter()
            .map(|range| {
                let mut row = json!({
                    "text":range.text,
                    "line":range.line,
                    "column":range.column,
                    "endColumn":range.end_column
                });
                if range.end_line != range.line {
                    row["endLine"] = json!(range.end_line);
                }
                row
            })
            .collect::<Vec<_>>();
        if !rows.is_empty() {
            ranges.insert(name, Value::Array(rows));
        }
    }
    // Capture maps are unordered; emit names sorted so output is deterministic.
    let mut captures: Vec<_> = m.metavars.into_iter().collect();
    captures.sort_by(|a, b| a.0.cmp(&b.0));
    let mut metavars = serde_json::Map::new();
    for (name, values) in captures {
        if !ranges.contains_key(&name) && !values.is_empty() {
            metavars.insert(name, json!(values));
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
        cut,
    }
}

/// Whitespace-normalized match text bounded to `limit` characters
/// (`matchContentLength`), including the trailing ellipsis when cut, and
/// whether it was cut.
fn compact_match(text: &str, limit: usize) -> (String, bool) {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
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

/// Characters of `text` once whitespace-normalized, as [`compact_match`]
/// measures it.
fn normalized_chars(text: &str) -> usize {
    let mut words = 0_usize;
    let chars = text
        .split_whitespace()
        .inspect(|_| words += 1)
        .map(|word| word.chars().count())
        .sum::<usize>();
    chars + words.saturating_sub(1)
}

fn match_display_path(root: &std::path::Path, path: &str) -> String {
    let candidate = std::path::Path::new(path);
    let relative = candidate.strip_prefix(root).unwrap_or(candidate);
    super::rooted_display(root, relative)
}

fn continuation_with(q: MatchQuery<'_>, changes: Value, snapshot: &str) -> Value {
    let mut query = q.to_value();
    query["snapshot"] = json!(snapshot);
    if let (Some(target), Some(changes)) = (query.as_object_mut(), changes.as_object()) {
        target.extend(changes.clone());
    }
    json!({"tool":ToolId::AstSearch.as_str(),"query":query,"confidence":"exact"})
}

/// Grammars whose extensions occur under `root`, from a bounded walk that
/// honors the same discovery policy, ignore files and excluded directories
/// as the match scan. Each maps to its full extension set.
pub(crate) fn present_grammars(
    root: &std::path::Path,
    prune: &[String],
    hidden: bool,
    no_ignore: bool,
    max_files: usize,
    permits: &dyn Fn(&std::path::Path) -> Result<bool, String>,
) -> Result<std::collections::BTreeMap<String, std::collections::BTreeSet<String>>, String> {
    let capabilities = octocode_engine::portable::grammar_capabilities()
        .into_iter()
        .filter(|capability| capability.structural_search)
        .collect::<Vec<_>>();
    let mut seen = std::collections::BTreeSet::new();
    let mut files = 0_usize;
    let pruned = prune.to_vec();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(!hidden)
        .git_ignore(!no_ignore)
        .git_exclude(!no_ignore)
        .ignore(!no_ignore)
        .parents(!no_ignore)
        .filter_entry(move |entry| {
            entry.depth() == 0
                || !entry.file_type().is_some_and(|kind| kind.is_dir())
                || !pruned
                    .iter()
                    .any(|name| entry.file_name().to_string_lossy() == name.as_str())
        })
        .build();
    for entry in walker {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        if !permits(entry.path())? {
            continue;
        }
        files += 1;
        if files > max_files {
            break;
        }
        if let Some(extension) = entry.path().extension().and_then(|ext| ext.to_str()) {
            seen.insert(extension.to_ascii_lowercase());
        }
    }
    let mut grammars = std::collections::BTreeMap::new();
    for extension in seen {
        if let Some(capability) = capabilities
            .iter()
            .find(|capability| capability.extensions.contains(&extension))
            && let Some(extensions) = language_extensions(&capability.language)
        {
            grammars.insert(grammar_selector(capability), extensions);
        }
    }
    Ok(grammars)
}

/// The langType a caller would write for a grammar: its language id
/// (`rust`, `typescript`), else its lowercased name.
pub(crate) fn grammar_selector(capability: &octocode_engine::types::GrammarCapability) -> String {
    capability
        .language_id
        .clone()
        .unwrap_or_else(|| capability.language.to_ascii_lowercase())
}

/// The single grammar for a directory `match` without langType: present in
/// the scope and able to compile the query. Several candidates (or none
/// present) fail with `ast.language.required`, naming what was found.
fn infer_directory_language(
    q: MatchQuery<'_>,
    root: &std::path::Path,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<String, super::AstError> {
    let prune = PruneMode::SyntaxVisible
        .directories(&q.exclude_dir().unwrap_or_default(), q.default_excludes());
    let present = present_grammars(
        root,
        &prune,
        q.hidden().unwrap_or(false),
        q.no_ignore().unwrap_or(false),
        q.max_files() as usize,
        &|path| super::allow_discovery(path, paths, cancel),
    )
    .map_err(super::native_error)?;
    let mut first_error = None;
    let mut parsing = Vec::new();
    for (language, extensions) in &present {
        match compile_check(extensions, q) {
            Ok(()) => parsing.push(language.clone()),
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    match parsing.as_slice() {
        [language] => Ok(language.clone()),
        [] => Err(first_error.unwrap_or_else(|| {
            super::AstError::new(
                "ast.language.required",
                "No source file of a supported structural grammar was found under path; set langType.",
            )
        })),
        several => {
            let mut error = super::AstError::new(
                "ast.language.required",
                format!(
                    "Several grammars in this directory parse the query ({}); set langType to one of them.",
                    several.join(", ")
                ),
            );
            error.hints.push(
                "Or narrow path/include to files of one language.".to_owned(),
            );
            Err(error)
        }
    }
}

pub(super) fn language_extensions(language: &str) -> Option<std::collections::BTreeSet<String>> {
    let selector = language.trim().to_ascii_lowercase();
    let capabilities = octocode_engine::portable::grammar_capabilities();
    if let Some(extension) = selector.strip_prefix('.') {
        return capabilities
            .iter()
            .any(|capability| capability.extensions.iter().any(|item| *item == extension))
            .then(|| std::collections::BTreeSet::from([extension.to_owned()]));
    }
    let mut extensions: std::collections::BTreeSet<String> = capabilities
        .into_iter()
        .filter(|capability| {
            capability.language.eq_ignore_ascii_case(&selector)
                || capability
                    .language_id
                    .as_deref()
                    .is_some_and(|id| id.eq_ignore_ascii_case(&selector))
                || capability
                    .selector_aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(&selector))
                || capability
                    .extensions
                    .iter()
                    .any(|extension| extension.eq_ignore_ascii_case(&selector))
        })
        .flat_map(|capability| capability.extensions)
        .map(|extension| extension.to_ascii_lowercase())
        .collect();
    if !extensions.is_empty() && (selector == "cpp" || selector == "c++") {
        extensions.insert("h".to_owned());
    }
    (!extensions.is_empty()).then_some(extensions)
}

pub(super) fn has_extension_in(
    path: &std::path::Path,
    extensions: &std::collections::BTreeSet<String>,
) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| extensions.contains(&ext.to_ascii_lowercase()))
}

fn diag(d: StructuralDiagnostic) -> Value {
    json!({"code":d.code,"severity":d.severity,"stage":d.stage,"message":d.message,"path":d.path,"recovery":d.recovery})
}

/// Compile the query for the directory's grammar before any file is read.
/// The literal prefilter skips files without the pattern's anchor unparsed,
/// so an unparseable pattern (`foo(`) would otherwise report `complete` and
/// empty wherever the anchor is absent. The query fails only when it
/// compiles for none of the language's extensions (`.tsx` accepts JSX that
/// `.ts` rejects).
fn compile_check(
    extensions: &std::collections::BTreeSet<String>,
    q: MatchQuery<'_>,
) -> Result<(), super::AstError> {
    let mut first_error = None;
    for extension in extensions {
        let result = octocode_engine::portable::structural_search_detailed(
            "",
            &format!("pattern.{extension}"),
            q.pattern().as_deref(),
            q.rule().as_deref(),
        )
        .map_err(super::native_error)?;
        match result
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "structural.query.compileFailed")
        {
            Some(diagnostic) => {
                first_error.get_or_insert_with(|| {
                    super::AstError::new(diagnostic.code.clone(), diagnostic.message.clone())
                });
            }
            None => return Ok(()),
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn diagnostic_error(diagnostics: &[StructuralDiagnostic]) -> Option<super::AstError> {
    diagnostics
        .iter()
        .find(|diagnostic| diagnostic.severity == "error")
        .map(|diagnostic| super::AstError::new(diagnostic.code.clone(), diagnostic.message.clone()))
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
        let cuda_enabled = octocode_engine::portable::supported_structural_extensions()
            .iter()
            .any(|extension| extension == "cu");
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
