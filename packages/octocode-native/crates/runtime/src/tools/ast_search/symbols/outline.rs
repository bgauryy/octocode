use super::ast_search_query_symbols;
use super::names::{NameFilter, names_equal, names_match};
use super::rows::{compact_declarations, error_text, name_impl_traits};
use crate::response::result::ToolError;
use octocode_contract_types::AstSearchQuerySymbols;
use octocode_contract_types::ErrorCode;
use octocode_engine::graph::{GraphFactCode, GraphFactDiagnostic};
use serde_json::{Value, json};

/// What one parsed file contributes to a symbols query. A file contributing
/// nothing is not held.
pub(crate) struct OutlineFile {
    /// The row path.
    pub(crate) path: String,
    /// The declaration rows its `symbolKinds` and `symbolFilter.terms` filters can keep.
    pub(crate) rows: Vec<Value>,
    /// Declarations of a kind `symbolKinds` leaves out whose name equals a
    /// `symbolFilter.terms` entry: they keep that entry exact, and the answer says
    /// when they are all the entry names.
    pub(crate) off_kind: Vec<Value>,
    /// Its parse notes that bear on the answer.
    pub(crate) notes: Vec<GraphFactDiagnostic>,
    pub(crate) source_descriptors: octocode_engine::text::analysis::SourceDescriptors,
}

/// One file's reduced rows: `(rows, off-kind rows, notes)`.
pub(super) type Reduced = (Vec<Value>, Vec<Value>, Vec<GraphFactDiagnostic>);

/// What one file contributes to a symbols query, reduced as soon as the file
/// is parsed so a scan holds the answer, not the repository.
///
/// The rows are the file's compact declarations that a `symbolKinds` entry names
/// and whose name (or trait-impl type name) contains some `symbolFilter.terms` text:
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
pub(super) fn declarations(
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

/// The per-symbol provenance keys `includeProvenance` adds to a row.
const PROVENANCE_KEYS: [&str; 6] = [
    "occurrenceId",
    "sourceDigest",
    "sourceRange",
    "sourceOrigin",
    "selectionByteRange",
    "selectionOrigin",
];

/// [`reduce_file`] over one file's declarations facts.
pub(super) fn reduce_document(
    q: &AstSearchQuerySymbols,
    filter: Option<&NameFilter>,
    facts: &octocode_engine::graph::GraphFactsDocument,
    source: impl FnOnce() -> Option<String>,
    evidence: &mut super::super::evidence::FileEvidence<'_, '_, '_>,
) -> Result<Reduced, ToolError> {
    let Ok(Value::Array(declarations)) = serde_json::to_value(&facts.declarations) else {
        return Err(ToolError::new(
            ErrorCode::ExecutionFailed,
            "Could not encode declarations.",
        ));
    };
    let (mut rows, mut off_kind, notes) = reduce_file(
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
    );
    let declarations = facts
        .declarations
        .iter()
        .map(|row| (row.id.as_str(), row))
        .collect::<std::collections::HashMap<_, _>>();
    let provenance = q.include_provenance.unwrap_or(false);
    for row in rows.iter_mut().chain(&mut off_kind) {
        if !provenance {
            // Per-symbol provenance only on request (`includeProvenance`):
            // without it the outline names each declaration once.
            if let Some(fields) = row.as_object_mut() {
                for key in PROVENANCE_KEYS {
                    fields.shift_remove(key);
                }
            }
            continue;
        }
        let id = row["occurrenceId"].as_str().unwrap_or("");
        let declaration = declarations.get(id).ok_or_else(|| {
            ToolError::new(
                ErrorCode::ExecutionFailed,
                "Declaration source occurrence is missing.",
            )
        })?;
        if let Some(span) = declaration.source_range {
            evidence.span(row, span, "sourceRange", "sourceOrigin", true)?;
        }
        if let Some(span) = declaration.selection_byte_range {
            evidence.span(row, span, "selectionByteRange", "selectionOrigin", false)?;
        }
        row["occurrenceId"] = json!(crate::digest::json_sha256(&json!([
            facts.file,
            facts.language,
            row["sourceOrigin"]["analysisUnitId"],
            declaration.id
        ])));
    }
    Ok((rows, off_kind, notes))
}

/// Declaration rows per file, after the kind filter and a name prefilter.
pub(crate) struct Outline {
    pub(crate) files: Vec<OutlineFile>,
    /// Files the scan considered, including those the name prefilter skipped.
    pub(crate) scanned: usize,
    pub(super) truncated: bool,
    /// Where a scan cut by the time budget resumes (`scanOffset`).
    pub(super) scan_resume: Option<u32>,
    pub(super) skipped: u32,
    pub(super) diagnostics: Vec<Value>,
    /// Candidate entries the security path policy withheld from the walk.
    pub(super) withheld: crate::policy::discovery::Withheld,
    /// Root-relative directories the default excludes pruned unseen.
    pub(super) pruned: Vec<String>,
    /// The scopes a walk cut at the ceiling never reached.
    pub(super) unwalked: Option<crate::tools::ast_search::unwalked::Unwalked>,
    /// Entries the walk could not read, and how many for want of
    /// permission: absence below them is unproven.
    pub(super) walk_errors: (u32, u32),
}

/// One file's declarations; `Err` is a terminal row (size limit, no
/// extractor) rather than a tool error.
pub(super) fn outline_file(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
    display_path: &str,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<Result<Outline, Value>, ToolError> {
    let Some(b) = crate::tools::ast_search::source::read_parse_source(canonical)? else {
        let name = q
            .symbol_filter
            .as_ref()
            .filter(|filter| filter.terms.len() == 1)
            .and_then(|filter| filter.terms.first())
            .map(|term| term.as_str());
        return Ok(Err(crate::tools::ast_search::source::source_limit(
            display_path,
            name,
        )));
    };
    // Parse the file as the directory scan does: admitted by the one text
    // rule, raw. Output strings are redacted by the response stage.
    let cancelled = || cancel.check().is_err();
    let mut maps = super::super::source::mapping_budget(&cancelled);
    let admitted = super::super::source::admitted(canonical, &b, &mut maps)?;
    let source = admitted.text();
    let source_path = canonical.to_string_lossy();
    let parser = crate::tools::ast_search::language::parser_override(
        canonical,
        ast_search_query_symbols::language(q).as_deref(),
    );
    let unsupported = || {
        let mut row = json!({"status":"error","errorCode":ErrorCode::SymbolsUnsupported,"path":display_path,"error":"No native declaration extractor supports this source."});
        crate::response::result::attach_related(
            &mut row,
            super::super::output::unsupported_read(&source_path),
        );
        row
    };
    let document = match parser.as_deref() {
        // An extension that selects no grammar outlines nothing: an empty
        // outline would read as a file without declarations. A document
        // format (Markdown, JSON, YAML, TOML) has no grammar to set: the
        // localFetch outline reads it.
        None if !crate::tools::ast_search::language::registered(canonical).is_some_and(
            |entry| entry.admits(octocode_engine::text::analysis::AnalysisOperation::Declarations),
        ) =>
        {
            return Ok(Err(
                if crate::content::DocumentFormat::of(&source_path).is_some() {
                    no_extractor(
                        canonical,
                        display_path,
                        &crate::tools::ast_search::language::extension_label(canonical),
                    )
                } else {
                    unsupported()
                },
            ));
        }
        // The cache keys the parser extension used, so every override reuses
        // its own parse.
        parser => declarations(source, &source_path, parser),
    };
    let path = canonical.to_string_lossy().into_owned();
    let Some(document) = document else {
        return Ok(Err(unsupported()));
    };
    let mut diagnostics = vec![];
    if let Some(note) = crate::tools::ast_search::language::cpp_header_note(
        canonical,
        ast_search_query_symbols::language(q).as_deref(),
        source,
    ) {
        diagnostics
            .push(json!({"code":crate::tools::ast_search::language::CPP_HEADER_NOTE,"severity":"warning","message":note}));
    }
    let bound = document
        .source_templates
        .bind(&admitted, &mut maps)
        .map_err(ToolError::source)?;
    let mut evidence = super::super::evidence::FileEvidence::new(&bound, &mut maps, cancel);
    let (rows, off_kind, notes) = reduce_document(
        q,
        ast_search_query_symbols::name_filter(q).as_ref(),
        &document,
        || Some(source.to_string()),
        &mut evidence,
    )?;
    let source_descriptors = evidence.finish()?;
    let files = vec![OutlineFile {
        path,
        rows,
        off_kind,
        notes,
        source_descriptors,
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
/// reads and no `parser` makes source: it names the file type and leads to
/// the localFetch outline of the file (`view:"symbols"`: its headings or
/// keys).
fn no_extractor(canonical: &std::path::Path, display_path: &str, kind: &str) -> Value {
    let path = canonical.to_string_lossy();
    let mut row = json!({
        "status": "error",
        "errorCode": ErrorCode::SymbolsUnsupported,
        "path": display_path,
        "error": format!("No declaration extractor supports {kind} files."),
        "hints": [format!(
            "symbols does not outline {kind} files; the read lead outlines this one, or search it with localSearch."
        )],
    });
    row["calls"]["read"] = super::super::output::unsupported_read(&path);
    row
}
