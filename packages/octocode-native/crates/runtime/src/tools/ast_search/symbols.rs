use crate::cache::{CacheClass, CacheConfig, CacheKey, Store};
use crate::id::ToolId;
use crate::num::u32_of;
use crate::policy::prune::DefaultsFlag;
use crate::response::result::Confidence;
use crate::response::result::ToolError;
use crate::response::symbol_outline::{declaration_rows, top_level_spans};
use crate::tools::empty_walk::{FileProbe, Globs, Walk, diagnose};
use crate::tools::leads::skipped;
use crate::{
    execution::cancel::CancellationCheck,
    policy::{path::PathPolicy, prune},
};
use ignore::gitignore::Gitignore;
pub use octocode_contract_types::AstSearchQuerySymbols;
use octocode_contract_types::ErrorCode;
use octocode_contract_types::SkippedPathCause;
use octocode_engine::graph::{GraphFactCode, GraphFactDiagnostic};
use octocode_engine::types::GraphFactsScanOptions;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Engine-unit views over the generated `symbols` query.
pub(crate) mod ast_search_query_symbols {
    use super::*;
    pub(crate) fn language(query: &AstSearchQuerySymbols) -> Option<String> {
        query.language.as_ref().map(ToString::to_string)
    }
    pub(crate) fn language_globs(
        query: &AstSearchQuerySymbols,
    ) -> Option<&BTreeMap<String, Vec<String>>> {
        (!query.language_globs.is_empty()).then_some(&query.language_globs)
    }
    pub(crate) fn kinds(query: &AstSearchQuerySymbols) -> Option<&Vec<String>> {
        (!query.kinds.is_empty()).then_some(&query.kinds)
    }
    pub(crate) fn exclude(query: &AstSearchQuerySymbols) -> Option<Vec<String>> {
        (!query.exclude.is_empty()).then(|| query.exclude.clone())
    }
    /// The caller's file scan cap; unset scans the scope within the scan
    /// time budget.
    pub(crate) fn max_files(query: &AstSearchQuerySymbols) -> Option<u32> {
        query.max_files.map(u32_of)
    }
    /// Leading files (walk order) an earlier window outlined.
    pub(crate) fn scan_offset(query: &AstSearchQuerySymbols) -> u32 {
        query.scan_offset.map_or(0, |offset| u32_of(offset.get()))
    }
    pub(crate) fn page(query: &AstSearchQuerySymbols) -> u32 {
        u32_of(query.page)
    }
    /// The caller's symbols per page, at most the contract's `pageSize`
    /// maximum (the type keeps it non-zero); unset fills the response window.
    pub(crate) fn page_size(query: &AstSearchQuerySymbols) -> Option<usize> {
        crate::response::paging::page_size(
            ToolId::AstSearch,
            Some("symbols"),
            query.page_size.map(|size| size.get()),
        )
    }
    pub(crate) fn snapshot(query: &AstSearchQuerySymbols) -> Option<&str> {
        query.snapshot.as_deref().map(String::as_str)
    }
    /// The `name` filter: a string is a one-entry list. Each entry that
    /// equals some declaration name matches only equal names; otherwise it
    /// matches as a substring. All-lowercase text ignores case.
    pub(super) fn name_filter(query: &AstSearchQuerySymbols) -> Option<NameFilter> {
        use octocode_contract_types::AstSearchQuerySymbolsSymbolName;
        let entries = match query.symbol_name.as_ref()? {
            AstSearchQuerySymbolsSymbolName::String(name) => vec![name.to_string()],
            AstSearchQuerySymbolsSymbolName::Array(names) => {
                names.iter().map(ToString::to_string).collect()
            }
        };
        Some(NameFilter {
            entries: entries.into_iter().map(NameEntry::new).collect(),
        })
    }
}

/// One `name` value; all-lowercase text compares without case.
struct NameEntry {
    text: String,
    fold: bool,
    /// The entry equals some declaration, so it matches only equal names.
    exact: bool,
}

impl NameEntry {
    fn new(text: String) -> Self {
        let fold = !text.chars().any(char::is_uppercase);
        Self {
            text,
            fold,
            exact: false,
        }
    }
    fn equals(&self, name: &str) -> bool {
        if self.fold {
            name.to_lowercase() == self.text
        } else {
            name == self.text
        }
    }
    /// Whether `name` only contains this exact entry: a declaration the
    /// exact match left out.
    fn hides(&self, name: &str) -> bool {
        self.exact
            && !self.equals(name)
            && if self.fold {
                name.to_lowercase().contains(&self.text)
            } else {
                name.contains(&self.text)
            }
    }
    fn matches(&self, name: &str) -> bool {
        if self.exact {
            self.equals(name)
        } else if self.fold {
            name.to_lowercase().contains(&self.text)
        } else {
            name.contains(&self.text)
        }
    }
}

struct NameFilter {
    entries: Vec<NameEntry>,
}

impl NameFilter {
    /// An entry that equals a declaration name keeps only equal names.
    fn settle<'a>(&mut self, names: impl Iterator<Item = &'a str> + Clone) {
        for entry in &mut self.entries {
            entry.exact = names.clone().any(|name| entry.equals(name));
        }
    }
    fn matches(&self, name: &str) -> bool {
        self.entries.iter().any(|entry| entry.matches(name))
    }
    fn equals(&self, name: &str) -> bool {
        self.entries.iter().any(|entry| entry.equals(name))
    }
    fn hides(&self, name: &str) -> bool {
        self.entries.iter().any(|entry| entry.hides(name))
    }
    /// Whether `source` spells some entry's text: a declaration whose name
    /// matches an entry spells it in the source.
    fn spelled_in(&self, source: &str) -> bool {
        let mut folded = None;
        self.entries.iter().any(|entry| {
            if entry.fold {
                folded
                    .get_or_insert_with(|| source.to_lowercase())
                    .contains(&entry.text)
            } else {
                source.contains(&entry.text)
            }
        })
    }
}

pub fn execute_symbols(
    q: &AstSearchQuerySymbols,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    response_window: Option<usize>,
) -> super::AstResult {
    cancel.check().map_err(ToolError::cancelled)?;
    if let Some(unknown) = q
        .kinds
        .iter()
        .find(|kind| !octocode_engine::signatures::DECLARATION_KINDS.contains(&kind.as_str()))
    {
        return Err(ToolError::new(
            ErrorCode::InvalidInput,
            format!(
                "Unknown declaration kind \"{unknown}\". Use one of: {}.",
                octocode_engine::signatures::DECLARATION_KINDS.join(", ")
            ),
        ));
    }
    let p = paths
        .validate(q.path.as_str())
        .map_err(super::policy_error)?;
    let meta =
        std::fs::metadata(&p.canonical).map_err(|error| super::io_error(q.path.as_str(), error))?;
    check_language_scope(q, &p.canonical, meta.is_file())?;
    if meta.is_file() {
        let outline = match outline_file(q, &p.canonical)? {
            Ok(outline) => outline,
            Err(terminal) => return Ok(terminal),
        };
        let set = select(q, &p.canonical, outline, cancel)?;
        return Ok(render_page(q, &set, response_window));
    }
    // Later pages of a directory outline reuse page 1's selection while no
    // scanned file changed (same paths, sizes and modification times). A
    // repeated page 1 reuses only a selection whose files were all settled
    // (last written over two seconds before) when it was stored and still
    // are, so a write in the clock tick of a scan cannot hide behind an
    // unchanged time.
    let mapped = mapped_files(q, &p.canonical)?;
    // One walk serves the memo fingerprint and, on a miss, the outline scan.
    let walk = walk_directory(q, &p.canonical, paths, cancel)?;
    let (memo_key, settled) = directory_memo_key(q, &p.canonical, &walk);
    let other_key = || with_settled(memo_key.clone(), !settled);
    let hit = if ast_search_query_symbols::page(q) > 1 {
        SYMBOL_PAGES
            .get(&memo_key)
            .or_else(|| SYMBOL_PAGES.get(&other_key()))
    } else if settled {
        SYMBOL_PAGES.get(&memo_key)
    } else {
        None
    };
    if let Some(hit) = hit {
        let mut page = render_page(q, &hit.value, response_window);
        note_skipped(q, &p.canonical, mapped.as_ref(), paths, cancel, &mut page);
        return Ok(page);
    }
    let outline = outline_walked(q, &p.canonical, walk, cancel)?;
    let set = select(q, &p.canonical, outline, cancel)?;
    let mut page = render_page(q, &set, response_window);
    note_skipped(q, &p.canonical, mapped.as_ref(), paths, cancel, &mut page);
    let bytes = octocode_engine::json_size::bytes(&set).unwrap_or(usize::MAX);
    SYMBOL_PAGES.put(
        memo_key,
        set,
        bytes,
        CacheClass::Volatile {
            ttl: std::time::Duration::from_secs(600),
        },
    );
    Ok(page)
}

/// An empty directory outline cannot prove absence in what its walk skipped:
/// the scan honors `.gitignore` and the default prune, and hides dot entries.
/// When files of an outlined language spell the searched name behind them,
/// the hint names why, and `includeSkipped` (as `match` and the text tools
/// offer it) is the same scope with the fewest flags that reach them
/// (`includeIgnored`, `includeHidden`, `defaultExcludes:false`), probed with
/// its own filters first, so it is never offered empty.
fn note_skipped(
    q: &AstSearchQuerySymbols,
    root: &std::path::Path,
    mapped: Option<&Gitignore>,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    out: &mut Value,
) {
    if out["status"] != "empty" {
        return;
    }
    let outlined = octocode_engine::signatures::graph_facts::graph_fact_extensions();
    let filter = ast_search_query_symbols::name_filter(q);
    let budget = ast_search_query_symbols::max_files(q)
        .map_or_else(octocode_engine::scan_budget::walk_ceiling, |files| {
            files as usize
        });
    let exclude = ast_search_query_symbols::exclude(q).unwrap_or_default();
    let discovery = paths.discovery_walk();
    let permits = |path: &std::path::Path| discovery.permits(path);
    let stop = || cancel.check().is_err();
    // A file counts when it can declare the name: an outlined language that
    // spells it, like the scan's own prefilter.
    let keep = |path: &std::path::Path| {
        outlinable(&outlined, mapped, path)
            && filter.as_ref().is_none_or(|filter| {
                super::read_parse_source(path)
                    .ok()
                    .flatten()
                    .is_some_and(|bytes| {
                        super::parse_text(path, &bytes).is_ok_and(|text| filter.spelled_in(&text))
                    })
            })
    };
    // What the outline scan itself would reach from `at` under `walk`: it
    // applies the ignore rules found from its own root (`at`), which a probe
    // rooted there does not read for files below the root.
    let probe_at = |at: &std::path::Path, exclude: &[String], walk: Walk| {
        let rules = (!walk.ignored && at.is_dir())
            .then(|| octocode_engine::search::walk::IgnoreRules::new(at));
        let permitted = |path: &std::path::Path| {
            permits(path)
                && rules
                    .as_ref()
                    .is_none_or(|rules| !rules.is_ignored(path, path.is_dir()))
        };
        FileProbe::new(at, budget, &permitted, &keep)
            .stopping(&stop)
            .within(Globs {
                include: &[],
                exclude,
            })
            .find(walk)
            .is_ok_and(|found| found.count > 0)
    };
    let walk = Walk {
        ignored: q.include_ignored.unwrap_or(false),
        hidden: q.include_hidden.unwrap_or(false),
        default_excludes: q.default_excludes.defaults(),
    };
    let scanned = FileProbe::new(root, budget, &permits, &keep)
        .stopping(&stop)
        .within(Globs {
            include: &[],
            exclude: &exclude,
        })
        .beyond(walk);
    let Ok(verdict) = diagnose(root, walk, |relaxed| scanned.find(relaxed)) else {
        return;
    };
    // A check cut before it found anything proved nothing: every cause the
    // walk skipped by is unproven.
    let skipping = walk.skipping();
    let named = skipping
        .iter()
        .map(|cause| cause.label())
        .collect::<Vec<_>>()
        .join(", ");
    let Some(disclosure) = skipped::disclose(
        &verdict,
        ["outlinable file", "outlinable files"],
        skipped::Unchecked {
            causes: &skipping,
            named: &named,
        },
    ) else {
        return;
    };
    // The same scope with exactly the flags the diagnosis found the files
    // behind, probed as the outline scan walks it, so it is never empty.
    if disclosure.unchecked || probe_at(root, &exclude, walk.lifting(&disclosure.causes)) {
        let lead = skipped::lead(ToolId::AstSearch, q, &disclosure.causes);
        skipped::attach(out, &disclosure, lead);
    } else {
        crate::tools::leads::push_hint(out, disclosure.text);
    }
    // The response keeps the first leads of `next`: the text fallback goes
    // after the leads that lift what the outline skipped.
    if let Some(next) = out["next"].as_object_mut()
        && let Some(fallback) = next.shift_remove("findText")
    {
        next.insert("findText".to_owned(), fallback);
    }
}

/// `language` names one file's grammar; `languageGlobs` maps a directory's.
fn check_language_scope(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
    is_file: bool,
) -> Result<(), ToolError> {
    if is_file && ast_search_query_symbols::language_globs(q).is_some() {
        return Err(ToolError::new(
            ErrorCode::LanguageDirectoryRequired,
            "languageGlobs is for directory symbols. Use language for a single file.",
        ));
    }
    if is_file {
        return super::validate_file_language(
            canonical,
            ast_search_query_symbols::language(q).as_deref(),
        );
    }
    if q.language.is_none() {
        return Ok(());
    }
    let mut error = ToolError::new(
        ErrorCode::LanguageFileRequired,
        "language on symbols requires a single source file.",
    );
    // Directory symbols pick each file's grammar from its extension, so
    // the same query without language is the exact repair.
    if let Ok(mut repaired) = serde_json::to_value(q) {
        if let Some(object) = repaired.as_object_mut() {
            object.remove("language");
        }
        error.next = Some(Box::new(json!({
            "repair": crate::response::result::Continuation::new(ToolId::AstSearch, repaired).confidence(Confidence::Exact).build()
        })));
    }
    Err(error)
}

/// What one parsed file contributes to a symbols query. A file contributing
/// nothing is not held.
pub(super) struct OutlineFile {
    /// The row path.
    pub(super) path: String,
    /// The declaration rows its `kinds` and `symbolName` filters can keep.
    pub(super) rows: Vec<Value>,
    /// Declarations of a kind `kinds` leaves out whose name equals a
    /// `symbolName` entry: they keep that entry exact, and the answer says
    /// when they are all the entry names.
    pub(super) off_kind: Vec<Value>,
    /// Its parse notes that bear on the answer.
    pub(super) notes: Vec<GraphFactDiagnostic>,
}

/// One file's reduced rows: `(rows, off-kind rows, notes)`.
type Reduced = (Vec<Value>, Vec<Value>, Vec<GraphFactDiagnostic>);

/// What one file contributes to a symbols query, reduced as soon as the file
/// is parsed so a scan holds the answer, not the repository.
///
/// The rows are the file's compact declarations that a `kinds` entry names
/// and whose name (or trait-impl type name) contains some `symbolName` text:
/// a superset of the final selection, since an entry that equals a name keeps
/// only equal names. The off-kind rows are the other declarations whose name
/// equals an entry. The notes are those the answer can show; a recovered
/// parse counts where the file lists a matching declaration (the answer
/// cites a partial inventory) or its syntax errors spell a queried name.
fn reduce_file<'a>(
    q: &AstSearchQuerySymbols,
    filter: Option<&NameFilter>,
    mut declarations: Vec<Value>,
    edges: impl Iterator<Item = (&'a str, &'a str, &'a str)>,
    notes: &[GraphFactDiagnostic],
    spans: &[[u32; 2]],
    source: impl FnOnce() -> Option<String>,
) -> Reduced {
    name_impl_traits(&mut declarations, edges);
    let mut rows = compact_declarations(&declarations);
    let mut off_kind = vec![];
    rows.retain_mut(|row| {
        if filter.is_some_and(|filter| !names_match(filter, row)) {
            return false;
        }
        let kind = row["kind"].as_str().unwrap_or("");
        if ast_search_query_symbols::kinds(q).is_none_or(|kinds| kinds.iter().any(|k| k == kind)) {
            return true;
        }
        if filter.is_some_and(|filter| names_equal(filter, row)) {
            off_kind.push(std::mem::take(row));
        }
        false
    });
    // A recovered parse hides a declaration of a queried name only where its
    // syntax errors spell that name; a file that lists none and spells none
    // there leaves the note irrelevant here.
    let unrelated = rows.is_empty()
        && filter.is_some_and(|filter| {
            !spans.is_empty()
                && source().is_some_and(|source| !filter.spelled_in(&error_text(&source, spans)))
        });
    // The syntax-only caveat is static and already in the tool description;
    // repeating it costs every call.
    let notes = notes
        .iter()
        .filter(|note| match note.code {
            GraphFactCode::SyntaxOnly => false,
            GraphFactCode::RecoveredParse => !unrelated,
            code => !code.is_linking_only(),
        })
        .cloned()
        .collect();
    (rows, off_kind, notes)
}

/// The declarations facts of `source` at `path`, parsed with `parser`'s
/// grammar (`None`: the file's own), through the one declarations cache
/// every reader shares: a file outlined alone and then in its directory is
/// parsed once.
fn declarations(
    source: &str,
    path: &str,
    parser: Option<&str>,
) -> Option<std::sync::Arc<octocode_engine::graph::GraphFactsDocument>> {
    crate::tools::declarations_cache::document(source, path, parser, || match parser {
        Some(extension) => {
            octocode_engine::signatures::extract_declarations_document_with_extension(
                source, path, extension,
            )
        }
        None => octocode_engine::signatures::extract_declarations_document(source, path),
    })
}

/// [`reduce_file`] over one file's declarations facts.
fn reduce_document(
    q: &AstSearchQuerySymbols,
    filter: Option<&NameFilter>,
    facts: &octocode_engine::graph::GraphFactsDocument,
    source: impl FnOnce() -> Option<String>,
) -> Reduced {
    let declarations = match serde_json::to_value(&facts.declarations) {
        Ok(Value::Array(rows)) => rows,
        _ => vec![],
    };
    reduce_file(
        q,
        filter,
        declarations,
        facts
            .edges
            .iter()
            .map(|edge| (edge.from.as_str(), edge.relation.as_str(), edge.to.as_str())),
        &facts.diagnostics,
        &facts.error_lines,
        source,
    )
}

/// Declaration rows per file, after the kind filter and a name prefilter.
pub(super) struct Outline {
    pub(super) files: Vec<OutlineFile>,
    /// Files the scan considered, including those the name prefilter skipped.
    pub(super) scanned: usize,
    truncated: bool,
    /// Where a scan cut by the time budget resumes (`scanOffset`).
    scan_resume: Option<u32>,
    skipped: u32,
    diagnostics: Vec<Value>,
    /// Candidate entries the security path policy withheld from the walk.
    withheld: crate::policy::discovery::Withheld,
    /// Root-relative directories the default excludes pruned unseen.
    pruned: Vec<String>,
    /// The scopes a walk cut at the ceiling never reached.
    unwalked: Option<super::unwalked::Unwalked>,
    /// Entries the walk could not read, and how many for want of
    /// permission: absence below them is unproven.
    walk_errors: (u32, u32),
}

/// One file's declarations; `Err` is a terminal row (size limit, no
/// extractor) rather than a tool error.
fn outline_file(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
) -> Result<Result<Outline, Value>, ToolError> {
    let Some(b) = super::read_parse_source(canonical)? else {
        let name = match q.symbol_name.as_ref() {
            Some(octocode_contract_types::AstSearchQuerySymbolsSymbolName::String(name)) => {
                Some(name.as_str())
            }
            _ => None,
        };
        return Ok(Err(super::source_limit(&canonical.to_string_lossy(), name)));
    };
    // Parse the file as the directory scan does: admitted by the one text
    // rule, raw. Output strings are redacted by the response stage.
    let source = super::parse_text(canonical, &b)?;
    let source_path = canonical.to_string_lossy();
    let parser =
        super::parser_override(canonical, ast_search_query_symbols::language(q).as_deref());
    let unsupported = || json!({"status":"error","errorCode":ErrorCode::SymbolsUnsupported,"path":canonical.to_string_lossy(),"error":"No native declaration extractor supports this source. Inspect its syntax tree or exact content."});
    let document = match parser.as_deref() {
        // An extension that selects no grammar outlines nothing: an empty
        // outline would read as a file without declarations. A document
        // format (Markdown, JSON, YAML, TOML) has no grammar to set: the
        // localFetch outline reads it.
        None if crate::tools::ast_rule::file_grammar(canonical).is_none() => {
            return Ok(Err(
                if crate::content::DocumentFormat::of(&source_path).is_some() {
                    no_extractor(canonical, &super::extension_label(canonical))
                } else {
                    unsupported()
                },
            ));
        }
        // The cache keys the parser extension used, so every override reuses
        // its own parse.
        parser => declarations(&source, &source_path, parser),
    };
    let path = super::display_name(canonical);
    let Some(document) = document else {
        return Ok(Err(unsupported()));
    };
    let mut diagnostics = vec![];
    if let Some(note) = super::cpp_header_note(
        canonical,
        ast_search_query_symbols::language(q).as_deref(),
        &source,
    ) {
        diagnostics
            .push(json!({"code":super::CPP_HEADER_NOTE,"severity":"warning","message":note}));
    }
    let (rows, off_kind, notes) = reduce_document(
        q,
        ast_search_query_symbols::name_filter(q).as_ref(),
        &document,
        || Some(source.to_string()),
    );
    let files = vec![OutlineFile {
        path,
        rows,
        off_kind,
        notes,
    }];
    let skipped = 0;
    Ok(Ok(Outline {
        files,
        scanned: 1,
        truncated: false,
        scan_resume: None,
        skipped,
        diagnostics,
        withheld: Default::default(),
        pruned: Vec::new(),
        unwalked: None,
        walk_errors: (0, 0),
    }))
}

/// The row of a document (`kind`: its extension) no declaration extractor
/// reads and no `language` makes source: it names the file type and leads to
/// the localFetch outline of the file (`minify:"symbols"`: its headings or
/// keys).
fn no_extractor(canonical: &std::path::Path, kind: &str) -> Value {
    let path = canonical.to_string_lossy();
    let mut row = json!({
        "status": "error",
        "errorCode": ErrorCode::SymbolsUnsupported,
        "path": path,
        "error": format!("No declaration extractor supports {kind} files."),
        "hints": [format!(
            "symbols does not outline {kind} files; the read lead outlines this one, or search it with localSearch."
        )],
    });
    row["next"]["read"] = crate::response::result::Continuation::new(
        ToolId::LocalFetch,
        json!({"path": path, "minify": "symbols"}),
    )
    .confidence(Confidence::Exact)
    .build();
    row
}

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
        max_file_bytes: u32::try_from(super::MAX_PARSE_SOURCE_BYTES).ok(),
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
pub(super) fn outline_directory(
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
struct DirectoryWalk {
    files: octocode_engine::types::FileSystemQueryResult,
    withheld: crate::policy::discovery::Withheld,
    /// What a walk cut at its file cap never reached.
    unwalked: Option<super::unwalked::Unwalked>,
    /// Entries the walk could not read, and how many for want of
    /// permission.
    walk_errors: (u32, u32),
}

/// Walk a directory outline's candidates once: the memo fingerprint reads
/// the walk, and a cold page scans it without walking again.
fn walk_directory(
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
                (q.include_hidden == Some(true) || !super::is_dot_entry(path))
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
) -> Option<super::unwalked::Unwalked> {
    let frontier = files.first_unlisted.as_deref().filter(|_| {
        files.was_capped && super::unwalked::at_ceiling(ast_search_query_symbols::max_files(q))
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
    super::unwalked::Unwalked::of(
        &super::unwalked::Rules {
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
fn outline_walked(
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
    let rooted = |relative: &str| super::rooted_display(canonical, std::path::Path::new(relative));
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

/// Page memo for directory outlines: one query shape over unchanged files.
static SYMBOL_PAGES: std::sync::LazyLock<Store<SymbolSet>> = std::sync::LazyLock::new(|| {
    Store::new(
        CacheConfig {
            max_entries: 8,
            max_bytes: 64 * 1024 * 1024,
            ..CacheConfig::default()
        },
        None,
    )
});

/// `key` in the partition of selections stored while their files were
/// `settled`.
fn with_settled(mut key: CacheKey, settled: bool) -> CacheKey {
    key.namespace = if settled {
        "ast-symbol-pages/settled"
    } else {
        "ast-symbol-pages"
    }
    .into();
    key
}

/// The memo key: every query field except the page cursor, plus the
/// fingerprint of the files the outline scans, partitioned by whether every
/// one of them was last written over two seconds ago (returned too).
fn directory_memo_key(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
    walk: &DirectoryWalk,
) -> (CacheKey, bool) {
    let shape = json!([
        canonical.to_string_lossy(),
        ast_search_query_symbols::language_globs(q),
        q.symbol_name,
        ast_search_query_symbols::kinds(q),
        ast_search_query_symbols::exclude(q),
        q.default_excludes.defaults(),
        q.include_ignored.unwrap_or(false),
        q.include_hidden.unwrap_or(false),
        ast_search_query_symbols::max_files(q),
        ast_search_query_symbols::scan_offset(q)
    ]);
    let (fingerprint, settled) = directory_fingerprint(walk);
    let key = CacheKey {
        namespace: "ast-symbol-pages".into(),
        scope: fingerprint,
        resource: crate::digest::json_sha256(&shape),
    };
    (with_settled(key, settled), settled)
}

/// Digest of the files a directory outline scans (its walk: an outlined
/// extension or a `languageGlobs` match, under the caller's path policy):
/// path, size and modification time; and whether every one was last
/// modified at least [`SourceStamp::SETTLE`] ago.
///
/// [`SourceStamp::SETTLE`]: octocode_engine::stamp::SourceStamp::SETTLE
fn directory_fingerprint(walk: &DirectoryWalk) -> (String, bool) {
    let now_ms = octocode_engine::stamp::settle_clock()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_millis() as f64);
    let settle_ms = octocode_engine::stamp::SourceStamp::SETTLE.as_millis() as f64;
    let mut stats = walk
        .files
        .entries
        .iter()
        .map(|entry| {
            (
                &entry.relative_path,
                entry.size,
                entry.modified_ms().map(f64::to_bits),
            )
        })
        .collect::<Vec<_>>();
    let settled = stats.iter().all(|(_, _, modified)| {
        modified.is_some_and(|bits| f64::from_bits(bits) + settle_ms <= now_ms)
    });
    stats.sort_unstable();
    (crate::digest::json_sha256(&json!(stats)), settled)
}

/// The files a directory query's `languageGlobs` map to a grammar, compiled
/// against the scanned root in the one local glob dialect the outline scan
/// uses; `None` without globs. A glob that does not compile is `invalidGlob`.
fn mapped_files(
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
    .map_err(super::native_error)
}

/// Whether the outline scan parses `path`: an outlined extension, or a
/// `languageGlobs` match.
fn outlinable(outlined: &[String], mapped: Option<&Gitignore>, path: &std::path::Path) -> bool {
    outlined.contains(&octocode_engine::text::extension_of(
        &path.to_string_lossy(),
        true,
        "",
    )) || mapped.is_some_and(|set| octocode_engine::search::glob::matches(set, path, false))
}

/// Applies the kind and name filters to an outline and pins the result.
fn select(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
    outline: Outline,
    cancel: &dyn CancellationCheck,
) -> Result<SymbolSet, ToolError> {
    cancel.check().map_err(ToolError::cancelled)?;
    let Outline {
        mut files,
        scanned,
        truncated,
        scan_resume,
        skipped,
        mut diagnostics,
        withheld,
        pruned,
        unwalked,
        walk_errors,
    } = outline;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    // A directory query writes each row's file; a single file hoists `path`.
    let per_row_path = !canonical.is_file();
    let candidates = kind_candidates(files);
    let filter = settled_filter(q, candidates.rows.iter().chain(&candidates.off_kind));
    let kept_rows: Vec<bool> = candidates
        .rows
        .iter()
        .map(|row| {
            filter
                .as_ref()
                .is_none_or(|filter| names_match(filter, row))
        })
        .collect();
    let suppressed = filter.as_ref().and_then(|filter| {
        Suppressed::of(
            filter,
            candidates
                .rows
                .iter()
                .zip(&kept_rows)
                .filter(|(row, kept)| !**kept && filter.hides(row["name"].as_str().unwrap_or("")))
                .map(|(row, _)| row),
        )
    });
    let kinds_left_out = filter.as_ref().and_then(|filter| {
        KindsLeftOut::of(
            filter,
            candidates
                .rows
                .iter()
                .zip(&kept_rows)
                .filter_map(|(row, kept)| kept.then_some(row)),
            &candidates.off_kind,
        )
    });
    place_notes(per_row_path, &candidates.notes, &mut diagnostics);
    let recovered = candidates.recovered;
    let mut declarations = candidates
        .rows
        .into_iter()
        .zip(kept_rows)
        .filter_map(|(mut row, kept)| {
            if !per_row_path && let Some(fields) = row.as_object_mut() {
                fields.remove("path");
            }
            kept.then_some(row)
        })
        .collect::<Vec<_>>();
    let references = match (&filter, declarations.as_slice()) {
        (Some(_), [row]) => references_lead(canonical, row),
        _ => None,
    };
    for row in &mut declarations {
        if let Some(fields) = row.as_object_mut() {
            fields.remove("typeName");
        }
    }
    // A file outline's natural next step reads its top declaration; a
    // directory outline (or a references lead) names its own route.
    let read = (!per_row_path && references.is_none())
        .then(|| {
            declarations
                .first()
                .and_then(|row| read_lead(canonical, row))
        })
        .flatten();
    let snapshot = crate::digest::json_sha256(&json!([
        // Row shape version (every declaration an object, pages of
        // top-level rows, 1-based `column`).
        "rows-v5",
        canonical.to_string_lossy(),
        q.language,
        ast_search_query_symbols::language_globs(q),
        q.symbol_name,
        ast_search_query_symbols::kinds(q),
        ast_search_query_symbols::exclude(q),
        ast_search_query_symbols::max_files(q),
        ast_search_query_symbols::scan_offset(q),
        &declarations,
        &diagnostics,
        truncated,
        skipped
    ]));
    // A name search that may be missing declarations (a recovered parse,
    // skipped files) completes with a text search for the same name.
    // An empty name search leads to the name's text as well.
    let text_search = (recovered || skipped > 0 || truncated || declarations.is_empty())
        .then(|| text_search_lead(q, canonical))
        .flatten();
    Ok(SymbolSet {
        references,
        read,
        text_search,
        path: super::display_name(canonical),
        snapshot,
        declarations,
        diagnostics,
        files_scanned: scanned,
        skipped,
        truncated,
        scan_resume,
        recovered,
        // Every path its own entry: pages show it with or without details.
        skipped_paths: skipped::skipped_paths(
            skipped::under(canonical, SkippedPathCause::DefaultExcluded, &pruned).chain(
                skipped::unlifted(&canonical.to_string_lossy(), &withheld, walk_errors.into()),
            ),
            true,
            truncated,
        )?,
        suppressed,
        kinds_left_out,
        directory: per_row_path,
        unwalked,
        walk_errors,
    })
}

/// The declaration rows of every file, each with its file, plus each file's
/// notes.
struct Candidates {
    rows: Vec<Value>,
    /// Rows of other kinds whose name equals a `symbolName` entry.
    off_kind: Vec<Value>,
    /// Per file: (path, its notes).
    notes: Vec<(String, Vec<GraphFactDiagnostic>)>,
    /// Some kept note says a parser recovered from syntax errors.
    recovered: bool,
}

fn kind_candidates(files: Vec<OutlineFile>) -> Candidates {
    let mut candidates = Candidates {
        rows: vec![],
        off_kind: vec![],
        notes: vec![],
        recovered: false,
    };
    for file in files {
        candidates.recovered |= file
            .notes
            .iter()
            .any(|note| note.code == GraphFactCode::RecoveredParse);
        for (rows, into) in [
            (file.rows, &mut candidates.rows),
            (file.off_kind, &mut candidates.off_kind),
        ] {
            into.extend(rows.into_iter().map(|mut row| {
                row["path"] = json!(file.path);
                row
            }));
        }
        candidates.notes.push((file.path, file.notes));
    }
    candidates
}

/// The `symbolName` filter, each entry settled against the candidates of
/// every kind: an entry equal to some name (or a trait impl's `typeName`)
/// keeps only equal names, even when `kinds` leaves out every declaration
/// of that name, so a kind filter never turns a name into a substring.
fn settled_filter<'a>(
    q: &AstSearchQuerySymbols,
    rows: impl Iterator<Item = &'a Value>,
) -> Option<NameFilter> {
    let mut filter = ast_search_query_symbols::name_filter(q)?;
    let names = rows.flat_map(row_names).collect::<Vec<_>>();
    filter.settle(names.iter().copied());
    Some(filter)
}

/// The declarations an exact `symbolName` entry left out because they only
/// contain it (`prune` equals `Prune`, so `prunes` is not listed): their
/// count and distinct names, so a prefix or substring search is not
/// answered by the one exact name alone. The notice names every one; the
/// `broadenSearch` lead runs them.
#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct Suppressed {
    /// The exact entries that suppressed them.
    exact: Vec<String>,
    /// Declarations left out.
    count: usize,
    /// Their distinct names, sorted: `broadenSearch` lists them.
    names: Vec<String>,
}

impl Suppressed {
    fn of<'a>(filter: &NameFilter, rows: impl Iterator<Item = &'a Value>) -> Option<Self> {
        let mut count = 0;
        let names = rows
            .inspect(|_| count += 1)
            .filter_map(|row| row["name"].as_str().map(str::to_owned))
            .collect::<std::collections::BTreeSet<_>>();
        (count > 0).then(|| Self {
            exact: filter
                .entries
                .iter()
                .filter(|entry| entry.exact)
                .map(|entry| entry.text.clone())
                .collect(),
            count,
            names: names.into_iter().collect(),
        })
    }

    /// Every suppressed name, so none is lost: passing them lists them.
    fn notice(&self) -> String {
        let one = self.count == 1;
        format!(
            "symbolName {} equals a declaration, so {} declaration{} that only contain{} it {} not listed: {}. Pass {} to list {}.",
            self.exact.join(", "),
            self.count,
            if one { "" } else { "s" },
            if one { "s" } else { "" },
            if one { "is" } else { "are" },
            self.names.join(", "),
            if self.names.len() == 1 {
                "this name"
            } else {
                "these names"
            },
            if one { "it" } else { "them" },
        )
    }

    /// The same query over the entries plus every suppressed name, each an
    /// exact entry now: the substring family the exact entries cut, one
    /// lead per schema item bound of names.
    fn leads(&self, q: &AstSearchQuerySymbols) -> Vec<Value> {
        let entries = ast_search_query_symbols::name_filter(q)
            .map(|filter| filter.entries.into_iter().map(|entry| entry.text))
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let names = entries
            .iter()
            .cloned()
            .chain(
                self.names
                    .iter()
                    .filter(|name| !entries.contains(name))
                    .cloned(),
            )
            .collect::<Vec<_>>();
        names
            .chunks(crate::contracts::query_limits::ast_search::symbols::SYMBOL_NAME_MAX_ITEMS)
            .map(|chunk| {
                let mut nq = crate::response::result::query_row(q, &["snapshot", "page"]);
                nq["symbolName"] = json!(chunk);
                crate::response::result::Continuation::new(ToolId::AstSearch, nq)
                    .confidence(Confidence::Exact)
                    .build()
            })
            .collect()
    }
}

/// The exact `symbolName` entries that `kinds` leaves without a single
/// declaration although other kinds declare them: the kinds that do and how
/// many declarations each holds, so an empty or substring-free answer is not
/// read as absence. The `broadenSearch` lead adds those kinds.
#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct KindsLeftOut {
    /// The entries, in query order.
    names: Vec<String>,
    /// Declarations of those names `kinds` left out.
    count: usize,
    /// Their distinct kinds, sorted.
    kinds: Vec<String>,
}

impl KindsLeftOut {
    fn of<'a>(
        filter: &NameFilter,
        kept: impl Iterator<Item = &'a Value> + Clone,
        off_kind: &[Value],
    ) -> Option<Self> {
        let declares =
            |entry: &NameEntry, row: &Value| row_names(row).any(|name| entry.equals(name));
        let names = filter
            .entries
            .iter()
            .filter(|entry| entry.exact && !kept.clone().any(|row| declares(entry, row)))
            .collect::<Vec<_>>();
        let left_out = off_kind
            .iter()
            .filter(|row| names.iter().any(|entry| declares(entry, row)))
            .collect::<Vec<_>>();
        (!left_out.is_empty()).then(|| Self {
            names: names
                .iter()
                .filter(|entry| left_out.iter().any(|row| declares(entry, row)))
                .map(|entry| entry.text.clone())
                .collect(),
            count: left_out.len(),
            kinds: left_out
                .iter()
                .filter_map(|row| row["kind"].as_str().map(str::to_owned))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
        })
    }

    fn notice(&self, q: &AstSearchQuerySymbols) -> String {
        let one = self.count == 1;
        let kinds = self.kinds.join(", ");
        format!(
            "kinds {} leaves out every declaration named {}: {} of kind{} {kinds}. Add {kinds} to kinds to list {}.",
            q.kinds.join(", "),
            self.names.join(", "),
            self.count,
            if self.kinds.len() == 1 { "" } else { "s" },
            if one { "it" } else { "them" },
        )
    }

    /// The same query with the left-out kinds added.
    fn lead(&self, q: &AstSearchQuerySymbols) -> Value {
        let mut nq = crate::response::result::query_row(q, &["snapshot", "page"]);
        let mut kinds = q.kinds.clone();
        kinds.extend(
            self.kinds
                .iter()
                .filter(|kind| !q.kinds.contains(kind))
                .cloned(),
        );
        nq["kinds"] = json!(kinds);
        crate::response::result::Continuation::new(ToolId::AstSearch, nq)
            .confidence(Confidence::Exact)
            .build()
    }
}

/// A row's names a `symbolName` entry can select: its name, and a trait
/// impl's type name.
fn row_names(row: &Value) -> impl Iterator<Item = &str> {
    [row["name"].as_str(), row["typeName"].as_str()]
        .into_iter()
        .flatten()
}

/// Whether some entry equals the row's name (or trait-impl type name).
fn names_equal(filter: &NameFilter, row: &Value) -> bool {
    row_names(row).any(|name| filter.equals(name))
}

/// A trait impl is shown as `Trait for Type`; its type name (`typeName`)
/// still selects it, as it selects the type's inherent impl.
fn names_match(filter: &NameFilter, row: &Value) -> bool {
    row_names(row).any(|name| filter.matches(name))
}

/// One entry per distinct note: a note one file raised names that file; a
/// note several files raised is counted once and lists every file (the
/// repeated message is the only thing removed).
fn place_notes(
    per_row_path: bool,
    file_notes: &[(String, Vec<GraphFactDiagnostic>)],
    diagnostics: &mut Vec<Value>,
) {
    let notes = file_notes.iter().flat_map(|(path, notes)| {
        notes.iter().map(move |note| {
            let message = note.to_string();
            if per_row_path {
                json!({"path": path, "message": message})
            } else {
                json!({"message": message})
            }
        })
    });
    if per_row_path {
        diagnostics.extend(crate::tools::ast_rule::group_diagnostics(notes.collect()));
    } else {
        diagnostics.extend(notes);
    }
}

/// One declaration a name filter singles out: its callers (a callable) or
/// references are the usual next question, and only a language server
/// answers it; none is offered when no server runs for the file.
fn references_lead(canonical: &std::path::Path, row: &Value) -> Option<Value> {
    let file = row["path"].as_str().map_or_else(
        || canonical.to_path_buf(),
        |shown| canonical.parent().unwrap_or(canonical).join(shown),
    );
    // A trait impl (`Trait for Type`) anchors on its type, like an inherent impl.
    let query = crate::tools::leads::lsp::verify_query(
        &file.to_string_lossy(),
        row["typeName"].as_str().or_else(|| row["name"].as_str())?,
        row["line"].as_u64()?,
        crate::tools::leads::lsp::Verify::for_kind(row["kind"].as_str().unwrap_or_default()),
    )?;
    Some(
        crate::response::result::Continuation::new(ToolId::LspSearch, query)
            .confidence(Confidence::High)
            .build(),
    )
}

/// A text search for the `symbolName` values under the outlined path: the
/// completion when a recovered parse or a skipped file may hide one.
fn text_search_lead(q: &AstSearchQuerySymbols, canonical: &std::path::Path) -> Option<Value> {
    let filter = ast_search_query_symbols::name_filter(q)?;
    let names: Vec<&str> = filter
        .entries
        .iter()
        .map(|entry| entry.text.as_str())
        .collect();
    let mut query = json!({"path": canonical.to_string_lossy()});
    if let [name] = names.as_slice() {
        query["matchString"] = json!(name);
        query["regex"] = json!("literal");
    } else {
        let alternation: Vec<String> = names.iter().map(|name| regex::escape(name)).collect();
        query["matchString"] = json!(alternation.join("|"));
        query["regex"] = json!("rust");
    }
    if filter.entries.iter().all(|entry| entry.fold) {
        query["caseMode"] = json!("insensitive");
    }
    Some(
        crate::response::result::Continuation::new(ToolId::LocalSearch, query)
            .confidence(Confidence::Medium)
            .build(),
    )
}

/// The read of one declaration's lines.
fn read_lead(canonical: &std::path::Path, row: &Value) -> Option<Value> {
    let start = row["line"].as_u64()?;
    let end = row["endLine"].as_u64().unwrap_or(start);
    Some(
        crate::response::result::Continuation::new(
            ToolId::LocalFetch,
            json!({"path": canonical.to_string_lossy(), "ranges": [format!("{start}-{end}")]}),
        )
        .confidence(Confidence::High)
        .build(),
    )
}

/// Every declaration of one symbols query, before paging.
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct SymbolSet {
    /// `next.findCallers` or `next.findReferences` for a single named declaration.
    references: Option<Value>,
    /// `next.read` of a file outline's top declaration.
    #[serde(default)]
    read: Option<Value>,
    /// `next.findText` for the name when the outline may be incomplete.
    #[serde(default)]
    text_search: Option<Value>,
    path: String,
    snapshot: String,
    declarations: Vec<Value>,
    diagnostics: Vec<Value>,
    files_scanned: usize,
    skipped: u32,
    truncated: bool,
    /// Where a scan cut by the time budget resumes (`scanOffset`); `None`
    /// when the cut is an explicit `maxFiles`.
    #[serde(default)]
    scan_resume: Option<u32>,
    /// A parser recovered from syntax errors: the declarations are a partial
    /// view of the source, not a complete inventory.
    recovered: bool,
    /// What the walk skipped, every path its own entry (paged with or
    /// without `extras:["details"]`).
    #[serde(default)]
    skipped_paths: Option<octocode_contract_types::SkippedPaths>,
    /// What an exact `symbolName` match left out.
    #[serde(default)]
    suppressed: Option<Suppressed>,
    /// Exact `symbolName` entries `kinds` left without a declaration.
    #[serde(default)]
    kinds_left_out: Option<KindsLeftOut>,
    /// A directory outline: rows group under their files, and the outlined
    /// directory (the query's own path) is not restated.
    #[serde(default)]
    directory: bool,
    /// The scopes a walk cut at the ceiling never reached.
    #[serde(default)]
    unwalked: Option<super::unwalked::Unwalked>,
    /// Entries the walk could not read, and how many for want of
    /// permission: absence below them is unproven.
    #[serde(default)]
    walk_errors: (u32, u32),
}

pub(super) fn render_page(
    q: &AstSearchQuerySymbols,
    set: &SymbolSet,
    response_window: Option<usize>,
) -> Value {
    // A page without a snapshot pins nothing: it pages the current outline,
    // as `match` and structureSearch do; only a snapshot that no longer
    // matches is stale.
    if ast_search_query_symbols::page(q) > 1
        && ast_search_query_symbols::snapshot(q).is_some_and(|given| given != set.snapshot)
    {
        return crate::response::result::stale_snapshot(crate::response::result::restart_query(
            ToolId::AstSearch,
            &crate::response::result::query_row(q, &[]),
        ));
    }
    let (skipped, truncated) = (set.skipped, set.truncated);
    let page = ast_search_query_symbols::page(q).max(1) as usize;
    let cut = SymbolPage::of(q, set, response_window);
    let unreadable = set.walk_errors.0 > 0;
    let incomplete = truncated || skipped > 0 || unreadable;
    let rows = &set.declarations[cut.range.clone()];
    // An explicit `maxFiles` cut below the schema maximum is raisable:
    // expandScan outlines on with a doubled bound. A cut by the scan time
    // budget resumes where it stopped. Either resumes after the files this
    // window outlined (`scanOffset`), so it is offered once the window's
    // last page is reached.
    let expand_scan = super::scan_window::expand_scan(
        truncated,
        ast_search_query_symbols::max_files(q),
        set.scan_resume,
        super::scan_window::ScanLimits::SYMBOLS,
    );
    let mut out = json!({"operation":"symbols","filesScanned":set.files_scanned,"filesSkipped":skipped,"diagnostics":outline_diagnostics(q, set),"isPartial":cut.more||incomplete||set.recovered});
    // Each coverage cut names itself, so a partial final page (no `next`)
    // still says why it is partial.
    for (cut, reason) in [
        (
            truncated,
            super::scan_window::cut_reason(ast_search_query_symbols::max_files(q)),
        ),
        (skipped > 0 || unreadable, "filesSkipped"),
        (set.recovered, "parseRecovered"),
    ] {
        if cut {
            crate::response::paging::mark_partial(&mut out, reason, &[]);
        }
    }
    // What the walk skipped is disclosed like localSearch does, once, on
    // page 1; an empty outline leads over it in `note_skipped`. What the
    // exact entries left out is a fact of the whole result, so page 1
    // states it once with its lead.
    if page == 1
        && let Some(shown) = set.skipped_paths.as_ref().and_then(|paths| {
            skipped::grouped(
                paths.entries.iter().cloned(),
                q.extras
                    .contains(&octocode_contract_types::ExtraKind::Details),
                paths.sampled,
            )
        })
    {
        out["skippedPaths"] = serde_json::to_value(shown).unwrap_or_default();
    }
    crate::response::paging::push_warnings(
        &mut out,
        set.kinds_left_out
            .as_ref()
            .filter(|_| page == 1)
            .map(|left_out| left_out.notice(q))
            .into_iter()
            .chain(
                set.suppressed
                    .as_ref()
                    .filter(|_| page == 1)
                    .map(Suppressed::notice),
            )
            .chain(crate::policy::include::parent_glob_warning(
                &[],
                &ast_search_query_symbols::exclude(q).unwrap_or_default(),
            )),
    );
    // A directory outline groups rows under their file, like `match` results,
    // so each path is written once instead of on every declaration; a file
    // outline names its file once. Rows are outline rows (see
    // `response::symbol_outline`).
    if set.directory || rows.iter().any(|row| row.get("path").is_some()) {
        out["files"] = Value::Array(group_by_file(rows));
    } else {
        out["path"] = json!(set.path);
        out["symbols"] = Value::Array(declaration_rows(rows));
    }
    // The snapshot only pins later pages to the same source; a single page
    // has nothing to pin.
    if cut.more || page > 1 {
        out["snapshot"] = json!(set.snapshot);
    }
    out["pagination"] = cut.pagination;
    if skipped > 0 || unreadable {
        crate::response::paging::terminal_limit(&mut out, "filesSkipped", &[]);
    }
    if truncated && expand_scan.is_none() {
        crate::response::paging::terminal_limit(
            &mut out,
            super::scan_window::cut_reason(ast_search_query_symbols::max_files(q)),
            &[],
        );
        if let Some(unwalked) = &set.unwalked {
            let query = crate::response::result::query_row(q, &[]);
            unwalked.attach(&mut out, |reach| super::unwalked::narrowed(&query, reach));
        }
    }
    attach_symbol_leads(q, set, &mut out, cut.more, expand_scan);
    if set.declarations.is_empty() && !incomplete {
        out["status"] = json!("empty");
        out["hints"] = json!([empty_hint(q, set.files_scanned)]);
    }
    out
}

/// The top-level rows of one symbols page.
struct SymbolPage {
    /// The page's rows (with their members) in the declarations.
    range: std::ops::Range<usize>,
    /// Top-level rows are left after the page.
    more: bool,
    /// Its pagination block.
    pagination: Value,
}

impl SymbolPage {
    /// Page `q.page()` of `set`: top-level rows cut by `pageSize`, else by
    /// the response window; a page past the end is empty.
    fn of(q: &AstSearchQuerySymbols, set: &SymbolSet, response_window: Option<usize>) -> Self {
        let declarations = &set.declarations;
        let page = ast_search_query_symbols::page(q).max(1) as usize;
        let budget = crate::response::page_windows::page_budget(
            response_window,
            &crate::response::result::query_value(q),
        );
        let paging = crate::response::page_windows::Paging::of(
            ast_search_query_symbols::page_size(q),
            budget,
        );
        // Pages count top-level rows; each carries its members.
        let spans = top_level_spans(declarations);
        let sizes = row_sizes(declarations, &spans, set.directory);
        let cut = crate::response::page_windows::select(paging, &sizes, page);
        let range = if cut.range.is_empty() {
            declarations.len()..declarations.len()
        } else {
            spans[cut.range.start].start..spans[cut.range.end - 1].end
        };
        Self {
            range,
            more: cut.more,
            pagination: cut.facts.to_value(),
        }
    }
}

/// The outline's diagnostics plus what its scan cut: a truncated candidate
/// scan, and files no page outlines.
fn outline_diagnostics(q: &AstSearchQuerySymbols, set: &SymbolSet) -> Vec<Value> {
    let mut diagnostics = set.diagnostics.clone();
    if set.truncated {
        let message = match ast_search_query_symbols::max_files(q) {
            Some(limit) => format!(
                "Candidate scan hit the maxFiles limit ({limit}); files beyond it were not outlined."
            ),
            None => super::scan_window::ceiling_note(set.scan_resume).map_or_else(
                || {
                    "Candidate scan stopped at its time budget; files beyond it were not outlined."
                        .to_owned()
                },
                |note| format!("Candidate scan stopped; files beyond it were not outlined. {note}"),
            ),
        };
        diagnostics.push(json!({"code":"structural.scan.truncated","message":message}));
    }
    if set.skipped > 0 {
        diagnostics.push(json!({
            "code":"symbols.filesSkipped",
            "message":format!("terminalLimit: {} files were not outlined (parse byte limit, encoding or read failure); no page reaches their declarations.", set.skipped),
        }));
    }
    diagnostics
}

/// `next`: the next page, the scan resume on the window's last page, and
/// the outline's leads (references, text search and read on page 1).
fn attach_symbol_leads(
    q: &AstSearchQuerySymbols,
    set: &SymbolSet,
    out: &mut Value,
    more: bool,
    expand_scan: Option<super::scan_window::ScanWindow>,
) {
    let page = ast_search_query_symbols::page(q).max(1) as usize;
    if more
        && let Some(lead) = crate::response::paging::next_page_call(
            out,
            ToolId::AstSearch,
            Some("symbols"),
            crate::response::result::query_row(q, &[]),
            "page",
            page + 1,
            &[("snapshot", json!(set.snapshot))],
        )
    {
        out["next"] = json!({ "nextPage": lead });
    }
    if let Some(window) = expand_scan.filter(|_| !more) {
        let mut nq = crate::response::result::query_row(q, &["snapshot"]);
        if let Some(bound) = window.max_files {
            nq["maxFiles"] = json!(bound);
        }
        nq["scanOffset"] = json!(window.scan_offset);
        nq["page"] = json!(1);
        out["next"]["expandScan"] =
            crate::response::result::Continuation::new(ToolId::AstSearch, nq)
                .confidence(Confidence::Exact)
                .build();
    }
    if let Some(lead) = &set.references {
        let name = crate::tools::leads::lsp::lead_name(&lead["query"]["queries"][0]);
        out["next"][name] = lead.clone();
    }
    // The kinds that declare an exact name lead first; the names an exact
    // entry cut follow, a list past the schema's item bound in
    // `broadenSearch2`, ….
    if page == 1 {
        let broaden = set
            .kinds_left_out
            .iter()
            .map(|left_out| left_out.lead(q))
            .chain(
                set.suppressed
                    .iter()
                    .flat_map(|suppressed| suppressed.leads(q)),
            );
        for (index, lead) in broaden.enumerate() {
            out["next"][crate::id::numbered("broadenSearch", index)] = lead;
        }
    }
    if let Some(lead) = set.text_search.as_ref().filter(|_| page == 1) {
        out["next"]["findText"] = lead.clone();
    }
    if let Some(lead) = set.read.as_ref().filter(|_| page == 1) {
        out["next"]["read"] = lead.clone();
    }
}

/// What an empty outline searched: the name (when asked) and the file count.
fn empty_hint(q: &AstSearchQuerySymbols, files_scanned: usize) -> String {
    let named = ast_search_query_symbols::name_filter(q)
        .map(|filter| {
            let names = filter
                .entries
                .iter()
                .map(|entry| entry.text.as_str())
                .collect::<Vec<_>>();
            format!("named {} ", names.join(", "))
        })
        .unwrap_or_default();
    let files = if files_scanned == 1 { "file" } else { "files" };
    format!("0 declarations {named}in {files_scanned} {files}.")
}

/// Characters each top-level row (a span of `declarations`) adds to its
/// page, as the page delivers it: its rendered row with its members, without
/// the `path` a directory page states once per file ([`group_by_file`]), plus
/// that file's `{path, symbols}` wrapper the first time the file appears.
fn row_sizes(
    declarations: &[Value],
    spans: &[std::ops::Range<usize>],
    directory: bool,
) -> Vec<usize> {
    let mut last_path: Option<&str> = None;
    spans
        .iter()
        .map(|span| {
            let rows = &declarations[span.clone()];
            let rendered = if directory {
                let unpathed = rows
                    .iter()
                    .map(|row| {
                        let mut row = row.clone();
                        if let Some(fields) = row.as_object_mut() {
                            fields.remove("path");
                        }
                        row
                    })
                    .collect::<Vec<_>>();
                declaration_rows(&unpathed)
            } else {
                declaration_rows(rows)
            };
            // Each row is one array element: its text and a separator.
            let mut chars = rendered
                .iter()
                .map(|row| octocode_engine::json_size::utf16_units(row).unwrap_or(0) + 1)
                .sum::<usize>();
            if directory {
                let path = declarations[span.start]["path"].as_str();
                if path != last_path {
                    chars += path.map_or(0, octocode_engine::json_size::string_body_units)
                        + FILE_WRAPPER_CHARS;
                    last_path = path;
                }
            }
            chars
        })
        .collect()
}

/// The `{"path":,"symbols":[]},` around one file's rows on a directory page.
const FILE_WRAPPER_CHARS: usize = 24;

/// Consecutive rows of one file become `{path, declarations}` with outline
/// rows; rows arrive in path order, so each file appears once per page.
fn group_by_file(rows: &[Value]) -> Vec<Value> {
    let mut files: Vec<(Value, Vec<Value>)> = Vec::new();
    for row in rows {
        let mut row = row.clone();
        let path = row
            .as_object_mut()
            .and_then(|fields| fields.remove("path"))
            .unwrap_or(Value::Null);
        match files.last_mut() {
            Some((seen, list)) if *seen == path => list.push(row),
            _ => files.push((path, vec![row])),
        }
    }
    files
        .into_iter()
        .map(|(path, rows)| json!({"path":path,"symbols":declaration_rows(&rows)}))
        .collect()
}

/// The source lines of `spans` (1-based, inclusive), joined.
fn error_text(source: &str, spans: &[[u32; 2]]) -> String {
    let lines = source.lines().collect::<Vec<_>>();
    spans
        .iter()
        .filter_map(|[start, end]| {
            let start = (*start as usize).max(1) - 1;
            let end = (*end as usize).min(lines.len());
            lines.get(start..end)
        })
        .flatten()
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
}

/// A declaration's shown name: a Rust trait impl reads `Trait for Type` (as
/// rust-analyzer and the source spell it), so the type's impl blocks are
/// told apart; every other declaration keeps its name.
fn display_name(d: &Value) -> std::borrow::Cow<'_, str> {
    let name = d["name"].as_str().unwrap_or("");
    match d["implements"].as_str() {
        Some(implemented) => format!("{implemented} for {name}").into(),
        None => name.into(),
    }
}

/// Mark each Rust `impl Trait for Type` declaration with the trait it
/// implements (`implements`, from the extractor's syntactic heritage edge),
/// for [`display_name`].
fn name_impl_traits<'a>(
    declarations: &mut [Value],
    edges: impl Iterator<Item = (&'a str, &'a str, &'a str)>,
) {
    let traits: std::collections::HashMap<&str, &str> = edges
        .filter(|(_, relation, _)| *relation == "implements")
        .map(|(from, _, to)| (from, to))
        .collect();
    if traits.is_empty() {
        return;
    }
    for declaration in declarations {
        if declaration["kind"] == "impl"
            && let Some(implemented) = declaration["id"].as_str().and_then(|id| traits.get(id))
        {
            declaration["implements"] = json!(implemented);
        }
    }
}

/// Projects engine declaration facts (one file) onto compact response rows.
///
/// `line` (1-based) anchors the declaration NAME, so a row feeds lspSearch as
/// `symbolName`+`line`; `character` (1-based) appears only when another
/// declaration of the same kind shares that name and line, so the column is
/// the only difference. `endLine` (1-based) ends the declaration and is omitted when
/// equal to `line`; `startLine` appears only when the declaration starts
/// before its name line (decorators, attributes); `docStartLine` is the first
/// line of the comment block directly above, when present. `parent` names the
/// containing declaration and stays meaningful when a `kinds` or `name`
/// filter drops the parent row; `parentLine` is added only when another
/// declaration in the file has the same name and kind (two `impl A` blocks). `exported` appears only when true, with
/// `exportedAs` listing public names that differ from `name`. Rows are
/// returned in input order, one per engine declaration.
fn compact_declarations(raw: &[Value]) -> Vec<Value> {
    let names: Vec<std::borrow::Cow<'_, str>> = raw.iter().map(display_name).collect();

    let pos = |d: &Value, range: &str, edge: &str, field: &str| {
        d.pointer(&format!("/{range}/{edge}/{field}"))
            .and_then(Value::as_u64)
    };
    let anchors: Vec<(&str, u64, u64)> = raw
        .iter()
        .enumerate()
        .map(|(index, d)| {
            let anchor = if d.get("selectionRange").is_some() {
                "selectionRange"
            } else {
                "range"
            };
            let line = pos(d, anchor, "start", "line")
                .map(|l| l + 1)
                .or_else(|| d["line"].as_u64())
                .unwrap_or(0);
            let character = pos(d, anchor, "start", "character").unwrap_or(0);
            (names[index].as_ref(), line, character)
        })
        .collect();
    let mut per_name_kind = std::collections::HashMap::<(&str, &str), usize>::new();
    let mut per_name_line = std::collections::HashMap::<(&str, u64, &str), usize>::new();
    for (d, (name, line, _)) in raw.iter().zip(&anchors) {
        let kind = d["kind"].as_str().unwrap_or("");
        *per_name_kind.entry((name, kind)).or_default() += 1;
        *per_name_line.entry((name, *line, kind)).or_default() += 1;
    }
    let by_engine_id: std::collections::HashMap<&str, usize> = raw
        .iter()
        .enumerate()
        .filter_map(|(index, d)| Some((d["id"].as_str()?, index)))
        .collect();
    raw.iter()
        .zip(&anchors)
        .map(|(d, &(name, line, character))| {
            let kind = d["kind"].as_str().unwrap_or("");
            let mut row = json!({"name":name,"kind":kind,"line":line});
            if d.get("implements").is_some() {
                row["typeName"] = d["name"].clone();
            }
            if per_name_line.get(&(name, line, kind)).copied().unwrap_or(0) > 1 {
                // 1-based, like every column.
                row["character"] = json!(character + 1);
            }
            if let Some(start) = pos(d, "range", "start", "line").map(|l| l + 1)
                && start < line
            {
                row["startLine"] = json!(start);
            }
            if let Some(doc) = d["docLine"].as_u64() {
                row["docStartLine"] = json!(doc + 1);
            }
            if let Some(end) = pos(d, "range", "end", "line").map(|l| l + 1)
                && end != line
            {
                row["endLine"] = json!(end);
            }
            if d["exported"].as_bool() == Some(true) {
                row["exported"] = json!(true);
                // Public names when exported under another name (`export { foo
                // as bar }`, `export default function foo`).
                if let Some(public) = d.get("exportedAs").filter(|v| v.is_array()) {
                    row["exportedAs"] = public.clone();
                }
            }
            // Parents precede children in engine preorder; an unknown parent
            // (never expected) is dropped rather than leaking an engine id.
            // A declaration never names itself as parent: C's one-line
            // `typedef struct X X;` holds `struct X` under `type X` at the
            // same name line, so the struct is the typedef's sibling and
            // takes the typedef's own parent.
            let mut parent_index = d["parent"]
                .as_str()
                .and_then(|id| by_engine_id.get(id))
                .copied();
            while let Some(index) = parent_index
                && anchors
                    .get(index)
                    .is_some_and(|&(parent, parent_line, _)| (parent, parent_line) == (name, line))
            {
                parent_index = raw[index]["parent"]
                    .as_str()
                    .and_then(|id| by_engine_id.get(id))
                    .copied();
            }
            if let Some(index) = parent_index
                && let Some(&(parent, parent_line, _)) = anchors.get(index)
            {
                row["parent"] = json!(parent);
                let parent_kind = raw[index]["kind"].as_str().unwrap_or("");
                if per_name_kind
                    .get(&(parent, parent_kind))
                    .copied()
                    .unwrap_or(0)
                    > 1
                {
                    row["parentLine"] = json!(parent_line);
                }
            }
            row
        })
        .collect()
}
