//! Partial-answer flags and the text-search leads that cover what the
//! server did not.
use super::*;

/// Partial reasons about *files* the server's project may not see (or, for
/// dynamic dispatch, files it could not tie to the symbol). When every file
/// that spells the name is in the answer (`textOnlyFiles == 0`), such a
/// reason is moot and is not flagged. Cfg-gated code can hide sites inside
/// listed files, so file agreement never clears it.
pub(in super::super) const FILE_LEVEL_REASONS: [crate::response::paging::PartialReason; 9] = [
    super::super::inferred_project::REASON,
    super::super::importers::CAPPED_REASON,
    super::super::inferred_project::FAILED_REASON,
    super::super::inferred_project::NO_COMPILE_DATABASE_REASON,
    super::super::server_coverage::BUILD_SCRIPTS_REASON,
    super::super::server_coverage::OUTSIDE_INCLUDE_REASON,
    super::super::server_coverage::SHARED_MODULE_REASON,
    super::super::server_coverage::DYNAMIC_REASON,
    super::super::server_coverage::UNRESOLVED_IMPORT_REASON,
];

/// At most this many text-only files are listed as one bulk lead.
pub(in super::super) const LISTED_TEXT_FILES: usize = 5;

/// Mark an incoming-direction row partial: coverage reason, a warning, and
/// (when the query names its symbol) a lexical `findText` lead over the
/// request's search scope. `coverage.textOnlyFiles` counts the files that
/// spell the name but are not in the answer; up to five are the lead's rows.
pub(in super::super) fn flag_partial(
    row: &mut Value,
    query: &LspSearchQuery,
    reason: crate::response::paging::PartialReason,
    warning: Option<&str>,
    scope: &super::super::scope::Scope,
) {
    let name = lsp_search_query::symbol_name(query).filter(|name| !name.trim().is_empty());
    let text_only = name.and_then(|name| scope.text_only_files(name));
    let agrees = text_only.as_ref().is_some_and(Vec::is_empty);
    // The lead also reads the anchor file when it uses the name on lines
    // the answer misses; it is answered, so it is not counted.
    let lead_files = text_only.clone().map(|mut files| {
        if let Some(anchor) = name.and_then(|name| scope.anchor_gap(name))
            && !files.contains(&anchor)
        {
            files.push(anchor);
        }
        files
    });
    if let (Some(name), Some(files)) = (name, &text_only) {
        record_text_only(row, scope, name, files.len());
    }
    if let Some(payload) = row.as_object_mut() {
        if agrees && FILE_LEVEL_REASONS.contains(&reason) {
            return;
        }
        // A named row states its reason once, in `partialReasons`.
        if name.is_none() {
            payload.entry("coverage").or_insert_with(|| json!({}))["reason"] = json!(reason);
        }
    }
    let warnings = warning.map(str::to_owned).into_iter().collect::<Vec<_>>();
    // A warning, not a hint: the hint policy keeps hints for empty/error rows
    // only, and this caveat matters most when the row looks complete.
    match name {
        // Partial only with an executable recovery.
        Some(name) => {
            crate::response::paging::mark_partial(row, reason, &warnings);
            crate::response::result::attach_related(
                row,
                text_search_lead(name, &scope.root, &scope.include, lead_files.as_deref()).build(),
            );
        }
        // A position anchor has no name to search for: coverage reason and
        // warning only.
        None => crate::response::paging::push_warnings(row, warnings),
    }
}

/// Stamp the text check of `name` on `row`: `coverage.textOnlyFiles`
/// (`files`, those that could still hold a missed site) and, when the
/// anchor's declaration bounds which files could reference it,
/// `coverage.outOfReachFiles` with that bound, said once as a notice: those
/// files are checked, not unverified.
pub(in super::super) fn record_text_only(
    row: &mut Value,
    scope: &super::super::scope::Scope,
    name: &str,
    files: usize,
) {
    let out = scope.out_of_reach(name);
    let Some(payload) = row.as_object_mut() else {
        return;
    };
    let coverage = payload.entry("coverage").or_insert_with(|| json!({}));
    coverage["textOnlyFiles"] = json!(files);
    if let Some(out) = out {
        coverage["outOfReachFiles"] = json!(out.files);
        crate::response::paging::push_notice(
            row,
            format!(
                "coverage.outOfReachFiles counts files that spell `{name}` outside its reach ({}): they cannot reference it, so they are neither missed sites nor leads.",
                out.bound
            ),
        );
    }
}

/// Whether [`flag_partial`] already flagged `row` with `reasons`' reason:
/// the coverage reason of an unnamed row, or a `partialReasons` entry.
pub(in super::super) fn flagged_with(
    row: &Value,
    reasons: &[crate::response::paging::PartialReason],
) -> bool {
    row.pointer("/coverage/reason").is_some()
        || row
            .get("partialReasons")
            .and_then(Value::as_array)
            .is_some_and(|listed| {
                listed
                    .iter()
                    .filter_map(|reason| reason.as_str()?.parse().ok())
                    .any(|reason| reasons.contains(&reason))
            })
}

/// How a coverage warning names the lexical check: by the search itself (the
/// `findText` lead, which a partial row without a page always keeps, names
/// the files).
pub(in super::super) const TEXT_CHECK: &str = "a localSearch of the name";

/// Recover known ignored paths and, when unfinished, the unsearched scope.
/// If the combined queries exceed the batch limit, an unfiltered root search
/// covers the known paths, including nested roots and files outside the family.
pub(in super::super) fn ignored_text_lead(
    name: &str,
    scope: &super::super::scope::Scope,
    files: &[String],
    unfinished: bool,
) -> Value {
    let mut rows = Vec::new();
    for file in files {
        let row = text_search_row(name, file);
        if !rows.contains(&row) {
            rows.push(row);
        }
    }
    let mut root = text_search_row(name, &scope.root);
    if !scope.include.is_empty() {
        root["include"] = json!(scope.include);
    }
    if (unfinished || rows.is_empty()) && !rows.contains(&root) {
        rows.push(root.clone());
    }
    let broad = rows.len() > LISTED_TEXT_FILES;
    if broad {
        let mut recovery = text_search_row(name, &scope.root);
        // A root walk must reach paths that explicit file/root queries reach.
        recovery["defaultExcludes"] = json!(false);
        recovery["includeHidden"] = json!(true);
        rows = vec![recovery];
        if unfinished && !rows.contains(&root) {
            rows.push(root);
        }
    }
    for row in &mut rows {
        row["includeIgnored"] = json!(true);
    }
    crate::response::result::Continuation::related_input(
        ToolId::LocalSearch,
        json!({"queries": rows}),
    )
    .build()
}

pub(super) fn text_search_row(name: &str, path: &str) -> Value {
    json!({
        "path": path,
        "matchString": [octocode_engine::text::word_pattern(name)],
        "regexEngine": "rust",
        "caseMode": "sensitive",
    })
}

/// The one lexical lead every lspSearch row builds: one row per listed file
/// when there are a few, otherwise `root` with the language family's globs.
/// Paths are the caller's (workspace-relative as the response names them).
/// The match is the symbol, whole-word and case-sensitive like the scans it
/// stands in for; the word-bounded pattern is a `rust` regex, as an unset
/// `regex` would search its `\b` spelling as literal text. Unbuilt, so a
/// caller can lower its confidence.
pub(in super::super) fn text_search_lead(
    name: &str,
    root: &str,
    include: &[String],
    files: Option<&[String]>,
) -> crate::response::result::Continuation {
    let row = |path: &str| text_search_row(name, path);
    if let Some(files) = files.filter(|files| (1..=LISTED_TEXT_FILES).contains(&files.len())) {
        let rows = files.iter().map(|file| row(file)).collect::<Vec<_>>();
        return crate::response::result::Continuation::related_input(
            ToolId::LocalSearch,
            json!({ "queries": rows }),
        );
    }
    let mut search = row(root);
    if !include.is_empty() {
        search["include"] = json!(include);
    }
    crate::response::result::Continuation::related(ToolId::LocalSearch, search)
}

/// The lexical lead of a `workspaceSymbol` query term over `root`: the term
/// as typed, literal text anywhere in a word, smart case. Servers match the
/// term as a fuzzy prefix (`ToolRunt` finds `ToolRuntime`), so a whole-word
/// search of it would find none of what they find.
pub(in super::super) fn term_search_lead(
    term: &str,
    root: &str,
    include: &[String],
) -> crate::response::result::Continuation {
    let mut search = json!({"path": root, "matchString": [term]});
    if !include.is_empty() {
        search["include"] = json!(include);
    }
    crate::response::result::Continuation::related(ToolId::LocalSearch, search)
}

/// The lexical lead of the query's symbol over `root`: its query term as
/// typed for `workspaceSymbol` ([`term_search_lead`]), else the declared
/// name ([`text_search_lead`]).
pub(super) fn symbol_search_lead(
    query: &LspSearchQuery,
    symbol: &str,
    root: &str,
) -> crate::response::result::Continuation {
    if lsp_search_query::op(query) == super::super::Op::WorkspaceSymbol {
        term_search_lead(symbol, root, &[])
    } else {
        text_search_lead(symbol, root, &[], None)
    }
}

/// `files` named as the response names them: relative to the workspace when
/// inside it, as given otherwise.
pub(in super::super) fn shown_paths(
    paths: &crate::policy::path::PathPolicy,
    files: &[String],
) -> Vec<String> {
    files
        .iter()
        .map(|file| {
            paths
                .workspace_relative(file)
                .unwrap_or_else(|| file.clone())
        })
        .collect()
}
