//! Partial-answer flags and the text-search leads that cover what the
//! server did not.
use super::*;
use octocode_contract_types::{CaseMode, LocalSearchQueryContent, RegexEngine};

/// Partial reasons about *files* the server's project may not see (or, for
/// dynamic dispatch, files it could not tie to the symbol). When every file
/// that spells the name is in the answer (`textOnlyFiles == 0`), such a
/// reason is moot and is not flagged. Cfg-gated code can hide sites inside
/// listed files, and failed importer verification leaves their bindings
/// unknown, so file agreement never clears either.
pub const FILE_LEVEL_REASONS: [octocode_runtime_base::response::paging::PartialReason; 8] = [
    super::super::inferred_project::REASON,
    super::super::importers::CAPPED_REASON,
    super::super::inferred_project::compile_database::NO_COMPILE_DATABASE_REASON,
    super::super::server_coverage::BUILD_SCRIPTS_REASON,
    super::super::server_coverage::OUTSIDE_INCLUDE_REASON,
    super::super::server_coverage::SHARED_MODULE_REASON,
    super::super::server_coverage::DYNAMIC_REASON,
    super::super::server_coverage::UNRESOLVED_IMPORT_REASON,
];

/// At most this many text-only files are listed as one bulk lead.
pub const LISTED_TEXT_FILES: usize = 5;

/// Mark an incoming-direction row partial: coverage reason, a warning, and
/// (when the query names its symbol) a lexical related localSearch over the
/// request's search scope. `coverage.textOnlyFiles` counts the files that
/// spell the name but are not in the answer; up to five are the lead's rows.
pub fn flag_partial(
    row: &mut Value,
    query: &LspSearchQuery,
    reason: octocode_runtime_base::response::paging::PartialReason,
    warning: Option<&str>,
    scope: &super::super::scope::Scope,
) {
    let name = lsp_search_query::symbol_name(query).filter(|name| !name.trim().is_empty());
    let text_only = name.and_then(|name| scope.text_only_files(name));
    let agrees = text_only.as_ref().is_some_and(Vec::is_empty);
    // The lead also reads each answered file that uses the name on lines
    // the answer misses; it is answered, so it is not counted.
    let lead_files = text_only.clone().map(|mut files| {
        for gap in name
            .map(|name| scope.answer_gaps(name))
            .iter()
            .flat_map(|gaps| gaps.iter())
        {
            if !files.contains(&gap.file) {
                files.push(gap.file.clone());
            }
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
            octocode_runtime_base::response::paging::mark_partial(row, reason, &warnings);
            let leads = text_leads(name, scope, lead_files.as_deref());
            if !leads.is_empty() {
                // One text check per row: this one names the files left.
                if let Some(related) = row.get_mut("related").and_then(Value::as_array_mut) {
                    related.retain(|call| call["tool"] != ToolId::LocalSearch.as_str());
                }
                for lead in leads {
                    octocode_runtime_base::response::result::attach_related(row, lead.build());
                }
            }
        }
        // A position anchor has no name to search for: coverage reason and
        // warning only.
        None => octocode_runtime_base::response::paging::push_warnings(row, warnings),
    }
}

/// The text check a flagged row leads to. Over an unbounded scope the root was
/// the scan's own, so the check searches the projects beside it, five per call
/// (every one reachable); otherwise it names `files`, or searches the root.
fn text_leads(
    name: &str,
    scope: &super::super::scope::Scope,
    files: Option<&[String]>,
) -> Vec<octocode_runtime_base::response::result::Continuation> {
    if scope.unbounded && !scope.siblings().is_empty() {
        // Each sibling is a directory: the family's globs keep the search to
        // the files the anchor's language spells the name in.
        return scope
            .siblings()
            .chunks(LISTED_TEXT_FILES)
            .filter_map(|chunk| listed_search_lead(name, chunk, &scope.include))
            .collect();
    }
    text_search_lead(name, &lead_root(name, scope), &scope.include, files)
        .into_iter()
        .collect()
}

/// Where a text check of `name` over many files searches: the directory its
/// declaration is reached under when one bounds it, else the scope's root.
fn lead_root(name: &str, scope: &super::super::scope::Scope) -> String {
    scope
        .reach_directory(name)
        .unwrap_or_else(|| scope.root.clone())
}

/// State what a `references` or `callers` answer does not list of the name
/// inside files it covers ([`super::super::scope::Scope::answer_gaps`]). A
/// site the server resolved to the anchor's declaration is a use it left
/// out: the row is partial, each file and line named. A site no identity
/// check placed may name another symbol of that spelling: it is named as
/// unverified, and proves no gap, so it leaves the row as it is. Either way
/// the text lead reads those files (or, when the row already leads to a
/// text search, that one does).
pub fn flag_answer_gaps(
    row: &mut Value,
    query: &LspSearchQuery,
    scope: &super::super::scope::Scope,
    paths: &octocode_runtime_base::policy::path::PathPolicy,
) {
    let Some(name) = lsp_search_query::symbol_name(query).filter(|name| !name.trim().is_empty())
    else {
        return;
    };
    let gaps = scope.answer_gaps(name);
    if gaps.is_empty() || !row.is_object() || row.get("errorCode").is_some() {
        return;
    }
    // A site in code the query's `testCode` leaves out is no gap of this
    // answer: it is counted as left out, like a listed site.
    let (gaps, left) = super::super::test_scope::retain_gaps(query, paths, &gaps);
    super::super::test_scope::disclose(row, left);
    if gaps.is_empty() {
        return;
    }
    let files = gaps.iter().map(|gap| gap.file.clone()).collect::<Vec<_>>();
    let shown = shown_paths(paths, &files);
    // The warnings name each file as the rows do; the lead keeps the paths
    // that run as given.
    let quoted = quoted_paths(paths, &files);
    let sites = |lines: fn(&super::super::scope::Gap) -> &[u32]| {
        let mut count = 0;
        let groups = gaps
            .iter()
            .zip(&quoted)
            .filter_map(|(gap, file)| {
                let lines = lines(gap);
                if lines.is_empty() {
                    return None;
                }
                count += lines.len();
                let positions = lines
                    .iter()
                    .map(|line| (line + 1).to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                Some(format!("{file}:{positions}"))
            })
            .collect::<Vec<_>>();
        (count, groups.join(", "))
    };
    let kind = if lsp_search_query::op(query) == super::super::Op::Callers {
        "call"
    } else {
        "use"
    };
    let (left_out_count, left_out) = sites(|gap| &gap.left_out);
    if left_out_count > 0 {
        octocode_runtime_base::response::paging::mark_partial(
            row,
            octocode_runtime_base::response::paging::PartialReason::ServerUnanswered,
            &[format!(
                "The server's answer leaves out {} where `{name}` names the anchor as a {kind}, in files it answers: {}. The text lead reads them.",
                octocode_engine::text::counted(left_out_count, "line", "lines"),
                left_out
            )],
        );
    }
    let (unverified_count, unverified) = sites(|gap| &gap.unverified);
    if unverified_count > 0 {
        octocode_runtime_base::response::paging::push_warnings(
            row,
            [format!(
                "{} in files the answer covers spell `{name}` in code and are not in it: {}. The server did not say what they name, so they are unverified (another symbol of that name, or code it does not analyze), not missing {kind}s. The text lead reads them.",
                octocode_engine::text::counted(unverified_count, "line", "lines"),
                unverified
            )],
        );
    }
    let leads_text = row
        .get("related")
        .and_then(Value::as_array)
        .is_some_and(|related| {
            related
                .iter()
                .any(|call| call["tool"] == ToolId::LocalSearch.as_str())
        });
    if !leads_text
        && let Some(lead) =
            text_search_lead(name, &lead_root(name, scope), &scope.include, Some(&shown))
    {
        octocode_runtime_base::response::result::attach_related(row, lead.build());
    }
}

/// Stamp the text check of `name` on `row`: `coverage.textOnlyFiles`
/// (`files`, those that could still hold a missed site) and, when the
/// anchor's declaration bounds which files could reference it,
/// `coverage.outOfReachFiles` with that bound, said once as a notice: those
/// files are checked, not unverified.
pub fn record_text_only(
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
        // The files read that spell the name keep their count; the files a
        // bounded check left unread are counted apart, never dropped.
        if out.files > 0 {
            coverage["outOfReachFiles"] = json!(out.files);
        }
        if out.unsearched > 0 {
            coverage["unreadOutOfReach"] = json!(out.unsearched);
        }
        let counted = match (out.files > 0, out.unsearched > 0) {
            (true, false) => "coverage.outOfReachFiles",
            (false, _) => "coverage.unreadOutOfReach",
            (true, true) => "coverage.outOfReachFiles and coverage.unreadOutOfReach",
        };
        octocode_runtime_base::response::paging::push_notice(
            row,
            format!(
                "{counted}: files outside the reach of `{name}` ({}); they cannot reference it.",
                out.bound
            ),
        );
    }
}

/// Whether [`flag_partial`] already flagged `row` with `reasons`' reason:
/// the coverage reason of an unnamed row, or a `partialReasons` entry.
pub fn flagged_with(
    row: &Value,
    reasons: &[octocode_runtime_base::response::paging::PartialReason],
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

/// The whole-word, case-sensitive content search of `name` under `path`: a
/// `rust` regex, as an unset `regex` would search its `\b` spelling as
/// literal text. `None` when `name` or `path` is not a representable value.
fn text_search_query(name: &str, path: &str) -> Option<LocalSearchQueryContent> {
    let pattern = octocode_engine::text::word_pattern(name);
    Some(LocalSearchQueryContent {
        case_mode: CaseMode::Sensitive,
        regex_engine: RegexEngine::Rust,
        ..octocode_tools_shared::source_view::extraction::text_search_query(path, &[&pattern])?
    })
}

/// One lead of a search row per path, each limited to the `include` globs.
fn listed_search_lead(
    name: &str,
    paths: &[String],
    include: &[String],
) -> Option<octocode_runtime_base::response::result::Continuation> {
    let rows = paths
        .iter()
        .map(|path| {
            let search = LocalSearchQueryContent {
                include: include.to_vec(),
                ..text_search_query(name, path)?
            };
            Some(octocode_runtime_base::response::result::query_row(
                &search,
                &[],
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(
        octocode_runtime_base::response::result::Continuation::related_input(
            ToolId::LocalSearch,
            json!({ "queries": rows }),
        ),
    )
}

/// The one lexical lead every lspSearch row builds: one row per listed file
/// when there are a few, otherwise `root` with the language family's globs.
/// Paths are the caller's (workspace-relative as the response names them).
/// The match is the symbol, whole-word and case-sensitive like the scans it
/// stands in for; the word-bounded pattern is a `rust` regex, as an unset
/// `regex` would search its `\b` spelling as literal text. Unbuilt, so a
/// caller can lower its confidence.
pub fn text_search_lead(
    name: &str,
    root: &str,
    include: &[String],
    files: Option<&[String]>,
) -> Option<octocode_runtime_base::response::result::Continuation> {
    if let Some(files) = files.filter(|files| (1..=LISTED_TEXT_FILES).contains(&files.len())) {
        return listed_search_lead(name, files, &[]);
    }
    let search = LocalSearchQueryContent {
        include: include.to_vec(),
        ..text_search_query(name, root)?
    };
    Some(octocode_runtime_base::response::result::Continuation::related_query(search))
}

/// The lexical lead of a `workspaceSymbol` query term over `root`: the term
/// as typed, literal text anywhere in a word, smart case. Servers match the
/// term as a fuzzy prefix (`ToolRunt` finds `ToolRuntime`), so a whole-word
/// search of it would find none of what they find.
pub fn term_search_lead(
    term: &str,
    root: &str,
    include: &[String],
) -> Option<octocode_runtime_base::response::result::Continuation> {
    let search = LocalSearchQueryContent {
        include: include.to_vec(),
        ..octocode_tools_shared::source_view::extraction::text_search_query(root, &[term])?
    };
    Some(octocode_runtime_base::response::result::Continuation::related_query(search))
}

/// The lexical lead of the query's symbol over `root`: its query term as
/// typed for `workspaceSymbol` ([`term_search_lead`]), else the declared
/// name ([`text_search_lead`]).
pub(super) fn symbol_search_lead(
    query: &LspSearchQuery,
    symbol: &str,
    root: &str,
) -> Option<octocode_runtime_base::response::result::Continuation> {
    if lsp_search_query::op(query) == super::super::Op::WorkspaceSymbol {
        term_search_lead(symbol, root, &[])
    } else {
        text_search_lead(symbol, root, &[], None)
    }
}

/// `files` as a warning quotes them: named as the rows name them, from the
/// workspace or from the root the row states outside it
/// ([`octocode_runtime_base::response::paths::shown_path`]).
pub fn quoted_paths(
    paths: &octocode_runtime_base::policy::path::PathPolicy,
    files: &[String],
) -> Vec<String> {
    files
        .iter()
        .map(|file| octocode_runtime_base::response::paths::shown_path(paths, file))
        .collect()
}

/// `files` named as a lead's input names them: relative to the workspace
/// when inside it, as given otherwise, so the call runs unchanged.
pub fn shown_paths(
    paths: &octocode_runtime_base::policy::path::PathPolicy,
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
