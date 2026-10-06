use crate::cache::{CacheClass, CacheConfig, CacheKey, CachePartition, Store};
pub use crate::contracts::tool_types::AstSearchQuerySymbols;
use crate::policy::prune::DefaultsFlag;
use crate::tools::id::ToolId;
use crate::tools::num::u32_of;
use crate::tools::symbol_outline::outline_rows;
use crate::{
    policy::{path::PathPolicy, prune::PruneMode},
    security::ContentSecurity,
    tools::cancel::CancellationCheck,
};
use octocode_engine::types::GraphFactsScanOptions;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Engine-unit views over the generated `symbols` query.
impl AstSearchQuerySymbols {
    pub fn language(&self) -> Option<String> {
        self.language.as_ref().map(ToString::to_string)
    }
    pub fn language_globs(&self) -> Option<&BTreeMap<String, Vec<String>>> {
        (!self.language_globs.is_empty()).then_some(&self.language_globs)
    }
    pub fn kinds(&self) -> Option<&Vec<String>> {
        (!self.kinds.is_empty()).then_some(&self.kinds)
    }
    pub fn exclude(&self) -> Option<Vec<String>> {
        (!self.exclude.is_empty()).then(|| self.exclude.clone())
    }
    pub fn max_files(&self) -> u32 {
        u32_of(self.max_files)
    }
    pub fn page(&self) -> u32 {
        u32_of(self.page)
    }
    pub fn page_size(&self) -> u32 {
        u32_of(self.page_size)
    }
    pub fn snapshot(&self) -> Option<&str> {
        self.snapshot.as_deref().map(String::as_str)
    }
    /// The `name` filter: a string is a one-entry list. Each entry that
    /// equals some declaration name matches only equal names; otherwise it
    /// matches as a substring. All-lowercase text ignores case.
    fn name_filter(&self) -> Option<NameFilter> {
        use crate::contracts::tool_types::AstSearchQuerySymbolsSymbolName;
        let entries = match self.symbol_name.as_ref()? {
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

/// Every declaration kind the native extractors emit: tree-sitter graph facts
/// (engine `signatures::nodes::declaration_kind`) and the JS/TS oxc extractor
/// (`signatures::js_oxc::symbol_kind_name`).
const DECLARATION_KINDS: &[&str] = &[
    "class",
    "constant",
    "constructor",
    "enum",
    "enumMember",
    "function",
    "impl",
    "interface",
    "label",
    "macro",
    "method",
    "module",
    "namespace",
    "property",
    "struct",
    "symbol",
    "trait",
    "type",
    "variable",
];
pub fn execute_symbols(
    q: &AstSearchQuerySymbols,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> super::AstResult {
    cancel.check().map_err(super::cancelled)?;
    if let Some(unknown) = q
        .kinds
        .iter()
        .find(|kind| !DECLARATION_KINDS.contains(&kind.as_str()))
    {
        return Err(super::AstError::new(
            "ast.symbols.input.invalid",
            format!(
                "Unknown declaration kind \"{unknown}\". Use one of: {}.",
                DECLARATION_KINDS.join(", ")
            ),
        ));
    }
    let p = paths
        .validate(q.path.as_str())
        .map_err(super::AstError::from)?;
    let meta =
        std::fs::metadata(&p.canonical).map_err(|error| super::io_error(q.path.as_str(), error))?;
    check_language_scope(q, &p.canonical, meta.is_file())?;
    if meta.is_file() {
        let outline = match outline_file(q, &p.canonical, security)? {
            Ok(outline) => outline,
            Err(terminal) => return Ok(terminal),
        };
        let set = select(q, &p.canonical, outline, security, cancel)?;
        return Ok(render_page(q, &set));
    }
    // Later pages of a directory outline reuse page 1's selection while no
    // scanned file changed (same paths, sizes and modification times).
    let memo_key = directory_memo_key(q, &p.canonical, paths, cancel)?;
    if q.page() > 1
        && let Some(hit) = SYMBOL_PAGES.get(&memo_key)
    {
        return Ok(render_page(q, &hit.value));
    }
    let outline = outline_directory(q, &p.canonical, paths, cancel)?;
    let set = select(q, &p.canonical, outline, security, cancel)?;
    let page = render_page(q, &set);
    let bytes = serde_json::to_vec(&set).map_or(usize::MAX, |encoded| encoded.len());
    SYMBOL_PAGES.put(memo_key, set, bytes, CacheClass::Volatile);
    Ok(page)
}

/// `language` names one file's grammar; `languageGlobs` maps a directory's.
fn check_language_scope(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
    is_file: bool,
) -> Result<(), super::AstError> {
    if is_file && q.language_globs().is_some() {
        return Err(super::AstError::new(
            "ast.language.directoryRequired",
            "languageGlobs is for directory symbols. Use language for a single file.",
        ));
    }
    if is_file {
        return super::validate_file_language(canonical, q.language().as_deref());
    }
    if q.language.is_none() {
        return Ok(());
    }
    let mut error = super::AstError::new(
        "ast.language.fileRequired",
        "language on symbols requires a single source file.",
    );
    // Directory symbols pick each file's grammar from its extension, so
    // the same query without language is the exact repair.
    if let Ok(mut repaired) = serde_json::to_value(q) {
        if let Some(object) = repaired.as_object_mut() {
            object.remove("language");
        }
        error.next = Some(Box::new(json!({
            "repair": crate::tools::result::Continuation::new(ToolId::AstSearch, repaired).confidence("exact").build()
        })));
    }
    Err(error)
}

/// `(row path, declarations, engine diagnostics, error text)` of one parsed
/// file; the error text is its recovered-parse error lines, when known.
type OutlineFile = (String, Vec<Value>, Vec<String>, Option<String>);

/// Declaration facts per file, before the kind and name filters.
struct Outline {
    files: Vec<OutlineFile>,
    /// Files the scan considered, including those the name prefilter skipped.
    scanned: usize,
    truncated: bool,
    skipped: u32,
    diagnostics: Vec<Value>,
}

/// One file's declarations; `Err` is a terminal row (size limit, no
/// extractor) rather than a tool error.
fn outline_file(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
    security: &ContentSecurity,
) -> Result<Result<Outline, Value>, super::AstError> {
    let Some(b) = super::read_parse_source(canonical)? else {
        return Ok(Err(super::source_limit(&super::display_name(canonical))));
    };
    // Parse the file as the directory scan does: raw. Output strings are
    // redacted by the response stage.
    let source = security
        .decode_source_bytes(&b, super::MAX_PARSE_SOURCE_BYTES)
        .map_err(super::AstError::from)?;
    let source_path = canonical.to_string_lossy();
    let cpp_header = super::cpp_header_override(canonical, q.language().as_deref());
    let raw = super::declarations_cache::extract(&source, &source_path, cpp_header, || {
        if cpp_header {
            octocode_engine::portable::extract_graph_facts_with_extension(
                &source,
                &source_path,
                "cpp",
            )
        } else {
            octocode_engine::portable::extract_declarations(&source, &source_path)
        }
    });
    let path = super::display_name(canonical);
    let Some(raw) = raw else {
        return Ok(Err(
            json!({"status":"error","errorCode":"ast.symbols.unsupported","path":path,"error":"No native declaration extractor supports this source. Inspect its syntax tree or exact content."}),
        ));
    };
    let (mut skipped, mut diagnostics, mut files) = (0, vec![], vec![]);
    match serde_json::from_str::<Value>(&raw) {
        Ok(mut facts) => {
            let declarations = match facts["declarations"].take() {
                Value::Array(rows) => rows,
                _ => vec![],
            };
            let notes = facts["diagnostics"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            let spans = serde_json::from_value::<Vec<[u32; 2]>>(facts["errorLines"].take())
                .unwrap_or_default();
            let errors = (!spans.is_empty()).then(|| error_text(&source, &spans));
            files.push((path, declarations, notes, errors));
        }
        Err(_) => {
            skipped = 1;
            diagnostics.push(json!({"path":path,"message":"graph facts decode failed"}));
        }
    }
    Ok(Ok(Outline {
        files,
        scanned: 1,
        truncated: false,
        skipped,
        diagnostics,
    }))
}

/// The engine walk options a directory outline scans with.
fn directory_scan_options(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
) -> GraphFactsScanOptions {
    GraphFactsScanOptions {
        path: canonical.to_string_lossy().into_owned(),
        exclude_dir: Some(PruneMode::SyntaxVisible.directories(q.default_excludes.defaults())),
        exclude: q.exclude(),
        max_files: Some(q.max_files()),
        max_file_bytes: u32::try_from(super::MAX_PARSE_SOURCE_BYTES).ok(),
        language_globs: q
            .language_globs()
            .map(crate::tools::ast_rule::language_globs),
    }
}

/// Every file's declarations under a directory. A name filter is a byte
/// prefilter first: a file that never spells any filter text cannot declare a
/// matching name, so it is read but not parsed.
fn outline_directory(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<Outline, super::AstError> {
    let filter = q.name_filter();
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
    let scan = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        octocode_engine::graph::scan_graph_facts_typed_selected(
            directory_scan_options(q, canonical),
            &|path| super::allow_discovery(path, paths, cancel),
            &keep,
        )
    }))
    .unwrap_or_else(|_| Err("graph-facts scan failed on pathological input".to_owned()))
    .map_err(super::native_error)?;
    let rooted = |relative: &str| super::rooted_display(canonical, std::path::Path::new(relative));
    // Only a name search weighs a recovered parse by its error lines.
    let named = filter.is_some();
    let diagnostics = scan
        .skipped
        .iter()
        .map(|d| json!({"path":rooted(&d.relative_path),"message":format!("{}: {}",d.code,d.message)}))
        .collect();
    let files = scan
        .entries
        .into_iter()
        .map(|entry| {
            let declarations = match serde_json::to_value(&entry.facts.declarations) {
                Ok(Value::Array(rows)) => rows,
                _ => vec![],
            };
            let spans = &entry.facts.error_lines;
            let errors = (named && !spans.is_empty())
                .then(|| std::fs::read(canonical.join(&entry.relative_path)).ok())
                .flatten()
                .map(|bytes| error_text(&String::from_utf8_lossy(&bytes), spans));
            (
                rooted(&entry.relative_path),
                declarations,
                entry.facts.diagnostics,
                errors,
            )
        })
        .collect::<Vec<_>>();
    Ok(Outline {
        scanned: files.len() + passed_over.into_inner(),
        files,
        truncated: scan.truncated,
        skipped: scan.files_skipped,
        diagnostics,
    })
}

/// Page memo for directory outlines: one query shape over unchanged files.
static SYMBOL_PAGES: std::sync::LazyLock<Store<SymbolSet>> = std::sync::LazyLock::new(|| {
    Store::new(
        CacheConfig {
            max_entries: 8,
            max_bytes: 64 * 1024 * 1024,
            ttl: std::time::Duration::from_secs(600),
            ..CacheConfig::default()
        },
        None,
    )
});

/// The memo key: every query field except the page cursor, plus the
/// fingerprint of the files the outline scans.
fn directory_memo_key(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<CacheKey, super::AstError> {
    let shape = json!([
        canonical.to_string_lossy(),
        q.language_globs(),
        q.symbol_name,
        q.kinds(),
        q.exclude(),
        q.default_excludes.defaults(),
        q.max_files()
    ]);
    Ok(CacheKey {
        namespace: "ast-symbol-pages".into(),
        resource: crate::digest::json_sha256(&shape),
        partition: CachePartition {
            endpoint: directory_fingerprint(q, canonical, paths, cancel)?,
            credential_fingerprint: String::new(),
        },
    })
}

/// Digest of the files a directory outline would scan: path, size and
/// modification time, walked under the caller's path policy.
fn directory_fingerprint(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<String, super::AstError> {
    let options = directory_scan_options(q, canonical);
    let walk = octocode_engine::portable::query_file_system_filtered(
        octocode_engine::types::FileSystemQueryOptions {
            path: options.path,
            recursive: Some(true),
            show_hidden: Some(false),
            entry_type: Some("f".to_owned()),
            extensions: Some(octocode_engine::signatures::graph_facts::graph_fact_extensions()),
            exclude_dir: options.exclude_dir,
            exclude: options.exclude,
            stop_at_limit: Some(true),
            limit: options.max_files,
            ..Default::default()
        },
        &|path| super::allow_discovery(path, paths, cancel),
    )
    .map_err(super::native_error)?;
    let mut stats = walk
        .entries
        .iter()
        .map(|entry| {
            (
                &entry.relative_path,
                entry.size,
                entry.modified_ms.map(f64::to_bits),
            )
        })
        .collect::<Vec<_>>();
    stats.sort_unstable();
    Ok(crate::digest::json_sha256(&json!(stats)))
}

/// Applies the kind and name filters to an outline and pins the result.
fn select(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
    outline: Outline,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> Result<SymbolSet, super::AstError> {
    cancel.check().map_err(super::cancelled)?;
    let Outline {
        mut files,
        scanned,
        truncated,
        skipped,
        mut diagnostics,
    } = outline;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    // A directory query writes each row's file; a single file hoists `path`.
    let per_row_path = !canonical.is_file();
    let mut candidates = vec![];
    let mut recovered = false;
    // Per file: (path, its notes, index of its first candidate row).
    let mut file_notes: Vec<(&String, Vec<&String>, usize)> = Vec::new();
    // A recovered parse hides a declaration of a queried name only where its
    // syntax errors spell that name; elsewhere its note is irrelevant here.
    let spelled = q.name_filter();
    for (path, declarations, notes, errors) in &files {
        let unrelated = spelled
            .as_ref()
            .zip(errors.as_ref())
            .is_some_and(|(filter, errors)| !filter.spelled_in(errors));
        // The syntax-only caveat is static and already in the tool
        // description; repeating it costs every call.
        let kept: Vec<&String> = notes
            .iter()
            .filter(|m| !is_linking_only(m) && *m != SYNTAX_ONLY_NOTE)
            .filter(|m| !(unrelated && m.starts_with(RECOVERED_PARSE_NOTE_PREFIX)))
            .collect();
        recovered |= kept
            .iter()
            .any(|m| m.starts_with(RECOVERED_PARSE_NOTE_PREFIX));
        file_notes.push((path, kept, candidates.len()));
        for (d, mut row) in declarations.iter().zip(compact_declarations(declarations)) {
            let kind = d["kind"].as_str().unwrap_or("");
            if q.kinds().is_none_or(|ks| ks.iter().any(|k| k == kind)) {
                row["path"] = json!(path);
                candidates.push(row)
            }
        }
    }
    let name_of = |row: &Value| row["name"].as_str().unwrap_or("").to_owned();
    let mut filter = q.name_filter();
    if let Some(filter) = filter.as_mut() {
        let names = candidates.iter().map(name_of).collect::<Vec<_>>();
        filter.settle(names.iter().map(String::as_str));
    }
    let kept_rows: Vec<bool> = candidates
        .iter()
        .map(|row| {
            filter
                .as_ref()
                .is_none_or(|filter| filter.matches(&name_of(row)))
        })
        .collect();
    // A note on a file that lists declarations stays on that file; notes on
    // files without a listed declaration are one entry per message with
    // every path (the repeated message is the only thing removed).
    let mut quiet: Vec<(&String, Vec<&String>)> = Vec::new();
    for (index, (path, notes, first)) in file_notes.iter().enumerate() {
        let end = file_notes
            .get(index + 1)
            .map_or(candidates.len(), |next| next.2);
        let listed = kept_rows[*first..end].iter().any(|kept| *kept);
        for m in notes {
            if !per_row_path {
                diagnostics.push(json!({"message":m}));
            } else if listed {
                diagnostics.push(json!({"path":path,"message":m}));
            } else if let Some((_, paths)) = quiet.iter_mut().find(|(message, _)| message == m) {
                paths.push(path);
            } else {
                quiet.push((m, vec![path]));
            }
        }
    }
    for (message, paths) in quiet {
        // `files[].path`, like every row path, is shaped by the response stage.
        let files: Vec<Value> = paths.iter().map(|path| json!({"path": path})).collect();
        diagnostics.push(json!({
            "message": format!("{message} (files without a listed declaration)"),
            "files": files,
        }));
    }
    let mut declarations = candidates
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
        canonical.to_string_lossy(),
        q.language,
        q.language_globs(),
        q.symbol_name,
        q.kinds(),
        q.exclude(),
        q.max_files(),
        &declarations,
        &diagnostics,
        truncated,
        skipped
    ]));
    for row in declarations.iter_mut().chain(diagnostics.iter_mut()) {
        if let Some(path) = row["path"].as_str() {
            row["path"] = json!(security.sanitize_text(path, None).content);
        }
        if let Some(files) = row.get_mut("files").and_then(Value::as_array_mut) {
            for file in files {
                if let Some(path) = file["path"].as_str() {
                    file["path"] = json!(security.sanitize_text(path, None).content);
                }
            }
        }
    }
    // A name search that may be missing declarations (a recovered parse,
    // skipped files) completes with a text search for the same name.
    // An empty name search leads to the name's text as well.
    let text_search = if recovered || skipped > 0 || truncated {
        text_search_lead(
            q,
            canonical,
            "A recovered parse or skipped file may hide a declaration; search its text.",
        )
    } else if declarations.is_empty() {
        text_search_lead(
            q,
            canonical,
            "No declaration has this name; search its text.",
        )
    } else {
        None
    };
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
        recovered,
    })
}

/// One declaration a name filter singles out: its callers (a callable) or
/// references are the usual next question, and only a language server
/// answers it; none is offered when no server runs for the file.
fn references_lead(canonical: &std::path::Path, row: &Value) -> Option<Value> {
    let file = row["path"].as_str().map_or_else(
        || canonical.to_path_buf(),
        |shown| canonical.parent().unwrap_or(canonical).join(shown),
    );
    let query = crate::tools::lsp_search::verify_query(
        &file.to_string_lossy(),
        row["name"].as_str()?,
        row["line"].as_u64()?,
        crate::tools::lsp_search::Verify::for_kind(row["kind"].as_str().unwrap_or_default()),
    )?;
    Some(
        crate::tools::result::Continuation::new(ToolId::LspSearch, query)
            .why("Who uses this declaration.")
            .confidence("high")
            .build(),
    )
}

/// A text search for the `symbolName` values under the outlined path: the
/// completion when a recovered parse or a skipped file may hide one.
fn text_search_lead(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
    why: &str,
) -> Option<Value> {
    let filter = q.name_filter()?;
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
        crate::tools::result::Continuation::new(ToolId::LocalSearch, query)
            .why(why)
            .confidence("medium")
            .build(),
    )
}

/// The read of one declaration's lines.
fn read_lead(canonical: &std::path::Path, row: &Value) -> Option<Value> {
    let start = row["line"].as_u64()?;
    let end = row["endLine"].as_u64().unwrap_or(start);
    Some(
        crate::tools::result::Continuation::new(
            ToolId::LocalFetch,
            json!({"path": canonical.to_string_lossy(), "ranges": [format!("{start}-{end}")]}),
        )
        .why("Read the top declaration.")
        .confidence("high")
        .build(),
    )
}

/// Every declaration of one symbols query, before paging.
#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct SymbolSet {
    /// `next.verifyReferences` for a single named declaration.
    references: Option<Value>,
    /// `next.read` of a file outline's top declaration.
    #[serde(default)]
    read: Option<Value>,
    /// `next.textSearch` for the name when the outline may be incomplete.
    #[serde(default)]
    text_search: Option<Value>,
    path: String,
    snapshot: String,
    declarations: Vec<Value>,
    diagnostics: Vec<Value>,
    files_scanned: usize,
    skipped: u32,
    truncated: bool,
    /// A parser recovered from syntax errors: the declarations are a partial
    /// view of the source, not a complete inventory.
    recovered: bool,
}

fn render_page(q: &AstSearchQuerySymbols, set: &SymbolSet) -> Value {
    let SymbolSet {
        references,
        read,
        text_search,
        path,
        snapshot,
        declarations,
        diagnostics,
        files_scanned,
        skipped,
        truncated,
        recovered,
    } = set;
    let (skipped, truncated) = (*skipped, *truncated);
    if q.page() > 1 && q.snapshot() != Some(snapshot.as_str()) {
        let mut restart = serde_json::to_value(q).unwrap_or_default();
        if let Some(query) = restart.as_object_mut() {
            query.retain(|key, value| key != "snapshot" && !value.is_null());
        }
        restart["page"] = json!(1);
        return crate::tools::result::stale_snapshot(
            crate::tools::result::Continuation::new(ToolId::AstSearch, restart)
                .confidence("exact")
                .build(),
        );
    }
    let size = q.page_size().clamp(1, 1000) as usize;
    let page = q.page().max(1) as usize;
    let start = (page - 1) * size;
    let more = start + size < declarations.len();
    let incomplete = truncated || skipped > 0;
    let rows = declarations
        .get(start..(start + size).min(declarations.len()))
        .unwrap_or(&[]);
    // A `maxFiles` cut below the schema maximum is raisable: expandScan
    // re-runs the outline with a doubled bound, so only the maximum is terminal.
    let max_files = u32::try_from(crate::contracts::query_schema_max(
        ToolId::AstSearch,
        Some("symbols"),
        "maxFiles",
    ))
    .unwrap_or(u32::MAX);
    let expand_scan = (truncated && q.max_files() < max_files)
        .then(|| q.max_files().saturating_mul(2).min(max_files));
    let mut diagnostics = diagnostics.clone();
    if truncated {
        let limit = q.max_files();
        diagnostics.push(json!({
            "code":"structural.scan.truncated",
            "message":format!("Candidate scan hit the maxFiles limit ({limit}); files beyond it were not outlined."),
        }));
    }
    let mut out = json!({"operation":"symbols","path":path,"filesScanned":files_scanned,"filesSkipped":skipped,"diagnostics":diagnostics,"isPartial":more||incomplete||*recovered,"pagination":{"totalItems":declarations.len()}});
    // A directory outline groups rows under their file, like `match` results,
    // so each path is written once instead of on every declaration. Rows are
    // compact outline strings (see `tools::symbol_outline`).
    if rows.iter().any(|row| row.get("path").is_some()) {
        out["files"] = Value::Array(group_by_file(rows));
    } else {
        out["symbols"] = Value::Array(outline_rows(rows));
    }
    // The snapshot only pins later pages to the same source; a single page
    // has nothing to pin.
    if more || page > 1 {
        out["snapshot"] = json!(snapshot);
        out["pagination"] = json!({"currentPage":page,"totalPages":declarations.len().div_ceil(size).max(1),"totalItems":declarations.len(),"hasMore":more});
    }
    if skipped > 0 || (truncated && expand_scan.is_none()) {
        out["terminalLimit"] = json!(true);
        if skipped > 0 {
            diagnostics.push(json!({
                "code":"symbols.filesSkipped",
                "message":format!("terminalLimit: {skipped} files were not outlined (parse byte limit, encoding or read failure); no page reaches their declarations. next.textSearch searches their text."),
            }));
            out["diagnostics"] = json!(diagnostics);
        }
    }
    if more {
        let mut nq = serde_json::to_value(q).unwrap_or_default();
        if let Some(map) = nq.as_object_mut() {
            map.retain(|_, value| !value.is_null());
        }
        nq["maxFiles"] = json!(q.max_files());
        nq["snapshot"] = json!(snapshot);
        nq["page"] = json!(page + 1);
        out["next"] = json!({"nextPage":crate::tools::result::Continuation::new(ToolId::AstSearch, nq).confidence("exact").build()})
    }
    if let Some(bound) = expand_scan {
        let mut nq = serde_json::to_value(q).unwrap_or_default();
        if let Some(map) = nq.as_object_mut() {
            map.retain(|key, value| key != "snapshot" && !value.is_null());
        }
        nq["maxFiles"] = json!(bound);
        nq["page"] = json!(1);
        out["next"]["expandScan"] = crate::tools::result::Continuation::new(ToolId::AstSearch, nq)
            .confidence("exact")
            .build();
    }
    if let Some(lead) = references {
        out["next"]["verifyReferences"] = lead.clone();
    }
    if let Some(lead) = text_search.as_ref().filter(|_| page == 1) {
        out["next"]["textSearch"] = lead.clone();
    }
    if let Some(lead) = read.as_ref().filter(|_| page == 1) {
        out["next"]["read"] = lead.clone();
    }
    if declarations.is_empty() && !incomplete {
        out["status"] = json!("empty");
        // What was searched: the name (when asked) and the file count.
        let named = q
            .name_filter()
            .map(|filter| {
                let names = filter
                    .entries
                    .iter()
                    .map(|entry| entry.text.as_str())
                    .collect::<Vec<_>>();
                format!("named {} ", names.join(", "))
            })
            .unwrap_or_default();
        let lead = if text_search.is_some() {
            "; next.textSearch searches the text"
        } else {
            "; broaden kinds, path, or filters"
        };
        out["hints"] = json!([format!(
            "0 declarations {named}in {files_scanned} files{lead}."
        )]);
    }
    out
}
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
        .map(|(path, rows)| json!({"path":path,"symbols":outline_rows(&rows)}))
        .collect()
}

/// Import/module-linking caveats from graph facts. Declaration listing never
/// links imports or modules, so these only add noise to symbols output.
fn is_linking_only(message: &str) -> bool {
    message.starts_with("unsupported Rust macro expansion")
        || message.starts_with("unsupported Rust conditional or custom module attributes")
        || message.starts_with("unsupported Rust inner conditional or custom crate attributes")
}

/// Engine diagnostic for a tree-sitter parse that recovered from syntax errors.
const RECOVERED_PARSE_NOTE_PREFIX: &str = "tree-sitter recovered from parse errors";

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

/// Static engine caveat attached to every tree-sitter graph-facts file.
const SYNTAX_ONLY_NOTE: &str =
    "tree-sitter graph facts are syntax-only; use LSP references/callHierarchy for semantic proof";

/// Projects engine declaration facts (one file) onto compact response rows.
///
/// `line` (1-based) anchors the declaration NAME, so a row feeds lspSearch as
/// `symbolName`+`lineHint`; `character` (0-based) appears only when another
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
    let pos = |d: &Value, range: &str, edge: &str, field: &str| {
        d.pointer(&format!("/{range}/{edge}/{field}"))
            .and_then(Value::as_u64)
    };
    let anchors: Vec<(&str, u64, u64)> = raw
        .iter()
        .map(|d| {
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
            (d["name"].as_str().unwrap_or(""), line, character)
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
            if per_name_line.get(&(name, line, kind)).copied().unwrap_or(0) > 1 {
                row["character"] = json!(character);
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
            if let Some(&index) = d["parent"].as_str().and_then(|id| by_engine_id.get(id))
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
