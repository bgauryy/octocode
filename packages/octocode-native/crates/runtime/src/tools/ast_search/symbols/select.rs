use super::ast_search_query_symbols;
use super::left_out::{KindsLeftOut, Suppressed};
use super::names::{NameFilter, names_match, row_names};
use super::outline::{Outline, OutlineFile};
use crate::execution::cancel::CancellationCheck;
use crate::id::ToolId;
use crate::response::result::ToolError;
use crate::tools::leads::skipped;
use octocode_contract_types::AstSearchQuerySymbols;
use octocode_contract_types::SkippedPathCause;
use octocode_engine::graph::{GraphFactCode, GraphFactDiagnostic};
use serde_json::{Value, json};

/// Applies the kind and name filters to an outline and pins the result.
pub(super) fn select(
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
        path: crate::tools::display_name(canonical),
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
    Some(crate::response::result::Continuation::new(ToolId::LspSearch, query).build())
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
    Some(crate::response::result::Continuation::new(ToolId::LocalSearch, query).build())
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
        .build(),
    )
}

/// Every declaration of one symbols query, before paging.
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub(crate) struct SymbolSet {
    /// `next.findCallers` or `next.findReferences` for a single named declaration.
    pub(super) references: Option<Value>,
    /// `next.read` of a file outline's top declaration.
    #[serde(default)]
    pub(super) read: Option<Value>,
    /// `next.findText` for the name when the outline may be incomplete.
    #[serde(default)]
    pub(super) text_search: Option<Value>,
    pub(super) path: String,
    pub(super) snapshot: String,
    pub(super) declarations: Vec<Value>,
    pub(super) diagnostics: Vec<Value>,
    pub(super) files_scanned: usize,
    pub(super) skipped: u32,
    pub(super) truncated: bool,
    /// Where a scan cut by the time budget resumes (`scanOffset`); `None`
    /// when the cut is an explicit `maxFiles`.
    #[serde(default)]
    pub(super) scan_resume: Option<u32>,
    /// A parser recovered from syntax errors: the declarations are a partial
    /// view of the source, not a complete inventory.
    pub(super) recovered: bool,
    /// What the walk skipped, every path its own entry (paged with or
    /// without `extras:["details"]`).
    #[serde(default)]
    pub(super) skipped_paths: Option<octocode_contract_types::SkippedPaths>,
    /// What an exact `symbolName` match left out.
    #[serde(default)]
    pub(super) suppressed: Option<Suppressed>,
    /// Exact `symbolName` entries `kinds` left without a declaration.
    #[serde(default)]
    pub(super) kinds_left_out: Option<KindsLeftOut>,
    /// A directory outline: rows group under their files, and the outlined
    /// directory (the query's own path) is not restated.
    #[serde(default)]
    pub(super) directory: bool,
    /// The scopes a walk cut at the ceiling never reached.
    #[serde(default)]
    pub(super) unwalked: Option<crate::tools::ast_search::unwalked::Unwalked>,
    /// Entries the walk could not read, and how many for want of
    /// permission: absence below them is unproven.
    #[serde(default)]
    pub(super) walk_errors: (u32, u32),
}
