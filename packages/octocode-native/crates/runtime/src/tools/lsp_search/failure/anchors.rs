//! Path and anchor recovery: directory anchors, nearby names, order hints
//! and the recovery `next` call.
use super::*;

/// A request whose path did not validate, as a row where its lead says
/// what to do (a directory anchor, a missing path), else the failure.
pub(in super::super) fn path_failure(
    query: &LspSearchQuery,
    paths: &crate::policy::path::PathPolicy,
    failure: LspFailure,
) -> Result<Value, LspFailure> {
    if let Some(row) = directory_anchor(query, paths) {
        return Ok(row);
    }
    if failure.code != ErrorCode::PathNotFound {
        return Err(failure);
    }
    let mut row = json!({
        "status": "error",
        "errorCode": failure.code,
        "error": failure.message,
        "lsp": { "serverAvailable": false },
    });
    let requested = lsp_search_query::path(query)
        .map(uri_to_path)
        .unwrap_or_default();
    if let Some(next) = crate::response::result::missing_path_lead(paths, &requested) {
        row["next"] = next;
    }
    Ok(row)
}

/// The `notAFile` row of a request whose file anchor names a directory,
/// leading into it: a whole-word search for the named symbol, else the
/// directory's file listing. `None` when the path is no authorized directory.
pub(in super::super) fn directory_anchor(
    query: &LspSearchQuery,
    paths: &crate::policy::path::PathPolicy,
) -> Option<Value> {
    let decoded = super::super::render::decode_uri_path(lsp_search_query::path(query)?).ok()?;
    let directory = paths
        .validate(&decoded)
        .ok()
        .filter(|valid| valid.canonical.is_dir())?
        .canonical;
    let name = paths
        .workspace_relative(&directory)
        .unwrap_or_else(|| directory.to_string_lossy().into_owned());
    let mut row = json!({
        "status": "error",
        "errorCode": ErrorCode::NotAFile,
        "error": format!("{name} is a directory, not a file."),
        "lsp": { "serverAvailable": false },
    });
    match lsp_search_query::symbol_name(query).filter(|symbol| !symbol.trim().is_empty()) {
        Some(symbol) => {
            row["next"]["findText"] = text_search_lead(symbol, &name, &[], None).build();
        }
        None => {
            row["next"]["viewTree"] = crate::response::result::Continuation::new(
                ToolId::StructureSearch,
                json!({"operation": "files", "path": name}),
            )
            .build();
        }
    }
    Some(row)
}

/// `next.read` recovery: the symbol's match windows when a name anchored
/// the request (not the whole file), otherwise the file's default view.
pub(in super::super) fn attach_recovery_next(value: &mut Value, query: &LspSearchQuery) {
    let path = lsp_search_query::path(query)
        .map(uri_to_path)
        .unwrap_or_default();
    let symbol = lsp_search_query::symbol_name(query).filter(|name| !name.trim().is_empty());
    // A directory (a workspaceSymbol scope) is searched, not fetched.
    if let Some(symbol) = symbol.filter(|_| std::path::Path::new(&path).is_dir()) {
        value["next"]["findText"] = symbol_search_lead(query, symbol, &path)
            .confidence(Confidence::Exact)
            .build();
        return;
    }
    let mut read = json!({ "path": path });
    if let Some(symbol) = symbol {
        read["matchString"] = json!(symbol);
        read["caseMode"] = json!("sensitive");
        read["contextLines"] = json!(3);
    }
    value["next"]["read"] = crate::response::result::Continuation::new(ToolId::LocalFetch, read)
        .confidence(Confidence::Exact)
        .build();
}

/// Lines scanned on each side of `line` for a near-miss name.
pub(super) const SUGGESTION_RADIUS: usize = 20;

/// Recovery for a `symbolName` that did not resolve: the name elsewhere in
/// the file or the closest identifier near `line` as an executable
/// `next.findSimilar`; without any candidate in the file, a
/// `workspaceSymbol` search that locates the name where it is declared.
pub(in super::super) fn anchor_recovery(
    value: &mut Value,
    query: &LspSearchQuery,
    source: Option<&str>,
) {
    let (Some(name), Some(path)) = (
        lsp_search_query::symbol_name(query),
        lsp_search_query::path(query).map(uri_to_path),
    ) else {
        return;
    };
    let hint = lsp_search_query::anchor_line(query).filter(|line| *line > 0);
    if let (Some(source), Some(line)) = (source, hint)
        && order_hint_recovery(value, query, source, name, line)
    {
        return;
    }
    // The name itself elsewhere in the file (a stale `line`) first,
    // declarations before uses; then near-miss spellings. A line the grammar
    // places the name only in comments or strings is no anchor, and the
    // failing line itself would repeat this query.
    let mentions = source
        .map(|source| name_lines(source, name, hint))
        .unwrap_or_default();
    let code = source.and_then(|source| {
        octocode_engine::lsp::resolver::code_occurrence_lines(&path, source, name)
    });
    let exact = mentions
        .iter()
        .copied()
        .filter(|line| {
            hint.is_none_or(|hint| *line != hint as usize)
                && code
                    .as_ref()
                    .is_none_or(|lines| lines.contains(&(line - 1)))
        })
        .collect::<Vec<_>>();
    let near = source
        .map(|source| near_names(&path, source, name, hint))
        .unwrap_or_default();
    let leads = exact
        .iter()
        .map(|line| (name.to_owned(), *line, Confidence::High))
        .chain(
            near.iter()
                .map(|(word, line)| (word.clone(), *line, Confidence::Medium)),
        )
        .take(crate::response::result::rows_per_call(ToolId::LspSearch))
        .collect::<Vec<_>>();
    if let Some(object) = value["next"].as_object_mut() {
        object.remove("read");
    }
    // No anchorable spelling here: the name is only in comments or strings,
    // or not in this file at all, so the symbol, if any, is declared
    // elsewhere. The lead locates it in the workspace, and its rows carry
    // the path and line to retry with; the lines around `line` cannot
    // hold it.
    if leads.is_empty() {
        value["next"]["findSimilar"] =
            crate::response::result::Continuation::new(ToolId::LspSearch, workspace_symbol(query))
                .confidence(Confidence::Medium)
                .build();
        return;
    }
    for (index, (symbol, line, confidence)) in leads.iter().enumerate() {
        let mut retry = lsp_search_query::to_row(query);
        if let Some(object) = retry.as_object_mut() {
            object.insert("symbolName".into(), json!(symbol));
            object.insert("line".into(), json!(line));
            object.remove("orderHint");
        }
        let key = match index {
            0 => "findSimilar".to_owned(),
            n => format!("findSimilar{}", n + 1),
        };
        value["next"][key] = crate::response::result::Continuation::new(ToolId::LspSearch, retry)
            .confidence(*confidence)
            .build();
    }
    // Candidates the leads already carry are not repeated; only lines and
    // names beyond one admitted batch are stated.
    let offered = leads.len();
    let mut notes = Vec::new();
    let uncovered_exact = exact.iter().skip(offered).collect::<Vec<_>>();
    if !uncovered_exact.is_empty() {
        let lines = uncovered_exact
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let s = if uncovered_exact.len() == 1 { "" } else { "s" };
        notes.push(format!("`{name}` also appears on line{s} {lines}."));
    }
    let uncovered_near = near
        .iter()
        .skip(offered.saturating_sub(exact.len()))
        .collect::<Vec<_>>();
    if !uncovered_near.is_empty() {
        let listed = uncovered_near
            .iter()
            .map(|(name, line)| format!("`{name}` (line {line})"))
            .collect::<Vec<_>>()
            .join(", ");
        notes.push(format!("Closest names: {listed}."));
    }
    if let Some(hints) = value["hints"].as_array_mut() {
        hints.extend(notes.into_iter().map(Value::from));
    }
}

/// A `workspaceSymbol` search for the query's name from the same file and
/// server: the fields that operation takes, nothing anchor-specific.
pub(super) fn workspace_symbol(query: &LspSearchQuery) -> Value {
    let mut search = json!({"operation": "workspaceSymbol"});
    let asked = lsp_search_query::to_row(query);
    for key in ["path", "symbolName", "workspaceRoot", "rustContext"] {
        if let Some(value) = asked.get(key).filter(|value| !value.is_null()) {
            search[key] = value.clone();
        }
    }
    search
}

/// An `orderHint` past the occurrences its line holds: the one lead is the
/// same anchor at the last occurrence (`orderHint` is zero-based), not
/// other lines of the file. `false` when the line does not spell the name
/// (the anchor is stale: the general recovery applies).
pub(super) fn order_hint_recovery(
    value: &mut Value,
    query: &LspSearchQuery,
    source: &str,
    name: &str,
    line: u32,
) -> bool {
    let Some(order_hint) = lsp_search_query::order_hint(query) else {
        return false;
    };
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let Some(text) = source.lines().nth(line as usize - 1) else {
        return false;
    };
    let count = text
        .match_indices(name)
        .filter(|(at, _)| {
            !text[..*at].chars().next_back().is_some_and(is_word)
                && !text[at + name.len()..].chars().next().is_some_and(is_word)
        })
        .count();
    let Some(last) = u32::try_from(count)
        .ok()
        .and_then(|count| count.checked_sub(1))
    else {
        return false;
    };
    if order_hint <= last {
        return false;
    }
    let mut retry = lsp_search_query::to_row(query);
    retry["orderHint"] = json!(last);
    if let Some(object) = value["next"].as_object_mut() {
        object.remove("read");
    }
    value["next"]["findSimilar"] =
        crate::response::result::Continuation::new(ToolId::LspSearch, retry)
            .confidence(Confidence::High)
            .build();
    true
}

/// Declaration keywords that may precede a declared name on its line.
pub(super) const DECLARATION_WORDS: &[&str] = &[
    "function",
    "class",
    "interface",
    "type",
    "enum",
    "const",
    "let",
    "var",
    "fn",
    "struct",
    "trait",
    "impl",
    "mod",
    "def",
    "func",
    "static",
    "namespace",
    "module",
];

/// One-based lines that spell `name` as a word: declaration lines first
/// (a declaration keyword just before the name), then the rest, each
/// group nearest `hint` first. One admitted batch becomes suggestions;
/// remaining candidates are stated as line numbers.
pub(super) fn name_lines(source: &str, name: &str, hint: Option<u32>) -> Vec<usize> {
    if name.trim().is_empty() {
        return Vec::new();
    }
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let hint = hint.map_or(0, |line| line as usize);
    let mut found: Vec<(bool, usize, usize)> = Vec::new();
    for (index, text) in source.lines().enumerate() {
        let declares = text.match_indices(name).any(|(at, _)| {
            let bounded = !text[..at].chars().next_back().is_some_and(is_word)
                && !text[at + name.len()..].chars().next().is_some_and(is_word);
            bounded
                && text[..at]
                    .split(|c: char| !is_word(c))
                    .rfind(|word| !word.is_empty())
                    .is_some_and(|word| DECLARATION_WORDS.contains(&word))
        });
        let mentions = declares
            || text.match_indices(name).any(|(at, _)| {
                !text[..at].chars().next_back().is_some_and(is_word)
                    && !text[at + name.len()..].chars().next().is_some_and(is_word)
            });
        if mentions {
            let line = index + 1;
            found.push((!declares, line.abs_diff(hint), line));
        }
    }
    found.sort_unstable();
    found.into_iter().map(|(_, _, line)| line).collect()
}

/// Up to three distinct identifiers within a small edit distance of `name`,
/// nearest `line` first (the whole file without a hint). A word the
/// grammar places only in comments or strings is no symbol to anchor on.
pub(super) fn near_names(
    path: &str,
    source: &str,
    name: &str,
    hint: Option<u32>,
) -> Vec<(String, usize)> {
    let hint = hint.map(|line| line as usize);
    let limit = (name.chars().count() / 4).max(1);
    let mut found: Vec<(usize, usize, String, usize)> = Vec::new();
    for (index, text) in source.lines().enumerate() {
        let line = index + 1;
        if hint.is_some_and(|hint| line.abs_diff(hint) > SUGGESTION_RADIUS) {
            continue;
        }
        for word in text
            .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
            .filter(|word| !word.is_empty() && *word != name && word.len() <= 256)
        {
            let distance = crate::contracts::levenshtein(word, name);
            if distance <= limit && !found.iter().any(|entry| entry.2 == word) {
                let offset = hint.map_or(0, |hint| line.abs_diff(hint));
                found.push((distance, offset, word.to_owned(), line));
            }
        }
    }
    found.sort();
    found
        .into_iter()
        .filter(|(_, _, word, line)| {
            octocode_engine::lsp::resolver::code_occurrence_lines(path, source, word)
                .is_none_or(|lines| lines.contains(&(line - 1)))
        })
        .take(3)
        .map(|(_, _, word, line)| (word, line))
        .collect()
}
